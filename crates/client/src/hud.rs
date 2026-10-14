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

/// Command card: 7 columns × 3 rows of cells.
/// Command card columns: a 7 × 3 block (cells 0–20, row-major) and a plan column (21–23).
pub const CARD_COLS: usize = 8;
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

/// Cell `i` of the command card: 0–20 row-major in the first seven columns, 21–23 down the last.
pub fn card_cell(screen: P2, i: usize) -> Rect {
    let c = card_frame(screen);
    let (col, row) = if i < 21 { (i % 7, i / 7) } else { (CARD_COLS - 1, i - 21) };
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

/// One row of a context menu: a leaf with an action, or a submenu.
#[derive(Debug, Clone)]
pub struct MenuItem<A> {
    pub label: String,
    pub value: String,
    /// Display text of the hotkey; a single letter is also the menu accelerator.
    pub key: &'static str,
    pub tip: String,
    pub enabled: bool,
    pub checked: bool,
    pub action: Option<A>,
    pub sub: Vec<MenuItem<A>>,
}

impl<A> MenuItem<A> {
    pub fn new(label: impl Into<String>, action: A) -> Self {
        Self {
            label: label.into(),
            value: String::new(),
            key: "",
            tip: String::new(),
            enabled: true,
            checked: false,
            action: Some(action),
            sub: Vec::new(),
        }
    }
    pub fn submenu(label: impl Into<String>, sub: Vec<MenuItem<A>>) -> Self {
        Self {
            label: label.into(),
            value: String::new(),
            key: "",
            tip: String::new(),
            enabled: !sub.is_empty(),
            checked: false,
            action: None,
            sub,
        }
    }
    pub fn key(mut self, k: &'static str) -> Self {
        self.key = k;
        self
    }
    pub fn value(mut self, v: impl Into<String>) -> Self {
        self.value = v.into();
        self
    }
    pub fn checked(mut self, c: bool) -> Self {
        self.checked = c;
        self
    }
    /// Greyed, with the reason as its tooltip.
    pub fn disabled(mut self, why: impl Into<String>) -> Self {
        self.enabled = false;
        self.tip = why.into();
        self
    }
}

pub const MENU_ROW_H: f32 = 20.0;

/// What a frame of menu input resulted in.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuEvent<A> {
    None,
    Close,
    Pick(A),
}

/// A cascading pop-up menu. `path` = indices of the open submenu items (level 0 upwards);
/// `focus` = the highlighted row of the deepest open level.
#[derive(Debug, Clone)]
pub struct PopupMenu<A> {
    pub at: P2,
    pub title: String,
    pub items: Vec<MenuItem<A>>,
    pub path: Vec<usize>,
    pub focus: usize,
}

impl<A: Clone> PopupMenu<A> {
    pub fn new(at: P2, title: impl Into<String>, items: Vec<MenuItem<A>>) -> Self {
        let focus = items.iter().position(|i| i.enabled).unwrap_or(0);
        Self { at, title: title.into(), items, path: Vec::new(), focus }
    }

    /// Open the submenus along `path` (as far as they exist and are enabled).
    pub fn open_path(&mut self, path: &[usize]) {
        for &k in path {
            let items = self.level(self.path.len());
            if items.get(k).is_none_or(|it| !it.enabled || it.sub.is_empty()) {
                if items.get(k).is_some_and(|it| it.enabled) {
                    self.focus = k;
                }
                return;
            }
            self.path.push(k);
            self.focus = self.level(self.path.len()).iter().position(|i| i.enabled).unwrap_or(0);
        }
    }

    fn level(&self, depth: usize) -> &[MenuItem<A>] {
        let mut items = &self.items[..];
        for &i in &self.path[..depth] {
            items = &items[i].sub;
        }
        items
    }

    fn row_text(it: &MenuItem<A>) -> (String, String) {
        let label = if it.checked { format!("* {}", it.label) } else { format!("  {}", it.label) };
        let mut value = it.value.clone();
        if !it.key.is_empty() {
            if !value.is_empty() {
                value.push_str("  ");
            }
            value.push_str(&format!("[{}]", it.key));
        }
        if !it.sub.is_empty() {
            value.push_str(" >");
        }
        (label, value)
    }

