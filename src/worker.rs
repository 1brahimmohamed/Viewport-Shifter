//! Background thread: global hotkeys, auto-apply of game profiles, keeping the window in place.
//! Everything here is plain window-manager calls; nothing touches the game process.

use crate::config::{Config, parse_hotkey};
use crate::layout::{FitMode, Mode, Rect, compute_rect};
use crate::shifter::{self, Saved};
use crate::target::{self, WindowInfo};
use crate::monitors;
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetTimer,
    WM_APP, WM_HOTKEY, WM_TIMER,
};

pub enum Command {
    SetTarget(Option<WindowInfo>),
    Apply(Mode),
    Restore,
    SetConfig(Config),
    Quit,
}

#[derive(Clone, Default)]
pub struct Status {
    pub target: Option<String>,
    pub active: bool,
    pub mode: Mode,
    pub want: Option<Rect>,
    pub got: Option<Rect>,
    pub message: String,
}

pub struct Handle {
    tx: Sender<Command>,
    thread_id: u32,
    pub status: Arc<Mutex<Status>>,
    join: Option<JoinHandle<()>>,
}

impl Handle {
    pub fn send(&self, c: Command) {
        if self.tx.send(c).is_ok() {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_APP, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn shutdown(&mut self) {
        self.send(Command::Restore);
        self.send(Command::Quit);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

pub fn spawn(cfg: Config, ctx: eframe::egui::Context) -> Handle {
    let (tx, rx) = channel();
    let (id_tx, id_rx) = sync_channel(1);
    let status = Arc::new(Mutex::new(Status::default()));
    let st = status.clone();
    let join = std::thread::spawn(move || {
        let mut msg = MSG::default();
        // Force creation of this thread's message queue before publishing the thread id.
        unsafe {
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let _ = id_tx.send(GetCurrentThreadId());
            SetTimer(None, 0, 500, None);
        }
        let mut w = Worker { cfg, rx, status: st, ctx, target: None, saved: None, active: false, mode: Mode::Normal, want: None };
        w.register_hotkeys();
        loop {
            let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if r.0 <= 0 {
                break;
            }
            match msg.message {
                WM_HOTKEY => w.on_hotkey(msg.wParam.0 as i32),
                WM_TIMER => w.tick(),
                WM_APP => {
                    if w.drain() {
                        break;
                    }
                }
                _ => {}
            }
        }
        w.unregister_hotkeys();
    });
    let thread_id = id_rx.recv().unwrap_or(0);
    Handle { tx, thread_id, status, join: Some(join) }
}

struct Worker {
    cfg: Config,
    rx: Receiver<Command>,
    status: Arc<Mutex<Status>>,
    ctx: eframe::egui::Context,
    target: Option<(isize, String)>,
    saved: Option<Saved>,
    active: bool,
    mode: Mode,
    want: Option<Rect>,
}

impl Worker {
    fn hwnd(&self) -> Option<HWND> {
        self.target.as_ref().map(|(h, _)| HWND(*h as *mut _))
    }

    fn publish(&self, message: impl Into<String>, got: Option<Rect>) {
        if let Ok(mut s) = self.status.lock() {
            s.target = self.target.as_ref().map(|(_, n)| n.clone());
            s.active = self.active;
            s.mode = self.mode;
            s.want = self.want;
            s.got = got;
            s.message = message.into();
        }
        self.ctx.request_repaint();
    }

    fn register_hotkeys(&self) {
        let hk = &self.cfg.hotkeys;
        for (id, s) in [(1, &hk.left), (2, &hk.right), (3, &hk.normal), (4, &hk.restore)] {
            unsafe {
                let _ = UnregisterHotKey(None, id);
                if let Some((m, k)) = parse_hotkey(s) {
                    let _ = RegisterHotKey(None, id, m, k);
                }
            }
        }
    }

    fn unregister_hotkeys(&self) {
        for id in 1..=4 {
            unsafe {
                let _ = UnregisterHotKey(None, id);
            }
        }
    }

    /// Returns true when asked to quit.
    fn drain(&mut self) -> bool {
        while let Ok(c) = self.rx.try_recv() {
            match c {
                Command::Quit => return true,
                Command::SetTarget(t) => {
                    self.restore();
                    self.target = t.map(|w| (w.hwnd, format!("{} ({})", w.exe, w.title)));
                    self.publish(if self.target.is_some() { "Target selected" } else { "Target cleared" }, None);
                }
                Command::Apply(m) => self.apply(m),
                Command::Restore => self.restore(),
                Command::SetConfig(cfg) => {
                    self.cfg = cfg;
                    self.register_hotkeys();
                    if self.active {
                        self.apply(self.mode);
                    }
                }
            }
        }
        false
    }

    fn on_hotkey(&mut self, id: i32) {
        match id {
            1 => self.apply(Mode::Left),
            2 => self.apply(Mode::Right),
            3 => self.apply(Mode::Normal),
            4 => self.restore(),
            _ => {}
        }
    }

    fn exe_of_target(&self) -> Option<String> {
        let (h, _) = self.target.as_ref()?;
        target::info_for(HWND(*h as *mut _)).map(|i| i.exe)
    }

    fn settings(&self) -> (f32, FitMode) {
        match self.exe_of_target().and_then(|e| self.cfg.profile_for(&e).cloned()) {
            Some(p) => (p.center_frac, p.fit),
            None => (self.cfg.center_frac, self.cfg.fit),
        }
    }

    fn apply(&mut self, mode: Mode) {
        if self.target.is_none() {
            // Hotkey with nothing selected: grab the foreground window (never our own).
            let fg = unsafe { GetForegroundWindow() };
            match target::info_for(fg) {
                Some(i) if i.pid != std::process::id() && !i.title.is_empty() => {
                    self.target = Some((i.hwnd, format!("{} ({})", i.exe, i.title)));
                }
                _ => return self.publish("No game window selected", None),
            }
        }
        let Some(hwnd) = self.hwnd() else { return };
        if !shifter::is_alive(hwnd) {
            self.target = None;
            self.active = false;
            return self.publish("Target window is gone", None);
        }
        if !self.active {
            self.saved = shifter::save(hwnd);
        }
        let panels = monitors::panels(self.cfg.surround_panels, self.cfg.bezel_px);
        let (frac, fit) = self.settings();
        let Some(want) = compute_rect(&panels, mode, frac, fit) else {
            return self.publish("No monitors found", None);
        };
        self.mode = mode;
        self.want = Some(want);
        match shifter::apply(hwnd, want) {
            Ok(got) => {
                self.active = true;
                let msg = if got == want {
                    "Applied".to_string()
                } else {
                    format!("Windows/the game changed the size: wanted {}x{}, got {}x{}", want.w, want.h, got.w, got.h)
                };
                self.publish(msg, Some(got));
            }
            Err(e) => self.publish(format!("Failed: {e}"), None),
        }
    }

    fn restore(&mut self) {
        if let (Some(hwnd), Some(saved)) = (self.hwnd(), self.saved.take()) {
            if self.active {
                let _ = shifter::restore(hwnd, saved);
            }
        }
        if self.active {
            self.active = false;
            self.want = None;
            self.mode = Mode::Normal;
            self.publish("Restored original window", None);
        }
    }

    /// Every 500 ms: drop dead targets, auto-attach profiles, and undo games moving their window back.
    fn tick(&mut self) {
        if let Some(h) = self.hwnd() {
            if !shifter::is_alive(h) {
                self.target = None;
                self.active = false;
                self.saved = None;
                return self.publish("Target window closed", None);
            }
            if self.active {
                if let (Some(want), Some(now)) = (self.want, shifter::window_rect(h)) {
                    if now != want {
                        self.apply(self.mode);
                    }
                }
            }
            return;
        }
        let hit = self
            .cfg
            .profiles
            .iter()
            .filter(|p| !p.exe.is_empty())
            .find_map(|p| target::find_by_exe(&p.exe).map(|w| (w, p.default_side)));
        if let Some((w, side)) = hit {
            self.target = Some((w.hwnd, format!("{} ({})", w.exe, w.title)));
            self.apply(side);
        }
    }
}
