//! Persistent settings at %APPDATA%\viewport-shifter\config.toml.

use crate::layout::{FitMode, Mode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub exe: String,
    pub default_side: Mode,
    pub center_frac: f32,
    pub fit: FitMode,
}

impl Default for Profile {
    fn default() -> Self {
        Self { exe: String::new(), default_side: Mode::Left, center_frac: 0.5, fit: FitMode::Full }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub left: String,
    pub right: String,
    pub normal: String,
    pub restore: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            left: "ctrl+alt+left".into(),
            right: "ctrl+alt+right".into(),
            normal: "ctrl+alt+down".into(),
            restore: "ctrl+alt+end".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// With NVIDIA Surround Windows sees one wide monitor; split it into this many panels.
    pub surround_panels: u32,
    pub bezel_px: i32,
    pub center_frac: f32,
    pub fit: FitMode,
    pub hotkeys: Hotkeys,
    pub profiles: Vec<Profile>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            surround_panels: 2,
            bezel_px: 0,
            center_frac: 0.5,
            fit: FitMode::Full,
            hotkeys: Hotkeys::default(),
            profiles: Vec::new(),
        }
    }
}

fn path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("viewport-shifter").join("config.toml"))
}

impl Config {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let p = path().ok_or_else(|| anyhow::anyhow!("APPDATA not set"))?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(p, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn profile_for(&self, exe: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.exe.eq_ignore_ascii_case(exe))
    }
}

/// "ctrl+alt+left" -> (modifiers, virtual key code)
pub fn parse_hotkey(s: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let mut mods = MOD_NOREPEAT;
    let mut key = None;
    for part in s.split('+').map(|p| p.trim().to_lowercase()) {
        match part.as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "win" | "super" => mods |= MOD_WIN,
            "left" => key = Some(0x25),
            "up" => key = Some(0x26),
            "right" => key = Some(0x27),
            "down" => key = Some(0x28),
            "end" => key = Some(0x23),
            "home" => key = Some(0x24),
            k if k.len() == 1 && k.as_bytes()[0].is_ascii_alphanumeric() => {
                key = Some(k.as_bytes()[0].to_ascii_uppercase() as u32)
            }
            k if k.starts_with('f') => {
                let n: u32 = k[1..].parse().ok()?;
                if !(1..=12).contains(&n) {
                    return None;
                }
                key = Some(0x70 + n - 1);
            }
            _ => return None,
        }
    }
    key.map(|k| (mods, k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys_parse() {
        let (m, k) = parse_hotkey("ctrl+alt+left").unwrap();
        assert_eq!(k, 0x25);
        assert!(m.0 & MOD_CONTROL.0 != 0 && m.0 & MOD_ALT.0 != 0);
        assert_eq!(parse_hotkey("Ctrl+F5").unwrap().1, 0x74);
        assert_eq!(parse_hotkey("shift+a").unwrap().1, b'A' as u32);
        assert!(parse_hotkey("ctrl+bogus").is_none());
        assert!(parse_hotkey("ctrl+alt").is_none());
    }

    #[test]
    fn config_roundtrip() {
        let mut c = Config::default();
        c.profiles.push(Profile { exe: "cs2.exe".into(), ..Default::default() });
        let s = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(back.profiles[0].exe, "cs2.exe");
        assert_eq!(back.surround_panels, 2);
    }
}
