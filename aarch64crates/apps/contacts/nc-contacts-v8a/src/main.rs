//! NetizenCore Independent Ring 3 / EL0 System App: Contacts (`nc.contacts`).
//!
//! - Follows live NetizenHome OS theme (light/dark, accent color, TR/EN localization).
//! - Requests `contacts.read` for listing/searching and `contacts.write` ONLY when
//!   creating, editing, or deleting contacts (never on initial read-only browse).
//! - All contacts persist exclusively via the trusted kernel Contacts service (`nc_app_sdk::contacts`).
//! - Favorites are persisted by real contact ID inside `/data/appdata/nc.contacts/favorites.cfg`.
//! - Features: instant live search, animated All/Favorites segmented filter, A-Z/Z-A sort,
//!   deterministic Turkish-aware monogram avatars, multi-select batch delete with confirmation sheet,
//!   keyboard-aware create/edit form with inline validation and draft preservation.
#![no_std]
#![no_main]

use nc_abi::draw::ICON_CONTACTS;
use nc_abi::event::{
    EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE, RET_ANIMATING, RET_CLOSE,
    RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_abi::perm::{PermState, PERM_CONTACTS_READ, PERM_CONTACTS_WRITE};
use nc_app_sdk::contacts::{self, ContactEntry};
use nc_app_sdk::telephony::{self, TelephonyStatus};
use nc_app_sdk::{dispatch, ime, perm, pointer, status, storage, sys, Gfx, Line, RingApp};
use nc_mobile_ui::{
    draw_btn, draw_confirm_sheet, draw_icon_btn, draw_monogram_avatar, draw_search_bar,
    draw_segmented_bar, draw_state_view, draw_status_badge, draw_text_field, draw_toast,
    draw_top_bar, draw_ui_icon, truncate_to_px, wrap_text_px, BtnKind, MobileTheme, SurfaceTokens,
    ToastState, TopBarGeo, Tween, UiIcon, UiRect, Viewport, AMBER, CORAL, GREEN, ONE, SPEC_SHEET,
    SPEC_TAB, WHITE,
};

const MAX_LIST: usize = 64;
const MAX_FAVS: usize = 48;
const FAV_PATH: &str = "/data/appdata/nc.contacts/favorites.cfg";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    List,
    Detail,
    Edit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditField {
    Name,
    Phone,
    Email,
    Note,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingWrite {
    None,
    Save,
    DeleteSingle,
    DeleteBatch,
}

struct ContactsApp {
    screen: Screen,
    read_state: PermState,
    write_state: PermState,
    prompted_read: bool,
    pending_write: PendingWrite,
    tel_status: TelephonyStatus,

    entries: [ContactEntry; MAX_LIST],
    count: usize,
    // Indices into `entries` matching current filter/sort
    view_idx: [u8; MAX_LIST],
    view_count: usize,
    scroll: usize,
    selected_entry: usize, // index into `entries`

    // Filter & sort
    fav_only: bool,
    sort_desc: bool,
    fav_ids: [u32; MAX_FAVS],
    fav_count: usize,
    favs_loaded: bool,

    // Multi-select
    select_mode: bool,
    selected_mask: u64,

    // Search
    search_active: bool,
    search: [u8; 32],
    search_len: usize,

    // Edit form (preserved across redraws and IME toggles)
    edit_id: u32,
    edit_field: EditField,
    f_name: [u8; 48],
    f_name_len: usize,
    f_phone: [u8; 28],
    f_phone_len: usize,
    f_email: [u8; 48],
    f_email_len: usize,
    f_note: [u8; 64],
    f_note_len: usize,
    inline_err: bool,
    ime_open: bool,

    // Confirmation sheet & animations
    confirm_delete_open: bool,
    confirm_batch: bool,
    reduced_motion: bool,
    tab_tween: Tween,
    sheet_tween: Tween,
    pressed_id: u8,
    pressed_sheet_btn: u8,
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

struct ListGeo {
    search_r: UiRect,
    seg_r: UiRect,
    sort_r: UiRect,
    list_box: UiRect,
    row_h: i32,
    visible_rows: usize,
    pager_prev: UiRect,
    pager_next: UiRect,
    batch_bar: UiRect,
}

impl ListGeo {
    fn compute(vp: &Viewport, ime_open: bool, select_mode: bool) -> Self {
        let top = TopBarGeo::compute(vp, 36);
        let search_h = vp.d(36).clamp(32, 42);
        let search_r = UiRect::new(vp.content_x, top.bar.bottom() + vp.d(8), vp.content_w, search_h);

        let seg_h = vp.d(32).clamp(28, 38);
        let seg_y = search_r.bottom() + vp.d(8);
        let sort_w = vp.d(52).clamp(44, 62);
        let seg_r = UiRect::new(vp.content_x, seg_y, (vp.content_w - sort_w - vp.d(8)).max(120), seg_h);
        let sort_r = UiRect::new(seg_r.right() + vp.d(8), seg_y, sort_w, seg_h);

        let usable_b = vp.usable_bottom(ime_open);
        let bot_reserve = if select_mode { vp.d(54) } else { vp.d(40) };
        let list_y = seg_r.bottom() + vp.d(10);
        let list_h = (usable_b - bot_reserve - list_y).max(vp.d(96));
        let row_h = vp.d(54).clamp(46, 62);
        let visible_rows = ((list_h - vp.d(8)) / row_h).clamp(2, 8) as usize;
        let actual_list_h = (visible_rows as i32 * row_h + vp.d(8)).min(list_h);
        let list_box = UiRect::new(vp.content_x, list_y, vp.content_w, actual_list_h);

        let py = list_box.bottom() + vp.d(6);
        let ph = vp.d(30).clamp(26, 34);
        let bw = vp.d(88).clamp(72, 108);
        let pager_prev = UiRect::new(vp.content_x, py, bw, ph);
        let pager_next = UiRect::new(vp.content_x + vp.content_w - bw, py, bw, ph);
        let batch_bar = UiRect::new(0, (usable_b - vp.d(52)).max(list_box.bottom() + 2), vp.area_w, vp.d(52));

        Self { search_r, seg_r, sort_r, list_box, row_h, visible_rows, pager_prev, pager_next, batch_bar }
    }
}

impl ContactsApp {
    const fn new() -> Self {
        Self {
            screen: Screen::List,
            read_state: PermState::Ask,
            write_state: PermState::Ask,
            prompted_read: false,
            pending_write: PendingWrite::None,
            tel_status: TelephonyStatus { voice_available: false, sms_available: false, call_log_available: false },
            entries: [ContactEntry::EMPTY; MAX_LIST],
            count: 0,
            view_idx: [0; MAX_LIST],
            view_count: 0,
            scroll: 0,
            selected_entry: 0,
            fav_only: false,
            sort_desc: false,
            fav_ids: [0; MAX_FAVS],
            fav_count: 0,
            favs_loaded: false,
            select_mode: false,
            selected_mask: 0,
            search_active: false,
            search: [0; 32],
            search_len: 0,
            edit_id: 0,
            edit_field: EditField::Name,
            f_name: [0; 48],
            f_name_len: 0,
            f_phone: [0; 28],
            f_phone_len: 0,
            f_email: [0; 48],
            f_email_len: 0,
            f_note: [0; 64],
            f_note_len: 0,
            inline_err: false,
            ime_open: false,
            confirm_delete_open: false,
            confirm_batch: false,
            reduced_motion: false,
            tab_tween: Tween::new(),
            sheet_tween: Tween::new(),
            pressed_id: 0,
            pressed_sheet_btn: 0,
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
        let _ = storage::mkdir("/data/appdata/nc.contacts");
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

    fn rebuild_view(&mut self) {
        self.view_count = 0;
        // Put favorites first in All mode (or filter strictly in Favorites mode)
        for pass in 0..2 {
            if self.fav_only && pass == 1 {
                break;
            }
            for i in 0..self.count {
                let fav = self.is_fav(self.entries[i].id);
                let match_pass = if self.fav_only { fav } else { (pass == 0) == fav };
                if match_pass && self.view_count < MAX_LIST {
                    self.view_idx[self.view_count] = i as u8;
                    self.view_count += 1;
                }
            }
        }
        if self.sort_desc {
            // Reverse within non-favorite / all ordering
            self.view_idx[..self.view_count].reverse();
        }
        if self.scroll >= self.view_count {
            self.scroll = 0;
        }
    }

    fn sync_permissions(&mut self, request_read_if_ask: bool) {
        let r = perm::check(PERM_CONTACTS_READ);
        let w = perm::check(PERM_CONTACTS_WRITE);
        self.read_state = r.state;
        self.write_state = w.state;
        if request_read_if_ask && self.read_state == PermState::Ask && !self.prompted_read && !r.pending {
            self.prompted_read = true;
            let _ = perm::request(PERM_CONTACTS_READ);
        }
        if self.read_state == PermState::Granted {
            self.reload_list();
        } else {
            self.count = 0;
            self.view_count = 0;
        }
    }

    fn reload_list(&mut self) {
        let buf = unsafe { &mut *core::ptr::addr_of_mut!(RESP_BUF) };
        let res = if self.search_len > 0 {
            let mut q = [0u8; 32];
            let qlen = self.search_len;
            q[..qlen].copy_from_slice(&self.search[..qlen]);
            contacts::lookup(s_of(&q, qlen), buf, &mut self.entries)
        } else {
            contacts::list(buf, &mut self.entries)
        };
        match res {
            Ok(cnt) => {
                self.count = cnt;
                self.rebuild_view();
            }
            Err(_) => {
                self.count = 0;
                self.view_count = 0;
            }
        }
    }

    fn show_ime(&mut self) {
        self.ime_open = true;
        let _ = ime::show();
    }

    fn hide_ime(&mut self) {
        self.ime_open = false;
        let _ = ime::hide();
    }

    fn start_new(&mut self) {
        self.edit_id = 0;
        self.edit_field = EditField::Name;
        self.f_name_len = 0;
        self.f_phone_len = 0;
        self.f_email_len = 0;
        self.f_note_len = 0;
        self.inline_err = false;
        self.search_active = false;
        self.select_mode = false;
        self.selected_mask = 0;
        self.screen = Screen::Edit;
        self.show_ime();
    }

    fn start_edit_selected(&mut self) {
        let Some(c) = self.entries.get(self.selected_entry).copied() else { return };
        self.edit_id = c.id;
        self.edit_field = EditField::Name;
        self.f_name_len = copy_str(&mut self.f_name, c.name_str());
        self.f_phone_len = copy_str(&mut self.f_phone, c.phone_str());
        self.f_email_len = copy_str(&mut self.f_email, c.email_str());
        self.f_note_len = copy_str(&mut self.f_note, c.note_str());
        self.inline_err = false;
        self.screen = Screen::Edit;
        self.show_ime();
    }

    fn commit_save(&mut self, now_ms: u32, en: bool) {
        let name = s_of(&self.f_name, self.f_name_len);
        let phone = s_of(&self.f_phone, self.f_phone_len);
        if name.trim().is_empty() && phone.trim().is_empty() {
            self.inline_err = true;
            return;
        }
        self.inline_err = false;

        let w = perm::check(PERM_CONTACTS_WRITE);
        self.write_state = w.state;
        if self.write_state == PermState::Ask {
            self.pending_write = PendingWrite::Save;
            let _ = perm::request(PERM_CONTACTS_WRITE);
            return;
        }
        if self.write_state != PermState::Granted {
            self.toast.show(
                now_ms,
                CORAL,
                if en { "Write permission denied (open Settings)" } else { "Yazma izni reddedildi (Ayarlar'dan açın)" },
            );
            return;
        }
        let email = s_of(&self.f_email, self.f_email_len);
        let note = s_of(&self.f_note, self.f_note_len);
        match contacts::upsert(self.edit_id, name, phone, email, note) {
            Ok(_) => {
                self.hide_ime();
                self.pending_write = PendingWrite::None;
                self.reload_list();
                self.screen = Screen::List;
                self.toast.show(now_ms, GREEN, if en { "Contact saved" } else { "Kişi kaydedildi" });
            }
            Err(_) => {
                self.toast.show(now_ms, CORAL, if en { "Could not save contact" } else { "Kişi kaydedilemedi" });
            }
        }
    }

    fn commit_delete_single(&mut self, now_ms: u32, en: bool) {
        let w = perm::check(PERM_CONTACTS_WRITE);
        self.write_state = w.state;
        if self.write_state == PermState::Ask {
            self.pending_write = PendingWrite::DeleteSingle;
            let _ = perm::request(PERM_CONTACTS_WRITE);
            return;
        }
        if self.write_state != PermState::Granted {
            self.confirm_delete_open = false;
            self.toast.show(now_ms, CORAL, if en { "Write permission denied" } else { "Silme izni reddedildi" });
            return;
        }
        let Some(c) = self.entries.get(self.selected_entry).copied() else { return };
        if contacts::delete(c.id).is_ok() {
            self.pending_write = PendingWrite::None;
            self.confirm_delete_open = false;
            self.reload_list();
            self.screen = Screen::List;
            self.toast.show(now_ms, GREEN, if en { "Contact deleted" } else { "Kişi silindi" });
        } else {
            self.confirm_delete_open = false;
            self.toast.show(now_ms, CORAL, if en { "Delete failed" } else { "Silinemedi" });
        }
    }

    fn commit_delete_batch(&mut self, now_ms: u32, en: bool) {
        let w = perm::check(PERM_CONTACTS_WRITE);
        self.write_state = w.state;
        if self.write_state == PermState::Ask {
            self.pending_write = PendingWrite::DeleteBatch;
            let _ = perm::request(PERM_CONTACTS_WRITE);
            return;
        }
        if self.write_state != PermState::Granted {
            self.confirm_delete_open = false;
            self.toast.show(now_ms, CORAL, if en { "Write permission denied" } else { "Silme izni reddedildi" });
            return;
        }
        let mut deleted = 0u32;
        for i in 0..self.count {
            if ((self.selected_mask >> i) & 1) != 0 {
                let id = self.entries[i].id;
                if contacts::delete(id).is_ok() {
                    deleted += 1;
                }
            }
        }
        self.pending_write = PendingWrite::None;
        self.confirm_delete_open = false;
        self.select_mode = false;
        self.selected_mask = 0;
        self.reload_list();
        if deleted > 0 {
            self.toast.show(now_ms, GREEN, if en { "Selected contacts deleted" } else { "Seçili kişiler silindi" });
        }
    }

    fn step_animations(&mut self, now_ms: u32) -> bool {
        self.tab_tween.step(
            now_ms,
            if self.fav_only { ONE } else { 0 },
            &SPEC_TAB,
            self.reduced_motion,
        );
        self.sheet_tween.step(
            now_ms,
            if self.confirm_delete_open { ONE } else { 0 },
            &SPEC_SHEET,
            self.reduced_motion,
        );
        !self.tab_tween.settled() || !self.sheet_tween.settled()
    }

    fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        let th = MobileTheme::from_event(ev);
        let tok = th.tokens();
        let vp = Viewport::from_event(ev);
        gfx.fill_rect(0, 0, vp.area_w, vp.area_h, tok.bg);

        let show_back = self.screen != Screen::List;
        let top = TopBarGeo::compute(&vp, 36);
        let title = match self.screen {
            Screen::List => th.tr("Kişiler", "Contacts"),
            Screen::Detail => th.tr("Profil", "Profile"),
            Screen::Edit => if self.edit_id == 0 { th.tr("Yeni Kişi", "New Contact") } else { th.tr("Düzenle", "Edit Contact") },
        };
        draw_top_bar(
            gfx,
            &vp,
            &tok,
            &top,
            title,
            show_back,
            self.pressed_id == 1,
            if show_back { None } else { Some(ICON_CONTACTS) },
        );

        if self.read_state != PermState::Granted {
            let area = UiRect::new(
                vp.content_x,
                top.bar.bottom() + vp.d(12),
                vp.content_w,
                (vp.area_h - top.bar.bottom() - vp.d(24)).max(160),
            );
            let denied = self.read_state == PermState::Denied;
            draw_state_view(
                gfx,
                &vp,
                &tok,
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
                        "Kişilerinizi görüntülemek için Ayarlar > Uygulama İzinleri bölümünden izin verin.",
                        "Enable Contacts Read in Settings > App Permissions to browse your contacts.",
                    )
                } else {
                    th.tr(
                        "Kayıtlı kişilerinizi listelemek ve aramak için yalnızca okuma izni istenir.",
                        "Only read access is requested to view and search your saved contacts.",
                    )
                },
                Some(if denied {
                    th.tr("Ayarları Aç", "Open Settings")
                } else {
                    th.tr("İzin İste", "Allow Read Access")
                }),
                self.pressed_id == 99,
            );
            return;
        }

        match self.screen {
            Screen::List => self.draw_list(gfx, &vp, &th, &tok, &top),
            Screen::Detail => self.draw_detail(gfx, &vp, &th, &tok, &top),
            Screen::Edit => self.draw_edit(gfx, &vp, &th, &tok),
        }

        // Animated confirmation sheet for single or batch deletion
        if self.sheet_tween.value() > 0 {
            let msg = if self.confirm_batch {
                th.tr(
                    "Seçili kişiler kalıcı olarak silinecek. Bu işlem geri alınamaz.",
                    "Selected contacts will be permanently deleted. This cannot be undone.",
                )
            } else {
                th.tr(
                    "Bu kişi kalıcı olarak silinecek. Devam etmek istiyor musunuz?",
                    "This contact will be permanently deleted. Do you want to continue?",
                )
            };
            draw_confirm_sheet(
                gfx,
                &vp,
                &tok,
                self.sheet_tween.value(),
                th.tr("Kişiyi Sil", "Delete Contact"),
                msg,
                th.tr("İptal", "Cancel"),
                th.tr("Sil", "Delete"),
                true,
                self.pressed_sheet_btn,
            );
        }

        if self.toast.active {
            draw_toast(gfx, &vp, &tok, top.bar.bottom() + vp.d(8), self.toast.color, self.toast.text());
        }
    }

    fn draw_list(&self, gfx: &mut Gfx<'_>, vp: &Viewport, th: &MobileTheme, tok: &SurfaceTokens, top: &TopBarGeo) {
        // Header trailing actions: Add (+) and Select mode
        draw_icon_btn(gfx, tok, top.trailing_btn, UiIcon::Plus, true, false, self.pressed_id == 2);
        let sel_btn = UiRect::new(
            top.trailing_btn.x - top.trailing_btn.w - vp.d(8),
            top.trailing_btn.y,
            top.trailing_btn.w,
            top.trailing_btn.h,
        );
        draw_icon_btn(gfx, tok, sel_btn, UiIcon::Select, self.select_mode, self.view_count == 0, self.pressed_id == 3);

        let geo = ListGeo::compute(vp, self.ime_open, self.select_mode);
        draw_search_bar(
            gfx,
            vp,
            tok,
            geo.search_r,
            s_of(&self.search, self.search_len),
            th.tr("Ad, numara veya e-posta ara...", "Search name, phone or email..."),
            self.search_active,
        );

        let seg_labels = [th.tr("Tümü", "All"), th.tr("Favoriler", "Favorites")];
        draw_segmented_bar(gfx, tok, geo.seg_r, &seg_labels, self.tab_tween.value(), vp.d(8).clamp(8, 10));
        draw_btn(
            gfx,
            tok,
            geo.sort_r,
            BtnKind::Quiet,
            self.pressed_id == 4,
            vp.d(8).max(7),
            if self.sort_desc { "Z-A" } else { "A-Z" },
        );

        if self.view_count == 0 {
            let (et, eb, act) = if self.search_len > 0 {
                (
                    th.tr("Sonuç bulunamadı", "No matching contacts"),
                    th.tr("Arama terimini değiştirin veya temizleyin.", "Try a different query or clear search."),
                    Some(th.tr("Aramayı Temizle", "Clear Search")),
                )
            } else if self.fav_only {
                (
                    th.tr("Favori kişi yok", "No favorite contacts"),
                    th.tr("Sık görüştüğünüz kişileri profillerinden yıldızlayabilirsiniz.", "Star important people from their profile to pin them here."),
                    None,
                )
            } else {
                (
                    th.tr("Henüz kişi yok", "No contacts yet"),
                    th.tr("Kişileriniz güvenli sistem veritabanında saklanır.", "Your contacts are stored safely in the system database."),
                    Some(th.tr("Kişi Ekle", "Add Contact")),
                )
            };
            draw_state_view(gfx, vp, tok, geo.list_box, UiIcon::Person, tok.accent, et, eb, act, self.pressed_id == 5);
            return;
        }

        gfx.panel(
            geo.list_box.x,
            geo.list_box.y,
            geo.list_box.w,
            geo.list_box.h,
            vp.d(16).clamp(12, 20),
            tok.surface,
            255,
            tok.border,
            tok.border_alpha,
        );

        let av_r = vp.d(17).clamp(13, 20);
        let name_cap = vp.d(10).clamp(9, 12);
        let sub_cap = vp.d(8).clamp(7, 10);
        let text_x = geo.list_box.x + vp.d(54);
        let text_max_w = (geo.list_box.w - vp.d(90)).max(80);

        for slot in 0..geo.visible_rows {
            let vpos = self.scroll + slot;
            if vpos >= self.view_count {
                break;
            }
            let eidx = self.view_idx[vpos] as usize;
            let c = &self.entries[eidx];
            let ry = geo.list_box.y + vp.d(4) + slot as i32 * geo.row_h;
            if slot > 0 {
                gfx.fill_rect(text_x, ry, geo.list_box.right() - text_x - vp.d(14), 1, tok.divider);
            }
            let is_sel = self.select_mode && ((self.selected_mask >> eidx) & 1) != 0;
            if is_sel {
                gfx.panel(geo.list_box.x + 4, ry + 2, geo.list_box.w - 8, geo.row_h - 4, vp.d(12), tok.accent, 28, 0, 0);
            }

            let disp_name = if c.name_str().is_empty() { c.phone_str() } else { c.name_str() };
            let sub = if !c.phone_str().is_empty() { c.phone_str() } else { c.email_str() };

            draw_monogram_avatar(gfx, geo.list_box.x + vp.d(28), ry + geo.row_h / 2, av_r, disp_name, c.id);

            let mut nb = [0u8; 40];
            let mut sb = [0u8; 40];
            gfx.text(text_x, ry + geo.row_h * 22 / 100, name_cap, tok.ink, 2, truncate_to_px(disp_name, text_max_w, name_cap, &mut nb));
            if !sub.is_empty() {
                gfx.text(text_x, ry + geo.row_h * 58 / 100, sub_cap, tok.muted, 1, truncate_to_px(sub, text_max_w, sub_cap, &mut sb));
            }

            let right_cx = geo.list_box.right() - vp.d(18);
            let right_cy = ry + geo.row_h / 2;
            if self.select_mode {
                let cr = vp.d(9).clamp(7, 11);
                gfx.dot(right_cx, right_cy, cr, if is_sel { tok.accent } else { tok.surface_subtle }, 1024);
                if is_sel {
                    draw_ui_icon(gfx, UiIcon::Check, right_cx, right_cy, cr * 6 / 10, WHITE, tok.accent);
                }
            } else {
                if self.is_fav(c.id) {
                    gfx.dot(right_cx - vp.d(16), right_cy, vp.d(4).max(3), AMBER, 1024);
                }
                draw_ui_icon(gfx, UiIcon::ChevronRight, right_cx, right_cy, vp.d(6).max(5), tok.subtle, tok.surface);
            }
        }

        // Pager or multi-select action bar
        if self.select_mode {
            gfx.fill_rect(0, geo.batch_bar.y, vp.area_w, geo.batch_bar.h, tok.surface);
            gfx.fill_rect(0, geo.batch_bar.y, vp.area_w, 1, tok.divider);
            let bh = vp.d(36).clamp(30, 42);
            let by = geo.batch_bar.y + (geo.batch_bar.h - bh) / 2;
            let gap = vp.d(10);
            let bw = (vp.content_w - gap) / 2;
            let sel_cnt = self.selected_mask.count_ones();
            let mut del_l = Line::new();
            del_l.text(th.tr("Sil (", "Delete (")).number(sel_cnt, 1).text(")");
            draw_btn(gfx, tok, UiRect::new(vp.content_x, by, bw, bh), BtnKind::Quiet, self.pressed_id == 6, vp.d(9).max(8), th.tr("Vazgeç", "Cancel"));
            draw_btn(
                gfx,
                tok,
                UiRect::new(vp.content_x + bw + gap, by, bw, bh),
                if sel_cnt > 0 { BtnKind::Destructive } else { BtnKind::Disabled },
                self.pressed_id == 7,
                vp.d(9).max(8),
                del_l.as_str(),
            );
        } else if self.view_count > geo.visible_rows {
            draw_btn(
                gfx,
                tok,
                geo.pager_prev,
                if self.scroll > 0 { BtnKind::Secondary } else { BtnKind::Disabled },
                self.pressed_id == 8,
                vp.d(8).max(7),
                th.tr("Önceki", "Prev"),
            );
            let page_n = self.scroll / geo.visible_rows + 1;
            let page_tot = (self.view_count + geo.visible_rows - 1) / geo.visible_rows;
            let mut pl = Line::new();
            pl.number(page_n as u32, 1).text(" / ").number(page_tot as u32, 1);
            gfx.text_centered(vp.area_w / 2, geo.pager_prev.y + (geo.pager_prev.h - vp.d(8).max(8)) / 2, vp.d(8).max(8), tok.muted, 1, pl.as_str());
            draw_btn(
                gfx,
                tok,
                geo.pager_next,
                if self.scroll + geo.visible_rows < self.view_count { BtnKind::Secondary } else { BtnKind::Disabled },
                self.pressed_id == 9,
                vp.d(8).max(7),
                th.tr("Sonraki", "Next"),
            );
        }
    }

    fn draw_detail(&self, gfx: &mut Gfx<'_>, vp: &Viewport, th: &MobileTheme, tok: &SurfaceTokens, top: &TopBarGeo) {
        let Some(c) = self.entries.get(self.selected_entry) else { return };
        let fav = self.is_fav(c.id);

        // Header trailing: Edit & Favorite star
        draw_icon_btn(gfx, tok, top.trailing_btn, UiIcon::Edit, false, false, self.pressed_id == 11);
        let fav_btn = UiRect::new(
            top.trailing_btn.x - top.trailing_btn.w - vp.d(8),
            top.trailing_btn.y,
            top.trailing_btn.w,
            top.trailing_btn.h,
        );
        draw_icon_btn(
            gfx,
            tok,
            fav_btn,
            if fav { UiIcon::StarFilled } else { UiIcon::Star },
            false,
            false,
            self.pressed_id == 10,
        );

        let mut y = top.bar.bottom() + vp.d(14);
        let av_r = vp.d(28).clamp(22, 34);
        let disp_name = if c.name_str().is_empty() { c.phone_str() } else { c.name_str() };
        draw_monogram_avatar(gfx, vp.area_w / 2, y + av_r, av_r, disp_name, c.id);
        y += av_r * 2 + vp.d(10);

        let name_cap = vp.d(14).clamp(11, 17);
        let mut nb = [0u8; 48];
        gfx.text_centered(vp.area_w / 2, y, name_cap, tok.ink, 2, truncate_to_px(disp_name, vp.content_w, name_cap, &mut nb));
        y += name_cap + vp.d(14);

        // Structured contact fields card
        let pad = vp.d(16);
        let lbl_cap = vp.d(7).max(7);
        let val_cap = vp.d(10).clamp(9, 12);
        let mut note_lines = [""; 2];
        let note_str = if c.note_str().is_empty() { "-" } else { c.note_str() };
        let n_note = wrap_text_px(note_str, vp.content_w - pad * 2, val_cap, &mut note_lines).max(1);
        let row_h = vp.d(48).clamp(40, 54);
        let note_h = vp.d(28) + (n_note as i32) * (val_cap + vp.d(4));
        let card_h = row_h * 2 + note_h + vp.d(8);
        let card = vp.row_rect(y, card_h);
        gfx.panel(card.x, card.y, card.w, card.h, vp.d(16), tok.surface, 255, tok.border, tok.border_alpha);

        let fields: [(&str, &str); 2] = [
            (th.tr("Telefon", "Phone"), if c.phone_str().is_empty() { "-" } else { c.phone_str() }),
            (th.tr("E-posta", "Email"), if c.email_str().is_empty() { "-" } else { c.email_str() }),
        ];
        for (i, &(lbl, val)) in fields.iter().enumerate() {
            let fy = card.y + i as i32 * row_h;
            if i > 0 {
                gfx.fill_rect(card.x + pad, fy, card.w - pad * 2, 1, tok.divider);
            }
            gfx.text(card.x + pad, fy + vp.d(9), lbl_cap, tok.muted, 1, lbl);
            let mut vb = [0u8; 48];
            gfx.text(card.x + pad, fy + vp.d(24), val_cap, tok.ink, 2, truncate_to_px(val, card.w - pad * 2, val_cap, &mut vb));
        }

        let ny = card.y + row_h * 2;
        gfx.fill_rect(card.x + pad, ny, card.w - pad * 2, 1, tok.divider);
        gfx.text(card.x + pad, ny + vp.d(9), lbl_cap, tok.muted, 1, th.tr("Not", "Note"));
        for (li, line) in note_lines[..n_note].iter().enumerate() {
            gfx.text(card.x + pad, ny + vp.d(24) + li as i32 * (val_cap + vp.d(4)), val_cap, tok.ink, 1, line);
        }
        y = card.bottom() + vp.d(12);

        // Truthful telephony availability note (never fake Call/SMS buttons when modem is absent)
        if !self.tel_status.voice_available && !self.tel_status.sms_available {
            let badge_r = vp.row_rect(y, vp.d(26).clamp(24, 30));
            draw_status_badge(
                gfx,
                badge_r,
                AMBER,
                vp.d(7).max(7),
                th.tr("Arama / SMS bu cihazda kullanılamıyor", "Call / SMS unavailable on this device"),
            );
            y = badge_r.bottom() + vp.d(12);
        }

        // Action row: Edit & Delete
        let btn_h = vp.d(40).clamp(34, 46);
        let btn_y = y.min(vp.area_h - btn_h - vp.d(12));
        let gap = vp.d(10);
        let bw = (vp.content_w - gap) / 2;
        draw_btn(gfx, tok, UiRect::new(vp.content_x, btn_y, bw, btn_h), BtnKind::Primary, self.pressed_id == 11, vp.d(9).clamp(8, 11), th.tr("Düzenle", "Edit"));
        draw_btn(gfx, tok, UiRect::new(vp.content_x + bw + gap, btn_y, bw, btn_h), BtnKind::Destructive, self.pressed_id == 12, vp.d(9).clamp(8, 11), th.tr("Sil", "Delete"));
    }

    fn edit_layout(&self, vp: &Viewport) -> (i32, i32, i32, i32) {
        let top = TopBarGeo::compute(vp, 36);
        let usable_b = vp.usable_bottom(self.ime_open);
        let field_h = if vp.is_compact_h || self.ime_open { vp.d(44).clamp(38, 48) } else { vp.d(50).clamp(44, 56) };
        let field_gap = if self.ime_open { vp.d(8).max(6) } else { vp.d(12).max(8) };
        let btn_h = vp.d(38).clamp(34, 44);
        let base_y = top.bar.bottom() + vp.d(10);
        // Keep focused field and Save button comfortably above the visible IME
        let active_idx = match self.edit_field {
            EditField::Name => 0,
            EditField::Phone => 1,
            EditField::Email => 2,
            EditField::Note => 3,
        };
        let active_bottom = base_y + (active_idx + 1) * (field_h + field_gap) + btn_h + vp.d(8);
        let scroll_up = if active_bottom > usable_b { active_bottom - usable_b } else { 0 };
        (base_y - scroll_up, field_h, field_gap, btn_h)
    }

    fn draw_edit(&self, gfx: &mut Gfx<'_>, vp: &Viewport, th: &MobileTheme, tok: &SurfaceTokens) {
        let top = TopBarGeo::compute(vp, 36);
        let (y0, field_h, field_gap, btn_h) = self.edit_layout(vp);
        let usable_b = vp.usable_bottom(self.ime_open);

        let fields: [(EditField, &str, &str, &str); 4] = [
            (EditField::Name, th.tr("Ad Soyad", "Full Name"), s_of(&self.f_name, self.f_name_len), th.tr("Örn. Ayşe Yılmaz", "e.g. Jane Doe")),
            (EditField::Phone, th.tr("Telefon", "Phone"), s_of(&self.f_phone, self.f_phone_len), th.tr("+90 555 ...", "+1 555 ...")),
            (EditField::Email, th.tr("E-posta", "Email"), s_of(&self.f_email, self.f_email_len), th.tr("ad@ornek.com", "name@example.com")),
            (EditField::Note, th.tr("Not", "Note"), s_of(&self.f_note, self.f_note_len), th.tr("Kısa not...", "Optional note...")),
        ];

        for (i, &(kind, lbl, val, ph)) in fields.iter().enumerate() {
            let fy = y0 + i as i32 * (field_h + field_gap);
            if fy + field_h <= top.bar.bottom() || fy >= usable_b - btn_h - vp.d(6) {
                continue;
            }
            let err = if self.inline_err && kind == EditField::Name {
                Some(th.tr("Ad veya telefon numarası gerekli", "Enter a name or phone number"))
            } else {
                None
            };
            draw_text_field(gfx, vp, tok, vp.row_rect(fy, field_h), lbl, val, ph, self.edit_field == kind, err);
        }

        // Sticky Cancel / Save bar right above IME (or below form)
        let natural_btn_y = y0 + 4 * (field_h + field_gap) + vp.d(4);
        let btn_y = natural_btn_y.min(usable_b - btn_h - vp.d(6));
        let gap = vp.d(10);
        let bw = (vp.content_w - gap) / 2;
        draw_btn(gfx, tok, UiRect::new(vp.content_x, btn_y, bw, btn_h), BtnKind::Secondary, self.pressed_id == 20, vp.d(9).clamp(8, 11), th.tr("İptal", "Cancel"));
        draw_btn(gfx, tok, UiRect::new(vp.content_x + bw + gap, btn_y, bw, btn_h), BtnKind::Primary, self.pressed_id == 21, vp.d(9).clamp(8, 11), th.tr("Kaydet", "Save"));
    }

    fn handle_click(&mut self, ev: &EventBlock, x: i32, y: i32) -> u32 {
        let vp = Viewport::from_event(ev);
        let en = ev.lang == 1;
        let pad = vp.d(6);

        // Confirmation sheet intercepts clicks when open
        if self.confirm_delete_open {
            let sheet_w = vp.content_w.min(vp.d(360));
            let sheet_x = (vp.area_w - sheet_w) / 2;
            let btn_h = vp.d(40).clamp(34, 46);
            let pad_s = vp.d(18).max(12);
            let by = vp.area_h - vp.d(10) - pad_s - btn_h;
            let gap = vp.d(10).max(8);
            let bw = (sheet_w - pad_s * 2 - gap) / 2;
            let cancel_r = UiRect::new(sheet_x + pad_s, by, bw, btn_h);
            let confirm_r = UiRect::new(sheet_x + pad_s + bw + gap, by, bw, btn_h);
            if confirm_r.expand(pad).contains(x, y) {
                if self.confirm_batch {
                    self.commit_delete_batch(ev.uptime_ms, en);
                } else {
                    self.commit_delete_single(ev.uptime_ms, en);
                }
                return RET_REDRAW | RET_ANIMATING;
            }
            if cancel_r.expand(pad).contains(x, y) || y < by - vp.d(90) {
                self.confirm_delete_open = false;
                return RET_REDRAW | RET_ANIMATING;
            }
            return RET_NONE;
        }

        let top = TopBarGeo::compute(&vp, 36);
        if self.screen != Screen::List && top.back_btn.expand(pad).contains(x, y) {
            self.hide_ime();
            self.screen = Screen::List;
            return RET_REDRAW;
        }

        if self.read_state != PermState::Granted {
            if self.read_state == PermState::Denied {
                let _ = perm::open_settings();
            } else {
                let _ = perm::request(PERM_CONTACTS_READ);
            }
            return RET_REDRAW;
        }

        match self.screen {
            Screen::List => {
                if top.trailing_btn.expand(pad).contains(x, y) {
                    self.start_new();
                    return RET_REDRAW;
                }
                let sel_btn = UiRect::new(
                    top.trailing_btn.x - top.trailing_btn.w - vp.d(8),
                    top.trailing_btn.y,
                    top.trailing_btn.w,
                    top.trailing_btn.h,
                );
                if sel_btn.expand(pad).contains(x, y) && self.view_count > 0 {
                    self.select_mode = !self.select_mode;
                    self.selected_mask = 0;
                    self.hide_ime();
                    self.search_active = false;
                    return RET_REDRAW;
                }

                let geo = ListGeo::compute(&vp, self.ime_open, self.select_mode);
                let clear_r = UiRect::new(geo.search_r.right() - geo.search_r.h, geo.search_r.y, geo.search_r.h, geo.search_r.h);
                if self.search_len > 0 && clear_r.expand(pad).contains(x, y) {
                    self.search_len = 0;
                    self.search_active = false;
                    self.hide_ime();
                    self.reload_list();
                    return RET_REDRAW;
                }
                if geo.search_r.contains(x, y) {
                    self.search_active = true;
                    self.show_ime();
                    return RET_REDRAW;
                }
                if geo.seg_r.contains(x, y) {
                    let next_fav = x >= geo.seg_r.cx();
                    if next_fav != self.fav_only {
                        self.fav_only = next_fav;
                        self.rebuild_view();
                        return RET_REDRAW | RET_ANIMATING;
                    }
                    return RET_NONE;
                }
                if geo.sort_r.expand(pad).contains(x, y) {
                    self.sort_desc = !self.sort_desc;
                    self.rebuild_view();
                    return RET_REDRAW;
                }

                if self.view_count == 0 {
                    if geo.list_box.contains(x, y) {
                        if self.search_len > 0 {
                            self.search_len = 0;
                            self.search_active = false;
                            self.hide_ime();
                            self.reload_list();
                        } else if !self.fav_only {
                            self.start_new();
                        }
                        return RET_REDRAW;
                    }
                    return RET_NONE;
                }

                for slot in 0..geo.visible_rows {
                    let vpos = self.scroll + slot;
                    if vpos >= self.view_count {
                        break;
                    }
                    let ry = geo.list_box.y + vp.d(4) + slot as i32 * geo.row_h;
                    if UiRect::new(geo.list_box.x, ry, geo.list_box.w, geo.row_h).contains(x, y) {
                        let eidx = self.view_idx[vpos] as usize;
                        if self.select_mode {
                            self.selected_mask ^= 1u64 << eidx;
                        } else {
                            self.selected_entry = eidx;
                            self.search_active = false;
                            self.hide_ime();
                            self.screen = Screen::Detail;
                        }
                        return RET_REDRAW;
                    }
                }

                if self.select_mode && y >= geo.batch_bar.y {
                    let gap = vp.d(10);
                    let bw = (vp.content_w - gap) / 2;
                    if x < vp.content_x + bw + gap / 2 {
                        self.select_mode = false;
                        self.selected_mask = 0;
                        return RET_REDRAW;
                    } else if self.selected_mask.count_ones() > 0 {
                        self.confirm_batch = true;
                        self.confirm_delete_open = true;
                        return RET_REDRAW | RET_ANIMATING;
                    }
                } else if self.view_count > geo.visible_rows {
                    if geo.pager_prev.expand(pad).contains(x, y) && self.scroll > 0 {
                        self.scroll = self.scroll.saturating_sub(geo.visible_rows);
                        return RET_REDRAW;
                    }
                    if geo.pager_next.expand(pad).contains(x, y) && self.scroll + geo.visible_rows < self.view_count {
                        self.scroll += geo.visible_rows;
                        return RET_REDRAW;
                    }
                }
            }
            Screen::Detail => {
                let fav_btn = UiRect::new(
                    top.trailing_btn.x - top.trailing_btn.w - vp.d(8),
                    top.trailing_btn.y,
                    top.trailing_btn.w,
                    top.trailing_btn.h,
                );
                if fav_btn.expand(pad).contains(x, y) {
                    if let Some(c) = self.entries.get(self.selected_entry).copied() {
                        let now_f = self.toggle_fav(c.id);
                        self.rebuild_view();
                        self.toast.show(
                            ev.uptime_ms,
                            if now_f { AMBER } else { 0x0096_A1B2 },
                            if now_f {
                                if en { "Pinned to Favorites" } else { "Favorilere eklendi" }
                            } else if en {
                                "Removed from Favorites"
                            } else {
                                "Favorilerden çıkarıldı"
                            },
                        );
                        return RET_REDRAW;
                    }
                }
                if top.trailing_btn.expand(pad).contains(x, y) {
                    self.start_edit_selected();
                    return RET_REDRAW;
                }
                // Bottom action buttons (Edit / Delete)
                let btn_h = vp.d(40).clamp(34, 46);
                let gap = vp.d(10);
                let bw = (vp.content_w - gap) / 2;
                if y >= vp.area_h - vp.d(120) {
                    if x >= vp.content_x && x < vp.content_x + bw + gap / 2 && y >= vp.area_h - btn_h - vp.d(64) {
                        self.start_edit_selected();
                        return RET_REDRAW;
                    }
                    if x >= vp.content_x + bw + gap / 2 && x <= vp.content_x + vp.content_w && y >= vp.area_h - btn_h - vp.d(64) {
                        self.confirm_batch = false;
                        self.confirm_delete_open = true;
                        return RET_REDRAW | RET_ANIMATING;
                    }
                }
            }
            Screen::Edit => {
                let (y0, field_h, field_gap, btn_h) = self.edit_layout(&vp);
                let usable_b = vp.usable_bottom(self.ime_open);
                let natural_btn_y = y0 + 4 * (field_h + field_gap) + vp.d(4);
                let btn_y = natural_btn_y.min(usable_b - btn_h - vp.d(6));
                let gap = vp.d(10);
                let bw = (vp.content_w - gap) / 2;
                if UiRect::new(vp.content_x, btn_y, bw, btn_h).expand(pad).contains(x, y) {
                    self.hide_ime();
                    self.screen = Screen::List;
                    return RET_REDRAW;
                }
                if UiRect::new(vp.content_x + bw + gap, btn_y, bw, btn_h).expand(pad).contains(x, y) {
                    self.commit_save(ev.uptime_ms, en);
                    return RET_REDRAW;
                }
                let kinds = [EditField::Name, EditField::Phone, EditField::Email, EditField::Note];
                for (i, kind) in kinds.iter().enumerate() {
                    let fy = y0 + i as i32 * (field_h + field_gap);
                    if fy + field_h <= top.bar.bottom() || fy >= btn_y {
                        continue;
                    }
                    if vp.row_rect(fy, field_h).contains(x, y) {
                        self.edit_field = *kind;
                        self.show_ime();
                        return RET_REDRAW;
                    }
                }
            }
        }
        RET_NONE
    }

    fn handle_key(&mut self, ev: &EventBlock) -> u32 {
        let k = ev.key;
        let en = ev.lang == 1;
        if k == key::NONE {
            return RET_NONE;
        }
        if k == key::ESCAPE {
            self.hide_ime();
            self.search_active = false;
            if self.confirm_delete_open {
                self.confirm_delete_open = false;
                return RET_REDRAW | RET_ANIMATING;
            }
            if self.select_mode {
                self.select_mode = false;
                self.selected_mask = 0;
                return RET_REDRAW;
            }
            if self.screen != Screen::List {
                self.screen = Screen::List;
                return RET_REDRAW;
            }
            return RET_CLOSE;
        }
        if self.screen == Screen::List && self.search_active {
            if k == key::BACKSPACE {
                pop_char(&self.search, &mut self.search_len);
                self.reload_list();
                return RET_REDRAW;
            }
            if k == key::ENTER {
                self.search_active = false;
                self.hide_ime();
                self.reload_list();
                return RET_REDRAW;
            }
            if let Some(c) = key::as_char(k) {
                if !c.is_control() {
                    push_char(&mut self.search, &mut self.search_len, c);
                    self.reload_list();
                    return RET_REDRAW;
                }
            }
        } else if self.screen == Screen::Edit {
            if k == key::ENTER {
                self.edit_field = match self.edit_field {
                    EditField::Name => EditField::Phone,
                    EditField::Phone => EditField::Email,
                    EditField::Email => EditField::Note,
                    EditField::Note => {
                        self.commit_save(ev.uptime_ms, en);
                        return RET_REDRAW;
                    }
                };
                return RET_REDRAW;
            }
            if k == key::BACKSPACE {
                match self.edit_field {
                    EditField::Name => pop_char(&self.f_name, &mut self.f_name_len),
                    EditField::Phone => pop_char(&self.f_phone, &mut self.f_phone_len),
                    EditField::Email => pop_char(&self.f_email, &mut self.f_email_len),
                    EditField::Note => pop_char(&self.f_note, &mut self.f_note_len),
                }
                return RET_REDRAW;
            }
            if let Some(c) = key::as_char(k) {
                if !c.is_control() && c != '|' {
                    self.inline_err = false;
                    match self.edit_field {
                        EditField::Name => push_char(&mut self.f_name, &mut self.f_name_len, c),
                        EditField::Phone => push_char(&mut self.f_phone, &mut self.f_phone_len, c),
                        EditField::Email => push_char(&mut self.f_email, &mut self.f_email_len, c),
                        EditField::Note => push_char(&mut self.f_note, &mut self.f_note_len, c),
                    }
                    return RET_REDRAW;
                }
            }
        }
        RET_NONE
    }
}

impl RingApp for ContactsApp {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.last_flags = ev.flags;
                self.last_accent = ev.accent;
                self.last_lang = ev.lang;
                if let Some(st) = status::query() {
                    self.reduced_motion = st.reduced_motion();
                }
                self.tel_status = telephony::status();
                self.load_favorites();
                self.sync_permissions(true);
                RET_REDRAW
            }
            OP_PAUSE => {
                self.hide_ime();
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
                let r_now = perm::check(PERM_CONTACTS_READ).state;
                let w_now = perm::check(PERM_CONTACTS_WRITE).state;
                if r_now != self.read_state || w_now != self.write_state {
                    self.sync_permissions(false);
                    ret |= RET_REDRAW;
                    if self.write_state == PermState::Granted {
                        let act = self.pending_write;
                        self.pending_write = PendingWrite::None;
                        match act {
                            PendingWrite::Save => self.commit_save(ev.uptime_ms, ev.lang == 1),
                            PendingWrite::DeleteSingle => self.commit_delete_single(ev.uptime_ms, ev.lang == 1),
                            PendingWrite::DeleteBatch => self.commit_delete_batch(ev.uptime_ms, ev.lang == 1),
                            PendingWrite::None => {}
                        }
                    } else if self.write_state == PermState::Denied {
                        self.pending_write = PendingWrite::None;
                    }
                }
                let kret = self.handle_key(ev);
                if kret & RET_CLOSE != 0 {
                    return kret;
                }
                ret |= kret;
                if let Some(p) = pointer(ev) {
                    if p.clicked {
                        self.pressed_id = 0;
                        self.pressed_sheet_btn = 0;
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

static mut APP: ContactsApp = ContactsApp::new();

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
