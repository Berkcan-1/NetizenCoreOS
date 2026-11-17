#![allow(dead_code)]
pub const BG: u32 = 0x00F1_F3F5;
pub const CARD: u32 = 0x00FF_FFFF;
pub const WHITE: u32 = 0x00FF_FFFF;
pub const TEXT: u32 = 0x0018_2431;
pub const TEXT_2: u32 = 0x0066_727F;
pub const TEXT_3: u32 = 0x0099_A2AC;
pub const DIVIDER: u32 = 0x00E5_E8EB;
pub const BLUE: u32 = 0x000A_59F7;
pub const BLUE_SOFT: u32 = 0x00E4_ECFD;
pub const GREEN: u32 = 0x0007_C160;
pub const SKY: u32 = 0x0012_B7F5;
pub const ORANGE: u32 = 0x00FA_7D00;
pub const PURPLE: u32 = 0x007A_5AF8;
pub const LAVENDER: u32 = 0x00EC_E8FB;

// Duvar kağıdı (koyu teal -> derin mavi) ve ışık lekeleri
pub const WALL_TOP: u32 = 0x0010_8F9B;
pub const WALL_MID: u32 = 0x0018_6FB8;
pub const WALL_BOTTOM: u32 = 0x0037_2E9E;
pub const GLOW_WARM: u32 = 0x00E8_B04A;
pub const GLOW_GREEN: u32 = 0x0030_D6A0;
pub const GLOW_VIOLET: u32 = 0x0080_5AD8;
pub const LABEL: u32 = 0x00F8_FAFC;

pub const BASE_DP: i32 = 360;

#[derive(Clone, Copy)]
pub struct Dp {
    pub width: i32,
    pub origin_x: i32,
}

impl Dp {
    pub fn new(area_w: i32, area_h: i32) -> Dp {
        let width = area_w.min(area_h * 9 / 16).max(120);
        Dp { width, origin_x: (area_w - width) / 2 }
    }

    pub fn d(&self, value: i32) -> i32 {
        (value as i64 * self.width as i64 / BASE_DP as i64) as i32
    }

    pub fn x(&self, value: i32) -> i32 {
        self.origin_x + self.d(value)
    }
}
