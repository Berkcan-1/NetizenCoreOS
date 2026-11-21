#![no_std]

pub use nc_mobile_ui::*;
use nc_app_sdk::Gfx;

pub const BEAD: u32 = 0x00FF_FFFF;
pub const TIDE: u32 = 0x0012_B5C6;
pub const TIDE_DEEP: u32 = 0x0016_5FA8;
pub const SAND_GLOW: u32 = 0x00E6_C45A;

const PLATE_SHADE_PCT: i32 = 26;

fn deepen(color: u32) -> u32 {
    mix_color(color, 0x0000_0000, PLATE_SHADE_PCT, 100)
}

pub fn draw_soft_glow(gfx: &mut Gfx<'_>, cx: i32, cy: i32, radius: i32, color: u32, peak: u32) {
    const RINGS: i32 = 12;
    let step = (peak / RINGS as u32).max(1);
    for ring in 0..RINGS {
        let r = radius * (RINGS - ring) / RINGS;
        gfx.dot(cx, cy, r.max(1), color, step);
    }
}

pub fn draw_bead(gfx: &mut Gfx<'_>, cx: i32, cy: i32, radius: i32, color: u32, strength: u32) {
    gfx.dot(cx, cy, radius.max(1), color, strength);
}

/// Düz, sakin kart: çok hafif alt gölge + beyaz yüzey (cam/parlama efekti yok).
pub fn draw_card(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, radius: i32) {
    if w > 8 && h > 8 {
        gfx.panel(x + 1, y + 2, w - 2, h, radius, SHADOW, 7, 0, 0);
    }
    gfx.panel(x, y, w, h, radius, CARD, 255, 0, 0);
}

