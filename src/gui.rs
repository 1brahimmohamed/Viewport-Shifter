use crate::config::{Config, Profile};
use crate::layout::{FitMode, Mode, Rect, compute_rect, desktop_of, wasted_fraction};
use crate::monitors;
use crate::target::{self, WindowInfo};
use crate::worker::{Command, Handle};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, StrokeKind, Vec2};

pub struct App {
    cfg: Config,
    worker: Handle,
    windows: Vec<WindowInfo>,
    selected: Option<usize>,
    save_error: Option<String>,
}

impl App {
    pub fn new(cfg: Config, worker: Handle) -> Self {
        let mut app = Self { cfg, worker, windows: Vec::new(), selected: None, save_error: None };
        app.refresh_windows();
        app
    }

    fn refresh_windows(&mut self) {
        let me = std::process::id();
        self.windows = target::list_windows().into_iter().filter(|w| w.pid != me).collect();
        self.selected = None;
    }

    fn push_config(&mut self) {
        self.save_error = self.cfg.save().err().map(|e| e.to_string());
        self.worker.send(Command::SetConfig(self.cfg.clone()));
    }
}

fn mode_label(m: Mode) -> &'static str {
    match m {
        Mode::Left => "Left",
        Mode::Right => "Right",
        Mode::Normal => "Normal",
    }
}

fn fit_label(f: FitMode) -> &'static str {
    match f {
        FitMode::Full => "Full (fills both screens)",
        FitMode::Capped => "Capped (no extra width)",
    }
}

