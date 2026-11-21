//! NetizenCore Independent Ring 3 / EL0 System App: Messages (`nc.messages`).
//!
//! - Follows live NetizenHome OS theme (light/dark, accent color, TR/EN localization).
//! - Granular runtime permissions:
//!   - `sms.read` requested before viewing SMS history.
//!   - `sms.send` requested ONLY when attempting to send an SMS.
//!   - `contacts.read` requested ONLY when opening the recipient picker.
//! - Automatic draft persistence in private `/data/appdata/nc.messages/draft.cfg`.
//! - Truthful SMS backend reporting: never fabricates conversations, never fakes
//!   sent status or delivery ticks when cellular/SMS hardware is unavailable.
#![no_std]
#![no_main]

use nc_abi::draw::ICON_MESSAGES;
use nc_abi::event::{
    EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE, RET_CLOSE, RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_abi::perm::{PermState, PERM_CONTACTS_READ, PERM_SMS_READ, PERM_SMS_SEND};
use nc_abi::sys as wire;
use nc_app_sdk::contacts::{self, ContactEntry};
use nc_app_sdk::telephony::{self, TelephonyStatus};
use nc_app_sdk::{dispatch, ime, perm, pointer, storage, sys, Gfx, Line, RingApp};
use nc_mobile_ui::{
    draw_btn, draw_icon_btn, draw_monogram_avatar, draw_search_bar, draw_state_view,
    draw_status_badge, draw_text_field, draw_toast, draw_top_bar, truncate_to_px, wrap_text_px,
    BtnKind, MobileTheme, SurfaceTokens, ToastState, TopBarGeo, UiIcon, UiRect, Viewport, AMBER,
    CORAL, GREEN,
};

const MAX_CONTACTS: usize = 48;
const DRAFT_PATH: &str = "/data/appdata/nc.messages/draft.cfg";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Inbox,
    Compose,
    PickContact,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Recipient,
    Body,
}

struct MessagesApp {
    screen: Screen,
    tel_status: TelephonyStatus,
    read_perm: PermState,
    send_perm: PermState,
    contacts_perm: PermState,
    prompted_read: bool,
    prompted_contacts: bool,
    pending_send: bool,
    last_send_failed: bool,

    // Compose draft state (persisted in app sandbox)
    field: Field,
    recipient: [u8; 32],
    recipient_len: usize,
    body: [u8; 160],
    body_len: usize,
    draft_loaded: bool,
    draft_dirty: bool,
    ime_open: bool,

    // Contact picker state
    contacts: [ContactEntry; MAX_CONTACTS],
    contact_count: usize,
    picker_scroll: usize,
    picker_search_active: bool,
    picker_search: [u8; 28],
    picker_search_len: usize,

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

fn push_char(buf: &mut [u8], len: &mut usize, c: char) {
    let mut tmp = [0u8; 4];
    let enc = c.encode_utf8(&mut tmp);
    if *len + enc.len() <= buf.len() {
        buf[*len..*len + enc.len()].copy_from_slice(enc.as_bytes());
        *len += enc.len();
    }
}

fn pop_char(buf: &[u8], len: &mut usize) {
    if *len == 0 {
        return;
    }
    let mut nl = *len - 1;
    while nl > 0 && (buf[nl] & 0xC0) == 0x80 {
        nl -= 1;
    }
    *len = nl;
}

impl MessagesApp {
    const fn new() -> Self {
        Self {
            screen: Screen::Inbox,
            tel_status: TelephonyStatus { voice_available: false, sms_available: false, call_log_available: false },
            read_perm: PermState::Ask,
            send_perm: PermState::Ask,
            contacts_perm: PermState::Ask,
            prompted_read: false,
            prompted_contacts: false,
            pending_send: false,
            last_send_failed: false,
            field: Field::Recipient,
            recipient: [0; 32],
            recipient_len: 0,
            body: [0; 160],
            body_len: 0,
            draft_loaded: false,
            draft_dirty: false,
            ime_open: false,
            contacts: [ContactEntry::EMPTY; MAX_CONTACTS],
            contact_count: 0,
            picker_scroll: 0,
            picker_search_active: false,
            picker_search: [0; 28],
            picker_search_len: 0,
            pressed_id: 0,
            toast: ToastState::new(),
            last_flags: 0,
            last_accent: 0,
            last_lang: 0,
        }
    }

