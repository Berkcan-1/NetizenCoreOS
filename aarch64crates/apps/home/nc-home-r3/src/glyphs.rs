use nc_mobile_ui::{draw_glyph as sdk_glyph, Glyph, WHITE};
use nc_app_sdk::Gfx;

pub fn draw(g: &mut Gfx<'_>, glyph: Glyph, x: i32, y: i32, size: i32, color: u32) {
    let p = |v: i32| size * v / 100;
    let cx = x + size / 2;
    let cy = y + size / 2;
    match glyph {
        Glyph::Files => {
            g.panel(x + p(20), y + p(26), p(28), p(18), p(6), WHITE, 150, 0, 0);
            g.panel(x + p(20), y + p(32), p(60), p(42), p(9), WHITE, 170, 0, 0);
            g.panel(x + p(20), y + p(41), p(60), p(35), p(9), WHITE, 255, 0, 0);
        }
        Glyph::Settings => {
            const TEETH: [(i32, i32); 8] = [(30, 0), (21, 21), (0, 30), (-21, 21), (-30, 0), (-21, -21), (0, -30), (21, -21)];
            for (dx, dy) in TEETH {
                g.dot(cx + p(dx), cy + p(dy), p(8), WHITE, 1024);
            }
            g.dot(cx, cy, p(25), WHITE, 1024);
            g.dot(cx, cy, p(13), color, 1024);
            g.dot(cx, cy, p(6), WHITE, 1024);
        }
        Glyph::System => {
            g.panel(x + p(24), y + p(24), p(22), p(22), p(7), WHITE, 255, 0, 0);
            g.panel(x + p(54), y + p(24), p(22), p(22), p(7), WHITE, 150, 0, 0);
            g.panel(x + p(24), y + p(54), p(22), p(22), p(7), WHITE, 150, 0, 0);
            g.panel(x + p(54), y + p(54), p(22), p(22), p(7), WHITE, 255, 0, 0);
        }
        _ => sdk_glyph(g, glyph, x, y, size, color),
    }
}
