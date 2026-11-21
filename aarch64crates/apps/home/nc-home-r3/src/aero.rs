#![allow(dead_code)]

use crate::motion::{curve, DECEL};
use nc_abi::draw::{
    ICON_BROWSER, ICON_CAMERA, ICON_CONTACTS, ICON_FILES, ICON_GALLERY, ICON_GENERIC,
    ICON_KEYBOARD, ICON_MESSAGES, ICON_PHONE, ICON_SETTINGS, ICON_STOPWATCH, ICON_SYSTEM,
};
use nc_app_sdk::Gfx;
use nc_mobile_ui::{mix_color, GlassTheme, WHITE};

pub const DEEP: u32 = 0x000B_0E14;
pub const SHADE: u32 = 0x0007_090E;
pub const FROST_CARD: u32 = 0x0018_1E2A;
pub const FROST_SHEET: u32 = 0x0015_1A24;
pub const SOFT_WHITE: u32 = 0x00D8_DFE9;
pub const MUTED_LIGHT: u32 = 0x0094_A0B2;
pub const MINT: u32 = 0x0034_D399;
pub const PANEL_RIM: u32 = 28;

pub fn stag(p: i32, idx: i32, step: i32) -> i32 {
    let span = (256 - idx * step).max(64);
    curve(DECEL, ((p - idx * step) * 256 / span).clamp(0, 256))
}

pub fn scale_alpha(a: i32, s: i32) -> u32 {
    (a * s.clamp(0, 256) / 256).max(0) as u32
}

pub fn frost_sheet(g: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, r: i32, alpha: u32) {
    if w <= 0 || h <= 0 {
        return;
    }
    g.panel(x, y, w, h, r, FROST_SHEET, alpha, WHITE, PANEL_RIM);
}

pub fn themed_sheet(g: &mut Gfx<'_>, th: &GlassTheme, x: i32, y: i32, w: i32, h: i32, r: i32) {
    if w <= 0 || h <= 0 {
        return;
    }
    g.panel(x, y, w, h, r, th.sheet_bg, th.sheet_alpha, th.rim, th.rim_alpha);
}

pub fn themed_card(g: &mut Gfx<'_>, th: &GlassTheme, x: i32, y: i32, w: i32, h: i32, r: i32, alpha_scale: i32) {
    if w <= 0 || h <= 0 {
        return;
    }
    g.panel(
        x,
        y,
        w,
        h,
        r,
        th.card_bg,
        scale_alpha(th.card_alpha as i32, alpha_scale),
        th.rim,
        scale_alpha(th.rim_alpha as i32, alpha_scale),
    );
}

pub fn glass(g: &mut Gfx<'_>, x: i32, y: i32, w: i32, h: i32, r: i32, tint: u32, alpha: u32, rim: u32) {
    g.panel(x, y, w, h, r, tint, alpha, WHITE, rim);
}

pub fn label(g: &mut Gfx<'_>, cx: i32, y: i32, cap: i32, color: u32, text: &str) {
    label_fit(g, cx, y, cap, 12, color, text);
}

/// Draw a centered label with bounded character count (zero heap allocation) so long
/// application names never collide with adjacent tiles or dock slots.
pub fn label_fit(g: &mut Gfx<'_>, cx: i32, y: i32, cap: i32, max_chars: usize, color: u32, text: &str) {
    let char_count = text.chars().count();
    if char_count <= max_chars || max_chars < 4 {
        g.text_centered(cx, y, cap, color, 2, text);
        return;
    }
    let mut buf = [0u8; 48];
    let mut len = 0usize;
    let keep = max_chars - 2;
    for (i, ch) in text.chars().enumerate() {
        if i >= keep {
            break;
        }
        let mut tmp = [0u8; 4];
        let s = ch.encode_utf8(&mut tmp);
        if len + s.len() + 2 > buf.len() {
            break;
        }
        buf[len..len + s.len()].copy_from_slice(s.as_bytes());
        len += s.len();
    }
    if len + 2 <= buf.len() {
        buf[len] = b'.';
        buf[len + 1] = b'.';
        len += 2;
    }
    if let Ok(s) = core::str::from_utf8(&buf[..len]) {
        g.text_centered(cx, y, cap, color, 2, s);
    }
}

pub fn valid_icon_id(id: u32) -> u32 {
    match id {
        ICON_FILES
        | ICON_SETTINGS
        | ICON_BROWSER
        | ICON_SYSTEM
        | ICON_STOPWATCH
        | ICON_GENERIC
        | ICON_KEYBOARD
        | ICON_GALLERY
        | ICON_MESSAGES
        | ICON_PHONE
        | ICON_CONTACTS
        | ICON_CAMERA => id,
        _ => ICON_GENERIC,
    }
}

/// Uygulama simgesi cizer; kayitli simge yoksa her zaman ortak NetizenCore genel simgesini (`ICON_GENERIC`) kullanir.
pub fn tile_icon(g: &mut Gfx<'_>, icon_id: u32, x: i32, y: i32, size: i32, pressed: bool) {
    let r = size * 24 / 100;
    g.icon(valid_icon_id(icon_id), x, y, size, size);
    if pressed {
        g.panel(x, y, size, size, r, SHADE, 48, 0, 0);
    }
}

pub fn mini_tile_icon(g: &mut Gfx<'_>, icon_id: u32, x: i32, y: i32, size: i32) {
    g.icon(valid_icon_id(icon_id), x, y, size, size);
}

pub fn folder_tile_icons(g: &mut Gfx<'_>, x: i32, y: i32, size: i32, slots: &[Option<u32>; 4], pressed: bool) {
    let r = size * 24 / 100;
    g.panel(x, y, size, size, r, WHITE, 118, WHITE, 48);
    let pad = size * 14 / 100;
    let gap = size * 8 / 100;
    let mini = (size - pad * 2 - gap) / 2;
    for (i, item) in slots.iter().enumerate() {
        let mx = x + pad + (i as i32 % 2) * (mini + gap);
        let my = y + pad + (i as i32 / 2) * (mini + gap);
        if let Some(id) = *item {
            mini_tile_icon(g, id, mx, my, mini);
        }
    }
    if pressed {
        g.panel(x, y, size, size, r, SHADE, 48, 0, 0);
    }
}

pub fn search_glyph(g: &mut Gfx<'_>, cx: i32, cy: i32, r: i32, color: u32) {
    g.dot(cx, cy, r, color, 1024);
    g.dot(cx, cy, (r - 2).max(2), FROST_SHEET, 1024);
    for s in 1..=(r * 7 / 10).max(3) {
        g.fill_rect(cx + r / 2 + s, cy + r / 2 + s, 2, 2, color);
    }
}

pub fn chevron(g: &mut Gfx<'_>, cx: i32, cy: i32, arm: i32, color: u32) {
    let arm = arm.max(3);
    for s in 0..=arm {
        g.fill_rect(cx - arm / 2 + s, cy - arm + s, 2, 2, color);
        g.fill_rect(cx - arm / 2 + s, cy + arm - s, 2, 2, color);
    }
}

pub fn fade_to(fg: u32, bg: u32, p: i32) -> u32 {
    mix_color(bg, fg, p.clamp(0, 256), 256)
}
