//! Player options (OPTIONS screen), kept in a small `key = value` text file in the user's
//! config directory. Unknown keys and bad values are ignored, so old and new files both load.

use std::path::PathBuf;

/// Frame-rate caps offered by the options screen (0 = no cap beyond vsync).
pub const FPS_CAPS: [u32; 4] = [30, 60, 120, 0];
/// Keyboard / edge pan speeds (screen px per second).
pub const PAN_SPEEDS: [(f32, &str); 3] = [(300.0, "SLOW"), (600.0, "NORMAL"), (1200.0, "FAST")];

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub fullscreen: bool,
    pub fps_cap: u32,
    /// IBM Plex Mono (true) or raylib's built-in font.
    pub ttf_font: bool,
    pub pan_speed: f32,
    /// Pan the map when the mouse touches a screen edge.
    pub edge_pan: bool,
    /// Map layers shown when a map opens.
    pub contours: bool,
    pub grid: bool,
    /// A plain right click opens the context menu (MOVE HERE first) instead of moving.
    pub right_click_menu: bool,
    /// The clock stops while the context menu is open.
    pub pause_on_menu: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            fps_cap: 60,
            ttf_font: true,
            pan_speed: 600.0,
            edge_pan: false,
            contours: true,
            grid: true,
            right_click_menu: false,
            pause_on_menu: true,
        }
    }
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

impl Settings {
    /// `$TANKS_CONFIG`, else `<config dir>/tanks/settings.cfg` (XDG, `~/.config`, `%APPDATA%`).
    pub fn path() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("TANKS_CONFIG") {
            return Some(PathBuf::from(p));
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))?;
        Some(base.join("tanks").join("settings.cfg"))
    }

    pub fn load() -> Self {
        Self::path().and_then(|p| std::fs::read_to_string(p).ok()).map_or_else(Self::default, |t| Self::parse(&t))
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path().ok_or("no config directory")?;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(&p, self.to_text()).map_err(|e| format!("{}: {e}", p.display()))
    }

    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let flag = match v {
                "on" | "true" => Some(true),
                "off" | "false" => Some(false),
                _ => None,
            };
            match (k, flag) {
                ("fullscreen", Some(b)) => s.fullscreen = b,
                ("ttf_font", Some(b)) => s.ttf_font = b,
                ("edge_pan", Some(b)) => s.edge_pan = b,
                ("contours", Some(b)) => s.contours = b,
                ("grid", Some(b)) => s.grid = b,
                ("pause_on_menu", Some(b)) => s.pause_on_menu = b,
                ("right_click", _) => s.right_click_menu = v == "menu",
                ("fps_cap", _) => {
                    if let Ok(n) = v.parse::<u32>()
                        && FPS_CAPS.contains(&n)
                    {
                        s.fps_cap = n;
                    }
                }
                ("pan_speed", _) => {
                    if let Ok(n) = v.parse::<f32>()
                        && n.is_finite()
                    {
                        s.pan_speed = n.clamp(100.0, 3000.0);
                    }
                }
                _ => {}
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        format!(
            "# Tanks options (written by the OPTIONS screen)\n\
             fullscreen = {}\nfps_cap = {}\nttf_font = {}\npan_speed = {}\nedge_pan = {}\ncontours = {}\ngrid = {}\nright_click = {}\npause_on_menu = {}\n",
            on_off(self.fullscreen),
            self.fps_cap,
            on_off(self.ttf_font),
            self.pan_speed,
            on_off(self.edge_pan),
            on_off(self.contours),
            on_off(self.grid),
            if self.right_click_menu { "menu" } else { "move" },
            on_off(self.pause_on_menu),
        )
    }

    pub fn fps_label(&self) -> String {
        if self.fps_cap == 0 { "VSYNC".into() } else { format!("{} FPS", self.fps_cap) }
    }

    pub fn next_fps(&mut self) {
        let k = FPS_CAPS.iter().position(|&c| c == self.fps_cap).unwrap_or(1);
        self.fps_cap = FPS_CAPS[(k + 1) % FPS_CAPS.len()];
    }

    pub fn pan_label(&self) -> &'static str {
        PAN_SPEEDS.iter().find(|(v, _)| *v == self.pan_speed).map_or("CUSTOM", |(_, l)| l)
    }

    pub fn next_pan(&mut self) {
        let k = PAN_SPEEDS.iter().position(|(v, _)| *v == self.pan_speed).map_or(0, |k| (k + 1) % PAN_SPEEDS.len());
        self.pan_speed = PAN_SPEEDS[k].0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Settings { fullscreen: true, fps_cap: 0, edge_pan: true, grid: false, ..Settings::default() };
        assert_eq!(Settings::parse(&s.to_text()), s);
    }

    #[test]
    fn junk_is_ignored() {
        let s = Settings::parse("fullscreen = maybe\nfps_cap = 7\nnoise\npan_speed = 1200\nfuture_key = 3");
        assert_eq!(s, Settings { pan_speed: 1200.0, ..Settings::default() });
    }

    #[test]
    fn cycles_wrap() {
        let mut s = Settings::default();
        for _ in 0..FPS_CAPS.len() {
            s.next_fps();
        }
        assert_eq!(s.fps_cap, 60);
        for _ in 0..PAN_SPEEDS.len() {
            s.next_pan();
        }
        assert_eq!(s.pan_speed, 600.0);
    }
}
