use core::sync::atomic::{AtomicBool, Ordering};
use nc_gfx::{lerp_color, rgb};

use crate::paint::{Glass, WHITE};

static DARK: AtomicBool = AtomicBool::new(false);

pub fn set_dark(on: bool) {
    DARK.store(on, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

pub fn ink() -> u32 {
    if is_dark() { rgb(236, 240, 246) } else { rgb(24, 28, 36) }
}

pub fn ink_soft() -> u32 {
    if is_dark() { rgb(190, 198, 212) } else { rgb(60, 66, 80) }
}

pub fn ink_muted() -> u32 {
    if is_dark() { rgb(146, 154, 170) } else { rgb(110, 118, 134) }
}

pub fn snow() -> u32 {
    if is_dark() { rgb(28, 31, 38) } else { rgb(247, 248, 251) }
}

pub fn frost() -> u32 {
    if is_dark() { rgb(38, 42, 52) } else { rgb(234, 238, 245) }
}

pub fn glass_text() -> u32 {
    if is_dark() { rgb(246, 248, 252) } else { rgb(22, 26, 34) }
}

pub fn glass_shadow() -> u32 {
    if is_dark() { rgb(4, 6, 10) } else { rgb(247, 248, 251) }
}

fn dk(alpha: u32) -> u32 {
    if is_dark() { (alpha + 58).min(236) } else { alpha }
}
pub const CLOSE_RED: u32 = rgb(232, 62, 74);
pub const CLOSE_RED_TOP: u32 = rgb(255, 128, 124);
pub const WARN: u32 = rgb(250, 168, 36);
pub const DANGER: u32 = rgb(240, 70, 80);
pub const BLACK: u32 = rgb(0, 0, 0);

const TILES: [(u32, u32); 9] = [
    (rgb(255, 180, 195), rgb(220, 78, 114)),
    (rgb(255, 216, 168), rgb(226, 133, 66)),
    (rgb(250, 233, 163), rgb(213, 174, 65)),
    (rgb(166, 235, 207), rgb(50, 171, 136)),
    (rgb(157, 228, 237), rgb(44, 166, 185)),
    (rgb(164, 218, 255), rgb(55, 145, 211)),
    (rgb(151, 190, 249), rgb(62, 111, 211)),
    (rgb(198, 184, 255), rgb(123, 98, 210)),
    (rgb(194, 209, 226), rgb(95, 121, 159)),
];

pub fn hash_str(s: &str) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h ^ (h >> 15)
}

pub fn tile_colors(seed: u32) -> (u32, u32) {
    TILES[(seed as usize) % TILES.len()]
}

pub fn lighten(c: u32, amount: u32) -> u32 {
    lerp_color(c, WHITE, amount)
}

pub fn darken(c: u32, amount: u32) -> u32 {
    lerp_color(c, 0, amount)
}

pub fn dock_glass(blur: usize, passes: usize) -> Glass {
    Glass {
        tint: snow(),
        alpha_top: dk(150),
        alpha_bot: dk(128),
        blur,
        passes,
        shadow: 70,
        shadow_dy: 6,
        shadow_blur: 28,
        gloss: 44,
        edge: 160,
        hole: None,
    }
}

pub fn start_glass(blur: usize, passes: usize) -> Glass {
    Glass {
        tint: snow(),
        alpha_top: dk(176),
        alpha_bot: dk(158),
        blur,
        passes,
        shadow: 120,
        shadow_dy: 10,
        shadow_blur: 36,
        gloss: 40,
        edge: 150,
        hole: None,
    }
}

pub fn menu_glass(blur: usize, passes: usize) -> Glass {
    Glass {
        tint: snow(),
        alpha_top: dk(188),
        alpha_bot: dk(170),
        blur,
        passes,
        shadow: 110,
        shadow_dy: 8,
        shadow_blur: 26,
        gloss: 28,
        edge: 170,
        hole: None,
    }
}

pub fn modal_glass(blur: usize, passes: usize) -> Glass {
    Glass {
        tint: snow(),
        alpha_top: dk(214),
        alpha_bot: dk(196),
        blur,
        passes,
        shadow: 150,
        shadow_dy: 12,
        shadow_blur: 44,
        gloss: 32,
        edge: 180,
        hole: None,
    }
}

pub fn window_glass(focused: bool, blur: usize, passes: usize) -> Glass {
    Glass {
        tint: snow(),
        alpha_top: dk(if focused { 150 } else { 126 }),
        alpha_bot: dk(if focused { 128 } else { 108 }),
        blur,
        passes,
        shadow: if focused { 104 } else { 60 },
        shadow_dy: if focused { 10 } else { 6 },
        shadow_blur: if focused { 36 } else { 24 },
        gloss: if focused { 38 } else { 22 },
        edge: if focused { 170 } else { 110 },
        hole: None,
    }
}

pub const TEAL: u32 = rgb(28, 170, 152);
pub const AZURE: u32 = rgb(41, 121, 245);
pub const INDIGO: u32 = rgb(88, 94, 230);

pub fn radius_small(unit: i32) -> i32 {
    (unit / 4).max(2)
}

pub fn radius_medium(unit: i32) -> i32 {
    (unit / 3).max(3)
}

pub fn radius_large(unit: i32) -> i32 {
    (unit * 3 / 5).max(5)
}

pub fn light_reach(unit: i32) -> i32 {
    unit * 9
}
