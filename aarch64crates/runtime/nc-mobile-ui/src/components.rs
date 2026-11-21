//! Shared NetizenCore Mobile Design System components, responsive viewport layout,
//! UTF-8 Turkish-safe text wrapping, and restrained visual primitives.
//!
//! Designed strictly for `#![no_std]` Ring 3 apps with bounded draw streams
//! (`MAX_RECORDS = 512`, `MAX_STREAM_BYTES = 16 KiB`).

use nc_app_sdk::Gfx;

use crate::{
    mix_color, GlassTheme, MobileTheme, Palette, UiRect, AMBER, BLUE, GREEN, PURPLE, TEAL, WHITE,
};

/// Responsive logical viewport derived directly from `EventBlock.area_w` and `area_h`
/// rather than forcing every screen into a fixed `360 x 476` vertical box.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub area_w: i32,
    pub area_h: i32,
    /// Logical reference width used for density scaling (`d(v)` = `v * scale_w / 360`).
    pub scale_w: i32,
    /// Horizontal outer margin in pixels.
    pub margin_x: i32,
    /// Main content column x-origin (uses full width on portrait, intelligent wider column on landscape).
    pub content_x: i32,
    /// Main content column width in pixels.
    pub content_w: i32,
    pub is_landscape: bool,
    pub is_compact_h: bool,
}

impl Viewport {
    pub fn new(area_w: i32, area_h: i32) -> Self {
        let aw = area_w.max(180);
        let ah = area_h.max(180);
        let is_landscape = aw > ah + (ah / 10);
        // Derive density from the shorter side so landscape/wide screens don't blow up fonts,
        // and short portrait screens keep legible typography.
        let min_side = aw.min(ah);
        let scale_w = if is_landscape {
            min_side.clamp(240, 440)
        } else {
            aw.clamp(220, 440)
        };
        let d = |v: i32| (v as i64 * scale_w as i64 / 360) as i32;
        let is_compact_h = ah < d(460);
        let margin_x = d(16).max(10);
        // On portrait use full width minus margins; on wide landscape cap comfortable reading column
        // at 560dp so forms/lists stay scannable while grids/viewfinders can use `area_w` directly.
        let max_col = d(560).max(aw * 72 / 100);
        let content_w = (aw - margin_x * 2).min(max_col).max(160);
        let content_x = (aw - content_w) / 2;
        Self {
            area_w: aw,
            area_h: ah,
            scale_w,
            margin_x,
            content_x,
            content_w,
            is_landscape,
            is_compact_h,
        }
    }

    pub fn from_event(ev: &nc_app_sdk::nc_abi::event::EventBlock) -> Self {
        Self::new(ev.area_w, ev.area_h)
    }

    /// Convert logical dp to physical pixels.
    #[inline]
    pub fn d(&self, dp: i32) -> i32 {
        (dp as i64 * self.scale_w as i64 / 360) as i32
    }

    /// Standard header bar height.
    pub fn header_h(&self) -> i32 {
        if self.is_compact_h { self.d(44).max(38) } else { self.d(52).max(44) }
    }

    /// Standard bottom navigation bar height.
    pub fn bottom_nav_h(&self) -> i32 {
        if self.is_compact_h { self.d(48).max(42) } else { self.d(56).max(48) }
    }

    /// Estimated height occupied by the modular IME overlay inside the child app viewport.
    pub fn ime_inset(&self, ime_visible: bool) -> i32 {
        if !ime_visible {
            return 0;
        }
        // Host IME is 216 dp from screen bottom; bottom_inset (~44 dp) is already subtracted from area_h.
        let ime_px = self.d(178);
        ime_px.min(self.area_h * 48 / 100)
    }

    /// Usable bottom y coordinate accounting for optional IME.
    pub fn usable_bottom(&self, ime_visible: bool) -> i32 {
        (self.area_h - self.ime_inset(ime_visible)).max(self.header_h() + self.d(90))
    }

    /// Content-column rect at vertical position `y` with height `h`.
    pub fn row_rect(&self, y: i32, h: i32) -> UiRect {
        UiRect::new(self.content_x, y, self.content_w, h)
    }
}

/// Refined surface and semantic color tokens built from `MobileTheme`.
/// Light and dark modes are intentionally crafted separately (never a crude invert).
#[derive(Clone, Copy)]
pub struct SurfaceTokens {
    pub bg: u32,
    pub surface: u32,
    pub surface_subtle: u32,
    pub elevated: u32,
    pub border: u32,
    pub border_alpha: u32,
    pub divider: u32,
    pub ink: u32,
    pub muted: u32,
    pub subtle: u32,
    pub pressed: u32,
    pub accent: u32,
    pub accent_pressed: u32,
    pub danger: u32,
    pub warning: u32,
    pub success: u32,
    pub scrim: u32,
    pub is_dark: bool,
}

impl SurfaceTokens {
    pub fn from_theme(th: &MobileTheme) -> Self {
        let accent = th.accent;
        if th.dark {
            Self {
                bg: 0x000A_0D12,
                surface: 0x0013_1820,
                surface_subtle: 0x001A_202B,
                elevated: 0x001D_2430,
                border: 0x00FF_FFFF,
                border_alpha: 24,
                divider: 0x0022_2A36,
                ink: 0x00F4_F6FA,
                muted: 0x0096_A1B2,
                subtle: 0x0066_7183,
                pressed: 0x0023_2B38,
                accent,
                accent_pressed: mix_color(accent, 0x0000_0000, 22, 100),
                danger: 0x00EF_4444,
                warning: 0x00F5_9E0B,
                success: 0x0010_B981,
                scrim: 0x0003_0508,
                is_dark: true,
            }
        } else {
            Self {
                bg: 0x00F5_F7FA,
                surface: 0x00FF_FFFF,
                surface_subtle: 0x00EC_F0F5,
                elevated: 0x00FF_FFFF,
                border: 0x000F_172A,
                border_alpha: 22,
                divider: 0x00E4_E9F0,
                ink: 0x000F_172A,
                muted: 0x0052_6072,
                subtle: 0x0085_92A3,
                pressed: 0x00E6_EBF2,
                accent,
                accent_pressed: mix_color(accent, 0x0000_0000, 18, 100),
                danger: 0x00DC_2626,
                warning: 0x00D9_7706,
                success: 0x0005_9669,
                scrim: 0x000A_0E16,
                is_dark: false,
            }
        }
    }