    /// Frame and rows of every open level; the row action is the item's index path.
    pub fn layout(&self, screen: P2) -> Vec<(Rect, Vec<Button<Vec<usize>>>)> {
        let mut out: Vec<(Rect, Vec<Button<Vec<usize>>>)> = Vec::new();
        let bottom = band_y(screen) - 4.0;
        for depth in 0..=self.path.len() {
            let items = self.level(depth);
            let w = items
                .iter()
                .map(|it| {
                    let (l, v) = Self::row_text(it);
                    text_w(&l, style::FONT_S) + text_w(&v, style::FONT_S) + 28.0
                })
                .fold(150.0f32, f32::max);
            let w = if depth == 0 { w.max(text_w(&self.title, style::FONT_S) + 20.0) } else { w };
            let head = if depth == 0 && !self.title.is_empty() { MENU_ROW_H } else { 0.0 };
            let h = items.len() as f32 * MENU_ROW_H + head + 4.0;
            let (mut x, y0) = match out.last() {
                None => (self.at[0], self.at[1]),
                Some((fr, rows)) => {
                    let open = self.path[depth - 1];
                    (fr.x + fr.w - 2.0, rows[open].r.y - 2.0)
                }
            };
            if x + w > screen[0] - 4.0 {
                x = match out.last() {
                    Some((fr, _)) => fr.x - w + 2.0,
                    None => screen[0] - w - 4.0,
                };
            }
            let x = x.max(4.0);
            let y = y0.min(bottom - h).max(TOP_H + 4.0);
            let mut rows = Vec::new();
            for (k, it) in items.iter().enumerate() {
                let (l, v) = Self::row_text(it);
                let mut p: Vec<usize> = self.path[..depth].to_vec();
                p.push(k);
                let r = Rect::new(x + 2.0, y + 2.0 + head + k as f32 * MENU_ROW_H, w - 4.0, MENU_ROW_H);
                let mut b = Button::new(r, Look::Row, l, p).value(v).tip(it.tip.clone()).enabled(it.enabled);
                b.focused = depth == self.path.len() && k == self.focus;
                b.active = self.path.get(depth) == Some(&k);
                rows.push(b);
            }
            out.push((Rect::new(x, y, w, h), rows));
        }
        out
    }

    fn item(&self, path: &[usize]) -> &MenuItem<A> {
        let mut items = &self.items[..];
        let mut it = &items[path[0]];
        for &i in &path[1..] {
            items = &it.sub;
            it = &items[i];
        }
        it
    }

    /// Open the submenu or return the action of the item at `path`.
    fn activate(&mut self, path: &[usize]) -> MenuEvent<A> {
        let it = self.item(path);
        if !it.enabled {
            return MenuEvent::None;
        }
        if !it.sub.is_empty() {
            self.path = path.to_vec();
            self.focus = self.item(path).sub.iter().position(|i| i.enabled).unwrap_or(0);
            return MenuEvent::None;
        }
        it.action.clone().map_or(MenuEvent::None, MenuEvent::Pick)
    }

