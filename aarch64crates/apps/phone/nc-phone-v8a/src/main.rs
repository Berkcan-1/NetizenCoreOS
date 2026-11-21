//! NetizenCore Independent Ring 3 / EL0 System App: Phone / Dialer (`nc.phone`).
//!
//! - Follows live NetizenHome OS theme (light/dark, accent color, TR/EN localization).
//! - Quiet 4-tab navigation: Keypad, Recents, Contacts, Favorites (speed-dial).
//! - Granular runtime permissions:
//!   - `phone.call` requested ONLY when placing a call on a telephony-capable device.
//!   - `call_log.read` requested ONLY when opening Recents.
//!   - `contacts.read` requested ONLY when opening Contacts or Favorites.
//! - Truthful telephony status via `nc_app_sdk::telephony`: never invents call logs,
//!   never enables call actions when modem hardware is absent, and never opens a fake call UI.
#![no_std]
#![no_main]

use nc_abi::draw::ICON_PHONE;
use nc_abi::event::{
    EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE, RET_CLOSE, RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_abi::perm::{PermState, PERM_CALL_LOG_READ, PERM_CONTACTS_READ, PERM_PHONE_CALL};
use nc_abi::sys as wire;
use nc_app_sdk::contacts::{self, ContactEntry};
use nc_app_sdk::telephony::{self, TelephonyStatus};
use nc_app_sdk::{dispatch, perm, pointer, storage, sys, Gfx, Line, RingApp};
use nc_mobile_ui::{
    draw_bottom_nav, draw_btn, draw_icon_btn, draw_monogram_avatar, draw_state_view,
    draw_status_badge, draw_toast, draw_top_bar, hit_bottom_nav, truncate_to_px, BtnKind,
    MobileTheme, SurfaceTokens, ToastState, TopBarGeo, UiIcon, UiRect, Viewport, AMBER, CORAL,
    GREEN, WHITE,
};

const MAX_CONTACTS: usize = 48;
const MAX_FAVS: usize = 24;
const FAV_PATH: &str = "/data/appdata/nc.phone/favorites.cfg";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Keypad,
    Recents,
    Contacts,
    Favorites,
}

struct KeypadGeo {
    display_box: UiRect,
    keys: [UiRect; 12],
    clear_btn: UiRect,
    call_btn: UiRect,
    del_btn: UiRect,
}

impl KeypadGeo {
    fn compute(vp: &Viewport) -> Self {
        let top = TopBarGeo::compute(vp, 74);
        let nav_h = vp.bottom_nav_h();
        let top_y = top.bar.bottom() + vp.d(6);
        let avail_h = (vp.area_h - nav_h - top_y - vp.d(8)).max(160);

        if vp.is_landscape {
            let left_w = (vp.area_w - vp.margin_x * 3) * 44 / 100;
            let right_w = (vp.area_w - vp.margin_x * 3) - left_w;
            let lx = vp.margin_x;
            let rx = lx + left_w + vp.margin_x;

            let disp_h = vp.d(78).min(avail_h * 45 / 100);
            let display_box = UiRect::new(lx, top_y + vp.d(8), left_w, disp_h);

            let act_h = vp.d(42).clamp(34, 48);
            let act_y = (display_box.bottom() + vp.d(14)).min(vp.area_h - nav_h - act_h - vp.d(8));
            let del_w = vp.d(46).clamp(38, 54);
            let call_w = (left_w - del_w * 2 - vp.d(12)).max(64);
            let clear_btn = UiRect::new(lx, act_y, del_w, act_h);
            let call_btn = UiRect::new(clear_btn.right() + vp.d(6), act_y, call_w, act_h);
            let del_btn = UiRect::new(call_btn.right() + vp.d(6), act_y, del_w, act_h);

            let gap = vp.d(6).max(4);
            let kw = (right_w - gap * 2) / 3;
            let kh = ((avail_h - gap * 3) / 4).clamp(30, vp.d(50));
            let mut keys = [UiRect::new(0, 0, 0, 0); 12];
            for idx in 0..12 {
                let col = (idx % 3) as i32;
                let row = (idx / 3) as i32;
                keys[idx] = UiRect::new(rx + col * (kw + gap), top_y + row * (kh + gap), kw, kh);
            }
            return Self { display_box, keys, clear_btn, call_btn, del_btn };
        }

        // Portrait one-handed dialer composition
        let disp_h = if vp.is_compact_h { vp.d(56).max(46) } else { vp.d(74).max(60) };
        let act_h = if vp.is_compact_h { vp.d(44).max(38) } else { vp.d(52).max(44) };
        let gap_y = if vp.is_compact_h { vp.d(6).max(4) } else { vp.d(10).max(6) };
        let pad_area_h = (avail_h - disp_h - act_h - gap_y * 2).max(vp.d(140));
        let kh = ((pad_area_h - gap_y * 3) / 4).clamp(vp.d(36), vp.d(58));
        let total_pad_h = kh * 4 + gap_y * 3;
        let total_h = disp_h + gap_y + total_pad_h + gap_y + act_h;
        // Anchor keypad comfortably toward the lower thumb zone on tall screens
        let start_y = top_y + ((avail_h - total_h) * 60 / 100).max(0);

        let display_box = UiRect::new(vp.content_x, start_y, vp.content_w, disp_h);

        let gap_x = vp.d(10).max(6);
        let pad_w = vp.content_w.min(vp.d(306));
        let pad_x = (vp.area_w - pad_w) / 2;
        let kw = (pad_w - gap_x * 2) / 3;
        let ky0 = display_box.bottom() + gap_y;
        let mut keys = [UiRect::new(0, 0, 0, 0); 12];
        for idx in 0..12 {
            let col = (idx % 3) as i32;
            let row = (idx / 3) as i32;
            keys[idx] = UiRect::new(pad_x + col * (kw + gap_x), ky0 + row * (kh + gap_y), kw, kh);
        }

        let act_y = ky0 + total_pad_h + gap_y;
        let side_w = kw;
        let call_w = (pad_w - side_w * 2 - gap_x * 2).max(vp.d(84));
        let clear_btn = UiRect::new(pad_x, act_y, side_w, act_h);
        let call_btn = UiRect::new(clear_btn.right() + gap_x, act_y, call_w, act_h);
        let del_btn = UiRect::new(call_btn.right() + gap_x, act_y, side_w, act_h);

        Self { display_box, keys, clear_btn, call_btn, del_btn }
    }
}