    /// Dedicated black/near-black camera theme used in BOTH OS light and dark modes,
    /// retaining the live OS accent color for active indicators.
    pub fn camera_black(accent: u32) -> Self {
        let acc = if accent != 0 { accent & 0x00FF_FFFF } else { BLUE };
        Self {
            bg: 0x0000_0000,
            surface: 0x000C_0E12,
            surface_subtle: 0x0016_1A21,
            elevated: 0x001A_1F28,
            border: 0x00FF_FFFF,
            border_alpha: 28,
            divider: 0x001E_232C,
            ink: 0x00F8_FAFC,
            muted: 0x009C_A3AF,
            subtle: 0x006B_7280,
            pressed: 0x0026_2C36,
            accent: acc,
            accent_pressed: mix_color(acc, 0x0000_0000, 20, 100),
            danger: 0x00EF_4444,
            warning: 0x00F5_9E0B,
            success: 0x0010_B981,
            scrim: 0x0000_0000,
            is_dark: true,
        }
    }
}

/// Construct a camera-specific `MobileTheme` (always dark chrome, live OS accent & language).
pub fn camera_theme_from_event(ev: &nc_app_sdk::nc_abi::event::EventBlock) -> MobileTheme {
    let accent = if ev.accent != 0 { ev.accent & 0x00FF_FFFF } else { BLUE };
    MobileTheme {
        dark: true,
        accent,
        en: ev.lang == 1,
        glass: GlassTheme::DARK,
        pal: Palette::DARK,
    }
}

/// Compact vector iconography (each icon emits <= 5 draw commands to stay well within budget).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiIcon {
    Back,
    Search,
    Plus,
    Close,
    Check,
    Star,
    StarFilled,
    Settings,
    Grid,
    Info,
    Trash,
    Edit,
    Phone,
    Message,
    Refresh,
    Timer,
    Camera,
    Backspace,
    ChevronRight,
    ChevronLeft,
    Sort,
    Select,
    Keypad,
    Clock,
    Person,
    Photo,
}

