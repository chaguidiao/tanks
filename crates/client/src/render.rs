//! The only module that touches raylib (CLAUDE.md rule 1). Everything else in the client uses
//! these plain types: screen-space drawing calls, an input snapshot and texture handles.

use raylib::ffi;
use raylib::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl From<Rgba> for Color {
    fn from(c: Rgba) -> Self {
        Color::new(c.0, c.1, c.2, c.3)
    }
}

pub type P2 = [f32; 2];

fn v(p: P2) -> Vector2 {
    Vector2::new(p[0], p[1])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Space,
    Tab,
    Enter,
    Escape,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    F(u8),
    Num(u8),
    Char(char),
    LeftBracket,
    RightBracket,
    Minus,
    Equal,
    Delete,
}

#[derive(Debug, Clone, Default)]
pub struct Input {
    pub mouse: P2,
    pub wheel: f32,
    pub left_pressed: bool,
    pub left_down: bool,
    pub left_released: bool,
    pub right_pressed: bool,
    pub middle_down: bool,
    pub mouse_delta: P2,
    pub ctrl: bool,
    /// Keys pressed this frame.
    pub pressed: Vec<Key>,
    /// Keys held down (navigation keys only).
    pub held: Vec<Key>,
    pub screen: P2,
    pub frame_time: f32,
}

impl Input {
    pub fn hit(&self, k: Key) -> bool {
        self.pressed.contains(&k)
    }
    pub fn down(&self, k: Key) -> bool {
        self.held.contains(&k)
    }
}

pub struct Tex {
    t: Texture2D,
    pub w: usize,
    pub h: usize,
}

pub struct Gfx {
    rl: RaylibHandle,
    thread: RaylibThread,
}

const KEYMAP: &[(KeyboardKey, Key)] = &[
    (KeyboardKey::KEY_SPACE, Key::Space),
    (KeyboardKey::KEY_TAB, Key::Tab),
    (KeyboardKey::KEY_ENTER, Key::Enter),
    (KeyboardKey::KEY_ESCAPE, Key::Escape),
    (KeyboardKey::KEY_BACKSPACE, Key::Backspace),
    (KeyboardKey::KEY_UP, Key::Up),
    (KeyboardKey::KEY_DOWN, Key::Down),
    (KeyboardKey::KEY_LEFT, Key::Left),
    (KeyboardKey::KEY_RIGHT, Key::Right),
    (KeyboardKey::KEY_LEFT_BRACKET, Key::LeftBracket),
    (KeyboardKey::KEY_RIGHT_BRACKET, Key::RightBracket),
    (KeyboardKey::KEY_MINUS, Key::Minus),
    (KeyboardKey::KEY_EQUAL, Key::Equal),
    (KeyboardKey::KEY_F1, Key::F(1)),
    (KeyboardKey::KEY_F2, Key::F(2)),
    (KeyboardKey::KEY_F3, Key::F(3)),
    (KeyboardKey::KEY_F4, Key::F(4)),
    (KeyboardKey::KEY_F5, Key::F(5)),
    (KeyboardKey::KEY_F6, Key::F(6)),
    (KeyboardKey::KEY_F7, Key::F(7)),
    (KeyboardKey::KEY_F8, Key::F(8)),
    (KeyboardKey::KEY_F9, Key::F(9)),
    (KeyboardKey::KEY_ONE, Key::Num(1)),
    (KeyboardKey::KEY_TWO, Key::Num(2)),
    (KeyboardKey::KEY_THREE, Key::Num(3)),
    (KeyboardKey::KEY_FOUR, Key::Num(4)),
    (KeyboardKey::KEY_FIVE, Key::Num(5)),
    (KeyboardKey::KEY_SIX, Key::Num(6)),
    (KeyboardKey::KEY_SEVEN, Key::Num(7)),
    (KeyboardKey::KEY_EIGHT, Key::Num(8)),
    (KeyboardKey::KEY_NINE, Key::Num(9)),
    (KeyboardKey::KEY_A, Key::Char('a')),
    (KeyboardKey::KEY_B, Key::Char('b')),
    (KeyboardKey::KEY_D, Key::Char('d')),
    (KeyboardKey::KEY_E, Key::Char('e')),
    (KeyboardKey::KEY_F, Key::Char('f')),
    (KeyboardKey::KEY_G, Key::Char('g')),
    (KeyboardKey::KEY_L, Key::Char('l')),
    (KeyboardKey::KEY_M, Key::Char('m')),
    (KeyboardKey::KEY_N, Key::Char('n')),
    (KeyboardKey::KEY_P, Key::Char('p')),
    (KeyboardKey::KEY_R, Key::Char('r')),
    (KeyboardKey::KEY_T, Key::Char('t')),
    (KeyboardKey::KEY_V, Key::Char('v')),
    (KeyboardKey::KEY_X, Key::Char('x')),
    (KeyboardKey::KEY_DELETE, Key::Delete),
    (KeyboardKey::KEY_H, Key::Char('h')),
    (KeyboardKey::KEY_S, Key::Char('s')),
    (KeyboardKey::KEY_W, Key::Char('w')),
    (KeyboardKey::KEY_Y, Key::Char('y')),
    (KeyboardKey::KEY_Z, Key::Char('z')),
];

const HELD: &[(KeyboardKey, Key)] = &[
    (KeyboardKey::KEY_UP, Key::Up),
    (KeyboardKey::KEY_DOWN, Key::Down),
    (KeyboardKey::KEY_LEFT, Key::Left),
    (KeyboardKey::KEY_RIGHT, Key::Right),
];