    fn has_draft(&self) -> bool {
        self.recipient_len > 0 || self.body_len > 0
    }

    fn load_draft(&mut self) {
        if self.draft_loaded {
            return;
        }
        self.draft_loaded = true;
        let mut buf = [0u8; 256];
        if let Some(n) = storage::read_file(DRAFT_PATH, &mut buf) {
            if let Ok(text) = core::str::from_utf8(&buf[..n]) {
                let mut lines = text.lines();
                if let Some(rec) = lines.next() {
                    self.recipient_len = copy_str(&mut self.recipient, rec.trim());
                }
                if let Some(msg) = lines.next() {
                    self.body_len = copy_str(&mut self.body, msg);
                }
            }
        }
    }

    fn save_draft(&mut self) {
        if !self.draft_dirty {
            return;
        }
        self.draft_dirty = false;
        let _ = storage::mkdir("/data/appdata/nc.messages");
        let mut out = [0u8; 256];
        let mut pos = 0usize;
        let rec = &self.recipient[..self.recipient_len];
        let msg = &self.body[..self.body_len];
        if pos + rec.len() + 1 + msg.len() <= out.len() {
            out[pos..pos + rec.len()].copy_from_slice(rec);
            pos += rec.len();
            out[pos] = b'\n';
            pos += 1;
            out[pos..pos + msg.len()].copy_from_slice(msg);
            pos += msg.len();
            let _ = storage::write_file(DRAFT_PATH, &out[..pos]);
        }
    }

    fn clear_draft(&mut self) {
        self.recipient_len = 0;
        self.body_len = 0;
        self.last_send_failed = false;
        self.draft_dirty = true;
        self.save_draft();
    }

    fn show_ime(&mut self) {
        self.ime_open = true;
        let _ = ime::show();
    }

    fn hide_ime(&mut self) {
        self.ime_open = false;
        let _ = ime::hide();
    }

    fn reload_contacts(&mut self) {
        if self.contacts_perm != PermState::Granted {
            self.contact_count = 0;
            return;
        }
        let buf = unsafe { &mut *core::ptr::addr_of_mut!(RESP_BUF) };
        let res = if self.picker_search_len > 0 {
            let mut q = [0u8; 28];
            let ql = self.picker_search_len;
            q[..ql].copy_from_slice(&self.picker_search[..ql]);
            contacts::lookup(s_of(&q, ql), buf, &mut self.contacts)
        } else {
            contacts::list(buf, &mut self.contacts)
        };
        self.contact_count = res.unwrap_or(0);
        if self.picker_scroll >= self.contact_count {
            self.picker_scroll = 0;
        }
    }

    fn refresh_all(&mut self, ask_read: bool) {
        self.tel_status = telephony::status();
        let r = perm::check(PERM_SMS_READ);
        self.read_perm = r.state;
        self.send_perm = perm::check(PERM_SMS_SEND).state;
        self.contacts_perm = perm::check(PERM_CONTACTS_READ).state;
        if ask_read && self.read_perm == PermState::Ask && !self.prompted_read && !r.pending {
            self.prompted_read = true;
            let _ = perm::request(PERM_SMS_READ);
        }
        if self.screen == Screen::PickContact {
            self.reload_contacts();
        }
    }

    fn attempt_send(&mut self, now_ms: u32, en: bool) {
        if !self.tel_status.sms_available {
            self.toast.show(
                now_ms,
                AMBER,
                if en { "SMS unavailable: no cellular modem" } else { "SMS kullanılamıyor: hücresel modem yok" },
            );
            return;
        }
        if self.recipient_len == 0 || self.body_len == 0 {
            self.toast.show(
                now_ms,
                AMBER,
                if en { "Enter recipient and message text" } else { "Alıcı ve mesaj metni girin" },
            );
            return;
        }
        let st = perm::check(PERM_SMS_SEND);
        self.send_perm = st.state;
        if self.send_perm == PermState::Ask {
            self.pending_send = true;
            let _ = perm::request(PERM_SMS_SEND);
            return;
        }
        if self.send_perm == PermState::Denied {
            self.toast.show(
                now_ms,
                CORAL,
                if en { "SMS send permission denied (open Settings)" } else { "SMS gönderme izni reddedildi (Ayarlar'ı açın)" },
            );
            return;
        }
        let rec = s_of(&self.recipient, self.recipient_len);
        let txt = s_of(&self.body, self.body_len);
        match telephony::sms_send(rec, txt) {
            Ok(()) => {
                self.clear_draft();
                self.toast.show(now_ms, GREEN, if en { "SMS sent" } else { "SMS gönderildi" });
            }
            Err(wire::ERR_UNSUPPORTED) => {
                self.last_send_failed = true;
                self.toast.show(
                    now_ms,
                    AMBER,
                    if en { "SMS unavailable on this device" } else { "SMS bu cihazda desteklenmiyor" },
                );
            }
            Err(wire::ERR_PERM_DENIED) => {
                self.last_send_failed = true;
                self.toast.show(now_ms, CORAL, if en { "SMS send permission denied" } else { "SMS gönderme izni reddedildi" });
            }
            Err(_) => {
                self.last_send_failed = true;
                self.toast.show(now_ms, CORAL, if en { "Send failed — tap Retry" } else { "Gönderilemedi — Tekrar deneyin" });
            }
        }
    }

    fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        let th = MobileTheme::from_event(ev);
        let tok = th.tokens();
        let vp = Viewport::from_event(ev);
        gfx.fill_rect(0, 0, vp.area_w, vp.area_h, tok.bg);

        let show_back = self.screen != Screen::Inbox;
        let top = TopBarGeo::compute(&vp, 36);
        let title = match self.screen {
            Screen::Inbox => th.tr("Mesajlar", "Messages"),
            Screen::Compose => th.tr("Yeni Mesaj", "New Message"),
            Screen::PickContact => th.tr("Alıcı Seç", "Pick Recipient"),
        };
        draw_top_bar(
            gfx,
            &vp,
            &tok,
            &top,
            title,
            show_back,
            self.pressed_id == 1,
            if show_back { None } else { Some(ICON_MESSAGES) },
        );

        match self.screen {
            Screen::Inbox => self.draw_inbox(gfx, &vp, &th, &tok, &top),
            Screen::Compose => self.draw_compose(gfx, &vp, &th, &tok, &top),
            Screen::PickContact => self.draw_pick_contact(gfx, &vp, &th, &tok, &top),
        }

        if self.toast.active {
            draw_toast(gfx, &vp, &tok, top.bar.bottom() + vp.d(8), self.toast.color, self.toast.text());
        }
    }