pub fn draw_ui_icon(gfx: &mut Gfx<'_>, icon: UiIcon, cx: i32, cy: i32, r: i32, color: u32, bg_cut: u32) {
    let r = r.max(4);
    let t = (r / 4).clamp(1, 2);
    match icon {
        UiIcon::Back | UiIcon::ChevronLeft => {
            let arm = (r * 65 / 100).max(3);
            gfx.fill_rect(cx - arm / 2, cy - t / 2, t + 1, t + 1, color);
            gfx.fill_rect(cx - arm / 2 + t, cy - arm / 2, t + 1, arm / 2, color);
            gfx.fill_rect(cx - arm / 2 + t, cy + 1, t + 1, arm / 2, color);
            gfx.fill_rect(cx + arm / 3, cy - arm, t + 1, arm / 2 + 1, color);
            gfx.fill_rect(cx + arm / 3, cy + arm / 2, t + 1, arm / 2 + 1, color);
        }
        UiIcon::ChevronRight => {
            let arm = (r * 65 / 100).max(3);
            gfx.fill_rect(cx + arm / 2 - t, cy - t / 2, t + 1, t + 1, color);
            gfx.fill_rect(cx + arm / 2 - t * 2, cy - arm / 2, t + 1, arm / 2, color);
            gfx.fill_rect(cx + arm / 2 - t * 2, cy + 1, t + 1, arm / 2, color);
            gfx.fill_rect(cx - arm / 3, cy - arm, t + 1, arm / 2 + 1, color);
            gfx.fill_rect(cx - arm / 3, cy + arm / 2, t + 1, arm / 2 + 1, color);
        }
        UiIcon::Search => {
            let lr = (r * 58 / 100).max(3);
            let ox = cx - r / 5;
            let oy = cy - r / 5;
            gfx.dot(ox, oy, lr, color, 1024);
            gfx.dot(ox, oy, (lr - t - 1).max(1), bg_cut, 1024);
            gfx.fill_rect(ox + lr - 1, oy + lr - 1, r / 2, t + 1, color);
        }
        UiIcon::Plus => {
            gfx.fill_rect(cx - r * 7 / 10, cy - t / 2, r * 14 / 10 + 1, t.max(2), color);
            gfx.fill_rect(cx - t / 2, cy - r * 7 / 10, t.max(2), r * 14 / 10 + 1, color);
        }
        UiIcon::Close => {
            let a = (r * 55 / 100).max(3);
            gfx.fill_rect(cx - a, cy - a, t + 1, t + 1, color);
            gfx.fill_rect(cx + a - t, cy - a, t + 1, t + 1, color);
            gfx.fill_rect(cx - t / 2, cy - t / 2, t + 1, t + 1, color);
            gfx.fill_rect(cx - a, cy + a - t, t + 1, t + 1, color);
            gfx.fill_rect(cx + a - t, cy + a - t, t + 1, t + 1, color);
        }
        UiIcon::Check => {
            gfx.fill_rect(cx - r * 6 / 10, cy, r * 4 / 10, t.max(2), color);
            gfx.fill_rect(cx - r * 2 / 10, cy + r / 5, r * 3 / 10, t.max(2), color);
            gfx.fill_rect(cx, cy - r * 4 / 10, r * 6 / 10, t.max(2), color);
        }
        UiIcon::Star => {
            gfx.dot(cx, cy, r * 72 / 100, color, 1024);
            gfx.dot(cx, cy, (r * 42 / 100).max(2), bg_cut, 1024);
        }
        UiIcon::StarFilled => {
            gfx.dot(cx, cy, r * 75 / 100, AMBER, 1024);
            gfx.dot(cx, cy, (r * 28 / 100).max(2), WHITE, 520);
        }
        UiIcon::Settings => {
            gfx.dot(cx, cy, r * 78 / 100, color, 1024);
            gfx.dot(cx, cy, (r * 38 / 100).max(2), bg_cut, 1024);
        }
        UiIcon::Grid => {
            let s = r * 75 / 100;
            gfx.fill_rect(cx - s, cy - s / 3, s * 2, 1, color);
            gfx.fill_rect(cx - s, cy + s / 3, s * 2, 1, color);
            gfx.fill_rect(cx - s / 3, cy - s, 1, s * 2, color);
            gfx.fill_rect(cx + s / 3, cy - s, 1, s * 2, color);
        }
        UiIcon::Info => {
            gfx.dot(cx, cy, r * 82 / 100, color, 1024);
            gfx.dot(cx, cy, (r * 62 / 100).max(2), bg_cut, 1024);
            gfx.dot(cx, cy - r * 28 / 100, (t).max(1), color, 1024);
            gfx.fill_rect(cx - t / 2, cy - r / 12, t.max(2), r * 48 / 100, color);
        }
        UiIcon::Trash => {
            let w = r * 12 / 10;
            gfx.fill_rect(cx - w / 2 - 1, cy - r * 6 / 10, w + 2, t.max(2), color);
            gfx.panel(cx - w / 2, cy - r * 3 / 10, w, r, (r / 4).max(2), 0, 0, color, 255);
        }
        UiIcon::Edit => {
            gfx.fill_rect(cx - r * 5 / 10, cy + r * 3 / 10, r, t.max(2), color);
            gfx.fill_rect(cx - r * 2 / 10, cy - r * 5 / 10, r * 6 / 10, t + 2, color);
        }
        UiIcon::Phone => {
            gfx.panel(cx - r * 55 / 100, cy - r * 75 / 100, r * 110 / 100, r * 150 / 100, (r / 3).max(2), 0, 0, color, 255);
            gfx.fill_rect(cx - r / 4, cy + r * 42 / 100, r / 2, t.max(2), color);
        }
        UiIcon::Message => {
            gfx.panel(cx - r * 80 / 100, cy - r * 65 / 100, r * 160 / 100, r * 115 / 100, (r / 3).max(3), 0, 0, color, 255);
            gfx.fill_rect(cx - r * 40 / 100, cy - r / 5, r * 80 / 100, t.max(1), color);
            gfx.fill_rect(cx - r * 40 / 100, cy + r / 6, r * 55 / 100, t.max(1), color);
        }
        UiIcon::Refresh => {
            gfx.dot(cx, cy, r * 78 / 100, color, 1024);
            gfx.dot(cx, cy, (r * 52 / 100).max(2), bg_cut, 1024);
            gfx.fill_rect(cx + r / 6, cy - r * 85 / 100, r * 65 / 100, r * 65 / 100, bg_cut);
            gfx.fill_rect(cx + r / 4, cy - r * 75 / 100, r / 2, t.max(2), color);
        }
        UiIcon::Timer => {
            gfx.dot(cx, cy + r / 8, r * 72 / 100, color, 1024);
            gfx.dot(cx, cy + r / 8, (r * 52 / 100).max(2), bg_cut, 1024);
            gfx.fill_rect(cx - r / 4, cy - r * 85 / 100, r / 2, t.max(2), color);
            gfx.fill_rect(cx - 1, cy - r / 4, 2, r / 2, color);
        }
        UiIcon::Camera => {
            gfx.panel(cx - r * 82 / 100, cy - r * 55 / 100, r * 164 / 100, r * 115 / 100, (r / 3).max(3), 0, 0, color, 255);
            gfx.dot(cx, cy + r / 12, (r * 34 / 100).max(2), color, 1024);
            gfx.dot(cx, cy + r / 12, (r * 16 / 100).max(1), bg_cut, 1024);
        }
        UiIcon::Backspace => {
            gfx.panel(cx - r * 65 / 100, cy - r * 55 / 100, r * 145 / 100, r * 110 / 100, (r / 3).max(3), 0, 0, color, 255);
            gfx.fill_rect(cx - r / 5, cy - 1, r * 6 / 10, 2, color);
        }
        UiIcon::Sort => {
            gfx.fill_rect(cx - r * 7 / 10, cy - r * 45 / 100, r * 14 / 10, t.max(2), color);
            gfx.fill_rect(cx - r * 7 / 10, cy - t / 2, r, t.max(2), color);
            gfx.fill_rect(cx - r * 7 / 10, cy + r * 35 / 100, r * 6 / 10, t.max(2), color);
        }
        UiIcon::Select => {
            gfx.panel(cx - r * 7 / 10, cy - r * 7 / 10, r * 14 / 10, r * 14 / 10, (r / 3).max(3), 0, 0, color, 255);
            gfx.dot(cx, cy, (r * 32 / 100).max(2), color, 1024);
        }
        UiIcon::Keypad => {
            let g = (r * 52 / 100).max(3);
            let dr = (r / 5).max(2);
            gfx.dot(cx - g, cy - g / 2, dr, color, 1024);
            gfx.dot(cx, cy - g / 2, dr, color, 1024);
            gfx.dot(cx + g, cy - g / 2, dr, color, 1024);
            gfx.dot(cx - g, cy + g / 2, dr, color, 1024);
            gfx.dot(cx, cy + g / 2, dr, color, 1024);
        }
        UiIcon::Clock => {
            gfx.dot(cx, cy, r * 80 / 100, color, 1024);
            gfx.dot(cx, cy, (r * 60 / 100).max(2), bg_cut, 1024);
            gfx.fill_rect(cx - 1, cy - r * 42 / 100, 2, r * 45 / 100, color);
            gfx.fill_rect(cx, cy - 1, r * 36 / 100, 2, color);
        }
        UiIcon::Person => {
            gfx.dot(cx, cy - r * 36 / 100, (r * 32 / 100).max(2), color, 1024);
            gfx.panel(cx - r * 65 / 100, cy + r * 10 / 100, r * 130 / 100, r * 60 / 100, (r * 30 / 100).max(3), color, 255, 0, 0);
        }
        UiIcon::Photo => {
            gfx.panel(cx - r * 78 / 100, cy - r * 62 / 100, r * 156 / 100, r * 124 / 100, (r / 3).max(3), 0, 0, color, 255);
            gfx.dot(cx - r * 32 / 100, cy - r * 20 / 100, (r / 5).max(2), color, 1024);
            gfx.fill_rect(cx - r * 50 / 100, cy + r * 25 / 100, r, t.max(2), color);
        }
    }
}