struct PhoneApp {
    tab: Tab,
    tel_status: TelephonyStatus,
    call_perm: PermState,
    log_perm: PermState,
    contacts_perm: PermState,
    pending_dial: bool,
    prompted_log: bool,
    prompted_contacts: bool,

    number: [u8; 24],
    number_len: usize,

    contacts: [ContactEntry; MAX_CONTACTS],
    contact_count: usize,
    contact_scroll: usize,

    fav_ids: [u32; MAX_FAVS],
    fav_count: usize,
    favs_loaded: bool,

    pressed_key: Option<usize>,
    pressed_nav: Option<usize>,
    pressed_id: u8,
    toast: ToastState,

    last_flags: u32,
    last_accent: u32,
    last_lang: u32,
}

static mut RESP_BUF: [u8; 4096] = [0; 4096];
static mut COMMANDS: [u8; 16384] = [0; 16384];

fn copy_str(dst: &mut [u8], s: &str) -> usize {
    let b = s.as_bytes();
    let n = b.len().min(dst.len());
    dst[..n].copy_from_slice(&b[..n]);
    n
}

fn s_of(buf: &[u8], len: usize) -> &str {
    core::str::from_utf8(&buf[..len]).unwrap_or("")
}

/// Format dialed digits lightly for readability without altering raw dial buffer.
fn format_display_number<'a>(raw: &'a str, out: &'a mut [u8; 32]) -> &'a str {
    let bytes = raw.as_bytes();
    let mut pos = 0usize;
    let only_digits = bytes.iter().all(|b| b.is_ascii_digit());
    if only_digits && bytes.len() >= 7 && bytes.len() <= 11 {
        for (i, &b) in bytes.iter().enumerate() {
            if (i == 4 || i == 7 || i == 9) && pos < out.len() {
                out[pos] = b' ';
                pos += 1;
            }
            if pos < out.len() {
                out[pos] = b;
                pos += 1;
            }
        }
        return core::str::from_utf8(&out[..pos]).unwrap_or(raw);
    }
    let n = bytes.len().min(out.len());
    out[..n].copy_from_slice(&bytes[..n]);
    core::str::from_utf8(&out[..n]).unwrap_or(raw)
}

impl PhoneApp {
    const fn new() -> Self {
        Self {
            tab: Tab::Keypad,
            tel_status: TelephonyStatus { voice_available: false, sms_available: false, call_log_available: false },
            call_perm: PermState::Ask,
            log_perm: PermState::Ask,
            contacts_perm: PermState::Ask,
            pending_dial: false,
            prompted_log: false,
            prompted_contacts: false,
            number: [0; 24],
            number_len: 0,
            contacts: [ContactEntry::EMPTY; MAX_CONTACTS],
            contact_count: 0,
            contact_scroll: 0,
            fav_ids: [0; MAX_FAVS],
            fav_count: 0,
            favs_loaded: false,
            pressed_key: None,
            pressed_nav: None,
            pressed_id: 0,
            toast: ToastState::new(),
            last_flags: 0,
            last_accent: 0,
            last_lang: 0,
        }
    }

