//! Moves/resizes a window and can put it back. Window-manager calls only.

use crate::layout::Rect;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetWindowLongPtrW, GetWindowRect, IsWindow, IsZoomed, SW_RESTORE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_POPUP, WS_VISIBLE,
};

#[derive(Clone, Copy, Debug)]
pub struct Saved {
    style: isize,
    rect: Rect,
}

pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut rc = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rc).ok()? };
    Some(Rect::new(rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top))
}

pub fn is_alive(hwnd: HWND) -> bool {
    unsafe { IsWindow(Some(hwnd)).as_bool() }
}

/// Remember how the window looked so `restore` can undo `apply`.
pub fn save(hwnd: HWND) -> Option<Saved> {
    let rect = window_rect(hwnd)?;
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
    Some(Saved { style, rect })
}

/// Make the window borderless and place it at `r`. Returns the rect Windows actually granted.
pub fn apply(hwnd: HWND, r: Rect) -> anyhow::Result<Rect> {
    unsafe {
        if IsZoomed(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let style = WS_POPUP.0 | WS_VISIBLE.0;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        SetWindowPos(
            hwnd,
            None,
            r.x,
            r.y,
            r.w,
            r.h,
            SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
        )?;
    }
    window_rect(hwnd).ok_or_else(|| anyhow::anyhow!("window vanished"))
}

pub fn restore(hwnd: HWND, saved: Saved) -> anyhow::Result<()> {
    if !is_alive(hwnd) {
        return Ok(());
    }
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, saved.style);
        let r = saved.rect;
        SetWindowPos(
            hwnd,
            None,
            r.x,
            r.y,
            r.w,
            r.h,
            SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
        )?;
    }
    Ok(())
}