/// Button semantic variants.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnKind {
    Primary,
    Secondary,
    Quiet,
    Destructive,
    Disabled,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_btn(
    gfx: &mut Gfx<'_>,
    tok: &SurfaceTokens,
    rect: UiRect,
    kind: BtnKind,
    pressed: bool,
    cap: i32,
    label: &str,
) {
    let r = (rect.h / 2).min(rect.w / 2);
    let (fill, border, ba, fg) = match kind {
        BtnKind::Primary => (
            if pressed { tok.accent_pressed } else { tok.accent },
            0,
            0,
            WHITE,
        ),
        BtnKind::Secondary => (
            if pressed { tok.pressed } else { tok.surface },
            tok.border,
            tok.border_alpha.max(26),
            tok.ink,
        ),
        BtnKind::Quiet => (
            if pressed { tok.pressed } else { tok.surface_subtle },
            0,
            0,
            tok.ink,
        ),
        BtnKind::Destructive => (
            if pressed { mix_color(tok.danger, 0, 20, 100) } else { tok.danger },
            0,
            0,
            WHITE,
        ),
        BtnKind::Disabled => (tok.surface_subtle, 0, 0, tok.subtle),
    };
    gfx.panel(rect.x, rect.y, rect.w, rect.h, r, fill, 255, border, ba);
    gfx.text_centered(rect.cx(), rect.y + (rect.h - cap) / 2, cap, fg, 2, label);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_icon_btn(
    gfx: &mut Gfx<'_>,
    tok: &SurfaceTokens,
    rect: UiRect,
    icon: UiIcon,
    active: bool,
    disabled: bool,
    pressed: bool,
) {
    let r = rect.w.min(rect.h) / 2;
    let (fill, border, ba, fg) = if disabled {
        (tok.surface_subtle, 0, 0, tok.subtle)
    } else if active {
        (
            if pressed { tok.accent_pressed } else { tok.accent },
            0,
            0,
            WHITE,
        )
    } else {
        (
            if pressed { tok.pressed } else { tok.surface },
            tok.border,
            tok.border_alpha,
            tok.ink,
        )
    };
    gfx.panel(rect.x, rect.y, rect.w, rect.h, r, fill, 255, border, ba);
    let ir = (rect.w.min(rect.h) * 26 / 100).max(5);
    draw_ui_icon(gfx, icon, rect.cx(), rect.cy(), ir, fg, fill);
}

/// Top navigation bar geometry (computed identically by draw & hit-test).
#[derive(Clone, Copy)]
pub struct TopBarGeo {
    pub bar: UiRect,
    pub back_btn: UiRect,
    pub trailing_btn: UiRect,
}

impl TopBarGeo {
    pub fn compute(vp: &Viewport, trailing_w_dp: i32) -> Self {
        let h = vp.header_h();
        let btn_s = vp.d(36).clamp(30, h - vp.d(8));
        let by = (h - btn_s) / 2;
        let back_btn = UiRect::new(vp.content_x, by, btn_s, btn_s);
        let tw = if trailing_w_dp > 0 { vp.d(trailing_w_dp).max(btn_s) } else { 0 };
        let trailing_btn = UiRect::new(vp.content_x + vp.content_w - tw, by, tw, btn_s);
        Self { bar: UiRect::new(0, 0, vp.area_w, h), back_btn, trailing_btn }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_top_bar(
    gfx: &mut Gfx<'_>,
    vp: &Viewport,
    tok: &SurfaceTokens,
    geo: &TopBarGeo,
    title: &str,
    show_back: bool,
    back_pressed: bool,
    app_icon: Option<u32>,
) {
    gfx.fill_rect(0, 0, vp.area_w, geo.bar.h, tok.bg);
    gfx.fill_rect(0, geo.bar.h - 1, vp.area_w, 1, tok.divider);
    let cap = vp.d(if show_back { 14 } else { 16 }).clamp(11, 20);
    let ty = (geo.bar.h - cap) / 2;
    if show_back {
        draw_icon_btn(gfx, tok, geo.back_btn, UiIcon::Back, false, false, back_pressed);
        let tx = geo.back_btn.right() + vp.d(10);
        let max_w = (vp.content_x + vp.content_w - tx - geo.trailing_btn.w - vp.d(8)).max(40);
        let mut trunc = [0u8; 48];
        let shown = truncate_to_px(title, max_w, cap, &mut trunc);
        gfx.text(tx, ty, cap, tok.ink, 2, shown);
    } else if let Some(icon_id) = app_icon {
        let isz = vp.d(28).clamp(22, geo.bar.h - vp.d(12));
        gfx.icon(icon_id, vp.content_x, (geo.bar.h - isz) / 2, isz, isz);
        let tx = vp.content_x + isz + vp.d(10);
        gfx.text(tx, ty, cap, tok.ink, 2, title);
    } else {
        gfx.text(vp.content_x, ty, cap, tok.ink, 2, title);
    }
}

/// Animated segmented tab bar (`pos_256` ranges `0..=(labels.len()-1)*256`).
pub fn draw_segmented_bar(
    gfx: &mut Gfx<'_>,
    tok: &SurfaceTokens,
    rect: UiRect,
    labels: &[&str],
    pos_256: i32,
    cap: i32,
) {
    let n = labels.len().max(1) as i32;
    let pad = 3;
    let r = rect.h / 2;
    gfx.panel(rect.x, rect.y, rect.w, rect.h, r, tok.surface_subtle, 255, tok.border, tok.border_alpha / 2);
    let seg_w = (rect.w - pad * 2) / n;
    let max_pos = (n - 1) * 256;
    let p = pos_256.clamp(0, max_pos);
    let thumb_x = rect.x + pad + (seg_w * p) / 256;
    let thumb_h = (rect.h - pad * 2).max(12);
    gfx.panel(thumb_x, rect.y + pad, seg_w, thumb_h, thumb_h / 2, tok.surface, 255, tok.border, tok.border_alpha);
    for (i, lbl) in labels.iter().enumerate() {
        let near = (256 - (i as i32 * 256 - p).abs()).clamp(0, 256);
        let fg = mix_color(tok.muted, tok.accent, near, 256);
        let cx = rect.x + pad + i as i32 * seg_w + seg_w / 2;
        gfx.text_centered(cx, rect.y + (rect.h - cap) / 2, cap, fg, 2, lbl);
    }
}

/// Quiet bottom navigation bar (up to 4 items).
pub fn draw_bottom_nav(
    gfx: &mut Gfx<'_>,
    vp: &Viewport,
    tok: &SurfaceTokens,
    items: &[(UiIcon, &str)],
    active: usize,
    pressed_idx: Option<usize>,
) -> UiRect {
    let h = vp.bottom_nav_h();
    let y = vp.area_h - h;
    let bar = UiRect::new(0, y, vp.area_w, h);
    gfx.fill_rect(0, y, vp.area_w, h, tok.surface);
    gfx.fill_rect(0, y, vp.area_w, 1, tok.divider);
    let n = items.len().max(1) as i32;
    let col_w = vp.content_w / n;
    let cap = vp.d(7).clamp(7, 9);
    let icon_r = vp.d(8).clamp(6, 10);
    for (i, &(icon, label)) in items.iter().enumerate() {
        let cx = vp.content_x + i as i32 * col_w + col_w / 2;
        let is_act = i == active;
        let is_prs = pressed_idx == Some(i);
        let pill_w = (col_w - vp.d(14)).clamp(vp.d(42), vp.d(68));
        let pill_h = vp.d(26).clamp(22, h / 2);
        let pill_y = y + vp.d(6);
        if is_act {
            gfx.panel(cx - pill_w / 2, pill_y, pill_w, pill_h, pill_h / 2, tok.accent, 34, 0, 0);
        } else if is_prs {
            gfx.panel(cx - pill_w / 2, pill_y, pill_w, pill_h, pill_h / 2, tok.pressed, 200, 0, 0);
        }
        let fg = if is_act { tok.accent } else { tok.muted };
        let cut = if is_act { mix_color(tok.surface, tok.accent, 14, 100) } else { tok.surface };
        draw_ui_icon(gfx, icon, cx, pill_y + pill_h / 2, icon_r, fg, cut);
        gfx.text_centered(cx, pill_y + pill_h + vp.d(3), cap, fg, if is_act { 2 } else { 1 }, label);
    }
    bar
}

/// Hit-test a bottom navigation bar item index.
pub fn hit_bottom_nav(vp: &Viewport, count: usize, x: i32, y: i32) -> Option<usize> {
    let h = vp.bottom_nav_h();
    let top = vp.area_h - h;
    if y < top || y >= vp.area_h || count == 0 {
        return None;
    }
    let rel = x - vp.content_x;
    if rel < 0 || rel >= vp.content_w {
        return None;
    }
    let idx = (rel * count as i32 / vp.content_w).clamp(0, count as i32 - 1) as usize;
    Some(idx)
}

/// Pill search bar with search glyph, placeholder/value, and clear button rect.
pub fn draw_search_bar(
    gfx: &mut Gfx<'_>,
    vp: &Viewport,
    tok: &SurfaceTokens,
    rect: UiRect,
    query: &str,
    placeholder: &str,
    active: bool,
) -> UiRect {
    let r = rect.h / 2;
    let border = if active { tok.accent } else { tok.border };
    let ba = if active { 220 } else { tok.border_alpha.max(24) };
    gfx.panel(rect.x, rect.y, rect.w, rect.h, r, tok.surface, 255, border, ba);
    let ir = vp.d(7).clamp(5, 9);
    draw_ui_icon(gfx, UiIcon::Search, rect.x + vp.d(18), rect.cy(), ir, if active { tok.accent } else { tok.muted }, tok.surface);
    let cap = vp.d(9).clamp(8, 12);
    let tx = rect.x + vp.d(34);
    let clear_r = UiRect::new(rect.right() - rect.h, rect.y, rect.h, rect.h);
    let max_text_w = (rect.w - vp.d(42) - if query.is_empty() { 0 } else { rect.h }).max(32);
    if query.is_empty() {
        let mut tb = [0u8; 48];
        let shown = truncate_to_px(placeholder, max_text_w, cap, &mut tb);
        gfx.text(tx, rect.y + (rect.h - cap) / 2, cap, tok.subtle, 1, shown);
    } else {
        let mut tb = [0u8; 48];
        let shown = truncate_to_px(query, max_text_w, cap, &mut tb);
        gfx.text(tx, rect.y + (rect.h - cap) / 2, cap, tok.ink, 1, shown);
        draw_ui_icon(gfx, UiIcon::Close, clear_r.cx(), clear_r.cy(), vp.d(5).max(4), tok.muted, tok.surface);
    }
    clear_r
}

/// Deterministic monogram avatar color palette (never random across renders).
const AVATAR_COLORS: [u32; 7] = [
    BLUE,
    TEAL,
    GREEN,
    PURPLE,
    0x00EA_580C,
    0x00DB_2777,
    0x004F_46E5,
];

pub fn deterministic_avatar_color(seed: &str, id: u32) -> u32 {
    let mut h: u32 = id.wrapping_mul(2654435761) ^ 0x811C_9DC5;
    for b in seed.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    AVATAR_COLORS[(h as usize) % AVATAR_COLORS.len()]
}

pub fn monogram_initial<'a>(name: &str, buf: &'a mut [u8; 4]) -> &'a str {
    let ch = name.trim().chars().next().unwrap_or('#');
    let upper = match ch {
        'i' => 'İ',
        'a'..='z' => (ch as u8 - 32) as char,
        'ı' => 'I',
        'ğ' => 'Ğ',
        'ü' => 'Ü',
        'ş' => 'Ş',
        'ö' => 'Ö',
        'ç' => 'Ç',
        other => other,
    };
    upper.encode_utf8(buf)
}

pub fn draw_monogram_avatar(gfx: &mut Gfx<'_>, cx: i32, cy: i32, radius: i32, name: &str, id: u32) {
    let col = deterministic_avatar_color(name, id);
    gfx.dot(cx, cy, radius, col, 1024);
    let mut buf = [0u8; 4];
    let init = monogram_initial(name, &mut buf);
    let cap = (radius * 72 / 100).clamp(8, 22);
    gfx.text_centered(cx, cy - cap / 2, cap, WHITE, 2, init);
}

/// Clean status/info badge.
#[allow(clippy::too_many_arguments)]
pub fn draw_status_badge(
    gfx: &mut Gfx<'_>,
    rect: UiRect,
    color: u32,
    cap: i32,
    label: &str,
) {
    let r = rect.h / 2;
    gfx.panel(rect.x, rect.y, rect.w, rect.h, r, color, 32, color, 130);
    gfx.text_centered(rect.cx(), rect.y + (rect.h - cap) / 2, cap, color, 2, label);
}

/// Form text field with floating label, focus ring, placeholder, and optional inline error.
#[allow(clippy::too_many_arguments)]
pub fn draw_text_field(
    gfx: &mut Gfx<'_>,
    vp: &Viewport,
    tok: &SurfaceTokens,
    rect: UiRect,
    label: &str,
    value: &str,
    placeholder: &str,
    focused: bool,
    error: Option<&str>,
) {
    let r = vp.d(12).clamp(8, 16);
    let has_err = error.is_some();
    let (border, ba) = if has_err {
        (tok.danger, 230)
    } else if focused {
        (tok.accent, 235)
    } else {
        (tok.border, tok.border_alpha.max(26))
    };
    gfx.panel(rect.x, rect.y, rect.w, rect.h, r, tok.surface, 255, border, ba);
    let lbl_cap = vp.d(7).clamp(7, 9);
    let val_cap = vp.d(10).clamp(9, 13);
    let pad_x = vp.d(12);
    let lbl_col = if has_err { tok.danger } else if focused { tok.accent } else { tok.muted };
    gfx.text(rect.x + pad_x, rect.y + vp.d(7), lbl_cap, lbl_col, 1, label);

    let max_w = (rect.w - pad_x * 2).max(40);
    let mut tb = [0u8; 64];
    if value.is_empty() {
        let shown = truncate_to_px(placeholder, max_w, val_cap, &mut tb);
        gfx.text(rect.x + pad_x, rect.y + vp.d(22), val_cap, tok.subtle, 1, shown);
    } else {
        let shown = truncate_tail_to_px(value, max_w, val_cap, &mut tb);
        gfx.text(rect.x + pad_x, rect.y + vp.d(22), val_cap, tok.ink, 1, shown);
    }
    if let Some(err_msg) = error {
        gfx.text(rect.x + pad_x, rect.bottom() + vp.d(3), vp.d(7).max(7), tok.danger, 1, err_msg);
    }
}

/// Intentional empty / permission / unavailable / error state panel.
/// Returns the exact `UiRect` of the primary action button (if `action_label.is_some()`).
#[allow(clippy::too_many_arguments)]
pub fn draw_state_view(
    gfx: &mut Gfx<'_>,
    vp: &Viewport,
    tok: &SurfaceTokens,
    area: UiRect,
    icon: UiIcon,
    tint: u32,
    title: &str,
    body: &str,
    action_label: Option<&str>,
    action_pressed: bool,
) -> Option<UiRect> {
    let pad = vp.d(20).max(14);
    let title_cap = vp.d(12).clamp(10, 15);
    let body_cap = vp.d(8).clamp(8, 10);
    let line_h = body_cap + vp.d(6);
    let text_w = (area.w - pad * 2 - vp.d(14)).max(110);
    let mut lines: [&str; 4] = [""; 4];
    let n_lines = wrap_text_px(body, text_w, body_cap, &mut lines);

    let icon_r = vp.d(20).clamp(16, 24);
    let btn_h = vp.d(40).clamp(34, 46);
    let needed_h = pad
        + icon_r * 2
        + vp.d(14)
        + title_cap
        + vp.d(10)
        + (n_lines as i32) * line_h
        + if action_label.is_some() { vp.d(18) + btn_h } else { 0 }
        + pad;
    let card_h = needed_h.min(area.h);
    let card_y = area.y + ((area.h - card_h) / 3).max(0);
    let card = UiRect::new(area.x, card_y, area.w, card_h);

    gfx.panel(card.x, card.y, card.w, card.h, vp.d(18).clamp(12, 22), tok.surface, 255, tok.border, tok.border_alpha);

    let mut cy = card.y + pad;
    gfx.dot(card.cx(), cy + icon_r, icon_r, tint, 32);
    draw_ui_icon(gfx, icon, card.cx(), cy + icon_r, icon_r * 52 / 100, tint, tok.surface);
    cy += icon_r * 2 + vp.d(14);

    gfx.text_centered(card.cx(), cy, title_cap, tok.ink, 2, title);
    cy += title_cap + vp.d(10);

    for line in &lines[..n_lines] {
        gfx.text_centered(card.cx(), cy, body_cap, tok.muted, 1, line);
        cy += line_h;
    }

    action_label.map(|lbl| {
        let bw = (card.w - pad * 2).min(vp.d(240));
        let bx = card.cx() - bw / 2;
        let by = (cy + vp.d(14)).min(card.bottom() - pad - btn_h);
        let br = UiRect::new(bx, by, bw, btn_h);
        draw_btn(gfx, tok, br, BtnKind::Primary, action_pressed, vp.d(9).clamp(8, 11), lbl);
        br
    })
}

/// Modal confirmation bottom sheet with animated progress `p_256` (`0..=256`).
/// Returns `(cancel_rect, confirm_rect)`.
#[allow(clippy::too_many_arguments)]
pub fn draw_confirm_sheet(
    gfx: &mut Gfx<'_>,
    vp: &Viewport,
    tok: &SurfaceTokens,
    p_256: i32,
    title: &str,
    message: &str,
    cancel_lbl: &str,
    confirm_lbl: &str,
    destructive: bool,
    pressed_btn: u8, // 0=none, 1=cancel, 2=confirm
) -> (UiRect, UiRect) {
    let p = p_256.clamp(0, 256);
    if p <= 0 {
        let z = UiRect::new(0, 0, 0, 0);
        return (z, z);
    }
    let scrim_a = (156 * p / 256) as u32;
    gfx.panel(0, 0, vp.area_w, vp.area_h, 0, tok.scrim, scrim_a, 0, 0);

    let pad = vp.d(18).max(12);
    let title_cap = vp.d(12).clamp(10, 14);
    let body_cap = vp.d(8).clamp(8, 10);
    let line_h = body_cap + vp.d(5);
    let sheet_w = vp.content_w.min(vp.d(360));
    let sheet_x = (vp.area_w - sheet_w) / 2;

    let mut lines: [&str; 3] = [""; 3];
    let n_lines = wrap_text_px(message, sheet_w - pad * 2, body_cap, &mut lines);
    let btn_h = vp.d(40).clamp(34, 46);
    let sheet_h = pad + title_cap + vp.d(10) + (n_lines as i32) * line_h + vp.d(18) + btn_h + pad;

    let slide = (256 - p) * (sheet_h + vp.d(16)) / 256;
    let sheet_y = vp.area_h - sheet_h - vp.d(10) + slide;
    let r = vp.d(20).clamp(14, 24);
    let card_a = (255 * p / 256).clamp(40, 255) as u32;
    gfx.panel(sheet_x, sheet_y, sheet_w, sheet_h, r, tok.elevated, card_a, tok.border, tok.border_alpha);

    let mut ty = sheet_y + pad;
    gfx.text(sheet_x + pad, ty, title_cap, tok.ink, 2, title);
    ty += title_cap + vp.d(10);
    for line in &lines[..n_lines] {
        gfx.text(sheet_x + pad, ty, body_cap, tok.muted, 1, line);
        ty += line_h;
    }

    let gap = vp.d(10).max(8);
    let bw = (sheet_w - pad * 2 - gap) / 2;
    let by = sheet_y + sheet_h - pad - btn_h;
    let cancel_r = UiRect::new(sheet_x + pad, by, bw, btn_h);
    let confirm_r = UiRect::new(sheet_x + pad + bw + gap, by, bw, btn_h);
    let bcap = vp.d(9).clamp(8, 11);
    draw_btn(gfx, tok, cancel_r, BtnKind::Quiet, pressed_btn == 1, bcap, cancel_lbl);
    draw_btn(
        gfx,
        tok,
        confirm_r,
        if destructive { BtnKind::Destructive } else { BtnKind::Primary },
        pressed_btn == 2,
        bcap,
        confirm_lbl,
    );
    (cancel_r, confirm_r)
}

/// Non-blocking floating toast pill.
pub fn draw_toast(gfx: &mut Gfx<'_>, vp: &Viewport, tok: &SurfaceTokens, y: i32, color: u32, text: &str) {
    if text.is_empty() {
        return;
    }
    let cap = vp.d(8).clamp(8, 10);
    let h = vp.d(30).clamp(26, 36);
    let est_w = estimate_text_px(text, cap) + vp.d(36);
    let w = est_w.clamp(vp.d(120), vp.content_w);
    let x = (vp.area_w - w) / 2;
    let r = h / 2;
    gfx.panel(x, y, w, h, r, tok.elevated, 248, color, 140);
    gfx.dot(x + vp.d(14), y + h / 2, vp.d(4).max(3), color, 1024);
    let mut tb = [0u8; 64];
    let shown = truncate_to_px(text, w - vp.d(30), cap, &mut tb);
    gfx.text(x + vp.d(24), y + (h - cap) / 2, cap, tok.ink, 1, shown);
}

/// Fast proportional text width estimation in pixels (zero syscalls, UTF-8 safe).
/// Calibrated conservatively against `nc-font` proportional metrics so wrapped text
/// never touches container edges.
pub fn estimate_text_px(text: &str, cap_height: i32) -> i32 {
    let cap = cap_height.max(6);
    let mut total: i32 = 0;
    for ch in text.chars() {
        let w = match ch {
            ' ' => cap * 38 / 100,
            'i' | 'l' | 'I' | 'ı' | 'İ' | '!' | '.' | ',' | ':' | ';' | '|' => cap * 40 / 100,
            'f' | 'j' | 'r' | 't' | '(' | ')' | '-' => cap * 52 / 100,
            'm' | 'w' | 'M' | 'W' => cap * 94 / 100,
            'A'..='Z' | 'Ç' | 'Ğ' | 'Ö' | 'Ş' | 'Ü' => cap * 78 / 100,
            _ => cap * 68 / 100,
        };
        total += w + (cap * 15 / 100).max(1);
    }
    total
}

/// Word-wrap `text` into up to `out.len()` slices fitting within `max_px` at `cap_height`.
/// UTF-8 safe (preserves all Turkish characters and never splits inside a multi-byte char).
pub fn wrap_text_px<'a>(text: &'a str, max_px: i32, cap_height: i32, out: &mut [&'a str]) -> usize {
    if text.is_empty() || out.is_empty() || max_px <= 0 {
        return 0;
    }
    let mut count = 0usize;
    let mut rest = text.trim();
    while !rest.is_empty() && count < out.len() {
        if estimate_text_px(rest, cap_height) <= max_px || count + 1 == out.len() {
            out[count] = rest;
            count += 1;
            break;
        }
        let mut best_space: Option<usize> = None;
        let mut last_fit = 0usize;
        for (idx, ch) in rest.char_indices() {
            let end = idx + ch.len_utf8();
            if estimate_text_px(&rest[..end], cap_height) > max_px {
                break;
            }
            last_fit = end;
            if ch == ' ' {
                best_space = Some(idx);
            }
        }
        let split = match best_space {
            Some(sp) if sp > 0 => sp,
            _ => last_fit.max(rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1)),
        };
        out[count] = rest[..split].trim_end();
        count += 1;
        rest = rest[split..].trim_start();
    }
    count
}