    fn load_favorites(&mut self) {
        if self.favs_loaded {
            return;
        }
        self.favs_loaded = true;
        self.fav_count = 0;
        let mut buf = [0u8; 512];
        if let Some(n) = storage::read_file(FAV_PATH, &mut buf) {
            if let Ok(text) = core::str::from_utf8(&buf[..n]) {
                for line in text.lines() {
                    if let Ok(id) = line.trim().parse::<u32>() {
                        if id > 0 && self.fav_count < MAX_FAVS {
                            self.fav_ids[self.fav_count] = id;
                            self.fav_count += 1;
                        }
                    }
                }
            }
        }
    }

    fn save_favorites(&self) {
        let _ = storage::mkdir("/data/appdata/nc.phone");
        let mut out = [0u8; 512];
        let mut pos = 0usize;
        for i in 0..self.fav_count {
            let mut l = Line::new();
            l.number(self.fav_ids[i], 1).text("\n");
            let b = l.as_str().as_bytes();
            if pos + b.len() > out.len() {
                break;
            }
            out[pos..pos + b.len()].copy_from_slice(b);
            pos += b.len();
        }
        let _ = storage::write_file(FAV_PATH, &out[..pos]);
    }

    fn is_fav(&self, id: u32) -> bool {
        self.fav_ids[..self.fav_count].contains(&id)
    }

    fn toggle_fav(&mut self, id: u32) -> bool {
        if let Some(pos) = self.fav_ids[..self.fav_count].iter().position(|&x| x == id) {
            for j in pos..self.fav_count - 1 {
                self.fav_ids[j] = self.fav_ids[j + 1];
            }
            self.fav_count -= 1;
            self.save_favorites();
            false
        } else if self.fav_count < MAX_FAVS {
            self.fav_ids[self.fav_count] = id;
            self.fav_count += 1;
            self.save_favorites();
            true
        } else {
            false
        }
    }

    fn reload_contacts_if_permitted(&mut self) {
        if self.contacts_perm == PermState::Granted {
            let buf = unsafe { &mut *core::ptr::addr_of_mut!(RESP_BUF) };
            self.contact_count = contacts::list(buf, &mut self.contacts).unwrap_or(0);
            if self.contact_scroll >= self.contact_count {
                self.contact_scroll = 0;
            }
        } else {
            self.contact_count = 0;
            self.contact_scroll = 0;
        }
    }

    fn refresh_all(&mut self) {
        self.tel_status = telephony::status();
        self.call_perm = perm::check(PERM_PHONE_CALL).state;
        self.log_perm = perm::check(PERM_CALL_LOG_READ).state;
        self.contacts_perm = perm::check(PERM_CONTACTS_READ).state;
        self.reload_contacts_if_permitted();
    }

    fn push_digit(&mut self, b: u8) {
        if self.number_len < self.number.len() {
            self.number[self.number_len] = b;
            self.number_len += 1;
        }
    }

    fn pop_digit(&mut self) {
        if self.number_len > 0 {
            self.number_len -= 1;
        }
    }

    fn attempt_call(&mut self, now_ms: u32, en: bool) {
        if !self.tel_status.voice_available {
            self.toast.show(
                now_ms,
                AMBER,
                if en { "Voice calls unavailable: no cellular modem" } else { "Sesli arama yok: hücresel modem bulunamadı" },
            );
            return;
        }
        if self.number_len == 0 {
            self.toast.show(now_ms, AMBER, if en { "Enter a phone number first" } else { "Önce bir numara girin" });
            return;
        }
        let st = perm::check(PERM_PHONE_CALL);
        self.call_perm = st.state;
        if self.call_perm == PermState::Ask {
            self.pending_dial = true;
            let _ = perm::request(PERM_PHONE_CALL);
            return;
        }
        if self.call_perm == PermState::Denied {
            self.toast.show(
                now_ms,
                CORAL,
                if en { "Phone call permission denied (open Settings)" } else { "Arama izni reddedildi (Ayarlar'ı açın)" },
            );
            return;
        }
        let num = s_of(&self.number, self.number_len);
        match telephony::dial(num) {
            Ok(()) => {
                self.toast.show(now_ms, GREEN, if en { "Call initiated" } else { "Arama başlatıldı" });
            }
            Err(wire::ERR_UNSUPPORTED) => {
                self.toast.show(
                    now_ms,
                    AMBER,
                    if en { "Voice calls unavailable on this device" } else { "Bu cihazda sesli arama desteklenmiyor" },
                );
            }
            Err(wire::ERR_PERM_DENIED) => {
                self.toast.show(now_ms, CORAL, if en { "Phone call permission denied" } else { "Arama izni reddedildi" });
            }
            Err(_) => {
                self.toast.show(now_ms, CORAL, if en { "Call service error" } else { "Arama hizmeti hatası" });
            }
        }
    }

