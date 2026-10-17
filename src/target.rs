//! Finding game windows. Read-only: window enumeration plus exe name lookup.

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PWSTR};

#[derive(Clone, Debug)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub title: String,
    pub exe: String,
    pub pid: u32,
    pub width: i32,
    pub height: i32,
}

impl WindowInfo {
    pub fn handle(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }
}

pub fn exe_name_of_pid(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(str::to_owned)
    }
}

pub fn info_for(hwnd: HWND) -> Option<WindowInfo> {
    unsafe {
        let mut title = [0u16; 256];
        let n = GetWindowTextW(hwnd, &mut title);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut rc = RECT::default();
        GetWindowRect(hwnd, &mut rc).ok()?;
        Some(WindowInfo {
            hwnd: hwnd.0 as isize,
            title: String::from_utf16_lossy(&title[..n.max(0) as usize]),
            exe: exe_name_of_pid(pid).unwrap_or_default(),
            pid,
            width: rc.right - rc.left,
            height: rc.bottom - rc.top,
        })
    }
}

unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
    let out = unsafe { &mut *(data.0 as *mut Vec<WindowInfo>) };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.0.is_null()) {
            return BOOL(1);
        }
        if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }
    }
    if let Some(info) = info_for(hwnd) {
        if !info.title.is_empty() && info.width > 100 && info.height > 100 {
            out.push(info);
        }
    }
    BOOL(1)
}

/// Visible top-level windows with a title, largest first.
pub fn list_windows() -> Vec<WindowInfo> {
    let mut out: Vec<WindowInfo> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut out as *mut _ as isize));
    }
    out.sort_by_key(|w| std::cmp::Reverse(w.width as i64 * w.height as i64));
    out
}

/// Largest visible window of a process whose exe name matches (case-insensitive).
pub fn find_by_exe(exe: &str) -> Option<WindowInfo> {
    list_windows().into_iter().find(|w| w.exe.eq_ignore_ascii_case(exe))
}

pub fn find_by_title(needle: &str) -> Option<WindowInfo> {
    let needle = needle.to_lowercase();
    list_windows().into_iter().find(|w| w.title.to_lowercase().contains(&needle))
}