    fn draw_inbox(&self, gfx: &mut Gfx<'_>, vp: &Viewport, th: &MobileTheme, tok: &SurfaceTokens, top: &TopBarGeo) {
        draw_icon_btn(gfx, tok, top.trailing_btn, UiIcon::Plus, true, false, self.pressed_id == 2);

        let mut y = top.bar.bottom() + vp.d(12);
        let bottom_pad = vp.d(14);

        if self.read_perm != PermState::Granted {
            let area = UiRect::new(vp.content_x, y, vp.content_w, (vp.area_h - y - bottom_pad).max(150));
            let denied = self.read_perm == PermState::Denied;
            draw_state_view(
                gfx,
                vp,
                tok,
                area,
                UiIcon::Message,
                if denied { CORAL } else { tok.accent },
                if denied {
                    th.tr("SMS okuma izni reddedildi", "SMS read permission denied")
                } else {
                    th.tr("SMS okuma izni gerekli", "SMS read permission required")
                },
                if denied {
                    th.tr(
                        "Mesaj konuşmalarınızı görmek için Ayarlar > Uygulama İzinleri'nden izin verin.",
                        "Allow SMS Read in Settings > App Permissions to view conversations.",
                    )
                } else {
                    th.tr(
                        "Korumalı SMS konuşmalarınızı görüntülemek için onayınız gerekir.",
                        "Your approval is required before reading protected SMS history.",
                    )
                },
                Some(if denied { th.tr("Ayarları Aç", "Open Settings") } else { th.tr("İzin İste", "Allow Access") }),
                self.pressed_id == 10,
            );
            return;
        }

        // Saved Draft card if user has an unsent draft
        if self.has_draft() {
            let dh = vp.d(62).clamp(52, 72);
            let dr = vp.row_rect(y, dh);
            gfx.panel(dr.x, dr.y, dr.w, dr.h, vp.d(14), tok.surface, 255, tok.accent, 110);
            let badge_w = vp.d(58);
            let badge_h = vp.d(22).clamp(18, 26);
            draw_status_badge(gfx, UiRect::new(dr.x + vp.d(12), dr.y + vp.d(10), badge_w, badge_h), tok.accent, vp.d(7).max(7), th.tr("Taslak", "Draft"));
            let rec_str = if self.recipient_len > 0 { s_of(&self.recipient, self.recipient_len) } else { th.tr("Alıcı yok", "No recipient") };
            let mut rb = [0u8; 32];
            gfx.text(dr.x + vp.d(78), dr.y + vp.d(13), vp.d(9).clamp(8, 11), tok.ink, 2, truncate_to_px(rec_str, dr.w - vp.d(124), vp.d(9).max(8), &mut rb));
            let msg_str = if self.body_len > 0 { s_of(&self.body, self.body_len) } else { th.tr("Taslağa devam et...", "Continue draft...") };
            let mut mb = [0u8; 44];
            gfx.text(dr.x + vp.d(14), dr.y + vp.d(38), vp.d(8).clamp(7, 10), tok.muted, 1, truncate_to_px(msg_str, dr.w - vp.d(64), vp.d(8).max(7), &mut mb));

            let clr_s = vp.d(28).clamp(24, 32);
            let clr_r = UiRect::new(dr.right() - clr_s - vp.d(10), dr.cy() - clr_s / 2, clr_s, clr_s);
            draw_icon_btn(gfx, tok, clr_r, UiIcon::Close, false, false, self.pressed_id == 3);
            y = dr.bottom() + vp.d(12);
        }

        let area = UiRect::new(vp.content_x, y, vp.content_w, (vp.area_h - y - bottom_pad).max(140));
        if !self.tel_status.sms_available {
            draw_state_view(
                gfx,
                vp,
                tok,
                area,
                UiIcon::Message,
                AMBER,
                th.tr("SMS servisi kullanılamıyor", "SMS service unavailable"),
                th.tr(
                    "Bu cihaz profilinde hücresel modem sürücüsü yok. Sahte konuşma veya teslimat durumu gösterilmez; yine de taslak hazırlayabilirsiniz.",
                    "No cellular modem driver is present on this device profile. No simulated messages are shown; you can still prepare drafts.",
                ),
                Some(th.tr("Taslak Oluştur", "Write Draft")),
                self.pressed_id == 4,
            );
        } else {
            draw_state_view(
                gfx,
                vp,
                tok,
                area,
                UiIcon::Message,
                tok.accent,
                th.tr("Henüz mesaj yok", "No conversations yet"),
                th.tr("Yeni bir SMS konuşması başlatmak için + düğmesine dokunun.", "Tap + to start a new SMS conversation."),
                Some(th.tr("Yeni Mesaj", "New Message")),
                self.pressed_id == 4,
            );
        }
    }

    fn compose_geo(&self, vp: &Viewport, top: &TopBarGeo) -> (UiRect, UiRect, UiRect, UiRect, UiRect, UiRect) {
        let usable_b = vp.usable_bottom(self.ime_open);
        let mut y = top.bar.bottom() + vp.d(8);
        let banner_h = if !self.tel_status.sms_available { vp.d(24).clamp(20, 28) } else { 0 };
        let banner_r = UiRect::new(vp.content_x, y, vp.content_w, banner_h);
        if banner_h > 0 {
            y = banner_r.bottom() + vp.d(8);
        }

        let rec_h = vp.d(46).clamp(40, 52);
        let pick_w = vp.d(74).clamp(62, 92);
        let rec_r = UiRect::new(vp.content_x, y, (vp.content_w - pick_w - vp.d(8)).max(120), rec_h);
        let pick_r = UiRect::new(rec_r.right() + vp.d(8), y, pick_w, rec_h);

        let btn_h = vp.d(38).clamp(34, 44);
        let act_y = (usable_b - btn_h - vp.d(8)).max(rec_r.bottom() + vp.d(64));
        let body_y = rec_r.bottom() + vp.d(8);
        let body_h = (act_y - vp.d(8) - body_y).clamp(vp.d(56), vp.d(148));
        let body_r = UiRect::new(vp.content_x, body_y, vp.content_w, body_h);

        let gap = vp.d(10);
        let bw = (vp.content_w - gap) / 2;
        let cancel_r = UiRect::new(vp.content_x, act_y, bw, btn_h);
        let send_r = UiRect::new(vp.content_x + bw + gap, act_y, bw, btn_h);
        (banner_r, rec_r, pick_r, body_r, cancel_r, send_r)
    }

