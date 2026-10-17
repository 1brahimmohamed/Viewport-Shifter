//! Pure geometry: where the game window must go so its center lands on a chosen panel.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Left,
    Right,
    #[default]
    Normal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FitMode {
    /// Window is wide enough to fill the whole desktop, wasting an off-screen panel.
    #[default]
    Full,
    /// Window is never wider than the desktop; part of the far panel stays uncovered.
    Capped,
}

/// Union of all panels.
pub fn desktop_of(panels: &[Rect]) -> Option<Rect> {
    let (first, rest) = panels.split_first()?;
    Some(rest.iter().fold(*first, |acc, p| acc.union(p)))
}

/// `panels` must be ordered left to right. `center_frac` is the position of the
/// game center inside the chosen panel (0.5 = middle).
pub fn compute_rect(panels: &[Rect], mode: Mode, center_frac: f32, fit: FitMode) -> Option<Rect> {
    let desktop = desktop_of(panels)?;
    let panel = match mode {
        Mode::Normal => return Some(desktop),
        Mode::Left => panels.first()?,
        Mode::Right => panels.last()?,
    };
    let frac = center_frac.clamp(0.0, 1.0);
    let center = panel.x + (panel.w as f32 * frac).round() as i32;
    let (w, x) = match fit {
        FitMode::Full => {
            let half = (center - desktop.x).max(desktop.right() - center);
            (half * 2, center - half)
        }
        FitMode::Capped => (desktop.w, center - desktop.w / 2),
    };
    Some(Rect::new(x, desktop.y, w, desktop.h))
}

/// Fraction of the window's pixels that fall outside the desktop.
pub fn wasted_fraction(window: &Rect, desktop: &Rect) -> f32 {
    let l = window.x.max(desktop.x);
    let r = window.right().min(desktop.right());
    let visible = (r - l).max(0) as f32;
    1.0 - visible / window.w.max(1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two() -> Vec<Rect> {
        vec![Rect::new(0, 0, 1920, 1080), Rect::new(1920, 0, 1920, 1080)]
    }

    #[test]
    fn normal_is_desktop() {
        assert_eq!(
            compute_rect(&two(), Mode::Normal, 0.5, FitMode::Full),
            Some(Rect::new(0, 0, 3840, 1080))
        );
    }

    #[test]
    fn left_full() {
        let r = compute_rect(&two(), Mode::Left, 0.5, FitMode::Full).unwrap();
        assert_eq!(r, Rect::new(-1920, 0, 5760, 1080));
        assert_eq!(r.x + r.w / 2, 960);
    }

    #[test]
    fn right_full() {
        let r = compute_rect(&two(), Mode::Right, 0.5, FitMode::Full).unwrap();
        assert_eq!(r, Rect::new(0, 0, 5760, 1080));
        assert_eq!(r.x + r.w / 2, 2880);
    }

    #[test]
    fn left_capped() {
        let r = compute_rect(&two(), Mode::Left, 0.5, FitMode::Capped).unwrap();
        assert_eq!(r, Rect::new(-960, 0, 3840, 1080));
    }

    #[test]
    fn center_frac_moves_center() {
        let r = compute_rect(&two(), Mode::Left, 0.75, FitMode::Full).unwrap();
        // center at 1440; farthest edge is the right one (3840) -> half = 2400
        assert_eq!(r, Rect::new(-960, 0, 4800, 1080));
        assert_eq!(r.x + r.w / 2, 1440);
    }

    #[test]
    fn full_always_covers_desktop() {
        for frac in [0.0, 0.2, 0.5, 0.9, 1.0] {
            for mode in [Mode::Left, Mode::Right] {
                let r = compute_rect(&two(), mode, frac, FitMode::Full).unwrap();
                assert!(r.x <= 0 && r.right() >= 3840);
            }
        }
    }

    #[test]
    fn surround_split_with_bezel() {
        // one 3840 wide display split in two panels with a 40px bezel gap
        let panels = vec![Rect::new(0, 0, 1900, 1080), Rect::new(1940, 0, 1900, 1080)];
        let r = compute_rect(&panels, Mode::Left, 0.5, FitMode::Full).unwrap();
        assert_eq!(r.x + r.w / 2, 950);
    }

    #[test]
    fn waste_is_a_third() {
        let d = desktop_of(&two()).unwrap();
        let r = compute_rect(&two(), Mode::Left, 0.5, FitMode::Full).unwrap();
        assert!((wasted_fraction(&r, &d) - 1.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn empty_panels() {
        assert_eq!(compute_rect(&[], Mode::Left, 0.5, FitMode::Full), None);
    }
}
