//! Physical monitors -> logical panels. With NVIDIA Surround Windows sees one wide
//! monitor, so it is split into `surround_panels` equal panels (minus `bezel_px`).

use crate::layout::Rect;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows::core::BOOL;

unsafe extern "system" fn collect(_: HMONITOR, _: HDC, rc: *mut RECT, data: LPARAM) -> BOOL {
    let out = unsafe { &mut *(data.0 as *mut Vec<Rect>) };
    let r = unsafe { *rc };
    out.push(Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top));
    BOOL(1)
}

/// Raw monitors as Windows reports them, ordered left to right.
pub fn physical() -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut out as *mut _ as isize));
    }
    out.sort_by_key(|r| (r.x, r.y));
    out
}

/// Split a single wide monitor into `n` panels, leaving `bezel_px` gaps between them.
pub fn split(monitor: Rect, n: u32, bezel_px: i32) -> Vec<Rect> {
    let n = n.max(1) as i32;
    let panel_w = (monitor.w - bezel_px * (n - 1)) / n;
    (0..n)
        .map(|i| Rect::new(monitor.x + i * (panel_w + bezel_px), monitor.y, panel_w, monitor.h))
        .collect()
}

pub fn panels(surround_panels: u32, bezel_px: i32) -> Vec<Rect> {
    let phys = physical();
    if phys.len() == 1 && surround_panels > 1 {
        split(phys[0], surround_panels, bezel_px)
    } else {
        phys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_surround() {
        let p = split(Rect::new(0, 0, 3840, 1080), 2, 0);
        assert_eq!(p, vec![Rect::new(0, 0, 1920, 1080), Rect::new(1920, 0, 1920, 1080)]);
    }

    #[test]
    fn split_with_bezel() {
        let p = split(Rect::new(0, 0, 3840, 1080), 2, 40);
        assert_eq!(p[0].w, 1900);
        assert_eq!(p[1].x, 1940);
    }
}
