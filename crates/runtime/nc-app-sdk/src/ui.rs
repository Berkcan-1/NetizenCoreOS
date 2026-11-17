//! Paylaşılan Ring 3 mobil arayüz altyapısı (`nc-app-sdk::ui`).
//! Tüm uygulamalar (`nc.home`, `nc.files`, `nc.settings`) bu modüldeki ortak tasarım dili,
//! vektör simgeler, kart/hap bileşenleri ve animasyon eğrilerini kullanır.

use crate::Gfx;

pub const SURFACE_BG: u32 = 0x00F2_F4F8;
pub const CARD: u32 = 0x00FF_FFFF;
pub const WHITE: u32 = 0x00FF_FFFF;
pub const SHADOW: u32 = 0x000F_172A;
pub const INK: u32 = 0x0018_2431;
pub const MUTED: u32 = 0x0064_748B;
pub const SUBTLE: u32 = 0x0094_A3B8;
pub const TRACK: u32 = 0x00E2_E8F0;
pub const DIVIDER: u32 = 0x00EE_F2F6;
pub const ROW_SEL: u32 = 0x00F4_F7FB;

pub const BLUE: u32 = 0x000A_59F7;
pub const GREEN: u32 = 0x0007_C160;
pub const GREEN_SOFT: u32 = 0x00EC_FDF5;
pub const AMBER: u32 = 0x00F5_9E0B;
pub const ORANGE: u32 = 0x00FA_7D00;
pub const PURPLE: u32 = 0x007A_5AF8;
pub const TEAL: u32 = 0x000E_A5E9;
pub const CORAL: u32 = 0x00E1_1D48;
pub const CORAL_SOFT: u32 = 0x00FF_F1F2;

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
        let width = area_w.min(area_h * 9 / 15).max(180).min(area_w);
        Dp { width, origin_x: (area_w - width) / 2 }
    }

    pub fn d(&self, value: i32) -> i32 {
        (value as i64 * self.width as i64 / BASE_DP as i64) as i32
    }

    pub fn x(&self, value: i32) -> i32 {
        self.origin_x + self.d(value)
    }
}

/// Kübik yavaşlama eğrisi (`t`: 0..=256 -> dönüş: 0..=256).
pub fn ease_out_cubic(t: i32) -> i32 {
    let t = t.clamp(0, 256);
    let inv = 256 - t;
    256 - (inv * inv * inv) / (256 * 256)
}

/// Değer hedefe doğru `step` kadar ilerler; `(yeni_değer, değer_değişti_mi)` döner.
pub fn step_towards(cur: i32, target: i32, step: i32) -> (i32, bool) {
    if cur < target {
        let next = (cur + step).min(target);
        (next, next != cur)
    } else if cur > target {
        let next = (cur - step).max(target);
        (next, next != cur)
    } else {
        (cur, false)
    }
}

