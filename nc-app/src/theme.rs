use nc_gfx::{lerp_color, rgb};
use nc_ui::WHITE;

pub struct Glow {
    pub x: i32,
    pub y: i32,
    pub radius: i32,
    pub color: u32,
    pub strength: u32,
}

pub struct Wallpaper {
    pub name: &'static str,
    pub top: u32,
    pub bottom: u32,
    pub glows: [Glow; 2],
}

pub const WALLPAPER_COUNT: usize = 6;

pub const WALLPAPERS: [Wallpaper; WALLPAPER_COUNT] = [
    Wallpaper {
        name: "Kuzey Isiklari",
        top: rgb(22, 106, 208),
        bottom: rgb(16, 164, 158),
        glows: [
            Glow { x: 76, y: 14, radius: 88, color: rgb(186, 236, 255), strength: 135 },
            Glow { x: 18, y: 88, radius: 82, color: rgb(116, 242, 218), strength: 125 },
        ],
    },
    Wallpaper {
        name: "Buzul Beyazi",
        top: rgb(232, 243, 250),
        bottom: rgb(206, 229, 240),
        glows: [
            Glow { x: 24, y: 18, radius: 85, color: rgb(138, 208, 252), strength: 110 },
            Glow { x: 80, y: 82, radius: 78, color: rgb(126, 232, 214), strength: 115 },
        ],
    },
    Wallpaper {
        name: "Mercan Koyu",
        top: rgb(12, 84, 148),
        bottom: rgb(24, 178, 152),
        glows: [
            Glow { x: 64, y: 22, radius: 92, color: rgb(156, 246, 234), strength: 130 },
            Glow { x: 14, y: 78, radius: 74, color: rgb(132, 206, 255), strength: 115 },
        ],
    },
    Wallpaper {
        name: "Alacakaranlik",
        top: rgb(42, 44, 122),
        bottom: rgb(164, 62, 118),
        glows: [
            Glow { x: 14, y: 10, radius: 72, color: rgb(167, 139, 250), strength: 96 },
            Glow { x: 82, y: 92, radius: 86, color: rgb(251, 191, 126), strength: 120 },
        ],
    },
    Wallpaper {
        name: "Zumrut Orman",
        top: rgb(8, 72, 64),
        bottom: rgb(22, 134, 104),
        glows: [
            Glow { x: 22, y: 12, radius: 80, color: rgb(167, 243, 208), strength: 98 },
            Glow { x: 88, y: 90, radius: 64, color: rgb(210, 246, 168), strength: 80 },
        ],
    },
    Wallpaper {
        name: "Gece Grafiti",
        top: rgb(32, 46, 68),
        bottom: rgb(14, 22, 38),
        glows: [
            Glow { x: 72, y: 12, radius: 88, color: rgb(88, 124, 168), strength: 88 },
            Glow { x: 16, y: 90, radius: 72, color: rgb(46, 138, 142), strength: 84 },
        ],
    },
];

pub const ACCENT_COUNT: usize = 4;
pub const ACCENTS: [u32; ACCENT_COUNT] = [
    rgb(37, 99, 235),
    rgb(20, 168, 152),
    rgb(124, 58, 237),
    rgb(234, 88, 12),
];

pub const SIDEBAR: u32 = rgb(244, 247, 250);
pub const SIDEBAR_HOVER: u32 = rgb(232, 238, 245);

const BLACK: u32 = rgb(0, 0, 0);

pub fn accent_tint(accent: u32) -> u32 {
    lerp_color(WHITE, accent, 30)
}

pub fn accent_hover(accent: u32) -> u32 {
    lerp_color(accent, BLACK, 40)
}