    fn draw_compose(&self, gfx: &mut Gfx<'_>, vp: &Viewport, th: &MobileTheme, tok: &SurfaceTokens, top: &TopBarGeo) {
        let (banner_r, rec_r, pick_r, body_r, cancel_r, send_r) = self.compose_geo(vp, top);

        if banner_r.h > 0 {
            draw_status_badge(
                gfx,
                banner_r,
                AMBER,
                vp.d(7).max(7),
                th.tr("Modem yok — gönderim devre dışı (taslak saklanır)", "No modem — sending unavailable (drafts saved)"),
            );
        }

        draw_text_field(
            gfx,
            vp,
            tok,
            rec_r,
            th.tr("Alıcı", "To"),
            s_of(&self.recipient, self.recipient_len),
            th.tr("Telefon numarası...", "Phone number..."),
            self.field == Field::Recipient,
            None,
        );
        draw_btn(gfx, tok, pick_r, BtnKind::Secondary, self.pressed_id == 5, vp.d(8).max(7), th.tr("Kişiler", "Contacts"));

        // Multi-line wrapped message composer
        let b_active = self.field == Field::Body;
        gfx.panel(
            body_r.x,
            body_r.y,
            body_r.w,
            body_r.h,
            vp.d(14).clamp(10, 18),
            tok.surface,
            255,
            if b_active { tok.accent } else { tok.border },
            if b_active { 235 } else { tok.border_alpha.max(26) },
        );
        let pad = vp.d(12);
        let lbl_cap = vp.d(7).max(7);
        let body_cap = vp.d(9).clamp(8, 12);
        gfx.text(body_r.x + pad, body_r.y + vp.d(8), lbl_cap, if b_active { tok.accent } else { tok.muted }, 1, th.tr("Mesaj", "Message"));

        // Honest character count indicator
        let char_cnt = s_of(&self.body, self.body_len).chars().count() as u32;
        let mut cnt_l = Line::new();
        cnt_l.number(char_cnt, 1).text(" / 160");
        gfx.text(body_r.right() - vp.d(58), body_r.y + vp.d(8), lbl_cap, tok.subtle, 1, cnt_l.as_str());

        let text_top = body_r.y + vp.d(24);
        let line_h = body_cap + vp.d(5);
        let max_lines = (((body_r.bottom() - text_top - vp.d(6)) / line_h).clamp(1, 5)) as usize;
        if self.body_len == 0 {
            gfx.text(body_r.x + pad, text_top, body_cap, tok.subtle, 1, th.tr("Mesajınızı yazın...", "Type your message..."));
        } else {
            let mut lines = [""; 5];
            let n = wrap_text_px(s_of(&self.body, self.body_len), body_r.w - pad * 2, body_cap, &mut lines[..max_lines]);
            for (i, line) in lines[..n].iter().enumerate() {
                gfx.text(body_r.x + pad, text_top + i as i32 * line_h, body_cap, tok.ink, 1, line);
            }
        }

        // Bottom Cancel / Send actions
        draw_btn(gfx, tok, cancel_r, BtnKind::Secondary, self.pressed_id == 6, vp.d(9).clamp(8, 11), th.tr("Taslağı Sakla", "Keep Draft"));
        let can_send = self.tel_status.sms_available && self.recipient_len > 0 && self.body_len > 0;
        let send_lbl = if self.last_send_failed && can_send {
            th.tr("Tekrar Dene", "Retry")
        } else {
            th.tr("Gönder", "Send")
        };
        draw_btn(
            gfx,
            tok,
            send_r,
            if can_send { BtnKind::Primary } else { BtnKind::Disabled },
            self.pressed_id == 7,
            vp.d(9).clamp(8, 11),
            send_lbl,
        );
    }

