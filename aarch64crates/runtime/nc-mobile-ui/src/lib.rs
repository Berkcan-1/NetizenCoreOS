//! Bağımsız ARMv8-A mobil Ring 3 arayüz ve tema kütüphanesi (`nc-mobile-ui`).
//! Mobil uygulamalar (`nc.home`, `nc.files`, `nc.settings`) bu modüldeki ortak tasarım dili,
//! `Dp` ölçekleme, renk paleti, vektör simgeler, kart/hap bileşenleri ve animasyon eğrilerini kullanır.
#![no_std]

pub mod a11y;
pub mod components;
pub mod keyboard;
pub mod motion;

pub use components::{
    camera_theme_from_event, deterministic_avatar_color, draw_bottom_nav, draw_btn,
    draw_confirm_sheet, draw_icon_btn, draw_monogram_avatar, draw_search_bar, draw_segmented_bar,
    draw_state_view, draw_status_badge, draw_text_field, draw_toast, draw_top_bar, draw_ui_icon,
    estimate_text_px, hit_bottom_nav, monogram_initial, truncate_tail_to_px, truncate_to_px,
    wrap_text_px, BtnKind, SurfaceTokens, TopBarGeo, UiIcon, Viewport,
};
pub use motion::{ToastState, Tween, ONE, SPEC_FADE, SPEC_PAGE, SPEC_PRESS, SPEC_SHEET, SPEC_TAB};

use nc_app_sdk::Gfx;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl UiRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> UiRect {
        UiRect { x, y, w, h }
    }

    pub const fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    pub const fn right(&self) -> i32 {
        self.x + self.w
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub const fn cx(&self) -> i32 {
        self.x + self.w / 2
    }

    pub const fn cy(&self) -> i32 {
        self.y + self.h / 2
    }

    pub const fn expand(&self, pad: i32) -> UiRect {
        UiRect::new(self.x - pad, self.y - pad, self.w + pad * 2, self.h + pad * 2)
    }
}

#[derive(Clone, Copy)]
pub struct Dp {
    pub width: i32,
    pub origin_x: i32,
    pub area_w: i32,
    pub area_h: i32,
}

impl Dp {
    pub fn new(area_w: i32, area_h: i32) -> Dp {
        let width = area_w.min(area_h * 9 / 15).max(180).min(area_w);
        Dp { width, origin_x: (area_w - width) / 2, area_w, area_h }
    }

    /// Responsive viewport-fit scaler: guarantees `design_h` logical dp fits
    /// vertically inside `area_h` without clipping or overlapping navigation bars,
    /// while preserving centered horizontal proportions on wide/landscape displays.
    pub fn fit(area_w: i32, area_h: i32, design_h: i32) -> Dp {
        let dh = design_h.max(320);
        let max_w_from_h = (area_h as i64 * BASE_DP as i64 / dh as i64) as i32;
        let width = area_w.min(max_w_from_h).max(160).min(area_w);
        Dp { width, origin_x: (area_w - width) / 2, area_w, area_h }
    }

    pub fn d(&self, value: i32) -> i32 {
        (value as i64 * self.width as i64 / BASE_DP as i64) as i32
    }

    pub fn x(&self, value: i32) -> i32 {
        self.origin_x + self.d(value)
    }

    pub fn rect(&self, x_dp: i32, y_dp: i32, w_dp: i32, h_dp: i32) -> UiRect {
        UiRect::new(self.x(x_dp), self.d(y_dp), self.d(w_dp), self.d(h_dp))
    }

    pub fn usable_h(&self, bottom_margin_dp: i32) -> i32 {
        (self.area_h - self.d(bottom_margin_dp)).max(self.d(200))
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
    Generic,
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

pub fn draw_glyph(gfx: &mut Gfx<'_>, glyph: Glyph, x: i32, y: i32, size: i32, color: u32) {
    draw_glyph_inner(gfx, glyph, x, y, size, color);
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
        Glyph::Generic => {
            // Sade NetizenCore amblem silueti (dis halka + ic cekirdek)
            gfx.dot(cx, cy, p(30), WHITE, 1024);
            gfx.dot(cx, cy, p(21), color, 1024);
            gfx.dot(cx, cy, p(9), WHITE, 1024);
        }
    }
}

/// Acik/Karanlik tema renk paleti.
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: u32,
    pub card: u32,
    pub ink: u32,
    pub muted: u32,
    pub subtle: u32,
    pub track: u32,
    pub divider: u32,
    pub row_sel: u32,
}