pub fn mix_color(a: u32, b: u32, t: i32, max: i32) -> u32 {
    let max = max.max(1);
    let ch = |shift: u32| {
        let x = ((a >> shift) & 0xFF) as i32;
        let y = ((b >> shift) & 0xFF) as i32;
        (x + (y - x) * t / max).clamp(0, 255) as u32
    };
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

pub fn draw_card(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, radius: i32) {
    let sh = (radius + 4).min(h);
    if sh > 4 && w > 8 {
        gfx.panel(x + 2, y + h - sh + 2, w - 4, sh, radius.min(sh / 2), SHADOW, 16, 0, 0);
    }
    gfx.panel(x, y, w, h, radius, CARD, 255, DIVIDER, 255);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_pill_btn(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, fill: u32, fg: u32, cap: i32, label: &str) {
    let r = h / 2;
    gfx.panel(x, y, w, h, r, fill, 255, DIVIDER, if fill == CARD { 255 } else { 0 });
    gfx.text_centered(x + w / 2, y + (h - cap) / 2, cap, fg, 2, label);
}

pub fn draw_switch(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, on: bool, accent: u32) {
    let r = h / 2;
    let bg = if on { accent } else { TRACK };
    gfx.panel(x, y, w, h, r, bg, 255, 0, 0);
    let pad = (h / 7).max(2);
    let knob = (h - pad * 2).max(6);
    let kx = if on { x + w - pad - knob / 2 } else { x + pad + knob / 2 };
    gfx.dot(kx, y + h / 2, knob / 2, WHITE, 1024);
}

pub fn draw_slider(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, pct: u8, accent: u32) {
    let r = h / 2;
    gfx.panel(x, y, w, h, r, TRACK, 255, 0, 0);
    let p = (pct.clamp(10, 100) as i32 - 10) * 100 / 90;
    let fill_w = (w * p / 100).max(h);
    gfx.panel(x, y, fill_w, h, r, accent, 255, 0, 0);
    gfx.dot(x + fill_w - r, y + r, (r - 2).max(3), WHITE, 1024);
}

pub fn draw_chevron_left(gfx: &mut Gfx<'_>, cx: i32, cy: i32, dp: &Dp, color: u32) {
    let arm = dp.d(4).max(3);
    for s in 0..=arm {
        gfx.fill_rect(cx - arm / 2 + s, cy - s, 2, 2, color);
        gfx.fill_rect(cx - arm / 2 + s, cy + s, 2, 2, color);
    }
}

pub fn draw_chevron_right(gfx: &mut Gfx<'_>, cx: i32, cy: i32, dp: &Dp, color: u32) {
    let arm = dp.d(4).max(3);
    for s in 0..=arm {
        gfx.fill_rect(cx - arm / 2 + s, cy - arm + s, 2, 2, color);
        gfx.fill_rect(cx - arm / 2 + s, cy + arm - s, 2, 2, color);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Files,
    Settings,
    Browser,
    Stopwatch,
    System,
}

pub fn draw_app_tile(gfx: &mut Gfx<'_>, glyph: Glyph, x: i32, y: i32, size: i32, color: u32, pressed: bool) {
    let radius = size * 28 / 100;
    gfx.panel(x, y + size / 16, size, size, radius, SHADOW, 24, 0, 0);
    gfx.panel(x, y, size, size, radius, color, 255, WHITE, 70);
    draw_glyph_inner(gfx, glyph, x, y, size, color);
    if pressed {
        gfx.panel(x, y, size, size, radius, SHADOW, 44, 0, 0);
    }
}

fn draw_glyph_inner(gfx: &mut Gfx<'_>, glyph: Glyph, x: i32, y: i32, size: i32, color: u32) {
    let p = |value: i32| size * value / 100;
    let cx = x + size / 2;
    let cy = y + size / 2;
    match glyph {
        Glyph::Files => {
            gfx.panel(x + p(20), y + p(27), p(28), p(15), p(5), WHITE, 255, 0, 0);
            gfx.panel(x + p(20), y + p(36), p(60), p(41), p(8), WHITE, 255, 0, 0);
        }
        Glyph::Settings => {
            const TEETH: [(i32, i32); 8] = [
                (29, 0), (21, 21), (0, 29), (-21, 21),
                (-29, 0), (-21, -21), (0, -29), (21, -21),
            ];
            for (dx, dy) in TEETH {
                gfx.dot(cx + p(dx), cy + p(dy), p(8).max(2), WHITE, 1024);
            }
            gfx.dot(cx, cy, p(25), WHITE, 1024);
            gfx.dot(cx, cy, p(13), color, 1024);
        }
        Glyph::Browser => {
            gfx.dot(cx, cy, p(31), WHITE, 1024);
            gfx.dot(cx, cy, p(26), color, 1024);
            gfx.fill_rect(cx - p(2), cy - p(26), p(4), p(52), WHITE);
            gfx.fill_rect(cx - p(26), cy - p(2), p(52), p(4), WHITE);
        }
        Glyph::Stopwatch => {
            gfx.dot(cx, cy + p(3), p(29), WHITE, 1024);
            gfx.dot(cx, cy + p(3), p(24), color, 1024);
            gfx.fill_rect(cx - p(2), cy - p(16), p(4), p(19), WHITE);
        }
        Glyph::System => {
            gfx.panel(x + p(28), y + p(28), p(44), p(44), p(9), WHITE, 255, 0, 0);
            gfx.dot(cx, cy, p(10), color, 1024);
        }
    }
}

/// 4'lü uygulama klasörü simgesi (2x2 mini ızgara).
pub fn draw_folder_tile(gfx: &mut Gfx<'_>, x: i32, y: i32, size: i32, slots: &[Option<(Glyph, u32)>; 4], pressed: bool) {
    let radius = size * 28 / 100;
    gfx.panel(x, y + size / 16, size, size, radius, SHADOW, 22, 0, 0);
    gfx.panel(x, y, size, size, radius, WHITE, 120, WHITE, 170);
    let pad = size * 16 / 100;
    let gap = size * 8 / 100;
    let mini = (size - pad * 2 - gap) / 2;
    for (idx, item) in slots.iter().enumerate() {
        let col = (idx % 2) as i32;
        let row = (idx / 2) as i32;
        let mx = x + pad + col * (mini + gap);
        let my = y + pad + row * (mini + gap);
        if let Some((glyph, color)) = *item {
            let mr = (mini * 30 / 100).max(3);
            gfx.panel(mx, my, mini, mini, mr, color, 255, 0, 0);
            draw_glyph_inner(gfx, glyph, mx, my, mini, color);
        } else {
            let mr = (mini * 30 / 100).max(3);
            gfx.panel(mx, my, mini, mini, mr, WHITE, 70, 0, 0);
        }
    }
    if pressed {
        gfx.panel(x, y, size, size, radius, SHADOW, 40, 0, 0);
    }
}

/// Durum çubuğu / Kontrol merkezi için vektör Wi-Fi simgesi.
pub fn draw_wifi_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, color: u32) {
    gfx.dot(cx, cy + r / 2, (r / 4).max(2), color, 1024);
    let w1 = r;
    let w2 = r * 3 / 2;
    gfx.fill_rect(cx - w1 / 2, cy, w1, 2, color);
    gfx.fill_rect(cx - w2 / 2, cy - r / 2, w2, 2, color);
}

/// Durum çubuğu / Kontrol merkezi için vektör Bluetooth simgesi.
pub fn draw_bt_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, color: u32) {
    gfx.fill_rect(cx - 1, cy - r, 2, r * 2 + 1, color);
    let arm = (r * 2 / 3).max(3);
    for s in 0..=arm {
        gfx.fill_rect(cx + s, cy - arm + s, 2, 2, color);
        gfx.fill_rect(cx + s, cy - s, 2, 2, color);
        gfx.fill_rect(cx + s, cy + s, 2, 2, color);
        gfx.fill_rect(cx + s, cy + arm - s, 2, 2, color);
    }
}

/// Parlaklık için güneş simgesi.
pub fn draw_sun_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, color: u32) {
    gfx.dot(cx, cy, (r * 55 / 100).max(3), color, 1024);
    const RAYS: [(i32, i32); 8] = [
        (10, 0), (7, 7), (0, 10), (-7, 7),
        (-10, 0), (-7, -7), (0, -10), (7, -7),
    ];
    for (dx, dy) in RAYS {
        gfx.dot(cx + r * dx / 10, cy + r * dy / 10, (r / 5).max(1), color, 1024);
    }
}
