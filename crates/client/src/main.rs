use raylib::prelude::*;
fn main() {
    let (mut rl, thread) = raylib::init().size(640, 480).title("tanks").build();
    let mut n = 0;
    while !rl.window_should_close() && n < 5 {
        let mut d = rl.begin_drawing(&thread);
        d.clear_background(Color::new(10, 14, 16, 255));
        d.draw_text("TANKS", 10, 10, 20, Color::new(160, 200, 160, 255));
        n += 1;
    }
    rl.take_screenshot(&thread, "out/spike.png");
}
