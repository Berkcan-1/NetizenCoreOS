//! NetizenCore Independent Ring 3 / EL0 System App: Camera (`nc.camera`).
//!
//! - Dedicated black/near-black professional camera theme in BOTH OS light and dark modes,
//!   retaining the live OS accent color and TR/EN localization.
//! - Responsive viewfinder-first layout (portrait, compact, and landscape) where the
//!   viewfinder occupies the majority of the usable app viewport.
//! - Requests `camera` runtime permission BEFORE any hardware probe or device open.
//! - NON-NEGOTIABLE RULE: When camera hardware/driver is absent, the viewfinder MUST
//!   remain pure black (`0x0000_0000`) with zero fake imagery, gradients, or mock grids,
//!   and capture MUST remain disabled.
#![no_std]
#![no_main]

use nc_abi::event::{
    EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE, RET_ANIMATING, RET_CLOSE,
    RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_abi::perm::{PermState, PERM_CAMERA};
use nc_abi::sys as wire;
use nc_app_sdk::camera::{self, CameraStatus};
use nc_app_sdk::{dispatch, perm, pointer, status, sys, Gfx, RingApp};
use nc_mobile_ui::{
    draw_btn, draw_icon_btn, draw_state_view, draw_status_badge, draw_toast, draw_top_bar,
    BtnKind, MobileTheme, SurfaceTokens, ToastState, TopBarGeo, Tween, UiIcon, UiRect, Viewport,
    AMBER, CORAL, ONE, SPEC_PAGE, SPEC_PRESS, WHITE,
};

const PURE_BLACK: u32 = 0x0000_0000;
const CHROME_BG: u32 = 0x0009_0B0E;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Viewfinder,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AspectMode {
    Full,
    Ratio4x3,
    Ratio1x1,
}

impl AspectMode {
    fn next(self) -> Self {
        match self {
            Self::Full => Self::Ratio4x3,
            Self::Ratio4x3 => Self::Ratio1x1,
            Self::Ratio1x1 => Self::Full,
        }
    }

    fn label(self, en: bool) -> &'static str {
        match self {
            Self::Full => if en { "Full" } else { "Tam" },
            Self::Ratio4x3 => "4:3",
            Self::Ratio1x1 => "1:1",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TimerMode {
    Off,
    Sec3,
    Sec10,
}

impl TimerMode {
    fn next(self) -> Self {
        match self {
            Self::Off => Self::Sec3,
            Self::Sec3 => Self::Sec10,
            Self::Sec10 => Self::Off,
        }
    }

    fn label(self, en: bool) -> &'static str {
        match self {
            Self::Off => if en { "Off" } else { "Yok" },
            Self::Sec3 => "3s",
            Self::Sec10 => "10s",
        }
    }
}

/// Shared computed layout for both `draw` and hit-testing (zero coordinate drift).
struct CameraLayout {
    top_strip: UiRect,
    vf_outer: UiRect,
    vf_frame: UiRect,
    bottom_deck: UiRect,
    btn_grid: UiRect,
    btn_timer: UiRect,
    btn_aspect: UiRect,
    btn_settings: UiRect,
    mode_photo: UiRect,
    mode_video: UiRect,
    shutter: UiRect,
    info_btn: UiRect,
}

impl CameraLayout {
    fn compute(vp: &Viewport, aspect: AspectMode) -> Self {
        let btn_s = vp.d(36).clamp(30, 44);
        if vp.is_landscape {
            let rail_w = vp.d(104).clamp(84, 136);
            let vf_outer = UiRect::new(0, 0, (vp.area_w - rail_w).max(120), vp.area_h);
            let vf_frame = fit_aspect(vf_outer, aspect);
            let rx = vf_outer.right();
            let top_strip = UiRect::new(rx, 0, rail_w, vp.d(48));
            let bottom_deck = UiRect::new(rx, 0, rail_w, vp.area_h);

            let gap = vp.d(8);
            let col_cx = rx + rail_w / 2;
            let btn_grid = UiRect::new(col_cx - btn_s - gap / 2, vp.d(12), btn_s, btn_s);
            let btn_settings = UiRect::new(col_cx + gap / 2, vp.d(12), btn_s, btn_s);
            let btn_timer = UiRect::new(col_cx - btn_s - gap / 2, btn_grid.bottom() + gap, btn_s, btn_s);
            let btn_aspect = UiRect::new(col_cx + gap / 2, btn_grid.bottom() + gap, btn_s, btn_s);

            let shutter_s = vp.d(58).clamp(48, 68);
            let shutter = UiRect::new(col_cx - shutter_s / 2, vp.area_h / 2 - shutter_s / 2 + vp.d(10), shutter_s, shutter_s);
            let mode_photo = UiRect::new(rx + vp.d(8), vp.area_h - vp.d(42), rail_w - vp.d(16), vp.d(28));
            let mode_video = UiRect::new(0, 0, 0, 0);
            let info_btn = UiRect::new(col_cx - btn_s / 2, shutter.bottom() + vp.d(10), btn_s, btn_s);

            return Self {
                top_strip,
                vf_outer,
                vf_frame,
                bottom_deck,
                btn_grid,
                btn_timer,
                btn_aspect,
                btn_settings,
                mode_photo,
                mode_video,
                shutter,
                info_btn,
            };
        }

        // Portrait: viewfinder-first composition (~66-75% of usable viewport height!)
        let top_h = if vp.is_compact_h { vp.d(42).max(36) } else { vp.d(48).max(42) };
        let deck_h = if vp.is_compact_h { vp.d(98).max(86) } else { vp.d(122).max(106) };
        let top_strip = UiRect::new(0, 0, vp.area_w, top_h);
        let bottom_deck = UiRect::new(0, (vp.area_h - deck_h).max(top_h + 80), vp.area_w, deck_h);
        let vf_outer = UiRect::new(0, top_strip.bottom(), vp.area_w, (bottom_deck.y - top_strip.bottom()).max(80));
        let vf_frame = fit_aspect(vf_outer, aspect);

        // 4 evenly spaced top controls
        let ty = (top_h - btn_s) / 2;
        let pad = vp.margin_x;
        let span = (vp.area_w - pad * 2 - btn_s * 4).max(12);
        let step = btn_s + span / 3;
        let btn_grid = UiRect::new(pad, ty, btn_s, btn_s);
        let btn_timer = UiRect::new(pad + step, ty, vp.d(46).max(btn_s), btn_s);
        let btn_aspect = UiRect::new(pad + step * 2, ty, vp.d(46).max(btn_s), btn_s);
        let btn_settings = UiRect::new(vp.area_w - pad - btn_s, ty, btn_s, btn_s);

        // Mode strip at top of capture deck
        let mode_h = vp.d(26).clamp(22, 30);
        let mode_y = bottom_deck.y + vp.d(8);
        let mode_w = vp.d(74).clamp(62, 92);
        let mode_gap = vp.d(8);
        let mode_photo = UiRect::new(vp.area_w / 2 - mode_w - mode_gap / 2, mode_y, mode_w, mode_h);
        let mode_video = UiRect::new(vp.area_w / 2 + mode_gap / 2, mode_y, mode_w, mode_h);

        // Shutter row
        let shutter_s = if vp.is_compact_h { vp.d(54).clamp(44, 60) } else { vp.d(64).clamp(54, 72) };
        let row_cy = mode_photo.bottom() + (bottom_deck.bottom() - mode_photo.bottom()) / 2;
        let shutter = UiRect::new(vp.area_w / 2 - shutter_s / 2, row_cy - shutter_s / 2, shutter_s, shutter_s);
        let info_btn = UiRect::new(vp.area_w - pad - btn_s, row_cy - btn_s / 2, btn_s, btn_s);

        Self {
            top_strip,
            vf_outer,
            vf_frame,
            bottom_deck,
            btn_grid,
            btn_timer,
            btn_aspect,
            btn_settings,
            mode_photo,
            mode_video,
            shutter,
            info_btn,
        }
    }
}

fn fit_aspect(outer: UiRect, aspect: AspectMode) -> UiRect {
    match aspect {
        AspectMode::Full => outer,
        AspectMode::Ratio4x3 => {
            // Portrait 3:4 or landscape 4:3
            let (rw, rh) = if outer.w >= outer.h { (4, 3) } else { (3, 4) };
            let w_by_h = outer.h * rw / rh;
            let (fw, fh) = if w_by_h <= outer.w {
                (w_by_h, outer.h)
            } else {
                (outer.w, outer.w * rh / rw)
            };
            UiRect::new(outer.x + (outer.w - fw) / 2, outer.y + (outer.h - fh) / 2, fw.max(40), fh.max(40))
        }
        AspectMode::Ratio1x1 => {
            let side = outer.w.min(outer.h).max(40);
            UiRect::new(outer.x + (outer.w - side) / 2, outer.y + (outer.h - side) / 2, side, side)
        }
    }
}

struct CameraApp {
    screen: Screen,
    cam_perm: PermState,
    prompted: bool,
    hw_status: CameraStatus,
    stream_open: bool,
    grid_overlay: bool,
    aspect: AspectMode,
    timer: TimerMode,
    reduced_motion: bool,
    shutter_down: bool,
    pressed_id: u8,
    shutter_tween: Tween,
    settings_tween: Tween,
    toast: ToastState,
    last_flags: u32,
    last_accent: u32,
    last_lang: u32,
}

static mut COMMANDS: [u8; 16384] = [0; 16384];

impl CameraApp {
    const fn new() -> Self {
        Self {
            screen: Screen::Viewfinder,
            cam_perm: PermState::Ask,
            prompted: false,
            hw_status: CameraStatus { camera_available: false, mic_available: false },
            stream_open: false,
            grid_overlay: false,
            aspect: AspectMode::Full,
            timer: TimerMode::Off,
            reduced_motion: false,
            shutter_down: false,
            pressed_id: 0,
            shutter_tween: Tween::new(),
            settings_tween: Tween::new(),
            toast: ToastState::new(),
            last_flags: 0,
            last_accent: 0,
            last_lang: 0,
        }
    }

    fn sync_permission_and_probe(&mut self, ask_if_needed: bool) {
        let st = perm::check(PERM_CAMERA);
        self.cam_perm = st.state;
        if ask_if_needed && self.cam_perm == PermState::Ask && !self.prompted && !st.pending {
            self.prompted = true;
            let _ = perm::request(PERM_CAMERA);
            return;
        }
        if self.cam_perm == PermState::Granted {
            self.hw_status = camera::status();
            if self.hw_status.camera_available {
                match camera::open() {
                    Ok(()) => self.stream_open = true,
                    Err(_) => self.stream_open = false,
                }
            } else {
                self.stream_open = false;
            }
        } else {
            self.hw_status = CameraStatus::default();
            self.stream_open = false;
        }
    }

    fn step_animations(&mut self, now_ms: u32) -> bool {
        let st = self.shutter_tween.step(
            now_ms,
            if self.shutter_down && self.stream_open { ONE } else { 0 },
            &SPEC_PRESS,
            self.reduced_motion,
        );
        let _ = st;
        self.settings_tween.step(
            now_ms,
            if self.screen == Screen::Settings { ONE } else { 0 },
            &SPEC_PAGE,
            self.reduced_motion,
        );
        !self.shutter_tween.settled() || !self.settings_tween.settled()
    }

    fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        // Camera ALWAYS uses dedicated black theme even in OS light mode, preserving live accent & language.
        let th = MobileTheme::camera_dark(ev);
        let tok = SurfaceTokens::camera_black(th.accent);
        let vp = Viewport::from_event(ev);

        gfx.fill_rect(0, 0, vp.area_w, vp.area_h, PURE_BLACK);

        if self.cam_perm != PermState::Granted {
            let top = TopBarGeo::compute(&vp, 0);
            draw_top_bar(gfx, &vp, &tok, &top, th.tr("Kamera", "Camera"), false, false, None);
            let area = UiRect::new(
                vp.content_x,
                top.bar.bottom() + vp.d(12),
                vp.content_w,
                (vp.area_h - top.bar.bottom() - vp.d(24)).max(160),
            );
            let denied = self.cam_perm == PermState::Denied;
            draw_state_view(
                gfx,
                &vp,
                &tok,
                area,
                UiIcon::Camera,
                if denied { CORAL } else { tok.accent },
                if denied {
                    th.tr("Kamera izni reddedildi", "Camera permission denied")
                } else {
                    th.tr("Kamera erişimi gerekli", "Camera access required")
                },
                if denied {
                    th.tr(
                        "Kamera donanımını kullanmak için Ayarlar > Uygulama İzinleri bölümünden izin verin.",
                        "Enable Camera access in Settings > App Permissions to use the camera.",
                    )
                } else {
                    th.tr(
                        "Kamera donanımı yalnızca açık onayınızdan sonra denetlenir ve açılır.",
                        "Camera hardware is probed and opened only after your explicit approval.",
                    )
                },
                Some(if denied {
                    th.tr("Ayarları Aç", "Open Settings")
                } else {
                    th.tr("İzin İste", "Allow Camera")
                }),
                self.pressed_id == 99,
            );
            return;
        }

        if self.screen == Screen::Settings {
            self.draw_settings(gfx, &vp, &th, &tok);
            return;
        }

        let lay = CameraLayout::compute(&vp, self.aspect);

        // 1. PURE BLACK viewfinder surface (NEVER fake preview, gradients, or mock grids)
        gfx.fill_rect(lay.vf_outer.x, lay.vf_outer.y, lay.vf_outer.w, lay.vf_outer.h, PURE_BLACK);

        // Subtle framing border ONLY when aspect ratio is cropped and live stream is active
        if self.stream_open && self.aspect != AspectMode::Full {
            gfx.panel(
                lay.vf_frame.x,
                lay.vf_frame.y,
                lay.vf_frame.w,
                lay.vf_frame.h,
                2,
                0,
                0,
                WHITE,
                38,
            );
        }

        // Rule-of-thirds grid ONLY when a real camera stream is open
        if self.stream_open && self.grid_overlay {
            let f = lay.vf_frame;
            gfx.fill_rect(f.x + f.w / 3, f.y, 1, f.h, 0x0032_363E);
            gfx.fill_rect(f.x + f.w * 2 / 3, f.y, 1, f.h, 0x0032_363E);
            gfx.fill_rect(f.x, f.y + f.h / 3, f.w, 1, 0x0032_363E);
            gfx.fill_rect(f.x, f.y + f.h * 2 / 3, f.w, 1, 0x0032_363E);
        }

        // Calm, minimal truthful status over the pure-black viewfinder when hardware/driver is absent
        if !self.stream_open {
            let cap = vp.d(10).clamp(9, 13);
            gfx.text_centered(
                lay.vf_outer.cx(),
                lay.vf_outer.cy() - cap / 2,
                cap,
                0x007C_8494,
                1,
                th.tr("Kamera kullanılamıyor", "Camera unavailable"),
            );
        }

        // 2. Top control strip (portrait) / capture rail background (landscape)
        if !vp.is_landscape {
            gfx.fill_rect(lay.top_strip.x, lay.top_strip.y, lay.top_strip.w, lay.top_strip.h, CHROME_BG);
            gfx.fill_rect(0, lay.top_strip.bottom() - 1, vp.area_w, 1, tok.divider);
            gfx.fill_rect(lay.bottom_deck.x, lay.bottom_deck.y, lay.bottom_deck.w, lay.bottom_deck.h, CHROME_BG);
            gfx.fill_rect(0, lay.bottom_deck.y, vp.area_w, 1, tok.divider);
        } else {
            gfx.fill_rect(lay.bottom_deck.x, lay.bottom_deck.y, lay.bottom_deck.w, lay.bottom_deck.h, CHROME_BG);
            gfx.fill_rect(lay.bottom_deck.x, 0, 1, vp.area_h, tok.divider);
        }

        // Top controls: Grid, Timer, Aspect, Settings
        draw_icon_btn(gfx, &tok, lay.btn_grid, UiIcon::Grid, self.grid_overlay, !self.stream_open, self.pressed_id == 1);
        let pill_cap = vp.d(8).clamp(7, 10);
        let timer_active = self.timer != TimerMode::Off;
        draw_btn(
            gfx,
            &tok,
            lay.btn_timer,
            if !self.stream_open { BtnKind::Disabled } else if timer_active { BtnKind::Primary } else { BtnKind::Quiet },
            self.pressed_id == 2,
            pill_cap,
            self.timer.label(th.en),
        );
        draw_btn(
            gfx,
            &tok,
            lay.btn_aspect,
            if self.aspect != AspectMode::Full { BtnKind::Primary } else { BtnKind::Quiet },
            self.pressed_id == 3,
            pill_cap,
            self.aspect.label(th.en),
        );
        draw_icon_btn(gfx, &tok, lay.btn_settings, UiIcon::Settings, false, false, self.pressed_id == 4);

        // Mode selector (Photo active; Video honestly marked unavailable)
        let mcap = vp.d(8).clamp(8, 10);
        draw_status_badge(gfx, lay.mode_photo, tok.accent, mcap, th.tr("Fotoğraf", "Photo"));
        if lay.mode_video.w > 0 {
            gfx.text_centered(
                lay.mode_video.cx(),
                lay.mode_video.y + (lay.mode_video.h - mcap) / 2,
                mcap,
                tok.subtle,
                1,
                th.tr("Video", "Video"),
            );
        }

        // Shutter button (large, centered, clearly disabled when !stream_open)
        let cx = lay.shutter.cx();
        let cy = lay.shutter.cy();
        let outer_r = lay.shutter.w / 2;
        let shrink = self.shutter_tween.value() * vp.d(4) / ONE;
        let inner_r = (outer_r - vp.d(6).max(4) - shrink).max(12);
        if self.stream_open {
            gfx.dot(cx, cy, outer_r, WHITE, 1024);
            gfx.dot(cx, cy, outer_r - vp.d(3).max(2), CHROME_BG, 1024);
            gfx.dot(cx, cy, inner_r, if self.shutter_down { tok.accent } else { WHITE }, 1024);
        } else {
            gfx.dot(cx, cy, outer_r, tok.surface_subtle, 1024);
            gfx.dot(cx, cy, outer_r - vp.d(3).max(2), CHROME_BG, 1024);
            gfx.dot(cx, cy, inner_r, tok.divider, 1024);
        }

        // Left hardware status pill (portrait only) & right Settings/Info button
        if !vp.is_landscape {
            let st_w = vp.d(76).clamp(62, 92);
            let st_h = vp.d(26).clamp(22, 30);
            let st_r = UiRect::new(vp.margin_x, cy - st_h / 2, st_w, st_h);
            if self.stream_open {
                draw_status_badge(gfx, st_r, tok.success, vp.d(7).max(7), th.tr("Hazır", "Ready"));
            } else {
                draw_status_badge(gfx, st_r, AMBER, vp.d(7).max(7), th.tr("Sürücü Yok", "No Driver"));
            }
        }
        draw_icon_btn(gfx, &tok, lay.info_btn, UiIcon::Info, false, false, self.pressed_id == 5);

        // Ephemeral non-blocking toast (placed cleanly below top strip)
        if self.toast.active {
            let ty = if vp.is_landscape { vp.d(12) } else { lay.top_strip.bottom() + vp.d(10) };
            draw_toast(gfx, &vp, &tok, ty, self.toast.color, self.toast.text());
        }
    }

    fn draw_settings(&self, gfx: &mut Gfx<'_>, vp: &Viewport, th: &MobileTheme, tok: &SurfaceTokens) {
        let top = TopBarGeo::compute(vp, 0);
        draw_top_bar(
            gfx,
            vp,
            tok,
            &top,
            th.tr("Kamera Ayarları", "Camera Settings"),
            true,
            self.pressed_id == 10,
            None,
        );

        let mut y = top.bar.bottom() + vp.d(14);
        let row_h = vp.d(48).clamp(40, 56);
        let cap = vp.d(10).clamp(9, 12);
        let sub_cap = vp.d(8).clamp(7, 10);

        // Preferences section
        let pref_h = row_h * 3;
        let pref_box = vp.row_rect(y, pref_h);
        gfx.panel(pref_box.x, pref_box.y, pref_box.w, pref_box.h, vp.d(16), tok.surface, 255, tok.border, tok.border_alpha);

        let rows: [(&str, &str, bool); 3] = [
            (
                th.tr("Üçte Bir Izgarası", "Rule of Thirds Grid"),
                if self.grid_overlay { th.tr("Açık", "On") } else { th.tr("Kapalı", "Off") },
                self.grid_overlay,
            ),
            (
                th.tr("Zamanlayıcı", "Self-Timer"),
                self.timer.label(th.en),
                self.timer != TimerMode::Off,
            ),
            (
                th.tr("Çerçeve Oranı", "Aspect Ratio"),
                self.aspect.label(th.en),
                self.aspect != AspectMode::Full,
            ),
        ];
        for (i, &(lbl, val, accent_val)) in rows.iter().enumerate() {
            let ry = y + i as i32 * row_h;
            if i > 0 {
                gfx.fill_rect(pref_box.x + vp.d(14), ry, pref_box.w - vp.d(28), 1, tok.divider);
            }
            gfx.text(pref_box.x + vp.d(14), ry + (row_h - cap) / 2, cap, tok.ink, 1, lbl);
            let vw = vp.d(68);
            let vh = vp.d(26).clamp(22, 30);
            let vr = UiRect::new(pref_box.right() - vp.d(14) - vw, ry + (row_h - vh) / 2, vw, vh);
            draw_status_badge(gfx, vr, if accent_val { tok.accent } else { tok.muted }, sub_cap, val);
        }
        y = pref_box.bottom() + vp.d(14);

        // Truthful hardware diagnostics card
        let diag_h = vp.d(126).clamp(108, 144);
        let diag_box = vp.row_rect(y, diag_h);
        gfx.panel(diag_box.x, diag_box.y, diag_box.w, diag_box.h, vp.d(16), tok.surface, 255, tok.border, tok.border_alpha);
        gfx.text(diag_box.x + vp.d(14), diag_box.y + vp.d(12), cap, tok.ink, 2, th.tr("Donanım Durumu", "Hardware Status"));

        let diag_rows: [(&str, &str, u32); 3] = [
            (
                th.tr("Kamera Sensörü", "Camera Sensor"),
                if self.hw_status.camera_available { th.tr("Algılandı", "Detected") } else { th.tr("Yok / Sürücü Yok", "Unavailable") },
                if self.hw_status.camera_available { tok.success } else { AMBER },
            ),
            (
                th.tr("Video / Mikrofon", "Video / Microphone"),
                if self.hw_status.mic_available { th.tr("Destekleniyor", "Supported") } else { th.tr("Desteklenmiyor", "Unsupported") },
                if self.hw_status.mic_available { tok.success } else { tok.muted },
            ),
            (
                th.tr("Kamera İzni", "Camera Permission"),
                th.tr("İzin Verildi", "Granted"),
                tok.success,
            ),
        ];
        let mut dy = diag_box.y + vp.d(38);
        for (lbl, val, col) in diag_rows {
            gfx.text(diag_box.x + vp.d(14), dy, sub_cap, tok.muted, 1, lbl);
            gfx.text(diag_box.x + diag_box.w / 2, dy, sub_cap, col, 2, val);
            dy += vp.d(26).max(20);
        }

        let btn_y = (diag_box.bottom() + vp.d(14)).min(vp.area_h - vp.d(48));
        if btn_y + vp.d(38) <= vp.area_h - vp.d(4) {
            let pr = vp.row_rect(btn_y, vp.d(40).clamp(34, 44));
            draw_btn(
                gfx,
                tok,
                pr,
                BtnKind::Secondary,
                self.pressed_id == 14,
                vp.d(9).clamp(8, 11),
                th.tr("Uygulama İzinleri", "App Permissions"),
            );
        }
    }

    fn hit_id(&self, ev: &EventBlock, x: i32, y: i32) -> u8 {
        let vp = Viewport::from_event(ev);
        let pad = vp.d(6);
        if self.cam_perm != PermState::Granted {
            return 99;
        }
        if self.screen == Screen::Settings {
            let top = TopBarGeo::compute(&vp, 0);
            if top.back_btn.expand(pad).contains(x, y) {
                return 10;
            }
            let y0 = top.bar.bottom() + vp.d(14);
            let row_h = vp.d(48).clamp(40, 56);
            for i in 0..3 {
                if vp.row_rect(y0 + i as i32 * row_h, row_h).contains(x, y) {
                    return 11 + i as u8;
                }
            }
            let diag_y = y0 + row_h * 3 + vp.d(14);
            let diag_h = vp.d(126).clamp(108, 144);
            let btn_y = (diag_y + diag_h + vp.d(14)).min(vp.area_h - vp.d(48));
            if vp.row_rect(btn_y, vp.d(40).clamp(34, 44)).expand(pad).contains(x, y) {
                return 14;
            }
            return 0;
        }
        let lay = CameraLayout::compute(&vp, self.aspect);
        if lay.btn_grid.expand(pad).contains(x, y) { return 1; }
        if lay.btn_timer.expand(pad).contains(x, y) { return 2; }
        if lay.btn_aspect.expand(pad).contains(x, y) { return 3; }
        if lay.btn_settings.expand(pad).contains(x, y) { return 4; }
        if lay.info_btn.expand(pad).contains(x, y) { return 5; }
        if lay.mode_video.w > 0 && lay.mode_video.expand(pad).contains(x, y) { return 6; }
        if lay.shutter.expand(pad).contains(x, y) { return 7; }
        0
    }

    fn handle_click(&mut self, ev: &EventBlock, x: i32, y: i32) -> u32 {
        let vp = Viewport::from_event(ev);
        let en = ev.lang == 1;
        let pad = vp.d(6);

        if self.cam_perm != PermState::Granted {
            let top = TopBarGeo::compute(&vp, 0);
            let area = UiRect::new(
                vp.content_x,
                top.bar.bottom() + vp.d(12),
                vp.content_w,
                (vp.area_h - top.bar.bottom() - vp.d(24)).max(160),
            );
            // Match state view button region comfortably
            if area.contains(x, y) {
                if self.cam_perm == PermState::Denied {
                    let _ = perm::open_settings();
                } else {
                    let _ = perm::request(PERM_CAMERA);
                }
                return RET_REDRAW;
            }
            return RET_NONE;
        }

        if self.screen == Screen::Settings {
            let top = TopBarGeo::compute(&vp, 0);
            if top.back_btn.expand(pad).contains(x, y) {
                self.screen = Screen::Viewfinder;
                return RET_REDRAW | RET_ANIMATING;
            }
            let y0 = top.bar.bottom() + vp.d(14);
            let row_h = vp.d(48).clamp(40, 56);
            if vp.row_rect(y0, row_h).contains(x, y) {
                self.grid_overlay = !self.grid_overlay;
                return RET_REDRAW;
            }
            if vp.row_rect(y0 + row_h, row_h).contains(x, y) {
                self.timer = self.timer.next();
                return RET_REDRAW;
            }
            if vp.row_rect(y0 + row_h * 2, row_h).contains(x, y) {
                self.aspect = self.aspect.next();
                return RET_REDRAW;
            }
            let diag_y = y0 + row_h * 3 + vp.d(14);
            let diag_h = vp.d(126).clamp(108, 144);
            let btn_y = (diag_y + diag_h + vp.d(14)).min(vp.area_h - vp.d(48));
            if vp.row_rect(btn_y, vp.d(40).clamp(34, 44)).expand(pad).contains(x, y) {
                let _ = perm::open_settings();
                return RET_REDRAW;
            }
            return RET_NONE;
        }

        let lay = CameraLayout::compute(&vp, self.aspect);
        if lay.btn_grid.expand(pad).contains(x, y) {
            if self.stream_open {
                self.grid_overlay = !self.grid_overlay;
            } else {
                self.toast.show(
                    ev.uptime_ms,
                    AMBER,
                    if en { "Grid available with active camera" } else { "Izgara aktif kamerayla kullanılır" },
                );
            }
            return RET_REDRAW;
        }
        if lay.btn_timer.expand(pad).contains(x, y) {
            if self.stream_open {
                self.timer = self.timer.next();
            } else {
                self.toast.show(
                    ev.uptime_ms,
                    AMBER,
                    if en { "Camera unavailable" } else { "Kamera kullanılamıyor" },
                );
            }
            return RET_REDRAW;
        }
        if lay.btn_aspect.expand(pad).contains(x, y) {
            self.aspect = self.aspect.next();
            return RET_REDRAW;
        }
        if lay.btn_settings.expand(pad).contains(x, y) || lay.info_btn.expand(pad).contains(x, y) {
            self.screen = Screen::Settings;
            return RET_REDRAW | RET_ANIMATING;
        }
        if lay.mode_video.w > 0 && lay.mode_video.expand(pad).contains(x, y) {
            self.toast.show(
                ev.uptime_ms,
                AMBER,
                if en { "Video capture unsupported on this device" } else { "Video kaydı bu cihazda desteklenmiyor" },
            );
            return RET_REDRAW;
        }
        if lay.shutter.expand(pad).contains(x, y) {
            // Disabled when !stream_open: never create fake photo or fake success!
            if !self.stream_open {
                return RET_NONE;
            }
            match camera::capture() {
                Ok(()) => {
                    self.toast.show(
                        ev.uptime_ms,
                        0x0010_B981,
                        if en { "Photo saved" } else { "Fotoğraf kaydedildi" },
                    );
                }
                Err(wire::ERR_UNSUPPORTED) | Err(_) => {
                    self.stream_open = false;
                    self.toast.show(
                        ev.uptime_ms,
                        AMBER,
                        if en { "Camera unavailable" } else { "Kamera kullanılamıyor" },
                    );
                }
            }
            return RET_REDRAW;
        }

        RET_NONE
    }
}

impl RingApp for CameraApp {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.last_flags = ev.flags;
                self.last_accent = ev.accent;
                self.last_lang = ev.lang;
                if let Some(st) = status::query() {
                    self.reduced_motion = st.reduced_motion();
                }
                self.sync_permission_and_probe(true);
                RET_REDRAW
            }
            OP_PAUSE => {
                // Immediately release/stop camera streaming when backgrounded
                self.stream_open = false;
                self.shutter_down = false;
                self.pressed_id = 0;
                RET_NONE
            }
            OP_UPDATE => {
                let mut ret = RET_NONE;
                if ev.flags != self.last_flags || ev.accent != self.last_accent || ev.lang != self.last_lang {
                    self.last_flags = ev.flags;
                    self.last_accent = ev.accent;
                    self.last_lang = ev.lang;
                    ret |= RET_REDRAW;
                }
                if self.toast.tick(ev.uptime_ms) {
                    ret |= RET_REDRAW;
                }
                let prev_perm = self.cam_perm;
                let prev_stream = self.stream_open;
                self.sync_permission_and_probe(false);
                if self.cam_perm != prev_perm || self.stream_open != prev_stream {
                    ret |= RET_REDRAW;
                }
                if ev.key == key::ESCAPE {
                    if self.screen == Screen::Settings {
                        self.screen = Screen::Viewfinder;
                        return RET_REDRAW;
                    }
                    self.stream_open = false;
                    return RET_CLOSE;
                }
                if let Some(p) = pointer(ev) {
                    let new_pressed = if p.down { self.hit_id(ev, p.x, p.y) } else { 0 };
                    let new_shutter_down = p.down && new_pressed == 7 && self.stream_open;
                    if new_pressed != self.pressed_id || new_shutter_down != self.shutter_down {
                        self.pressed_id = new_pressed;
                        self.shutter_down = new_shutter_down;
                        ret |= RET_REDRAW;
                    }
                    if p.clicked {
                        self.pressed_id = 0;
                        self.shutter_down = false;
                        ret |= self.handle_click(ev, p.x, p.y);
                    }
                }
                if self.step_animations(ev.uptime_ms) {
                    ret |= RET_REDRAW | RET_ANIMATING;
                }
                ret
            }
            OP_DRAW => {
                self.draw(ev, gfx);
                RET_NONE
            }
            _ => RET_NONE,
        }
    }
}

static mut APP: CameraApp = CameraApp::new();

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
