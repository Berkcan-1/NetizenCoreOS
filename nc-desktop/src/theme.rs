use nc_gfx::{lerp_color, rgb};

use crate::paint::{Glass, WHITE};

pub const INK: u32 = rgb(14, 24, 42);
pub const INK_SOFT: u32 = rgb(50, 66, 92);
pub const INK_MUTED: u32 = rgb(98, 116, 142);
pub const SNOW: u32 = rgb(250, 252, 255);
pub const FROST: u32 = rgb(236, 244, 252);
pub const CLOSE_RED: u32 = rgb(232, 62, 74);
pub const CLOSE_RED_TOP: u32 = rgb(255, 128, 124);
pub const WARN: u32 = rgb(250, 168, 36);
pub const DANGER: u32 = rgb(240, 70, 80);
pub const BLACK: u32 = rgb(0, 0, 0);

const TILES: [(u32, u32); 9] = [
    (rgb(255, 132, 160), rgb(236, 44, 96)),
    (rgb(255, 196, 128), rgb(248, 108, 72)),
    (rgb(255, 226, 120), rgb(248, 168, 40)),
    (rgb(126, 242, 196), rgb(28, 186, 132)),
    (rgb(116, 236, 244), rgb(22, 168, 204)),
    (rgb(138, 206, 255), rgb(44, 120, 246)),
    (rgb(176, 156, 255), rgb(104, 84, 238)),
    (rgb(244, 148, 252), rgb(192, 72, 222)),
    (rgb(160, 180, 210), rgb(84, 104, 148)),
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
        tint: WHITE,
        alpha_top: 92,
        alpha_bot: 58,
        blur,
        passes,
        shadow: 120,
        shadow_dy: 6,
        shadow_blur: 26,
        gloss: 190,
        edge: 190,
        hole: None,
    }
}

pub fn start_glass(blur: usize, passes: usize) -> Glass {
    Glass {
        tint: WHITE,
        alpha_top: 104,
        alpha_bot: 70,
        blur,
        passes,
        shadow: 140,
        shadow_dy: 10,
        shadow_blur: 40,
        gloss: 200,
        edge: 200,
        hole: None,
    }
}

pub fn menu_glass(blur: usize, passes: usize) -> Glass {
    Glass {
        tint: SNOW,
        alpha_top: 168,
        alpha_bot: 150,
        blur,
        passes,
        shadow: 130,
        shadow_dy: 8,
        shadow_blur: 28,
        gloss: 120,
        edge: 210,
        hole: None,
    }
}

pub fn modal_glass(blur: usize, passes: usize) -> Glass {
    Glass {
        tint: SNOW,
        alpha_top: 214,
        alpha_bot: 196,
        blur,
        passes,
        shadow: 150,
        shadow_dy: 12,
        shadow_blur: 44,
        gloss: 110,
        edge: 220,
        hole: None,
    }
}

pub fn window_glass(focused: bool, blur: usize, passes: usize) -> Glass {
    Glass {
        tint: WHITE,
        alpha_top: if focused { 128 } else { 104 },
        alpha_bot: if focused { 104 } else { 88 },
        blur,
        passes,
        shadow: if focused { 150 } else { 80 },
        shadow_dy: if focused { 14 } else { 8 },
        shadow_blur: if focused { 46 } else { 28 },
        gloss: if focused { 170 } else { 90 },
        edge: if focused { 220 } else { 140 },
        hole: None,
    }
}

pub const TEAL: u32 = rgb(24, 190, 170);
pub const AZURE: u32 = rgb(44, 130, 246);
pub const INDIGO: u32 = rgb(86, 98, 232);