impl Gfx {
    pub fn open(w: i32, h: i32, title: &str, hidden: bool) -> Self {
        raylib::core::logging::set_trace_log(TraceLogLevel::LOG_WARNING);
        let mut b = raylib::init();
        b.size(w, h).title(title).resizable().msaa_4x().vsync();
        let (mut rl, thread) = b.build();
        if hidden {
            // Screenshot runs: keep going even without focus.
            rl.set_target_fps(0);
        } else {
            rl.set_target_fps(60);
        }
        rl.set_exit_key(None);
        Self { rl, thread }
    }

    pub fn should_close(&self) -> bool {
        self.rl.window_should_close()
    }

    pub fn input(&mut self) -> Input {
        let rl = &mut self.rl;
        let m = rl.get_mouse_position();
        let d = rl.get_mouse_delta();
        let mut inp = Input {
            mouse: [m.x, m.y],
            wheel: rl.get_mouse_wheel_move(),
            left_pressed: rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT),
            left_down: rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT),
            left_released: rl.is_mouse_button_released(MouseButton::MOUSE_BUTTON_LEFT),
            right_pressed: rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_RIGHT),
            middle_down: rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_MIDDLE),
            mouse_delta: [d.x, d.y],
            ctrl: rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL) || rl.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL),
            screen: [rl.get_screen_width() as f32, rl.get_screen_height() as f32],
            frame_time: rl.get_frame_time(),
            ..Default::default()
        };
        for &(rk, k) in KEYMAP {
            if rl.is_key_pressed(rk) {
                inp.pressed.push(k);
            }
        }
        for &(rk, k) in HELD {
            if rl.is_key_down(rk) {
                inp.held.push(k);
            }
        }
        inp
    }

    /// Upload an RGBA8 image (row 0 = top) as a bilinear-filtered texture.
    pub fn texture(&mut self, w: usize, h: usize, rgba: &[u8]) -> Tex {
        let img = Image::gen_image_color(w as i32, h as i32, Color::BLANK);
        let t = self.rl.load_texture_from_image(&self.thread, &img).expect("texture upload");
        let mut tex = Tex { t, w, h };
        self.update(&mut tex, rgba);
        // SAFETY: valid texture handle owned by `tex`.
        unsafe { ffi::SetTextureFilter(*tex.t, ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32) };
        tex
    }

    pub fn update(&mut self, tex: &mut Tex, rgba: &[u8]) {
        assert_eq!(rgba.len(), tex.w * tex.h * 4);
        // SAFETY: buffer size matches the texture's RGBA8 format.
        unsafe { ffi::UpdateTexture(*tex.t, rgba.as_ptr() as *const _) };
    }

    /// Replace a sub-rectangle (x, y, w, h in texels, row 0 = top).
    pub fn update_rect(&mut self, tex: &mut Tex, x: usize, y: usize, w: usize, h: usize, rgba: &[u8]) {
        assert_eq!(rgba.len(), w * h * 4);
        let rec = ffi::Rectangle { x: x as f32, y: y as f32, width: w as f32, height: h as f32 };
        // SAFETY: buffer size matches the rectangle in RGBA8.
        unsafe { ffi::UpdateTextureRec(*tex.t, rec, rgba.as_ptr() as *const _) };
    }

    pub fn frame(&mut self, draw: impl FnOnce(&mut Frame)) {
        let d = self.rl.begin_drawing(&self.thread);
        let mut f = Frame { d };
        draw(&mut f);
    }

    pub fn screenshot(&mut self, path: &str) {
        self.rl.take_screenshot(&self.thread, path);
    }
}

pub struct Frame<'a> {
    d: RaylibDrawHandle<'a>,
}

impl Frame<'_> {
    pub fn clear(&mut self, c: Rgba) {
        self.d.clear_background(Color::from(c));
    }

    /// Draw the whole texture into a screen rectangle.
    pub fn texture(&mut self, t: &Tex, x: f32, y: f32, w: f32, h: f32, tint: Rgba) {
        let src = Rectangle::new(0.0, 0.0, t.w as f32, t.h as f32);
        let dst = Rectangle::new(x, y, w, h);
        self.d.draw_texture_pro(&t.t, src, dst, Vector2::zero(), 0.0, Color::from(tint));
    }

    pub fn line(&mut self, a: P2, b: P2, width: f32, c: Rgba) {
        if width <= 1.0 {
            self.d.draw_line_v(v(a), v(b), Color::from(c));
        } else {
            self.d.draw_line_ex(v(a), v(b), width, Color::from(c));
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
        self.d.draw_rectangle_v(Vector2::new(x, y), Vector2::new(w, h), Color::from(c));
    }

    pub fn rect_lines(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
        let (a, b, cc, dd) = ([x, y], [x + w, y], [x + w, y + h], [x, y + h]);
        for (p, q) in [(a, b), (b, cc), (cc, dd), (dd, a)] {
            self.line(p, q, 1.0, c);
        }
    }

    pub fn circle(&mut self, p: P2, r: f32, c: Rgba) {
        self.d.draw_circle_v(v(p), r, Color::from(c));
    }

    pub fn circle_lines(&mut self, p: P2, r: f32, c: Rgba) {
        self.d.draw_circle_lines_v(v(p), r, Color::from(c));
    }

    pub fn ellipse_lines(&mut self, p: P2, rx: f32, ry: f32, c: Rgba) {
        self.d.draw_ellipse_lines(p[0] as i32, p[1] as i32, rx, ry, Color::from(c));
    }

    pub fn text(&mut self, s: &str, x: f32, y: f32, size: i32, c: Rgba) {
        self.d.draw_text(s, x as i32, y as i32, size, Color::from(c));
    }

    pub fn text_width(&self, s: &str, size: i32) -> f32 {
        self.d.measure_text(s, size) as f32
    }
}