    /// One frame of input: arrows/Enter/Esc/accelerators and the mouse. A click outside every
    /// level closes the menu.
    pub fn update(&mut self, inp: &crate::render::Input) -> MenuEvent<A> {
        use crate::render::Key;
        let layout = self.layout(inp.screen);
        let n = self.level(self.path.len()).len();
        let depth = self.path.len();
        let step =
            |from: usize, up: bool, items: &[MenuItem<A>]| {
                (1..=items.len())
                    .map(|d| {
                        if up { (from + items.len() - d % items.len()) % items.len() } else { (from + d) % items.len() }
                    })
                    .find(|&k| items[k].enabled)
                    .unwrap_or(from)
            };
        if n > 0 {
            if inp.hit(Key::Down) {
                self.focus = step(self.focus, false, self.level(depth));
            }
            if inp.hit(Key::Up) {
                self.focus = step(self.focus, true, self.level(depth));
            }
        }
        if inp.hit(Key::Escape) || inp.hit(Key::Left) {
            if let Some(open) = self.path.pop() {
                self.focus = open;
            } else {
                return MenuEvent::Close;
            }
        }
        if inp.hit(Key::Enter) || inp.hit(Key::Right) {
            let mut p = self.path.clone();
            p.push(self.focus);
            if self.focus < n && (inp.hit(Key::Enter) || !self.item(&p).sub.is_empty()) {
                return self.activate(&p);
            }
        }
        for k in &inp.pressed {
            let c = match k {
                Key::Char(c) => Some(*c),
                Key::Num(d) => char::from_digit(u32::from(*d), 10),
                _ => None,
            };
            if let Some(c) = c {
                let hit = self.level(depth).iter().position(|it| {
                    it.enabled && it.key.len() == 1 && it.key.chars().next().is_some_and(|x| x.eq_ignore_ascii_case(&c))
                });
                if let Some(i) = hit {
                    let mut p = self.path.clone();
                    p.push(i);
                    return self.activate(&p);
                }
            }
        }
        let over = layout.iter().enumerate().rev().find(|(_, (fr, _))| fr.contains(inp.mouse));
        if inp.mouse_delta != [0.0, 0.0]
            && let Some((d, (_, rows))) = over
            && d == depth
            && let Some(k) = rows.iter().position(|b| b.r.contains(inp.mouse) && b.enabled)
        {
            self.focus = k;
        }
        if inp.left_pressed || inp.right_pressed {
            return match over {
                None => MenuEvent::Close,
                Some((_, (_, rows))) if inp.left_pressed => match rows.iter().find(|b| b.r.contains(inp.mouse)) {
                    Some(b) if b.enabled => {
                        let p = b.action.clone();
                        self.activate(&p)
                    }
                    _ => MenuEvent::None,
                },
                Some(_) => MenuEvent::None,
            };
        }
        MenuEvent::None
    }

    pub fn draw(&self, f: &mut Frame, screen: P2, mouse: P2) {
        let layout = self.layout(screen);
        for (d, (fr, rows)) in layout.iter().enumerate() {
            panel(f, *fr);
            if d == 0 && !self.title.is_empty() {
                f.text(&self.title, fr.x + 8.0, fr.y + 5.0, style::FONT_S, style::TEXT_DIM);
            }
            draw(f, rows, mouse);
        }
        let all: Vec<Button<Vec<usize>>> = layout.into_iter().flat_map(|(_, r)| r).collect();
        tooltip(f, &all, mouse, screen);
    }

    /// Screen rectangles of every open level (hit area).
    pub fn frames(&self, screen: P2) -> Vec<Rect> {
        self.layout(screen).into_iter().map(|(r, _)| r).collect()
    }
}

#[cfg(test)]
mod popup_tests {
    use super::*;
    use crate::render::{Input, Key};

    fn menu() -> PopupMenu<u8> {
        PopupMenu::new(
            [100.0, 100.0],
            "T",
            vec![
                MenuItem::new("A", 1).key("A"),
                MenuItem::new("B", 2).disabled("NO"),
                MenuItem::submenu("C", vec![MenuItem::new("C1", 3), MenuItem::new("C2", 4).key("X")]),
            ],
        )
    }

    fn keys(ks: &[Key]) -> Input {
        Input { screen: [1600.0, 900.0], mouse: [5.0, 5.0], pressed: ks.to_vec(), ..Input::default() }
    }

    #[test]
    fn arrows_skip_disabled_open_submenus_and_pick() {
        let mut m = menu();
        assert_eq!(m.update(&keys(&[Key::Down])), MenuEvent::None);
        assert_eq!(m.focus, 2);
        assert_eq!(m.update(&keys(&[Key::Right])), MenuEvent::None);
        assert_eq!(m.path, vec![2]);
        assert_eq!(m.update(&keys(&[Key::Down])), MenuEvent::None);
        assert_eq!(m.update(&keys(&[Key::Enter])), MenuEvent::Pick(4));
    }

    #[test]
    fn escape_backs_out_then_closes_and_letters_pick() {
        let mut m = menu();
        m.update(&keys(&[Key::Down, Key::Right]));
        assert_eq!(m.update(&keys(&[Key::Escape])), MenuEvent::None);
        assert!(m.path.is_empty());
        assert_eq!(m.update(&keys(&[Key::Escape])), MenuEvent::Close);
        assert_eq!(menu().update(&keys(&[Key::Char('a')])), MenuEvent::Pick(1));
    }

    #[test]
    fn click_outside_closes() {
        let mut m = menu();
        let mut i = keys(&[]);
        i.left_pressed = true;
        assert_eq!(m.update(&i), MenuEvent::Close);
    }
}