/// Kenarları çizgili boş kart (ikincil yüzeyler için).
pub fn draw_card_flat(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, radius: i32, fill: u32) {
    gfx.panel(x, y, w, h, radius, fill, 255, 0, 0);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_pill_btn(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, fill: u32, fg: u32, cap: i32, label: &str) {
    let r = h / 2;
    gfx.panel(x, y, w, h, r, fill, 255, DIVIDER, if fill == CARD { 255 } else { 0 });
    gfx.text_centered(x + w / 2, y + (h - cap) / 2, cap, fg, 2, label);
}

/// `on`: açık/kapalı (anında). Animasyonlu sürüm için `draw_switch_p`.
pub fn draw_switch(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, on: bool, accent: u32) {
    draw_switch_p(gfx, x, y, w, h, if on { 256 } else { 0 }, accent);
}

/// `p`: 0 (kapalı) ..= 256 (açık). Anahtar topu ve iz rengi p ile akıcı değişir.
pub fn draw_switch_p(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, p: i32, accent: u32) {
    let p = p.clamp(0, 256);
    let r = h / 2;
    let bg = mix_color(TRACK_OFF, accent, p, 256);
    gfx.panel(x, y, w, h, r, bg, 255, 0, 0);
    let pad = (h / 8).max(2);
    let knob = (h - pad * 2).max(6);
    let travel = w - pad * 2 - knob;
    let kx = x + pad + knob / 2 + travel * p / 256;
    gfx.dot(kx, y + h / 2 + 1, knob / 2 + 1, SHADOW, 50);
    gfx.dot(kx, y + h / 2, knob / 2, BEAD, 256);
}

/// Kaydırıcı: ince iz + dolgu + beyaz tutamaç.
pub fn draw_slider(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, pct: u8, accent: u32) {
    let r = h / 2;
    gfx.panel(x, y, w, h, r, TRACK, 255, 0, 0);
    let p = (pct.clamp(10, 100) as i32 - 10) * 100 / 90;
    let fill_w = (w * p / 100).max(h);
    gfx.panel(x, y, fill_w, h, r, accent, 255, 0, 0);
    let kr = (r + 2).max(6);
    gfx.dot(x + fill_w - r, y + r + 1, kr + 1, SHADOW, 50);
    gfx.dot(x + fill_w - r, y + r, kr, BEAD, 256);
}

/// Sayfa başlığı: büyük, sade (süs noktası yok).
pub fn draw_title(gfx: &mut Gfx<'_>, dp: &Dp, y: i32, _accent: u32, text: &str) {
    gfx.text(dp.x(20), dp.d(y), dp.d(20), INK, 3, text);
}

/// Segment kontrolü (anında).
pub fn draw_segmented(gfx: &mut Gfx<'_>, dp: &Dp, y: i32, labels: &[&str], active: usize, accent: u32) {
    draw_segmented_p(gfx, dp, y, labels, active as i32 * 256, accent);
}

/// `pos`: seçili parçanın konumu * 256 (ör. 1.5. parça = 384); kayan beyaz tutamaç animasyonu için.
pub fn draw_segmented_p(gfx: &mut Gfx<'_>, dp: &Dp, y: i32, labels: &[&str], pos: i32, _accent: u32) {
    let count = labels.len().max(1) as i32;
    let total = dp.d(328);
    gfx.panel(dp.x(16), dp.d(y), total, dp.d(34), dp.d(17), TRACK, 255, 0, 0);
    let seg = (total - dp.d(6)) / count;
    let thumb_x = dp.x(16) + dp.d(3) + seg * pos.max(0) / 256;
    gfx.panel(thumb_x + 1, dp.d(y) + dp.d(4), seg, dp.d(28), dp.d(14), SHADOW, 10, 0, 0);
    gfx.panel(thumb_x, dp.d(y) + dp.d(3), seg, dp.d(28), dp.d(14), CARD, 255, 0, 0);
    for (i, label) in labels.iter().enumerate() {
        let near = (256 - (i as i32 * 256 - pos).abs()).clamp(0, 256);
        let color = mix_color(MUTED, INK, near, 256);
        let cx = dp.x(16) + dp.d(3) + seg * i as i32 + seg / 2;
        gfx.text_centered(cx, dp.d(y + 12), dp.d(8), color, 2, label);
    }
}

/// Dairesel renkli rozet (ayarlar listesi, One UI tarzı).
pub fn draw_badge(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, color: u32) {
    gfx.dot(cx, cy, r, color, 1024);
}

/// Ayar satırı: soldaki rozet + başlık + alt başlık + sağ ok. `pressed`: hafif basılı gölge.
#[allow(clippy::too_many_arguments)]
pub fn draw_row(
    gfx: &mut Gfx<'_>,
    dp: &Dp,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    badge: u32,
    title: &str,
    subtitle: &str,
    pressed: bool,
) -> (i32, i32) {
    if pressed {
        gfx.panel(x, y, w, h, dp.d(18), ROW_SEL, 255, 0, 0);
    }
    let cx = x + dp.d(36);
    let cy = y + h / 2;
    draw_badge(gfx, cx, cy, dp.d(20), badge);
    gfx.text(x + dp.d(70), y + h / 2 - dp.d(15), dp.d(10), INK, 2, title);
    gfx.text(x + dp.d(70), y + h / 2 + dp.d(4), dp.d(7), MUTED, 1, subtitle);
    draw_chevron_right(gfx, x + w - dp.d(22), cy, dp, SUBTLE);
    (cx, cy)
}

/// Rozet içi basit simgeler (beyaz).
pub fn draw_storage_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32) {
    gfx.panel(cx - r, cy - r * 8 / 10, r * 2, r * 8 / 10, r / 3, BEAD, 255, 0, 0);
    gfx.panel(cx - r, cy + r / 8, r * 2, r * 8 / 10, r / 3, BEAD, 255, 0, 0);
    gfx.dot(cx + r * 5 / 10, cy - r * 4 / 10, (r / 7).max(1), BLUE_DEEP, 1024);
    gfx.dot(cx + r * 5 / 10, cy + r * 5 / 10, (r / 7).max(1), BLUE_DEEP, 1024);
}

pub fn draw_gear_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, hole: u32) {
    const TEETH: [(i32, i32); 8] = [(100, 0), (71, 71), (0, 100), (-71, 71), (-100, 0), (-71, -71), (0, -100), (71, -71)];
    for (dx, dy) in TEETH {
        gfx.dot(cx + r * dx * 82 / 10000, cy + r * dy * 82 / 10000, (r * 30 / 100).max(2), BEAD, 1024);
    }
    gfx.dot(cx, cy, r * 72 / 100, BEAD, 1024);
    gfx.dot(cx, cy, r * 36 / 100, hole, 1024);
}