impl Palette {
    pub const LIGHT: Palette = Palette {
        bg: SURFACE_BG,
        card: CARD,
        ink: INK,
        muted: MUTED,
        subtle: SUBTLE,
        track: TRACK,
        divider: DIVIDER,
        row_sel: ROW_SEL,
    };

    pub const DARK: Palette = Palette {
        bg: 0x000D_1117,
        card: 0x0016_1B22,
        ink: 0x00F0_F6FC,
        muted: 0x008B_949E,
        subtle: 0x006E_7681,
        track: 0x0026_2C36,
        divider: 0x0021_262D,
        row_sel: 0x001F_242C,
    };

    pub const fn for_dark(dark: bool) -> Palette {
        if dark { Self::DARK } else { Self::LIGHT }
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

/// Shared black-and-white restrained Liquid Glass theme tokens for NetizenHome,
/// Widgets, Status Bar, and Control Center.
#[derive(Clone, Copy)]
pub struct GlassTheme {
    pub dark: bool,
    pub sheet_bg: u32,
    pub sheet_alpha: u32,
    pub card_bg: u32,
    pub card_alpha: u32,
    pub dock_bg: u32,
    pub dock_alpha: u32,
    pub rim: u32,
    pub rim_alpha: u32,
    pub ink: u32,
    pub muted: u32,
    pub subtle: u32,
    pub tile_off_bg: u32,
    pub tile_off_alpha: u32,
    pub tile_on_bg: u32,
    pub tile_on_fg: u32,
    pub slider_track: u32,
    pub slider_track_alpha: u32,
    pub slider_fill: u32,
    pub slider_fill_ink: u32,
    pub scrim: u32,
    pub status_bar_bg: u32,
    pub status_fg_on_bar: u32,
    pub nav_bg: u32,
    pub nav_ink: u32,
}

impl GlassTheme {
    /// Clean neutral light surfaces with crisp dark ink and subtle structural borders.
    pub const LIGHT: GlassTheme = GlassTheme {
        dark: false,
        sheet_bg: 0x00F4_F5F7,
        sheet_alpha: 238,
        card_bg: 0x00FF_FFFF,
        card_alpha: 224,
        dock_bg: 0x00F8_F9FB,
        dock_alpha: 186,
        rim: 0x0010_141C,
        rim_alpha: 26,
        ink: 0x0011_1318,
        muted: 0x005A_6270,
        subtle: 0x008A_93A2,
        tile_off_bg: 0x00E4_E7EC,
        tile_off_alpha: 215,
        tile_on_bg: 0x0014_171F,
        tile_on_fg: 0x00FF_FFFF,
        slider_track: 0x00DC_E0E7,
        slider_track_alpha: 230,
        slider_fill: 0x0014_171F,
        slider_fill_ink: 0x00FF_FFFF,
        scrim: 0x0008_0A0F,
        status_bar_bg: 0x00F5_F6F8,
        status_fg_on_bar: 0x0011_1318,
        nav_bg: 0x00F5_F6F8,
        nav_ink: 0x0052_5A68,
    };

    /// Deep obsidian surfaces with clean high-contrast light ink and restrained rim highlights.
    pub const DARK: GlassTheme = GlassTheme {
        dark: true,
        sheet_bg: 0x0010_1319,
        sheet_alpha: 240,
        card_bg: 0x001A_1E27,
        card_alpha: 224,
        dock_bg: 0x0014_1820,
        dock_alpha: 196,
        rim: 0x00FF_FFFF,
        rim_alpha: 30,
        ink: 0x00F5_F7FA,
        muted: 0x0098_A2B3,
        subtle: 0x0066_7085,
        tile_off_bg: 0x0024_2935,
        tile_off_alpha: 220,
        tile_on_bg: 0x00F5_F7FA,
        tile_on_fg: 0x0010_1319,
        slider_track: 0x0024_2935,
        slider_track_alpha: 230,
        slider_fill: 0x00F5_F7FA,
        slider_fill_ink: 0x0010_1319,
        scrim: 0x0004_060A,
        status_bar_bg: 0x0009_0B10,
        status_fg_on_bar: 0x00F5_F7FA,
        nav_bg: 0x000B_0E14,
        nav_ink: 0x0098_A2B3,
    };

    pub const fn for_dark(dark: bool) -> GlassTheme {
        if dark { Self::DARK } else { Self::LIGHT }
    }
}

/// Compact battery indicator for status bar and Control Center.
/// When `available == false`, draws an honest hollow outline with a centered slash/dash.
#[allow(clippy::too_many_arguments)]
pub fn draw_battery_icon(
    gfx: &mut Gfx<'_>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fg: u32,
    pct: u8,
    charging: bool,
    available: bool,
) {
    let body_w = (w - 3).max(12);
    let r = (h / 4).max(2);
    // Outer shell
    gfx.panel(x, y, body_w, h, r, fg, 18, fg, 210);
    // Positive terminal nub
    let nub_h = (h * 44 / 100).max(4);
    gfx.fill_rect(x + body_w + 1, y + (h - nub_h) / 2, 2, nub_h, fg);

    if !available {
        // Honest unavailable dash inside battery outline
        let dw = (body_w / 3).max(4);
        gfx.fill_rect(x + (body_w - dw) / 2, y + h / 2 - 1, dw, 2, fg);
        return;
    }

    let pad = 2;
    let inner_w = (body_w - pad * 2).max(2);
    let inner_h = (h - pad * 2).max(2);
    let fill_w = ((inner_w as u32 * (pct.min(100) as u32) + 50) / 100).max(if pct > 0 { 2 } else { 0 }) as i32;
    let fill_col = if charging {
        GREEN
    } else if pct <= 15 {
        CORAL
    } else if pct <= 25 {
        AMBER
    } else {
        fg
    };
    if fill_w > 0 {
        gfx.panel(x + pad, y + pad, fill_w.min(inner_w), inner_h, (r - 1).max(1), fill_col, 255, 0, 0);
    }
    if charging {
        // Minimal centered charging indicator bar
        let cx = x + body_w / 2;
        let cy = y + h / 2;
        gfx.fill_rect(cx - 1, cy - 2, 2, 5, WHITE);
    }
}

/// Status-aware Wi-Fi glyph:
/// state: 0=unavail, 1=disabled, 2=disconnected, 3=connecting, 4=connected, 5=limited.
pub fn draw_wifi_status_icon(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, fg: u32, state: u8, rssi: u8) {
    let dot_r = (r / 4).max(2);
    let w1 = r;
    let w2 = r * 3 / 2;
    match state {
        4 => {
            // Connected: solid bars
            gfx.dot(cx, cy + r / 2, dot_r, fg, 1024);
            let bars = if rssi == 0 { 3 } else { rssi.clamp(1, 3) };
            if bars >= 2 {
                gfx.fill_rect(cx - w1 / 2, cy, w1, 2, fg);
            }
            if bars >= 3 {
                gfx.fill_rect(cx - w2 / 2, cy - r / 2, w2, 2, fg);
            }
        }
        3 => {
            // Connecting: dot + lower bar
            gfx.dot(cx, cy + r / 2, dot_r, fg, 1024);
            gfx.fill_rect(cx - w1 / 2, cy, w1, 2, fg);
        }
        2 => {
            // Enabled but disconnected: dot only + subtle top bar
            gfx.dot(cx, cy + r / 2, dot_r, fg, 520);
            gfx.panel(cx - w1 / 2, cy, w1, 2, 1, fg, 90, 0, 0);
        }
        5 => {
            // Limited connectivity: icon + small exclamation dot
            gfx.dot(cx, cy + r / 2, dot_r, fg, 1024);
            gfx.fill_rect(cx - w1 / 2, cy, w1, 2, fg);
            gfx.fill_rect(cx + w1 / 2 + 2, cy - r / 2, 2, r, AMBER);
        }
        _ => {
            // Disabled / unavailable: dimmed + diagonal slash
            gfx.dot(cx, cy + r / 2, dot_r, fg, 320);
            gfx.panel(cx - w1 / 2, cy, w1, 2, 1, fg, 70, 0, 0);
            for s in -r / 2..=r / 2 {
                gfx.fill_rect(cx + s, cy + s, 2, 2, fg);
            }
        }
    }
}

/// Compact cellular signal bars (rendered only when cellular hardware exists).
pub fn draw_cellular_bars(gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, fg: u32, bars: u8) {
    let bw = ((w - 6) / 4).max(2);
    let gap = 2;
    for i in 0..4i32 {
        let bh = (h * (i + 1) / 4).max(3);
        let bx = x + i * (bw + gap);
        let by = y + h - bh;
        let active = (i as u8) < bars;
        gfx.panel(bx, by, bw, bh, 1, fg, if active { 255 } else { 70 }, 0, 0);
    }
}

/// Do Not Disturb crescent moon glyph.
pub fn draw_dnd_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, fg: u32, cut_bg: u32) {
    gfx.dot(cx, cy, r.max(4), fg, 1024);
    gfx.dot(cx + (r * 45 / 100).max(2), cy - (r * 25 / 100).max(1), (r * 78 / 100).max(3), cut_bg, 1024);
}

/// Airplane glyph (clean minimal silhouette).
pub fn draw_airplane_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, fg: u32) {
    let r = r.max(4);
    // Fuselage
    gfx.fill_rect(cx - 1, cy - r, 2, r * 2, fg);
    // Wings
    gfx.fill_rect(cx - r, cy - 1, r * 2, 2, fg);
    // Tailplane
    gfx.fill_rect(cx - r / 2, cy + r - 3, r, 2, fg);
}

