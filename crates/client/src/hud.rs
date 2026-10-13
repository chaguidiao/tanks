//! Clickable heads-up display: a small immediate-mode button list shared by the top bar,
//! the command card, the platoon roster and the selection panel (DESIGN.md §10.2).
//!
//! Layout is computed from the screen size each frame, once for hit-testing in `update` and
//! again for drawing, so there is no retained widget state. Every button also has a hotkey;
//! the mouse path and the keyboard path issue the same actions.

use crate::render::{Frame, P2, Rgba};
use crate::style;

/// Height of the top menu bar (px).
pub const TOP_H: f32 = 30.0;
/// Height of the bottom status line (px).
pub const STATUS_H: f32 = 24.0;
/// Height of the bottom band (minimap, radio log, command card).
pub const BAND_H: f32 = 190.0;

/// Command card: 6 columns × 3 rows of cells.
pub const CARD_COLS: usize = 6;
pub const CELL_W: f32 = 76.0;
pub const CELL_H: f32 = 52.0;
pub const CELL_GAP: f32 = 4.0;

pub fn band_y(screen: P2) -> f32 {
    screen[1] - STATUS_H - BAND_H
}

/// Minimap frame, bottom left.
pub fn minimap_frame(screen: P2) -> Rect {
    Rect::new(6.0, band_y(screen) + 4.0, BAND_H - 8.0, BAND_H - 8.0)
}

/// Command card frame, bottom right.
pub fn card_frame(screen: P2) -> Rect {
    let w = CARD_COLS as f32 * (CELL_W + CELL_GAP) - CELL_GAP + 12.0;
    Rect::new(screen[0] - w - 6.0, band_y(screen) + 4.0, w, BAND_H - 8.0)
}

/// Cell `i` (row-major) of the command card.
pub fn card_cell(screen: P2, i: usize) -> Rect {
    let c = card_frame(screen);
    let (col, row) = (i % CARD_COLS, i / CARD_COLS);
    Rect::new(
        c.x + 6.0 + col as f32 * (CELL_W + CELL_GAP),
        c.y + 22.0 + row as f32 * (CELL_H + CELL_GAP),
        CELL_W,
        CELL_H,
    )
}

/// Radio log frame, between minimap and command card.
pub fn log_frame(screen: P2) -> Rect {
    let m = minimap_frame(screen);
    let c = card_frame(screen);
    let x = m.x + m.w + 6.0;
    Rect::new(x, m.y, (c.x - 6.0 - x).max(100.0), m.h)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, p: P2) -> bool {
        p[0] >= self.x && p[0] < self.x + self.w && p[1] >= self.y && p[1] < self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Top-bar button: one centred label.
    Bar,
    /// Command-card cell: hotkey in the corner, label, current value below.
    Card,
    /// List row (roster, selection panel): left-aligned text.
    Row,
    /// Menu screens: large left-aligned label, value (option setting) on the right.
    Menu,
}

#[derive(Debug, Clone)]
pub struct Button<A> {
    pub r: Rect,
    pub label: String,
    pub value: String,
    pub key: &'static str,
    pub tip: String,
    pub action: A,
    pub enabled: bool,
    pub active: bool,
    pub look: Look,
    pub colour: Rgba,
    /// Keyboard focus (menus): drawn like the mouse hover.
    pub focused: bool,
}