    fn matched_contact(&self) -> Option<&ContactEntry> {
        if self.number_len < 2 || self.contacts_perm != PermState::Granted {
            return None;
        }
        let num = s_of(&self.number, self.number_len);
        for c in &self.contacts[..self.contact_count] {
            if !c.phone_str().is_empty() && c.phone_str().contains(num) {
                return Some(c);
            }
        }
        None
    }

    fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        let th = MobileTheme::from_event(ev);
        let tok = th.tokens();
        let vp = Viewport::from_event(ev);
        gfx.fill_rect(0, 0, vp.area_w, vp.area_h, tok.bg);

        let top = TopBarGeo::compute(&vp, 82);
        let title = match self.tab {
            Tab::Keypad => th.tr("Telefon", "Phone"),
            Tab::Recents => th.tr("Son Aramalar", "Recents"),
            Tab::Contacts => th.tr("Kişiler", "Contacts"),
            Tab::Favorites => th.tr("Hızlı Arama", "Favorites"),
        };
        draw_top_bar(gfx, &vp, &tok, &top, title, false, false, Some(ICON_PHONE));

        // Calm truthful cellular hardware status badge
        if !self.tel_status.voice_available {
            let bh = vp.d(24).clamp(20, 28);
            let br = UiRect::new(
                top.trailing_btn.x,
                (top.bar.h - bh) / 2,
                top.trailing_btn.w,
                bh,
            );
            draw_status_badge(gfx, br, AMBER, vp.d(7).max(7), th.tr("Modem Yok", "No Modem"));
        }

        match self.tab {
            Tab::Keypad => self.draw_keypad(gfx, &vp, &th, &tok),
            Tab::Recents => self.draw_recents(gfx, &vp, &th, &tok, &top),
            Tab::Contacts => self.draw_contacts_list(gfx, &vp, &th, &tok, &top, false),
            Tab::Favorites => self.draw_contacts_list(gfx, &vp, &th, &tok, &top, true),
        }

        let nav_items: [(UiIcon, &str); 4] = [
            (UiIcon::Keypad, th.tr("Tuşlar", "Keypad")),
            (UiIcon::Clock, th.tr("Geçmiş", "Recents")),
            (UiIcon::Person, th.tr("Kişiler", "Contacts")),
            (UiIcon::Star, th.tr("Favori", "Favs")),
        ];
        let active_i = match self.tab {
            Tab::Keypad => 0,
            Tab::Recents => 1,
            Tab::Contacts => 2,
            Tab::Favorites => 3,
        };
        draw_bottom_nav(gfx, &vp, &tok, &nav_items, active_i, self.pressed_nav);