    fn draw_pick_contact(&self, gfx: &mut Gfx<'_>, vp: &Viewport, th: &MobileTheme, tok: &SurfaceTokens, top: &TopBarGeo) {
        let usable_b = vp.usable_bottom(self.ime_open);
        let y0 = top.bar.bottom() + vp.d(10);

        if self.contacts_perm != PermState::Granted {
            let area = UiRect::new(vp.content_x, y0, vp.content_w, (usable_b - y0 - vp.d(12)).max(150));
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
                        "Alıcı seçmek için Ayarlar > Uygulama İzinleri'nden Kişiler okuma iznini açın.",
                        "Enable Contacts Read in Settings > App Permissions to pick a recipient.",
                    )
                } else {
                    th.tr(
                        "Kayıtlı kişilerinizden alıcı seçmek için okuma izni gerekir.",
                        "Allow Contacts Read to choose a recipient from your saved contacts.",
                    )
                },
                Some(if denied { th.tr("Ayarları Aç", "Open Settings") } else { th.tr("İzin İste", "Allow Access") }),
                self.pressed_id == 11,
            );
            return;
        }

        let search_h = vp.d(36).clamp(32, 42);
        let search_r = UiRect::new(vp.content_x, y0, vp.content_w, search_h);
        draw_search_bar(
            gfx,
            vp,
            tok,
            search_r,
            s_of(&self.picker_search, self.picker_search_len),
            th.tr("Kişi veya numara ara...", "Search contact or number..."),
            self.picker_search_active,
        );

        let list_y = search_r.bottom() + vp.d(10);
        let pager_h = if self.contact_count > 5 { vp.d(36) } else { 0 };
        let list_h = (usable_b - pager_h - vp.d(10) - list_y).max(vp.d(96));

        if self.contact_count == 0 {
            draw_state_view(
                gfx,
                vp,
                tok,
                UiRect::new(vp.content_x, list_y, vp.content_w, list_h),
                UiIcon::Person,
                tok.muted,
                th.tr("Kişi bulunamadı", "No contacts found"),
                th.tr("Kişiler uygulamasında kişi ekleyin veya numarayı elle yazın.", "Add contacts in the Contacts app or enter a number manually."),
                None,
                false,
            );
            return;
        }

        let row_h = vp.d(52).clamp(44, 60);
        let visible = ((list_h - vp.d(8)) / row_h).clamp(2, 7) as usize;
        let card = UiRect::new(vp.content_x, list_y, vp.content_w, (visible as i32 * row_h + vp.d(8)).min(list_h));
        gfx.panel(card.x, card.y, card.w, card.h, vp.d(16), tok.surface, 255, tok.border, tok.border_alpha);

        let av_r = vp.d(16).clamp(13, 20);
        let tx = card.x + vp.d(52);
        let max_tw = (card.w - vp.d(66)).max(80);
        let name_cap = vp.d(10).clamp(9, 12);
        let sub_cap = vp.d(8).clamp(7, 10);

        for slot in 0..visible {
            let idx = self.picker_scroll + slot;
            if idx >= self.contact_count {
                break;
            }
            let c = &self.contacts[idx];
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
        }

        if self.contact_count > visible {
            let py = card.bottom() + vp.d(6);
            let ph = vp.d(28).clamp(24, 34);
            let bw = vp.d(84).clamp(70, 100);
            draw_btn(gfx, tok, UiRect::new(card.x, py, bw, ph), if self.picker_scroll > 0 { BtnKind::Secondary } else { BtnKind::Disabled }, false, vp.d(8).max(7), th.tr("Önceki", "Prev"));
            draw_btn(gfx, tok, UiRect::new(card.right() - bw, py, bw, ph), if self.picker_scroll + visible < self.contact_count { BtnKind::Secondary } else { BtnKind::Disabled }, false, vp.d(8).max(7), th.tr("Sonraki", "Next"));
        }
    }

    fn handle_click(&mut self, ev: &EventBlock, x: i32, y: i32) -> u32 {
        let vp = Viewport::from_event(ev);
        let en = ev.lang == 1;
        let pad = vp.d(6);
        let top = TopBarGeo::compute(&vp, 36);

        if self.screen != Screen::Inbox && top.back_btn.expand(pad).contains(x, y) {
            self.hide_ime();
            self.picker_search_active = false;
            self.save_draft();
            self.screen = if self.screen == Screen::PickContact { Screen::Compose } else { Screen::Inbox };
            return RET_REDRAW;
        }

        match self.screen {
            Screen::Inbox => {
                if top.trailing_btn.expand(pad).contains(x, y) {
                    self.screen = Screen::Compose;
                    self.field = if self.recipient_len == 0 { Field::Recipient } else { Field::Body };
                    self.show_ime();
                    return RET_REDRAW;
                }
                if self.read_perm != PermState::Granted {
                    if self.read_perm == PermState::Denied {
                        let _ = perm::open_settings();
                    } else {
                        let _ = perm::request(PERM_SMS_READ);
                    }
                    return RET_REDRAW;
                }
                let mut y0 = top.bar.bottom() + vp.d(12);
                if self.has_draft() {
                    let dh = vp.d(62).clamp(52, 72);
                    let dr = vp.row_rect(y0, dh);
                    let clr_s = vp.d(28).clamp(24, 32);
                    let clr_r = UiRect::new(dr.right() - clr_s - vp.d(10), dr.cy() - clr_s / 2, clr_s, clr_s);
                    if clr_r.expand(pad).contains(x, y) {
                        self.clear_draft();
                        self.toast.show(ev.uptime_ms, 0x0096_A1B2, if en { "Draft discarded" } else { "Taslak silindi" });
                        return RET_REDRAW;
                    }
                    if dr.contains(x, y) {
                        self.screen = Screen::Compose;
                        self.field = if self.recipient_len == 0 { Field::Recipient } else { Field::Body };
                        self.show_ime();
                        return RET_REDRAW;
                    }
                    y0 = dr.bottom() + vp.d(12);
                }
                if y >= y0 {
                    self.screen = Screen::Compose;
                    self.field = Field::Recipient;
                    self.show_ime();
                    return RET_REDRAW;
                }
            }
            Screen::Compose => {
                let (_, rec_r, pick_r, body_r, cancel_r, send_r) = self.compose_geo(&vp, &top);
                if rec_r.contains(x, y) {
                    self.field = Field::Recipient;
                    self.show_ime();
                    return RET_REDRAW;
                }
                if pick_r.expand(pad).contains(x, y) {
                    self.hide_ime();
                    self.save_draft();
                    self.screen = Screen::PickContact;
                    self.picker_scroll = 0;
                    if self.contacts_perm == PermState::Ask && !self.prompted_contacts {
                        self.prompted_contacts = true;
                        let _ = perm::request(PERM_CONTACTS_READ);
                    }
                    self.refresh_all(false);
                    return RET_REDRAW;
                }
                if body_r.contains(x, y) {
                    self.field = Field::Body;
                    self.show_ime();
                    return RET_REDRAW;
                }
                if cancel_r.expand(pad).contains(x, y) {
                    self.hide_ime();
                    self.save_draft();
                    self.screen = Screen::Inbox;
                    return RET_REDRAW;
                }
                if send_r.expand(pad).contains(x, y) {
                    self.hide_ime();
                    self.save_draft();
                    self.attempt_send(ev.uptime_ms, en);
                    return RET_REDRAW;
                }
            }
            Screen::PickContact => {
                let usable_b = vp.usable_bottom(self.ime_open);
                let y0 = top.bar.bottom() + vp.d(10);
                if self.contacts_perm != PermState::Granted {
                    if self.contacts_perm == PermState::Denied {
                        let _ = perm::open_settings();
                    } else {
                        let _ = perm::request(PERM_CONTACTS_READ);
                    }
                    return RET_REDRAW;
                }
                let search_h = vp.d(36).clamp(32, 42);
                let search_r = UiRect::new(vp.content_x, y0, vp.content_w, search_h);
                let clear_r = UiRect::new(search_r.right() - search_r.h, search_r.y, search_r.h, search_r.h);
                if self.picker_search_len > 0 && clear_r.expand(pad).contains(x, y) {
                    self.picker_search_len = 0;
                    self.picker_search_active = false;
                    self.hide_ime();
                    self.reload_contacts();
                    return RET_REDRAW;
                }
                if search_r.contains(x, y) {
                    self.picker_search_active = true;
                    self.show_ime();
                    return RET_REDRAW;
                }
                let list_y = search_r.bottom() + vp.d(10);
                let pager_h = if self.contact_count > 5 { vp.d(36) } else { 0 };
                let list_h = (usable_b - pager_h - vp.d(10) - list_y).max(vp.d(96));
                let row_h = vp.d(52).clamp(44, 60);
                let visible = ((list_h - vp.d(8)) / row_h).clamp(2, 7) as usize;
                let card = UiRect::new(vp.content_x, list_y, vp.content_w, (visible as i32 * row_h + vp.d(8)).min(list_h));

                for slot in 0..visible {
                    let idx = self.picker_scroll + slot;
                    if idx >= self.contact_count {
                        break;
                    }
                    let ry = card.y + vp.d(4) + slot as i32 * row_h;
                    if UiRect::new(card.x, ry, card.w, row_h).contains(x, y) {
                        let chosen = if self.contacts[idx].phone_str().is_empty() {
                            self.contacts[idx].name_str()
                        } else {
                            self.contacts[idx].phone_str()
                        };
                        let mut tmp = [0u8; 32];
                        let n = copy_str(&mut tmp, chosen);
                        self.recipient[..n].copy_from_slice(&tmp[..n]);
                        self.recipient_len = n;
                        self.draft_dirty = true;
                        self.save_draft();
                        self.picker_search_active = false;
                        self.screen = Screen::Compose;
                        self.field = Field::Body;
                        self.show_ime();
                        return RET_REDRAW;
                    }
                }
                if self.contact_count > visible {
                    let py = card.bottom() + vp.d(6);
                    let ph = vp.d(28).clamp(24, 34);
                    let bw = vp.d(84).clamp(70, 100);
                    if UiRect::new(card.x, py, bw, ph).expand(pad).contains(x, y) && self.picker_scroll > 0 {
                        self.picker_scroll = self.picker_scroll.saturating_sub(visible);
                        return RET_REDRAW;
                    }
                    if UiRect::new(card.right() - bw, py, bw, ph).expand(pad).contains(x, y) && self.picker_scroll + visible < self.contact_count {
                        self.picker_scroll += visible;
                        return RET_REDRAW;
                    }
                }
            }
        }
        RET_NONE
    }
}