/// Speaker / volume glyph.
pub fn draw_volume_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, fg: u32, muted: bool) {
    let r = r.max(4);
    gfx.fill_rect(cx - r, cy - r / 3, r / 2, r * 2 / 3, fg);
    gfx.fill_rect(cx - r / 2, cy - r * 2 / 3, r / 2, r * 4 / 3, fg);
    if muted {
        for s in -r / 3..=r / 3 {
            gfx.fill_rect(cx + r / 2 + s, cy + s, 2, 2, fg);
            gfx.fill_rect(cx + r / 2 + s, cy - s, 2, 2, fg);
        }
    } else {
        gfx.fill_rect(cx + r / 3, cy - r / 3, 2, r * 2 / 3, fg);
        gfx.fill_rect(cx + r * 2 / 3, cy - r * 2 / 3, 2, r * 4 / 3, fg);
    }
}

/// Light/Dark theme half-circle glyph.
pub fn draw_theme_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, fg: u32, bg: u32) {
    let r = r.max(4);
    gfx.dot(cx, cy, r, fg, 1024);
    gfx.dot(cx, cy, (r - 2).max(2), bg, 1024);
    gfx.fill_rect(cx, cy - r + 1, r - 1, (r - 1) * 2, fg);
}

/// Plus glyph (used for Add Widget / Edit toolbar).
pub fn draw_plus_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, fg: u32) {
    let r = r.max(3);
    gfx.fill_rect(cx - r, cy - 1, r * 2 + 1, 2, fg);
    gfx.fill_rect(cx - 1, cy - r, 2, r * 2 + 1, fg);
}