fn preview(ui: &mut egui::Ui, panels: &[Rect], window: Option<Rect>) {
    let Some(desktop) = desktop_of(panels) else {
        ui.label("No monitors detected");
        return;
    };
    let world = window.map_or(desktop, |w| desktop.union(&w));
    let width = ui.available_width();
    let height = (width * world.h as f32 / world.w as f32).clamp(40.0, 160.0);
    let (resp, painter) = ui.allocate_painter(Vec2::new(width, height + 4.0), Sense::hover());
    let scale = (width / world.w as f32).min(height / world.h as f32);
    let origin = resp.rect.min + Vec2::new((width - world.w as f32 * scale) / 2.0, 2.0);
    let map = |r: &Rect| {
        egui::Rect::from_min_size(
            origin + Vec2::new((r.x - world.x) as f32, (r.y - world.y) as f32) * scale,
            Vec2::new(r.w as f32, r.h as f32) * scale,
        )
    };
    if let Some(w) = window {
        painter.rect_filled(map(&w), 0.0, Color32::from_rgba_unmultiplied(80, 140, 255, 50));
        painter.rect_stroke(map(&w), 0.0, Stroke::new(2.0, Color32::from_rgb(80, 140, 255)), StrokeKind::Inside);
        let c = map(&w).center();
        let red = Stroke::new(1.5, Color32::from_rgb(255, 80, 80));
        painter.line_segment([Pos2::new(c.x - 7.0, c.y), Pos2::new(c.x + 7.0, c.y)], red);
        painter.line_segment([Pos2::new(c.x, c.y - 7.0), Pos2::new(c.x, c.y + 7.0)], red);
    }
    for (i, p) in panels.iter().enumerate() {
        let r = map(p);
        painter.rect_stroke(r, 0.0, Stroke::new(1.5, Color32::GRAY), StrokeKind::Inside);
        painter.text(r.left_top() + Vec2::new(6.0, 4.0), Align2::LEFT_TOP, format!("Screen {}", i + 1), FontId::proportional(12.0), Color32::GRAY);
    }
    if let Some(w) = window {
        let off = map(&Rect::new(w.x, w.y, (desktop.x - w.x).max(0), w.h));
        if off.width() > 1.0 {
            painter.text(off.center(), Align2::CENTER_CENTER, "off-screen", FontId::proportional(12.0), Color32::from_rgb(80, 140, 255));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            let mut dirty = false;
            let status = self.worker.status.lock().map(|s| s.clone()).unwrap_or_default();
            let panels = monitors::panels(self.cfg.surround_panels, self.cfg.bezel_px);
            let (frac, fit) = self.cfg.profile_for_status(&status).unwrap_or((self.cfg.center_frac, self.cfg.fit));
            let preview_mode = if status.active { status.mode } else { Mode::Left };
            let planned = status.want.or_else(|| compute_rect(&panels, preview_mode, frac, fit));

            ui.heading("Viewport Shifter");
            ui.label("Moves the game's center (crosshair) onto one screen by placing a borderless window off-center. Window-manager calls only; no injection.");
            ui.separator();

            ui.label("Layout (blue = game window, red cross = game center)");
            preview(ui, &panels, planned);
            if let (Some(w), Some(d)) = (planned, desktop_of(&panels)) {
                ui.label(format!("Window {}x{} at ({}, {}). Wasted pixels: {:.0}%", w.w, w.h, w.x, w.y, wasted_fraction(&w, &d) * 100.0));
            }
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Game window:");
                let text = self.selected.and_then(|i| self.windows.get(i)).map_or("(pick one)".to_string(), |w| format!("{} - {}", w.exe, w.title));
                egui::ComboBox::from_id_salt("windows").width(380.0).selected_text(text).show_ui(ui, |ui| {
                    for (i, w) in self.windows.iter().enumerate() {
                        ui.selectable_value(&mut self.selected, Some(i), format!("{} - {} ({}x{})", w.exe, w.title, w.width, w.height));
                    }
                });
                if ui.button("Refresh").clicked() {
                    self.refresh_windows();
                }
                if ui.button("Use").clicked() {
                    if let Some(w) = self.selected.and_then(|i| self.windows.get(i)) {
                        self.worker.send(Command::SetTarget(Some(w.clone())));
                    }
                }
            });
            ui.horizontal(|ui| {
                for m in [Mode::Left, Mode::Normal, Mode::Right] {
                    if ui.selectable_label(status.active && status.mode == m, mode_label(m)).clicked() {
                        self.worker.send(Command::Apply(m));
                    }
                }
                if ui.button("Restore original").clicked() {
                    self.worker.send(Command::Restore);
                }
            });
            ui.label(format!(
                "Target: {} | {}",
                status.target.as_deref().unwrap_or("none"),
                if status.message.is_empty() { "idle" } else { &status.message }
            ));
            let hk = &self.cfg.hotkeys;
            ui.weak(format!("Hotkeys: {} left, {} right, {} normal, {} restore (with no target they grab the foreground window)", hk.left, hk.right, hk.normal, hk.restore));
            ui.separator();

            egui::CollapsingHeader::new("Settings").default_open(true).show(ui, |ui| {
                dirty |= ui.add(egui::Slider::new(&mut self.cfg.center_frac, 0.0..=1.0).text("Center position in chosen screen")).changed();
                egui::ComboBox::from_label("Fit mode").selected_text(fit_label(self.cfg.fit)).show_ui(ui, |ui| {
                    for f in [FitMode::Full, FitMode::Capped] {
                        dirty |= ui.selectable_value(&mut self.cfg.fit, f, fit_label(f)).changed();
                    }
                });
                dirty |= ui.add(egui::Slider::new(&mut self.cfg.surround_panels, 1..=4).text("Surround panels (used when Windows sees one monitor)")).changed();
                dirty |= ui.add(egui::Slider::new(&mut self.cfg.bezel_px, 0..=400).text("Bezel gap (px)")).changed();
            });

            egui::CollapsingHeader::new("Game profiles (auto-apply when the exe appears)").default_open(true).show(ui, |ui| {
                let mut remove = None;
                for (i, p) in self.cfg.profiles.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        dirty |= ui.add(egui::TextEdit::singleline(&mut p.exe).desired_width(140.0).hint_text("game.exe")).changed();
                        egui::ComboBox::from_id_salt(("side", i)).width(70.0).selected_text(mode_label(p.default_side)).show_ui(ui, |ui| {
                            for m in [Mode::Left, Mode::Right, Mode::Normal] {
                                dirty |= ui.selectable_value(&mut p.default_side, m, mode_label(m)).changed();
                            }
                        });
                        dirty |= ui.add(egui::Slider::new(&mut p.center_frac, 0.0..=1.0).fixed_decimals(2)).changed();
                        egui::ComboBox::from_id_salt(("fit", i)).width(80.0).selected_text(match p.fit {
                            FitMode::Full => "Full",
                            FitMode::Capped => "Capped",
                        }).show_ui(ui, |ui| {
                            dirty |= ui.selectable_value(&mut p.fit, FitMode::Full, "Full").changed();
                            dirty |= ui.selectable_value(&mut p.fit, FitMode::Capped, "Capped").changed();
                        });
                        if ui.button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.cfg.profiles.remove(i);
                    dirty = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Add profile").clicked() {
                        self.cfg.profiles.push(Profile::default());
                        dirty = true;
                    }
                    if ui.button("Add from selected window").clicked() {
                        if let Some(w) = self.selected.and_then(|i| self.windows.get(i)) {
                            self.cfg.profiles.push(Profile { exe: w.exe.clone(), ..Profile::default() });
                            dirty = true;
                        }
                    }
                });
            });

            ui.separator();
            ui.weak("Limits: game must be in borderless/windowed mode (not exclusive fullscreen). HUD on the off-screen part is cut off. Full mode renders about 33% extra pixels.");
            if let Some(e) = &self.save_error {
                ui.colored_label(Color32::RED, format!("Could not save config: {e}"));
            }
            if dirty {
                self.push_config();
            }
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }

    fn on_exit(&mut self) {
        self.worker.shutdown();
        let _ = self.cfg.save();
    }
}

impl Config {
    /// Profile-specific (center, fit) for the current target, if its exe has a profile.
    fn profile_for_status(&self, status: &crate::worker::Status) -> Option<(f32, FitMode)> {
        let name = status.target.as_ref()?;
        let exe = name.split(" (").next()?;
        self.profile_for(exe).map(|p| (p.center_frac, p.fit))
    }
}