        if self.toast.active {
            draw_toast(gfx, &vp, &tok, top.bar.bottom() + vp.d(8), self.toast.color, self.toast.text());
        }
    }

    fn draw_keypad(&self, gfx: &mut Gfx<'_>, vp: &Viewport, th: &MobileTheme, tok: &SurfaceTokens) {
        let geo = KeypadGeo::compute(vp);
        let d = geo.display_box;

        // Clean number readout surface
        gfx.panel(d.x, d.y, d.w, d.h, vp.d(16).clamp(12, 20), tok.surface, 255, tok.border, tok.border_alpha);
        let num_cap = if self.number_len > 14 { vp.d(14).clamp(11, 16) } else { vp.d(19).clamp(14, 24) };
        let mut fmt_buf = [0u8; 32];
        let shown = if self.number_len == 0 {
            th.tr("Numara girin", "Enter number")
        } else {
            format_display_number(s_of(&self.number, self.number_len), &mut fmt_buf)
        };
        let mut trunc_buf = [0u8; 32];
        let clipped = truncate_to_px(shown, d.w - vp.d(24), num_cap, &mut trunc_buf);
        gfx.text_centered(
            d.cx(),
            d.y + (d.h * 22 / 100),
            num_cap,
            if self.number_len == 0 { tok.subtle } else { tok.ink },
            2,
            clipped,
        );

        let sub_cap = vp.d(8).clamp(7, 10);
        if let Some(mc) = self.matched_contact() {
            gfx.text_centered(d.cx(), d.bottom() - sub_cap - vp.d(10), sub_cap, tok.accent, 2, mc.name_str());
        } else if !self.tel_status.voice_available {
            gfx.text_centered(
                d.cx(),
                d.bottom() - sub_cap - vp.d(10),
                sub_cap,
                tok.muted,
                1,
                th.tr("Sesli arama bu cihazda kullanılamıyor", "Voice calling unavailable on this device"),
            );
        }

        // 3x4 Keypad with classic sub-letters and tactile pressed feedback
        const DIGITS: [(&str, &str); 12] = [
            ("1", ""), ("2", "ABC"), ("3", "DEF"),
            ("4", "GHI"), ("5", "JKL"), ("6", "MNO"),
            ("7", "PQRS"), ("8", "TUV"), ("9", "WXYZ"),
            ("*", ""), ("0", "+"), ("#", ""),
        ];
        let d_cap = vp.d(15).clamp(12, 19);
        let s_cap = vp.d(7).clamp(6, 8);
        for (idx, &(dig, sub)) in DIGITS.iter().enumerate() {
            let r = geo.keys[idx];
            let prs = self.pressed_key == Some(idx);
            let fill = if prs { tok.pressed } else { tok.surface };
            let rad = (r.h * 42 / 100).clamp(10, r.h / 2);
            gfx.panel(r.x, r.y, r.w, r.h, rad, fill, 255, tok.border, tok.border_alpha);
            if sub.is_empty() {
                gfx.text_centered(r.cx(), r.y + (r.h - d_cap) / 2, d_cap, tok.ink, 2, dig);
            } else {
                gfx.text_centered(r.cx(), r.y + r.h * 20 / 100, d_cap, tok.ink, 2, dig);
                gfx.text_centered(r.cx(), r.y + r.h * 66 / 100, s_cap, tok.subtle, 1, sub);
            }
        }

        // Bottom actions: Clear, Call (disabled when !voice_available or empty), Backspace
        draw_btn(
            gfx,
            tok,
            geo.clear_btn,
            if self.number_len > 0 { BtnKind::Quiet } else { BtnKind::Disabled },
            self.pressed_id == 1,
            vp.d(8).clamp(7, 10),
            th.tr("Temizle", "Clear"),
        );

        let can_call = self.tel_status.voice_available && self.number_len > 0;
        let call_r = geo.call_btn;
        let call_col = if can_call { GREEN } else { tok.surface_subtle };
        let call_fg = if can_call { WHITE } else { tok.subtle };
        gfx.panel(call_r.x, call_r.y, call_r.w, call_r.h, call_r.h / 2, call_col, 255, 0, 0);
        gfx.text_centered(call_r.cx(), call_r.y + (call_r.h - vp.d(10).clamp(9, 12)) / 2, vp.d(10).clamp(9, 12), call_fg, 2, th.tr("Ara", "Call"));

        draw_icon_btn(gfx, tok, geo.del_btn, UiIcon::Backspace, false, self.number_len == 0, self.pressed_id == 2);
    }

    fn draw_recents(&self, gfx: &mut Gfx<'_>, vp: &Viewport, th: &MobileTheme, tok: &SurfaceTokens, top: &TopBarGeo) {
        let area = UiRect::new(
            vp.content_x,
            top.bar.bottom() + vp.d(12),
            vp.content_w,
            (vp.area_h - vp.bottom_nav_h() - top.bar.bottom() - vp.d(24)).max(150),
        );
        if self.log_perm != PermState::Granted {
            let denied = self.log_perm == PermState::Denied;
            draw_state_view(
                gfx,
                vp,
                tok,
                area,
                UiIcon::Clock,
                if denied { CORAL } else { tok.accent },
                if denied {
                    th.tr("Arama geçmişi izni reddedildi", "Call history permission denied")
                } else {
                    th.tr("Arama geçmişi izni gerekli", "Call log permission required")
                },
                if denied {
                    th.tr(
                        "Geçmiş aramalarınızı görmek için Ayarlar > Uygulama İzinleri'nden izin verin.",
                        "Allow Call History in Settings > App Permissions to view recent calls.",
                    )
                } else {
                    th.tr(
                        "Geçmiş arama kayıtlarını görüntülemek için onayınız gerekir.",
                        "Your approval is required to read call history records.",
                    )
                },
                Some(if denied { th.tr("Ayarları Aç", "Open Settings") } else { th.tr("İzin İste", "Allow Access") }),
                self.pressed_id == 10,
            );
            return;
        }
        draw_state_view(
            gfx,
            vp,
            tok,
            area,
            UiIcon::Clock,
            AMBER,
            th.tr("Arama geçmişi kullanılamıyor", "Call history unavailable"),
            th.tr(
                "Bu cihaz profilinde hücresel telefon/modem sürücüsü yok. Gerçek arama kaydı olmadan sahte liste gösterilmez.",
                "No cellular telephony/modem driver is present on this device. No simulated call history is displayed.",
            ),
            None,
            false,
        );
    }

    fn draw_contacts_list(
        &self,
        gfx: &mut Gfx<'_>,
        vp: &Viewport,
        th: &MobileTheme,
        tok: &SurfaceTokens,
        top: &TopBarGeo,
        fav_only: bool,
    ) {
        let nav_h = vp.bottom_nav_h();
        let area = UiRect::new(
            vp.content_x,
            top.bar.bottom() + vp.d(10),
            vp.content_w,
            (vp.area_h - nav_h - top.bar.bottom() - vp.d(20)).max(140),
        );

        if self.contacts_perm != PermState::Granted {
            let denied = self.contacts_perm == PermState::Denied;
            draw_state_view(
                gfx,
                vp,
                tok,
                area,
                UiIcon::Person,
                if denied { CORAL } else { tok.accent },
                if denied {
                    th.tr("Kişiler izni reddedildi", "Contacts permission denied")
                } else {
                    th.tr("Kişiler okuma izni gerekli", "Contacts read permission required")
                },
                if denied {
                    th.tr(
                        "Kayıtlı kişileri ve hızlı aramaları görmek için Ayarlar'dan Kişiler iznini açın.",
                        "Enable Contacts Read in Settings to view saved contacts and speed dial.",
                    )
                } else {
                    th.tr(
                        "Kişilerinizi telefon uygulamasından seçmek için okuma izni gerekir.",
                        "Allow Contacts Read to look up saved people and numbers.",
                    )
                },
                Some(if denied { th.tr("Ayarları Aç", "Open Settings") } else { th.tr("İzin İste", "Allow Read Access") }),
                self.pressed_id == 11,
            );
            return;
        }

        let mut list = [0u8; MAX_CONTACTS];
        let mut lcnt = 0usize;
        for i in 0..self.contact_count {
            if !fav_only || self.is_fav(self.contacts[i].id) {
                list[lcnt] = i as u8;
                lcnt += 1;
            }
        }

        if lcnt == 0 {
            draw_state_view(
                gfx,
                vp,
                tok,
                area,
                if fav_only { UiIcon::Star } else { UiIcon::Person },
                tok.muted,
                if fav_only { th.tr("Hızlı arama favorisi yok", "No speed-dial favorites") } else { th.tr("Kayıtlı kişi yok", "No saved contacts") },
                if fav_only {
                    th.tr(
                        "Kişiler sekmesindeki yıldız simgesine dokunarak hızlı arama favorisi ekleyin.",
                        "Tap the star icon in the Contacts tab to pin speed-dial favorites.",
                    )
                } else {
                    th.tr(
                        "Kişiler uygulamasından kişi eklediğinizde burada görünür.",
                        "Contacts you save in the Contacts app will appear here.",
                    )
                },
                None,
                false,
            );
            return;
        }

        let pager_h = if lcnt > 5 { vp.d(36) } else { 0 };
        let list_h = (area.h - pager_h).max(vp.d(100));
        let row_h = vp.d(54).clamp(46, 62);
        let visible = ((list_h - vp.d(8)) / row_h).clamp(2, 8) as usize;
        let card_h = (visible as i32 * row_h + vp.d(8)).min(list_h);
        let card = UiRect::new(area.x, area.y, area.w, card_h);
        gfx.panel(card.x, card.y, card.w, card.h, vp.d(16), tok.surface, 255, tok.border, tok.border_alpha);

        let av_r = vp.d(16).clamp(13, 20);
        let name_cap = vp.d(10).clamp(9, 12);
        let sub_cap = vp.d(8).clamp(7, 10);
        let tx = card.x + vp.d(52);
        let max_tw = (card.w - vp.d(104)).max(72);
        let scroll = self.contact_scroll.min(lcnt.saturating_sub(1));

        for slot in 0..visible {
            let pos = scroll + slot;
            if pos >= lcnt {
                break;
            }
            let c = &self.contacts[list[pos] as usize];
            let ry = card.y + vp.d(4) + slot as i32 * row_h;
            if slot > 0 {
                gfx.fill_rect(tx, ry, card.right() - tx - vp.d(14), 1, tok.divider);
            }
            let disp = if c.name_str().is_empty() { c.phone_str() } else { c.name_str() };
            draw_monogram_avatar(gfx, card.x + vp.d(26), ry + row_h / 2, av_r, disp, c.id);
            let mut nb = [0u8; 40];
            let mut sb = [0u8; 32];
            gfx.text(tx, ry + row_h * 22 / 100, name_cap, tok.ink, 2, truncate_to_px(disp, max_tw, name_cap, &mut nb));
            gfx.text(tx, ry + row_h * 58 / 100, sub_cap, tok.muted, 1, truncate_to_px(c.phone_str(), max_tw, sub_cap, &mut sb));

            let star_s = vp.d(32).clamp(28, 38);
            let star_r = UiRect::new(card.right() - star_s - vp.d(10), ry + (row_h - star_s) / 2, star_s, star_s);
            let fav = self.is_fav(c.id);
            draw_icon_btn(gfx, tok, star_r, if fav { UiIcon::StarFilled } else { UiIcon::Star }, false, false, false);
        }

        if lcnt > visible {
            let py = card.bottom() + vp.d(6);
            let ph = vp.d(28).clamp(24, 34);
            let bw = vp.d(84).clamp(70, 100);
            draw_btn(gfx, tok, UiRect::new(card.x, py, bw, ph), if scroll > 0 { BtnKind::Secondary } else { BtnKind::Disabled }, false, vp.d(8).max(7), th.tr("Önceki", "Prev"));
            draw_btn(gfx, tok, UiRect::new(card.right() - bw, py, bw, ph), if scroll + visible < lcnt { BtnKind::Secondary } else { BtnKind::Disabled }, false, vp.d(8).max(7), th.tr("Sonraki", "Next"));
        }
    }

    fn handle_click(&mut self, ev: &EventBlock, x: i32, y: i32) -> u32 {
        let vp = Viewport::from_event(ev);
        let en = ev.lang == 1;
        let pad = vp.d(6);

        if let Some(idx) = hit_bottom_nav(&vp, 4, x, y) {
            let next = match idx {
                0 => Tab::Keypad,
                1 => Tab::Recents,
                2 => Tab::Contacts,
                _ => Tab::Favorites,
            };
            if next != self.tab {
                self.tab = next;
                self.contact_scroll = 0;
                if next == Tab::Recents && self.log_perm == PermState::Ask && !self.prompted_log {
                    self.prompted_log = true;
                    let _ = perm::request(PERM_CALL_LOG_READ);
                } else if matches!(next, Tab::Contacts | Tab::Favorites)
                    && self.contacts_perm == PermState::Ask
                    && !self.prompted_contacts
                {
                    self.prompted_contacts = true;
                    let _ = perm::request(PERM_CONTACTS_READ);
                }
                self.refresh_all();
                return RET_REDRAW;
            }
            return RET_NONE;
        }

        match self.tab {
            Tab::Keypad => {
                let geo = KeypadGeo::compute(&vp);
                // Tapping matched contact fills its full number
                if geo.display_box.contains(x, y) {
                    if let Some(mc) = self.matched_contact() {
                        let mut tmp = [0u8; 24];
                        let n = copy_str(&mut tmp, mc.phone_str());
                        self.number[..n].copy_from_slice(&tmp[..n]);
                        self.number_len = n;
                        return RET_REDRAW;
                    }
                }
                const CHARS: [u8; 12] = *b"123456789*0#";
                for (idx, &ch) in CHARS.iter().enumerate() {
                    if geo.keys[idx].contains(x, y) {
                        self.push_digit(ch);
                        return RET_REDRAW;
                    }
                }
                if geo.clear_btn.expand(pad).contains(x, y) && self.number_len > 0 {
                    self.number_len = 0;
                    return RET_REDRAW;
                }
                if geo.del_btn.expand(pad).contains(x, y) && self.number_len > 0 {
                    self.pop_digit();
                    return RET_REDRAW;
                }
                if geo.call_btn.expand(pad).contains(x, y) {
                    self.attempt_call(ev.uptime_ms, en);
                    return RET_REDRAW;
                }
            }
            Tab::Recents => {
                if self.log_perm != PermState::Granted {
                    if self.log_perm == PermState::Denied {
                        let _ = perm::open_settings();
                    } else {
                        let _ = perm::request(PERM_CALL_LOG_READ);
                    }
                    return RET_REDRAW;
                }
            }
            Tab::Contacts | Tab::Favorites => {
                let fav_only = self.tab == Tab::Favorites;
                if self.contacts_perm != PermState::Granted {
                    if self.contacts_perm == PermState::Denied {
                        let _ = perm::open_settings();
                    } else {
                        let _ = perm::request(PERM_CONTACTS_READ);
                    }
                    return RET_REDRAW;
                }
                let top = TopBarGeo::compute(&vp, 82);
                let nav_h = vp.bottom_nav_h();
                let area = UiRect::new(
                    vp.content_x,
                    top.bar.bottom() + vp.d(10),
                    vp.content_w,
                    (vp.area_h - nav_h - top.bar.bottom() - vp.d(20)).max(140),
                );
                let mut list = [0u8; MAX_CONTACTS];
                let mut lcnt = 0usize;
                for i in 0..self.contact_count {
                    if !fav_only || self.is_fav(self.contacts[i].id) {
                        list[lcnt] = i as u8;
                        lcnt += 1;
                    }
                }
                if lcnt == 0 {
                    return RET_NONE;
                }
                let pager_h = if lcnt > 5 { vp.d(36) } else { 0 };
                let list_h = (area.h - pager_h).max(vp.d(100));
                let row_h = vp.d(54).clamp(46, 62);
                let visible = ((list_h - vp.d(8)) / row_h).clamp(2, 8) as usize;
                let card_h = (visible as i32 * row_h + vp.d(8)).min(list_h);
                let card = UiRect::new(area.x, area.y, area.w, card_h);
                let scroll = self.contact_scroll.min(lcnt.saturating_sub(1));

                for slot in 0..visible {
                    let pos = scroll + slot;
                    if pos >= lcnt {
                        break;
                    }
                    let ry = card.y + vp.d(4) + slot as i32 * row_h;
                    let row_r = UiRect::new(card.x, ry, card.w, row_h);
                    if row_r.contains(x, y) {
                        let c = self.contacts[list[pos] as usize];
                        let star_s = vp.d(32).clamp(28, 38);
                        let star_r = UiRect::new(card.right() - star_s - vp.d(10), ry + (row_h - star_s) / 2, star_s, star_s);
                        if star_r.expand(pad).contains(x, y) {
                            let added = self.toggle_fav(c.id);
                            self.toast.show(
                                ev.uptime_ms,
                                if added { AMBER } else { 0x0096_A1B2 },
                                if added {
                                    if en { "Added to speed dial" } else { "Hızlı aramaya eklendi" }
                                } else if en {
                                    "Removed from speed dial"
                                } else {
                                    "Hızlı aramadan çıkarıldı"
                                },
                            );
                            return RET_REDRAW;
                        }
                        if !c.phone_str().is_empty() {
                            let mut tmp = [0u8; 24];
                            let n = copy_str(&mut tmp, c.phone_str());
                            self.number[..n].copy_from_slice(&tmp[..n]);
                            self.number_len = n;
                            self.tab = Tab::Keypad;
                            return RET_REDRAW;
                        }
                    }
                }

                if lcnt > visible {
                    let py = card.bottom() + vp.d(6);
                    let ph = vp.d(28).clamp(24, 34);
                    let bw = vp.d(84).clamp(70, 100);
                    if UiRect::new(card.x, py, bw, ph).expand(pad).contains(x, y) && scroll > 0 {
                        self.contact_scroll = scroll.saturating_sub(visible);
                        return RET_REDRAW;
                    }
                    if UiRect::new(card.right() - bw, py, bw, ph).expand(pad).contains(x, y) && scroll + visible < lcnt {
                        self.contact_scroll = scroll + visible;
                        return RET_REDRAW;
                    }
                }
            }
        }
        RET_NONE
    }
}

