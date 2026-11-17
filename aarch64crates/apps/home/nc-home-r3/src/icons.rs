use crate::theme::WHITE;
use nc_app_sdk::Gfx;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Files,
    Browser,
    Stopwatch,
    System,
    Settings,
}

const SHADOW: u32 = 0x0018_2431;

pub fn tile(gfx: &mut Gfx<'_>, glyph: Glyph, x: i32, y: i32, size: i32, color: u32, pressed: bool) {
    let radius = size * 7 / 25;
    gfx.panel(x, y + size / 16, size, size, radius, SHADOW, 26, 0, 0);
    gfx.panel(x, y, size, size, radius, color, 255, WHITE, 70);
    draw_glyph(gfx, glyph, x, y, size, color);
    if pressed {
        gfx.panel(x, y, size, size, radius, SHADOW, 46, 0, 0);
    }
}

fn draw_glyph(gfx: &mut Gfx<'_>, glyph: Glyph, x: i32, y: i32, size: i32, color: u32) {
    let p = |value: i32| size * value / 100;
    let cx = x + size / 2;
    let cy = y + size / 2;
    match glyph {
        Glyph::Files => {
            gfx.panel(x + p(20), y + p(28), p(28), p(16), p(5), WHITE, 255, 0, 0);
            gfx.panel(x + p(20), y + p(37), p(60), p(41), p(8), WHITE, 255, 0, 0);
        }
        Glyph::Browser => {
            gfx.dot(cx, cy, p(31), WHITE, 1024);
            gfx.dot(cx, cy, p(26), color, 1024);
            gfx.fill_rect(cx - p(2), cy - p(26), p(4), p(52), WHITE);
            gfx.fill_rect(cx - p(26), cy - p(2), p(52), p(4), WHITE);
            gfx.panel(cx - p(12), cy - p(26), p(24), p(52), p(12), 0, 0, WHITE, 255);
        }
        Glyph::Stopwatch => {
            let cy = cy + p(3);
            gfx.fill_rect(cx - p(5), y + p(16), p(10), p(6), WHITE);
            gfx.dot(cx, cy, p(29), WHITE, 1024);
            gfx.dot(cx, cy, p(24), color, 1024);
            gfx.fill_rect(cx - p(2), cy - p(19), p(4), p(19), WHITE);
            gfx.fill_rect(cx - p(2), cy - p(2), p(13), p(4), WHITE);
            gfx.dot(cx, cy, p(4), WHITE, 1024);
        }
        Glyph::System => {
            gfx.panel(x + p(30), y + p(30), p(40), p(40), p(8), WHITE, 255, 0, 0);
            gfx.dot(cx, cy, p(9), color, 1024);
            for step in 0..3 {
                let offset = p(36 + step * 12);
                gfx.fill_rect(x + offset, y + p(21), p(4), p(9), WHITE);
                gfx.fill_rect(x + offset, y + p(70), p(4), p(9), WHITE);
                gfx.fill_rect(x + p(21), y + offset, p(9), p(4), WHITE);
                gfx.fill_rect(x + p(70), y + offset, p(9), p(4), WHITE);
            }
        }
        Glyph::Settings => {
            const TEETH: [(i32, i32); 8] = [(29, 0), (21, 21), (0, 29), (-21, 21), (-29, 0), (-21, -21), (0, -29), (21, -21)];
            for (dx, dy) in TEETH {
                gfx.dot(cx + p(dx), cy + p(dy), p(8), WHITE, 1024);
            }
            gfx.dot(cx, cy, p(25), WHITE, 1024);
            gfx.dot(cx, cy, p(14), color, 1024);
        }
    }
}