/// Close / remove cross glyph.
pub fn draw_close_glyph(gfx: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, fg: u32) {
    let r = r.max(3);
    for s in -r..=r {
        gfx.fill_rect(cx + s, cy + s, 2, 2, fg);
        gfx.fill_rect(cx + s, cy - s, 2, 2, fg);
    }
}

/// Live shared mobile theme context built from `EventBlock` on every frame.
/// Unifies NetizenHome's `GlassTheme`, `Palette`, live dark/light mode, accent color,
/// and TR/EN language across all Ring 3 mobile system apps.
#[derive(Clone, Copy)]
pub struct MobileTheme {
    pub dark: bool,
    pub accent: u32,
    pub en: bool,
    pub glass: GlassTheme,
    pub pal: Palette,
}

impl MobileTheme {
    pub fn from_event(ev: &nc_app_sdk::nc_abi::event::EventBlock) -> Self {
        let dark = nc_app_sdk::nc_abi::event::unpack_dark_theme(ev.flags);
        let accent = if ev.accent != 0 { ev.accent & 0x00FF_FFFF } else { BLUE };
        let en = ev.lang == 1;
        Self {
            dark,
            accent,
            en,
            glass: GlassTheme::for_dark(dark),
            pal: Palette::for_dark(dark),
        }
    }