impl RingApp for MessagesApp {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.last_flags = ev.flags;
                self.last_accent = ev.accent;
                self.last_lang = ev.lang;
                self.load_draft();
                self.refresh_all(true);
                RET_REDRAW
            }
            OP_PAUSE => {
                self.hide_ime();
                self.save_draft();
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
                let r_now = perm::check(PERM_SMS_READ).state;
                let s_now = perm::check(PERM_SMS_SEND).state;
                let c_now = perm::check(PERM_CONTACTS_READ).state;
                if (r_now, s_now, c_now) != (self.read_perm, self.send_perm, self.contacts_perm) {
                    self.read_perm = r_now;
                    self.send_perm = s_now;
                    self.contacts_perm = c_now;
                    if self.screen == Screen::PickContact {
                        self.reload_contacts();
                    }
                    ret |= RET_REDRAW;
                    if self.pending_send && self.send_perm != PermState::Ask {
                        self.pending_send = false;
                        self.attempt_send(ev.uptime_ms, ev.lang == 1);
                    }
                }
                if ev.key == key::ESCAPE {
                    self.hide_ime();
                    self.picker_search_active = false;
                    self.save_draft();
                    if self.screen != Screen::Inbox {
                        self.screen = if self.screen == Screen::PickContact { Screen::Compose } else { Screen::Inbox };
                        return RET_REDRAW;
                    }
                    return RET_CLOSE;
                }
                if self.screen == Screen::Compose {
                    if ev.key == key::ENTER && self.field == Field::Recipient {
                        self.field = Field::Body;
                        return RET_REDRAW;
                    }
                    if ev.key == key::BACKSPACE {
                        match self.field {
                            Field::Recipient => pop_char(&self.recipient, &mut self.recipient_len),
                            Field::Body => pop_char(&self.body, &mut self.body_len),
                        }
                        self.draft_dirty = true;
                        return RET_REDRAW;
                    }
                    if let Some(c) = key::as_char(ev.key) {
                        if !c.is_control() {
                            match self.field {
                                Field::Recipient => push_char(&mut self.recipient, &mut self.recipient_len, c),
                                Field::Body => push_char(&mut self.body, &mut self.body_len, c),
                            }
                            self.draft_dirty = true;
                            return RET_REDRAW;
                        }
                    }
                } else if self.screen == Screen::PickContact && self.picker_search_active {
                    if ev.key == key::BACKSPACE {
                        pop_char(&self.picker_search, &mut self.picker_search_len);
                        self.reload_contacts();
                        return RET_REDRAW;
                    }
                    if ev.key == key::ENTER {
                        self.picker_search_active = false;
                        self.hide_ime();
                        return RET_REDRAW;
                    }
                    if let Some(c) = key::as_char(ev.key) {
                        if !c.is_control() {
                            push_char(&mut self.picker_search, &mut self.picker_search_len, c);
                            self.reload_contacts();
                            return RET_REDRAW;
                        }
                    }
                }
                if let Some(p) = pointer(ev) {
                    if p.clicked {
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

static mut APP: MessagesApp = MessagesApp::new();

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