impl RingApp for PhoneApp {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.last_flags = ev.flags;
                self.last_accent = ev.accent;
                self.last_lang = ev.lang;
                self.load_favorites();
                self.refresh_all();
                RET_REDRAW
            }
            OP_PAUSE => {
                self.pressed_key = None;
                self.pressed_nav = None;
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
                let c_now = perm::check(PERM_PHONE_CALL).state;
                let l_now = perm::check(PERM_CALL_LOG_READ).state;
                let k_now = perm::check(PERM_CONTACTS_READ).state;
                if (c_now, l_now, k_now) != (self.call_perm, self.log_perm, self.contacts_perm) {
                    self.call_perm = c_now;
                    self.log_perm = l_now;
                    self.contacts_perm = k_now;
                    self.reload_contacts_if_permitted();
                    ret |= RET_REDRAW;
                    if self.pending_dial && self.call_perm != PermState::Ask {
                        self.pending_dial = false;
                        self.attempt_call(ev.uptime_ms, ev.lang == 1);
                    }
                }
                if ev.key == key::ESCAPE {
                    if self.tab != Tab::Keypad {
                        self.tab = Tab::Keypad;
                        return RET_REDRAW;
                    }
                    return RET_CLOSE;
                }
                if ev.key == key::BACKSPACE && self.tab == Tab::Keypad {
                    self.pop_digit();
                    return RET_REDRAW;
                }
                if let Some(c) = key::as_char(ev.key) {
                    if self.tab == Tab::Keypad && (c.is_ascii_digit() || c == '*' || c == '#' || c == '+') {
                        self.push_digit(c as u8);
                        return RET_REDRAW;
                    }
                }
                if let Some(p) = pointer(ev) {
                    let vp = Viewport::from_event(ev);
                    let new_nav = if p.down { hit_bottom_nav(&vp, 4, p.x, p.y) } else { None };
                    let new_key = if p.down && self.tab == Tab::Keypad {
                        let geo = KeypadGeo::compute(&vp);
                        geo.keys.iter().position(|r| r.contains(p.x, p.y))
                    } else {
                        None
                    };
                    if new_nav != self.pressed_nav || new_key != self.pressed_key {
                        self.pressed_nav = new_nav;
                        self.pressed_key = new_key;
                        ret |= RET_REDRAW;
                    }
                    if p.clicked {
                        self.pressed_nav = None;
                        self.pressed_key = None;
                        self.pressed_id = 0;
                        ret |= self.handle_click(ev, p.x, p.y);
                    }
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

static mut APP: PhoneApp = PhoneApp::new();

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