    /// Dedicated black/near-black camera theme (ignores OS light/dark, preserves live OS accent & language).
    pub fn camera_dark(ev: &nc_app_sdk::nc_abi::event::EventBlock) -> Self {
        camera_theme_from_event(ev)
    }

    pub fn tokens(&self) -> SurfaceTokens {
        SurfaceTokens::from_theme(self)
    }

    pub const fn tr(&self, tr_text: &'static str, en_text: &'static str) -> &'static str {
        if self.en { en_text } else { tr_text }
    }

    pub fn fill_background(&self, gfx: &mut Gfx<'_>, w: i32, h: i32) {
        gfx.fill_rect(0, 0, w, h, self.glass.sheet_bg);
    }

    pub fn draw_card_box(&self, gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, radius: i32) {
        gfx.panel(
            x,
            y,
            w,
            h,
            radius,
            self.glass.card_bg,
            255,
            self.glass.rim,
            self.glass.rim_alpha.max(28),
        );
    }

    pub fn draw_header(
        &self,
        gfx: &mut Gfx<'_>,
        dp: &Dp,
        icon_id: u32,
        title: &str,
        show_back: bool,
    ) {
        if show_back {
            let bs = dp.d(30).max(24);
            self.draw_card_box(gfx, dp.x(16), dp.d(12), bs, bs, bs / 2);
            draw_chevron_left(gfx, dp.x(16) + bs / 2, dp.d(12) + bs / 2, dp, self.glass.ink);
            gfx.text(dp.x(54), dp.d(18), dp.d(14).max(11), self.glass.ink, 2, title);
        } else {
            let isz = dp.d(30).max(24);
            gfx.icon(icon_id, dp.x(16), dp.d(12), isz, isz);
            gfx.text(dp.x(54), dp.d(16), dp.d(16).max(12), self.glass.ink, 2, title);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_primary_btn(&self, gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, cap: i32, label: &str) {
        let r = h / 2;
        gfx.panel(x, y, w, h, r, self.accent, 255, 0, 0);
        gfx.text_centered(x + w / 2, y + (h - cap) / 2, cap, WHITE, 2, label);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_secondary_btn(&self, gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, cap: i32, label: &str) {
        let r = h / 2;
        gfx.panel(x, y, w, h, r, self.glass.card_bg, 255, self.glass.rim, self.glass.rim_alpha.max(32));
        gfx.text_centered(x + w / 2, y + (h - cap) / 2, cap, self.glass.ink, 2, label);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_danger_btn(&self, gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, cap: i32, label: &str) {
        let r = h / 2;
        gfx.panel(x, y, w, h, r, CORAL, 255, 0, 0);
        gfx.text_centered(x + w / 2, y + (h - cap) / 2, cap, WHITE, 2, label);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_badge(&self, gfx: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, color: u32, cap: i32, label: &str) {
        let r = h / 2;
        gfx.panel(x, y, w, h, r, color, 34, color, 150);
        gfx.text_centered(x + w / 2, y + (h - cap) / 2, cap, color, 2, label);
    }
}