impl<A> Button<A> {
    pub fn new(r: Rect, look: Look, label: impl Into<String>, action: A) -> Self {
        Self {
            r,
            label: label.into(),
            value: String::new(),
            key: "",
            tip: String::new(),
            action,
            enabled: true,
            active: false,
            look,
            colour: style::TEXT,
            focused: false,
        }
    }
    pub fn key(mut self, k: &'static str) -> Self {
        self.key = k;
        self
    }
    pub fn tip(mut self, t: impl Into<String>) -> Self {
        self.tip = t.into();
        self
    }
    pub fn value(mut self, v: impl Into<String>) -> Self {
        self.value = v.into();
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
    pub fn active(mut self, a: bool) -> Self {
        self.active = a;
        self
    }
    pub fn colour(mut self, c: Rgba) -> Self {
        self.colour = c;
        self
    }
}

/// The enabled button under the mouse, if any.
pub fn hovered<A>(buttons: &[Button<A>], mouse: P2) -> Option<&Button<A>> {
    buttons.iter().rev().find(|b| b.r.contains(mouse))
}

/// The action clicked this frame (left button pressed over an enabled button).
pub fn clicked<A: Clone>(buttons: &[Button<A>], mouse: P2, pressed: bool) -> Option<A> {
    if !pressed {
        return None;
    }
    hovered(buttons, mouse).filter(|b| b.enabled).map(|b| b.action.clone())
}

/// Approximate text width (px) of raylib's default font, for layout before a frame exists.
pub fn text_w(s: &str, size: i32) -> f32 {
    s.chars().count() as f32 * size as f32 * 0.62
}

/// Lay out top-bar buttons left to right from `x`; returns the x after the last one.
pub fn bar_row<A>(out: &mut Vec<Button<A>>, mut x: f32, items: Vec<Button<A>>) -> f32 {
    for mut b in items {
        let w = text_w(&b.label, style::FONT_S) + 16.0;
        b.r = Rect::new(x, 4.0, w, TOP_H - 8.0);
        x += w + 4.0;
        out.push(b);
    }
    x
}

/// Width a row of top-bar buttons would take.
pub fn bar_width<A>(items: &[Button<A>]) -> f32 {
    items.iter().map(|b| text_w(&b.label, style::FONT_S) + 20.0).sum()
}

pub fn panel(f: &mut Frame, r: Rect) {
    f.rect(r.x, r.y, r.w, r.h, style::PANEL);
    f.rect_lines(r.x, r.y, r.w, r.h, style::PANEL_EDGE);
}

pub fn draw<A>(f: &mut Frame, buttons: &[Button<A>], mouse: P2) {
    // With keyboard focus in the list, the focus alone is highlighted (mouse moves move it).
    let has_focus = buttons.iter().any(|b| b.focused);
    for b in buttons {
        let hover = b.enabled && if has_focus { b.focused } else { b.r.contains(mouse) };
        let (bg, edge) = match (b.active, hover) {
            (true, _) => (style::BUTTON_ACTIVE, style::SELECT),
            (false, true) => (style::BUTTON_HOVER, style::TEXT),
            (false, false) => (style::BUTTON, style::PANEL_EDGE),
        };
        let text = if !b.enabled {
            style::TEXT_OFF
        } else if b.active {
            style::SELECT
        } else {
            b.colour
        };
        let r = b.r;
        f.rect(r.x, r.y, r.w, r.h, bg);
        f.rect_lines(r.x, r.y, r.w, r.h, edge);
        match b.look {
            Look::Bar => {
                let tw = f.text_width(&b.label, style::FONT_S);
                f.text(&b.label, r.x + (r.w - tw) / 2.0, r.y + (r.h - style::FONT_S as f32) / 2.0, style::FONT_S, text);
            }
            Look::Card => {
                if !b.key.is_empty() {
                    let kc = if b.enabled { style::AMBER } else { style::TEXT_OFF };
                    f.text(b.key, r.x + 4.0, r.y + 3.0, style::FONT, kc);
                }
                let tw = f.text_width(&b.label, style::FONT);
                f.text(&b.label, r.x + (r.w - tw) / 2.0, r.y + r.h / 2.0 - 6.0, style::FONT, text);
                if !b.value.is_empty() {
                    let vw = f.text_width(&b.value, style::FONT);
                    let vc = if b.enabled { style::TEXT_DIM } else { style::TEXT_OFF };
                    f.text(&b.value, r.x + (r.w - vw) / 2.0, r.y + r.h - 15.0, style::FONT, vc);
                }
            }
            Look::Row => {
                f.text(&b.label, r.x + 6.0, r.y + (r.h - style::FONT_S as f32) / 2.0, style::FONT_S, text);
                if !b.value.is_empty() {
                    let vw = f.text_width(&b.value, style::FONT_S);
                    let vc = if b.enabled { style::TEXT_DIM } else { style::TEXT_OFF };
                    f.text(&b.value, r.x + r.w - vw - 6.0, r.y + (r.h - style::FONT_S as f32) / 2.0, style::FONT_S, vc);
                }
            }
            Look::Menu => {
                let ty = r.y + (r.h - style::FONT_M as f32) / 2.0;
                if hover {
                    f.rect(r.x, r.y, 3.0, r.h, style::AMBER);
                }
                f.text(&b.label, r.x + 16.0, ty, style::FONT_M, text);
                let mut right = r.x + r.w - 12.0;
                if !b.key.is_empty() {
                    let kw = f.text_width(b.key, style::FONT_S);
                    let kc = if b.enabled { style::AMBER } else { style::TEXT_OFF };
                    f.text(b.key, right - kw, r.y + (r.h - style::FONT_S as f32) / 2.0, style::FONT_S, kc);
                    right -= kw + 16.0;
                }
                if !b.value.is_empty() {
                    let vw = f.text_width(&b.value, style::FONT_M);
                    let vc = if b.enabled { style::SELECT } else { style::TEXT_OFF };
                    f.text(&b.value, right - vw, ty, style::FONT_M, vc);
                }
            }
        }
    }
}

/// Tooltip for the hovered button: description and hotkey.
pub fn tooltip<A>(f: &mut Frame, buttons: &[Button<A>], mouse: P2, screen: P2) {
    let Some(b) = hovered(buttons, mouse) else { return };
    if b.tip.is_empty() {
        return;
    }
    let text = if b.key.is_empty() { b.tip.clone() } else { format!("{}   [{}]", b.tip, b.key) };
    let w = f.text_width(&text, style::FONT_S) + 16.0;
    let h = 22.0;
    let x = (mouse[0] + 14.0).min(screen[0] - w - 4.0).max(4.0);
    let mut y = mouse[1] + 18.0;
    if y + h > screen[1] - STATUS_H {
        y = mouse[1] - h - 8.0;
    }
    f.rect(x, y, w, h, style::TOOLTIP);
    f.rect_lines(x, y, w, h, style::TEXT_DIM);
    f.text(&text, x + 8.0, y + 5.0, style::FONT_S, style::TEXT);
}