/// Truncate `text` to fit within `max_px`, appending `…` if clipped.
pub fn truncate_to_px<'a>(text: &str, max_px: i32, cap_height: i32, buf: &'a mut [u8]) -> &'a str {
    if estimate_text_px(text, cap_height) <= max_px && text.len() <= buf.len() {
        buf[..text.len()].copy_from_slice(text.as_bytes());
        return core::str::from_utf8(&buf[..text.len()]).unwrap_or("");
    }
    let ell = "...";
    let ell_w = estimate_text_px(ell, cap_height);
    let budget = (max_px - ell_w).max(0);
    let mut end = 0usize;
    for (idx, ch) in text.char_indices() {
        let next = idx + ch.len_utf8();
        if next + ell.len() > buf.len() || estimate_text_px(&text[..next], cap_height) > budget {
            break;
        }
        end = next;
    }
    buf[..end].copy_from_slice(&text.as_bytes()[..end]);
    let ell_len = ell.len().min(buf.len() - end);
    buf[end..end + ell_len].copy_from_slice(&ell.as_bytes()[..ell_len]);
    core::str::from_utf8(&buf[..end + ell_len]).unwrap_or("")
}

/// Keep the trailing portion of `text` visible inside an active single-line text input.
pub fn truncate_tail_to_px<'a>(text: &str, max_px: i32, cap_height: i32, buf: &'a mut [u8]) -> &'a str {
    if estimate_text_px(text, cap_height) <= max_px && text.len() <= buf.len() {
        buf[..text.len()].copy_from_slice(text.as_bytes());
        return core::str::from_utf8(&buf[..text.len()]).unwrap_or("");
    }
    let mut start = text.len();
    for (idx, _) in text.char_indices().rev() {
        let slice = &text[idx..];
        if slice.len() > buf.len() || estimate_text_px(slice, cap_height) > max_px {
            break;
        }
        start = idx;
    }
    let slice = &text[start..];
    buf[..slice.len()].copy_from_slice(slice.as_bytes());
    core::str::from_utf8(&buf[..slice.len()]).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_handles_turkish_utf8_cleanly() {
        let text = "Kayıtlı kişilerinizi görüntüleme ve arama izni verilsin mi?";
        let mut out = [""; 4];
        let n = wrap_text_px(text, 140, 10, &mut out);
        assert!(n >= 2 && n <= 4);
        for line in &out[..n] {
            assert!(!line.is_empty());
        }
    }

    #[test]
    fn viewport_adapts_to_portrait_compact_and_landscape() {
        let portrait = Viewport::new(360, 700);
        assert!(!portrait.is_landscape);
        assert!(!portrait.is_compact_h);
        assert!(portrait.content_w > 300);

        let compact = Viewport::new(320, 380);
        assert!(compact.is_compact_h);
        assert!(compact.usable_bottom(true) < compact.area_h);

        let landscape = Viewport::new(800, 380);
        assert!(landscape.is_landscape);
        assert!(landscape.content_w > 360);
    }

    #[test]
    fn deterministic_monogram_is_stable_and_turkish_aware() {
        let c1 = deterministic_avatar_color("İsmail Şahin", 7);
        let c2 = deterministic_avatar_color("İsmail Şahin", 7);
        assert_eq!(c1, c2);
        let mut b = [0u8; 4];
        assert_eq!(monogram_initial("ismail", &mut b), "İ");
        assert_eq!(monogram_initial("şahin", &mut b), "Ş");
        assert_eq!(monogram_initial("ırmak", &mut b), "I");
    }

    #[test]
    fn camera_theme_stays_dark_in_os_light_mode_with_live_accent_and_lang() {
        let light_ev = nc_app_sdk::nc_abi::event::EventBlock {
            flags: 0, // dark_theme bit = 0 (light OS)
            accent: 0x00FA_7D00,
            lang: 1,
            area_w: 360,
            area_h: 720,
            ..Default::default()
        };
        let normal = MobileTheme::from_event(&light_ev);
        assert!(!normal.dark);
        assert_eq!(normal.accent, 0x00FA_7D00);
        assert!(normal.en);

        let cam = MobileTheme::camera_dark(&light_ev);
        assert!(cam.dark);
        assert_eq!(cam.accent, 0x00FA_7D00);
        assert!(cam.en);
        let tok = SurfaceTokens::camera_black(cam.accent);
        assert_eq!(tok.bg, 0x0000_0000);
        assert_eq!(tok.accent, 0x00FA_7D00);
    }

    #[test]
    fn shared_components_emit_valid_bounded_streams_across_viewports() {
        let viewports = [
            (280, 520), // narrow phone
            (360, 720), // standard portrait
            (420, 840), // wide phone
            (360, 340), // short usable height
            (800, 360), // landscape
        ];
        for (w, h) in viewports {
            for dark in [false, true] {
                let ev = nc_app_sdk::nc_abi::event::EventBlock {
                    flags: if dark { 1u32 << 31 } else { 0 },
                    accent: BLUE,
                    lang: 0,
                    area_w: w,
                    area_h: h,
                    ..Default::default()
                };
                let th = MobileTheme::from_event(&ev);
                let tok = th.tokens();
                let vp = Viewport::from_event(&ev);
                let mut buf = [0u8; 16384];
                let mut gfx = Gfx::new(&mut buf);

                gfx.fill_rect(0, 0, vp.area_w, vp.area_h, tok.bg);
                let top = TopBarGeo::compute(&vp, 36);
                draw_top_bar(&mut gfx, &vp, &tok, &top, "Kişiler", true, false, None);
                draw_search_bar(&mut gfx, &vp, &tok, vp.row_rect(top.bar.bottom() + 8, 36), "Ayşe", "Ara...", true);
                draw_segmented_bar(&mut gfx, &tok, vp.row_rect(top.bar.bottom() + 52, 32), &["Tümü", "Favoriler"], 128, 9);
                draw_monogram_avatar(&mut gfx, vp.content_x + 24, top.bar.bottom() + 110, 16, "Şule Çelik", 3);
                draw_text_field(&mut gfx, &vp, &tok, vp.row_rect(top.bar.bottom() + 140, 48), "Ad", "Ömer", "Yaz...", true, Some("Gerekli"));
                draw_bottom_nav(
                    &mut gfx,
                    &vp,
                    &tok,
                    &[
                        (UiIcon::Keypad, "Tuşlar"),
                        (UiIcon::Clock, "Geçmiş"),
                        (UiIcon::Person, "Kişiler"),
                        (UiIcon::Star, "Favori"),
                    ],
                    0,
                    None,
                );
                draw_toast(&mut gfx, &vp, &tok, top.bar.bottom() + 6, GREEN, "Kaydedildi");
                draw_confirm_sheet(&mut gfx, &vp, &tok, 256, "Sil", "Bu kayıt silinecek.", "İptal", "Sil", true, 0);

                assert!(!gfx.dropped(), "stream dropped for viewport {}x{}", w, h);
                let summary = nc_app_sdk::nc_abi::draw::validate(gfx.as_bytes()).expect("valid draw stream");
                assert!(summary.records < 140, "expected lean record count, got {}", summary.records);
            }
        }
    }
}