pub const TRACK_OFF: u32 = 0x00D3_D9E3;
pub const BLUE_DEEP: u32 = 0x0036_6BE8;

pub fn draw_app_tile(gfx: &mut Gfx<'_>, glyph: Glyph, x: i32, y: i32, size: i32, color: u32, pressed: bool) {
    let radius = size * 30 / 100;
    gfx.panel(x + size / 12, y + size * 14 / 100, size - size / 6, size, radius, SHADOW, 12, 0, 0);
    gfx.panel(x, y + size / 16, size, size, radius, SHADOW, 22, 0, 0);
    gfx.panel(x, y, size, size, radius, color, 255, BEAD, 90);
    gfx.panel(x + size / 20, y + size * 58 / 100, size - size / 10, size * 38 / 100, radius * 7 / 10, deepen(color), 44, 0, 0);
    gfx.panel(x + size / 12, y + size / 14, size - size / 6, size * 38 / 100, radius * 6 / 10, BEAD, 46, 0, 0);
    draw_glyph(gfx, glyph, x, y, size, color);
    gfx.dot(x + size * 24 / 100, y + size * 20 / 100, (size * 5 / 100).max(2), BEAD, 150);
    if pressed {
        gfx.panel(x, y, size, size, radius, SHADOW, 44, 0, 0);
    }
}

pub fn draw_folder_tile(gfx: &mut Gfx<'_>, x: i32, y: i32, size: i32, slots: &[Option<(Glyph, u32)>; 4], pressed: bool) {
    let radius = size * 30 / 100;
    gfx.panel(x, y + size / 16, size, size, radius, SHADOW, 20, 0, 0);
    gfx.panel(x, y, size, size, radius, BEAD, 120, BEAD, 170);
    gfx.panel(x + size / 12, y + size / 14, size - size / 6, size * 32 / 100, radius * 6 / 10, BEAD, 40, 0, 0);
    let pad = size * 16 / 100;
    let gap = size * 8 / 100;
    let mini = (size - pad * 2 - gap) / 2;
    for (idx, item) in slots.iter().enumerate() {
        let col = (idx % 2) as i32;
        let row = (idx / 2) as i32;
        let mx = x + pad + col * (mini + gap);
        let my = y + pad + row * (mini + gap);
        let mr = (mini * 32 / 100).max(3);
        if let Some((glyph, color)) = *item {
            gfx.panel(mx, my, mini, mini, mr, color, 255, 0, 0);
            gfx.panel(mx + mini / 12, my + mini / 14, mini - mini / 6, mini * 38 / 100, mr * 6 / 10, BEAD, 46, 0, 0);
            draw_glyph(gfx, glyph, mx, my, mini, color);
        } else {
            gfx.panel(mx, my, mini, mini, mr, BEAD, 70, 0, 0);
        }
    }
    gfx.dot(x + size * 22 / 100, y + size * 18 / 100, (size * 4 / 100).max(2), BEAD, 130);
    if pressed {
        gfx.panel(x, y, size, size, radius, SHADOW, 40, 0, 0);
    }
}

pub fn draw_dock(gfx: &mut Gfx<'_>, dp: &Dp, area_w: i32, dock_y: i32) {
    let _ = area_w;
    gfx.panel(dp.x(30), dock_y + dp.d(6), dp.d(300), dp.d(68), dp.d(26), SHADOW, 16, 0, 0);
    gfx.panel(dp.x(26), dock_y, dp.d(308), dp.d(68), dp.d(26), BEAD, 84, BEAD, 130);
    gfx.panel(dp.x(36), dock_y + dp.d(3), dp.d(288), dp.d(18), dp.d(18), BEAD, 18, 0, 0);
}

pub fn draw_tide_indicator(gfx: &mut Gfx<'_>, dp: &Dp, cx: i32, y: i32) {
    gfx.panel(cx - dp.d(9), y, dp.d(18), dp.d(5).max(3), dp.d(3), BEAD, 235, 0, 0);
}
