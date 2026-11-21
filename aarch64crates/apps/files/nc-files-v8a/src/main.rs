//! Dosyalar (ARMv8-A / Ring 3 EL0): minimalist, ticari mobil işletim sistemi tasarımlı dosya yöneticisi.
#![no_std]
#![no_main]

use nc_abi::event::{
    unpack_dark_theme, EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE, RET_ANIMATING,
    RET_CLOSE, RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_abi::vfs as wire;
use nc_abi::draw::ICON_FILES;
use nc_brand::{
    draw_card, draw_chevron_left, draw_chevron_right, draw_pill_btn, draw_segmented_p, ease_out_cubic, mix_color,
    step_towards, Dp, Palette, AMBER, BLUE, CARD, CORAL, CORAL_SOFT, DIVIDER, GREEN, INK, MUTED, PURPLE, SUBTLE,
    TEAL, TRACK, WHITE,
};
use nc_app_sdk::{apps, dispatch, pointer, storage, sys, Gfx, Line, RingApp};

const MAX_ITEMS: usize = 48;
const MAX_NAME: usize = 48;
const MAX_PATH: usize = 160;
const MAX_PREVIEW: usize = 768;
const ROWS_PER_PAGE: usize = 5;
const LIST_MS: u32 = 220;
const ROW_STAGGER_MS: u32 = 40;

#[derive(Clone, Copy)]
struct Item {
    kind: u8,
    size: u64,
    name: [u8; MAX_NAME],
    name_len: u8,
}

impl Item {
    const EMPTY: Item = Item { kind: wire::KIND_FILE, size: 0, name: [0; MAX_NAME], name_len: 0 };

    fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Browse,
    Inspect,
}

struct FilesApp {
    initialized: bool,
    mode: Mode,
    path: [u8; MAX_PATH],
    path_len: usize,
    items: [Item; MAX_ITEMS],
    item_count: usize,
    scroll: usize,
    selected: usize,
    file_path: [u8; MAX_PATH],
    file_path_len: usize,
    file_name: [u8; MAX_NAME],
    file_name_len: usize,
    file_size: u64,
    file_ro: bool,
    preview: [u8; MAX_PREVIEW],
    preview_len: usize,
    dirty: bool,
    counter: u32,
    toast: [u8; 48],
    toast_len: usize,
    now_ms: u32,
    list_t0: u32,
    seg_pos: i32,
    hl_pos: i32,
    usb: storage::UsbStatus,
}

static mut REQ_BUF: [u8; 1536] = [0; 1536];
static mut RESP_BUF: [u8; 8192] = [0; 8192];

fn copy_str(dst: &mut [u8], text: &str) -> usize {
    let bytes = text.as_bytes();
    let len = bytes.len().min(dst.len());
    dst[..len].copy_from_slice(&bytes[..len]);
    len
}

fn format_size(size: u64, line: &mut Line) {
    if size < 1024 {
        line.number(size as u32, 1).text(" B");
    } else if size < 1024 * 1024 {
        line.number((size / 1024) as u32, 1).text(" KB");
    } else {
        line.number((size / (1024 * 1024)) as u32, 1).text(" MB");
    }
}

fn vfs_request(op: u8, arg0: u64, arg1: u64, path: &str, data: &[u8]) -> Result<(i32, &'static [u8]), i32> {
    unsafe {
        let req = &mut *core::ptr::addr_of_mut!(REQ_BUF);
        let resp = &mut *core::ptr::addr_of_mut!(RESP_BUF);
        let Some(req_len) = wire::encode_request(req, op, arg0, arg1, path, data) else {
            return Err(wire::ERR_INVALID_PATH);
        };
        let n = sys::vfs_call(&req[..req_len], resp);
        if n < 0 {
            return Err(wire::ERR_DISK);
        }
        let n = (n as usize).min(resp.len());
        match wire::parse_response(&resp[..n]) {
            Ok((status, payload)) => Ok((status, payload)),
            Err(_) => Err(wire::ERR_CORRUPTED),
        }
    }
}

fn draw_item_icon(gfx: &mut Gfx<'_>, x: i32, y: i32, size: i32, kind: u8, name: &str) {
    let p = |v: i32| size * v / 100;
    let r = size * 28 / 100;
    if kind == wire::KIND_DIR {
        gfx.panel(x, y, size, size, r, AMBER, 255, 0, 0);
        gfx.panel(x + p(20), y + p(26), p(28), p(14), p(4), WHITE, 255, 0, 0);
        gfx.panel(x + p(20), y + p(35), p(60), p(40), p(7), WHITE, 255, 0, 0);
        return;
    }
    let lower_ntzc = name.ends_with(".ntzc");
    let lower_media = name.ends_with(".png") || name.ends_with(".jpg") || name.ends_with(".wav");
    if lower_ntzc {
        gfx.panel(x, y, size, size, r, PURPLE, 255, 0, 0);
        gfx.panel(x + p(26), y + p(26), p(48), p(48), p(10), WHITE, 255, 0, 0);
        gfx.dot(x + size / 2, y + size / 2, p(11), PURPLE, 1024);
    } else if lower_media {
        gfx.panel(x, y, size, size, r, TEAL, 255, 0, 0);
        gfx.panel(x + p(22), y + p(24), p(56), p(52), p(8), WHITE, 235, 0, 0);
        gfx.dot(x + p(38), y + p(40), p(7), TEAL, 1024);
        gfx.panel(x + p(28), y + p(54), p(44), p(14), p(5), TEAL, 255, 0, 0);
    } else {
        gfx.panel(x, y, size, size, r, BLUE, 255, 0, 0);
        gfx.panel(x + p(25), y + p(20), p(50), p(60), p(7), WHITE, 255, 0, 0);
        gfx.fill_rect(x + p(33), y + p(34), p(34), p(5).max(2), BLUE);
        gfx.fill_rect(x + p(33), y + p(46), p(34), p(5).max(2), BLUE);
        gfx.fill_rect(x + p(33), y + p(58), p(22), p(5).max(2), BLUE);
    }
}

impl FilesApp {
    const fn new() -> Self {
        Self {
            initialized: false,
            mode: Mode::Browse,
            path: [0; MAX_PATH],
            path_len: 0,
            items: [Item::EMPTY; MAX_ITEMS],
            item_count: 0,
            scroll: 0,
            selected: 0,
            file_path: [0; MAX_PATH],
            file_path_len: 0,
            file_name: [0; MAX_NAME],
            file_name_len: 0,
            file_size: 0,
            file_ro: true,
            preview: [0; MAX_PREVIEW],
            preview_len: 0,
            dirty: false,
            counter: 1,
            toast: [0; 48],
            toast_len: 0,
            now_ms: 0,
            list_t0: 0,
            seg_pos: 0,
            hl_pos: 0,
            usb: storage::UsbStatus::EMPTY,
        }
    }

    fn refresh_usb(&mut self) {
        if let Some(st) = storage::usb_info() {
            self.usb = st;
        } else {
            self.usb = storage::UsbStatus::EMPTY;
        }
    }

    fn path_str(&self) -> &str {
        core::str::from_utf8(&self.path[..self.path_len]).unwrap_or("/data/media")
    }

    fn file_name_str(&self) -> &str {
        core::str::from_utf8(&self.file_name[..self.file_name_len]).unwrap_or("")
    }

    fn toast_str(&self) -> &str {
        core::str::from_utf8(&self.toast[..self.toast_len]).unwrap_or("")
    }

    fn set_toast(&mut self, msg: &str) {
        self.toast_len = copy_str(&mut self.toast, msg);
    }

    fn active_section(&self) -> usize {
        let p = self.path_str();
        if p.starts_with("/storage/usb0") {
            3
        } else if p.starts_with("/system/packages") || p.starts_with("/data/app") {
            1
        } else if p.starts_with("/system") {
            2
        } else {
            0
        }
    }

    fn is_in_subfolder(&self) -> bool {
        let p = self.path_str();
        p != "/data/media" && p != "/system/packages" && p != "/system" && p != "/storage/usb0" && p != "/"
    }

    fn current_folder_title<'a>(&'a self, en: bool) -> &'a str {
        let p = self.path_str();
        match p {
            "/data/media" => if en { "Documents" } else { "Belgelerim" },
            "/system/packages" | "/data/app" => if en { "Applications" } else { "Uygulamalar" },
            "/system" => if en { "System" } else { "Sistem" },
            "/storage/usb0" => if en { "USB Storage" } else { "USB Depolama" },
            _ => match p.rfind('/') {
                Some(idx) if idx + 1 < p.len() => &p[idx + 1..],
                _ => p,
            },
        }
    }

    fn is_writable_dir(&self) -> bool {
        let p = self.path_str();
        p.starts_with("/data/media")
    }

    fn set_path(&mut self, new_path: &str) {
        self.path_len = copy_str(&mut self.path, new_path);
        self.scroll = 0;
        self.selected = 0;
        self.mode = Mode::Browse;
        self.toast_len = 0;
        self.refresh_dir();
    }

    fn refresh_dir(&mut self) {
        self.list_t0 = self.now_ms;
        let mut p_copy = [0u8; MAX_PATH];
        let p_len = self.path_len;
        p_copy[..p_len].copy_from_slice(&self.path[..p_len]);
        let path = core::str::from_utf8(&p_copy[..p_len]).unwrap_or("/data/media");

        self.item_count = 0;
        if let Ok((0, payload)) = vfs_request(wire::OP_LIST, 0, 0, path, &[]) {
            if let Ok(entries) = wire::DirEntries::new(payload) {
                for entry in entries.flatten() {
                    if self.item_count >= MAX_ITEMS {
                        break;
                    }
                    let mut item = Item::EMPTY;
                    item.kind = entry.kind;
                    item.size = entry.size;
                    item.name_len = copy_str(&mut item.name, entry.name) as u8;
                    self.items[self.item_count] = item;
                    self.item_count += 1;
                }
            }
            for i in 1..self.item_count {
                let mut j = i;
                while j > 0 {
                    let a = &self.items[j - 1];
                    let b = &self.items[j];
                    let swap = (a.kind != wire::KIND_DIR && b.kind == wire::KIND_DIR)
                        || (a.kind == b.kind && a.name_str() > b.name_str());
                    if swap {
                        self.items.swap(j - 1, j);
                        j -= 1;
                    } else {
                        break;
                    }
                }
            }
            if self.selected >= self.item_count && self.item_count > 0 {
                self.selected = self.item_count - 1;
            }
        }
    }

    fn go_up(&mut self) -> bool {
        if self.mode == Mode::Inspect {
            self.mode = Mode::Browse;
            self.toast_len = 0;
            self.refresh_dir();
            return true;
        }
        if !self.is_in_subfolder() {
            return false;
        }
        let current = self.path_str();
        let parent_len = match current.rfind('/') {
            Some(0) | None => 1,
            Some(idx) => idx,
        };
        let mut parent = [0u8; MAX_PATH];
        parent[..parent_len].copy_from_slice(&self.path[..parent_len]);
        let p_str = core::str::from_utf8(&parent[..parent_len]).unwrap_or("/data/media");
        self.set_path(p_str);
        true
    }

    fn open_item(&mut self, index: usize) {
        if index >= self.item_count {
            return;
        }
        self.selected = index;
        let item = self.items[index];
        let mut next = [0u8; MAX_PATH];
        let cur = self.path_str();
        let mut pos;
        if cur == "/" {
            next[0] = b'/';
            pos = 1;
        } else {
            pos = copy_str(&mut next, cur);
            if pos < MAX_PATH {
                next[pos] = b'/';
                pos += 1;
            }
        }
        let name_bytes = &item.name[..item.name_len as usize];
        let add = name_bytes.len().min(MAX_PATH.saturating_sub(pos));
        next[pos..pos + add].copy_from_slice(&name_bytes[..add]);
        pos += add;
        let full_str = core::str::from_utf8(&next[..pos]).unwrap_or("/data/media");

        if item.kind == wire::KIND_DIR {
            self.set_path(full_str);
        } else if item.name_str().ends_with(".ntzc") {
            // .ntzc paketini bagimsiz Ring 3 Paket Kurucu'ya devret (USB dahil!)
            if apps::send_intent("nc.package-installer", full_str).is_ok() {
                let _ = apps::launch("nc.package-installer");
            } else {
                self.file_name_len = copy_str(&mut self.file_name, item.name_str());
                self.inspect_file(full_str, item.size);
            }
        } else {
            self.file_name_len = copy_str(&mut self.file_name, item.name_str());
            self.inspect_file(full_str, item.size);
        }
    }

    fn inspect_file(&mut self, full_path: &str, fallback_size: u64) {
        self.file_path_len = copy_str(&mut self.file_path, full_path);
        if let Some(idx) = full_path.rfind('/') {
            self.file_name_len = copy_str(&mut self.file_name, &full_path[idx + 1..]);
        }
        self.file_size = fallback_size;
        self.file_ro = !full_path.starts_with("/data/media");
        self.preview_len = 0;
        self.dirty = false;
        self.toast_len = 0;

        let mut p_copy = [0u8; MAX_PATH];
        let p_len = self.file_path_len;
        p_copy[..p_len].copy_from_slice(&self.file_path[..p_len]);
        let path = core::str::from_utf8(&p_copy[..p_len]).unwrap_or("");

        if let Ok((0, stat_payload)) = vfs_request(wire::OP_STAT, 0, 0, path, &[]) {
            if stat_payload.len() >= 10 {
                self.file_size = u64::from_le_bytes([
                    stat_payload[0], stat_payload[1], stat_payload[2], stat_payload[3],
                    stat_payload[4], stat_payload[5], stat_payload[6], stat_payload[7],
                ]);
            }
        }
        if let Ok((0, data)) = vfs_request(wire::OP_READ_RANGE, 0, MAX_PREVIEW as u64, path, &[]) {
            let n = data.len().min(MAX_PREVIEW);
            self.preview[..n].copy_from_slice(&data[..n]);
            self.preview_len = n;
        }
        self.mode = Mode::Inspect;
    }

    fn create_new_folder(&mut self, en: bool) {
        if !self.is_writable_dir() {
            return;
        }
        for _ in 0..32 {
            let mut path_line = Line::new();
            path_line
                .text(self.path_str())
                .text(if en { "/Folder_" } else { "/Klasor_" })
                .number(self.counter, 1);
            self.counter = self.counter.wrapping_add(1);
            match vfs_request(wire::OP_MKDIR, 0, 0, path_line.as_str(), &[]) {
                Ok((0, _)) => {
                    self.refresh_dir();
                    self.set_toast(if en { "Folder created" } else { "Yeni klasör oluşturuldu" });
                    return;
                }
                Ok((wire::ERR_ALREADY_EXISTS, _)) => continue,
                _ => return,
            }
        }
    }

    fn create_new_note(&mut self, en: bool) {
        if !self.is_writable_dir() {
            return;
        }
        for _ in 0..32 {
            let mut path_line = Line::new();
            path_line
                .text(self.path_str())
                .text(if en { "/Note_" } else { "/Not_" })
                .number(self.counter, 1)
                .text(".txt");
            self.counter = self.counter.wrapping_add(1);
            if let Ok((0, _)) = vfs_request(wire::OP_STAT, 0, 0, path_line.as_str(), &[]) {
                continue;
            }
            let initial = if en {
                "Personal note\n".as_bytes()
            } else {
                "Yeni not\n".as_bytes()
            };
            if let Ok((0, _)) = vfs_request(wire::OP_WRITE, 0, initial.len() as u64, path_line.as_str(), initial) {
                self.refresh_dir();
                self.inspect_file(path_line.as_str(), initial.len() as u64);
                return;
            }
            return;
        }
    }

    fn save_current_file(&mut self, en: bool) {
        if self.file_ro {
            return;
        }
        let mut p_copy = [0u8; MAX_PATH];
        let p_len = self.file_path_len;
        p_copy[..p_len].copy_from_slice(&self.file_path[..p_len]);
        let path = core::str::from_utf8(&p_copy[..p_len]).unwrap_or("");
        let mut data_copy = [0u8; MAX_PREVIEW];
        let d_len = self.preview_len;
        data_copy[..d_len].copy_from_slice(&self.preview[..d_len]);

        if let Ok((0, _)) = vfs_request(wire::OP_WRITE, 0, d_len as u64, path, &data_copy[..d_len]) {
            self.file_size = d_len as u64;
            self.dirty = false;
            self.set_toast(if en { "Saved" } else { "Kaydedildi" });
        }
    }

    fn delete_current(&mut self, en: bool) {
        let mut target = [0u8; MAX_PATH];
        let t_len = if self.mode == Mode::Inspect {
            target[..self.file_path_len].copy_from_slice(&self.file_path[..self.file_path_len]);
            self.file_path_len
        } else if self.selected < self.item_count {
            let mut line = Line::new();
            line.text(self.path_str()).text("/").text(self.items[self.selected].name_str());
            copy_str(&mut target, line.as_str())
        } else {
            0
        };
        if t_len == 0 {
            return;
        }
        let path = core::str::from_utf8(&target[..t_len]).unwrap_or("");
        if !path.starts_with("/data/media") {
            return;
        }
        if let Ok((0, _)) = vfs_request(wire::OP_REMOVE, 0, 0, path, &[]) {
            self.mode = Mode::Browse;
            self.refresh_dir();
            self.set_toast(if en { "Deleted" } else { "Silindi" });
        }
    }

    fn is_ntzc(&self) -> bool {
        self.preview_len >= 64
            && (&self.preview[..4] == b"NTZ2" || &self.preview[..4] == b"NTZ3" || &self.preview[..4] == b"NTZC")
    }

    fn extract_ntzc_field<'a>(&'a self, key_prefix: &str) -> Option<&'a str> {
        if !self.is_ntzc() {
            return None;
        }
        let m_off = u32::from_le_bytes([self.preview[16], self.preview[17], self.preview[18], self.preview[19]]) as usize;
        let m_len = u32::from_le_bytes([self.preview[20], self.preview[21], self.preview[22], self.preview[23]]) as usize;
        let end = m_off.saturating_add(m_len).min(self.preview_len);
        if m_off >= end {
            return None;
        }
        let text = core::str::from_utf8(&self.preview[m_off..end]).ok()?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix(key_prefix) {
                return Some(rest.trim());
            }
        }
        None
    }

    /// Her güncellemede çağrılır: sekme tutamağı ve seçim vurgusu hedefe kayar; devam ediyorsa `true`.
    fn tick(&mut self, now: u32) -> bool {
        self.now_ms = now;
        let (sp, c1) = step_towards(self.seg_pos, self.active_section() as i32 * 256, 56);
        self.seg_pos = sp;
        let row = if self.selected >= self.scroll { (self.selected - self.scroll) as i32 * 256 } else { 0 };
        let (hp, c2) = step_towards(self.hl_pos, row, 72);
        self.hl_pos = hp;
        let listing = now.wrapping_sub(self.list_t0) < LIST_MS + ROWS_PER_PAGE as u32 * ROW_STAGGER_MS;
        c1 || c2 || listing
    }

    fn handle_key(&mut self, k: u32) -> u32 {
        if k == 0 {
            return RET_NONE;
        }
        if k == key::ESCAPE {
            if self.go_up() {
                return RET_REDRAW;
            }
            return RET_CLOSE;
        }
        if self.mode == Mode::Inspect && !self.file_ro && !self.is_ntzc() {
            if k == key::BACKSPACE {
                if self.preview_len > 0 {
                    self.preview_len -= 1;
                    self.dirty = true;
                    return RET_REDRAW;
                }
            } else if k == key::ENTER {
                if self.preview_len < MAX_PREVIEW {
                    self.preview[self.preview_len] = b'\n';
                    self.preview_len += 1;
                    self.dirty = true;
                    return RET_REDRAW;
                }
            } else if k == key::SPACE {
                if self.preview_len < MAX_PREVIEW {
                    self.preview[self.preview_len] = b' ';
                    self.preview_len += 1;
                    self.dirty = true;
                    return RET_REDRAW;
                }
            } else if let Some(ch) = key::as_char(k) {
                let mut utf8 = [0u8; 4];
                let encoded = ch.encode_utf8(&mut utf8);
                if self.preview_len + encoded.len() <= MAX_PREVIEW {
                    self.preview[self.preview_len..self.preview_len + encoded.len()]
                        .copy_from_slice(encoded.as_bytes());
                    self.preview_len += encoded.len();
                    self.dirty = true;
                    return RET_REDRAW;
                }
            }
        }
        RET_NONE
    }

    fn handle_click(&mut self, x: i32, y: i32, dp: &Dp, lang: u32) -> u32 {
        let en = lang == 1;

        // Üst sol geri düğmesi (alt klasörde veya dosya görünümündeyken)
        if (self.mode == Mode::Inspect || self.is_in_subfolder())
            && y >= dp.d(14)
            && y < dp.d(48)
            && x >= dp.x(14)
            && x < dp.x(56)
        {
            if self.go_up() {
                return RET_REDRAW;
            }
        }

        // Üst sekme hapları (y: 54..88 dp) — 4 sekme: Belgeler | Paketler | Sistem | USB
        if y >= dp.d(54) && y < dp.d(88) {
            if x >= dp.x(16) && x < dp.x(98) {
                self.set_path("/data/media");
                return RET_REDRAW;
            }
            if x >= dp.x(98) && x < dp.x(180) {
                self.set_path("/system/packages");
                return RET_REDRAW;
            }
            if x >= dp.x(180) && x < dp.x(262) {
                self.set_path("/system");
                return RET_REDRAW;
            }
            if x >= dp.x(262) && x < dp.x(344) {
                self.refresh_usb();
                self.set_path("/storage/usb0");
                return RET_REDRAW;
            }
        }

        if self.mode == Mode::Inspect {
            if self.is_ntzc() {
                // Paket Kurucu ile acma dugmesi (y: 316..366 dp)
                if y >= dp.d(316) && y < dp.d(366) && x >= dp.x(48) && x < dp.x(312) {
                    let mut p_copy = [0u8; MAX_PATH];
                    let p_len = self.file_path_len;
                    p_copy[..p_len].copy_from_slice(&self.file_path[..p_len]);
                    if let Ok(path) = core::str::from_utf8(&p_copy[..p_len]) {
                        if apps::send_intent("nc.package-installer", path).is_ok() {
                            let _ = apps::launch("nc.package-installer");
                        }
                    }
                    return RET_REDRAW;
                }
                return RET_NONE;
            }
            // Metin/Not görünümü alt düğmeleri (y: 416..452 dp)
            if !self.file_ro && y >= dp.d(414) && y < dp.d(454) {
                if x >= dp.x(16) && x < dp.x(168) {
                    self.save_current_file(en);
                    return RET_REDRAW;
                }
                if x >= dp.x(176) && x < dp.x(266) {
                    if self.preview_len < MAX_PREVIEW - 16 {
                        let stamp = (sys::clock_get(0) as u32) % 1000;
                        let mut line = Line::new();
                        line.text(if en { "\nNote " } else { "\nNot " }).number(stamp, 1);
                        let b = line.as_str().as_bytes();
                        let take = b.len().min(MAX_PREVIEW - self.preview_len);
                        self.preview[self.preview_len..self.preview_len + take].copy_from_slice(&b[..take]);
                        self.preview_len += take;
                        self.dirty = true;
                    }
                    return RET_REDRAW;
                }
                if x >= dp.x(274) && x < dp.x(344) {
                    self.delete_current(en);
                    return RET_REDRAW;
                }
            }
            return RET_NONE;
        }

        // Liste satırları (y: 102..102 + 5*58 dp)
        let list_top = dp.d(102);
        let row_h = dp.d(58);
        if y >= list_top && y < list_top + row_h * (ROWS_PER_PAGE as i32) && x >= dp.x(16) && x < dp.x(344) {
            let row = ((y - list_top) / row_h) as usize;
            let idx = self.scroll + row;
            if idx < self.item_count {
                if self.selected == idx {
                    self.open_item(idx);
                } else {
                    self.selected = idx;
                }
                return RET_REDRAW;
            }
        }

        // Sayfalama noktaları / düğmeleri (y: 394..414 dp)
        if self.item_count > ROWS_PER_PAGE && y >= dp.d(390) && y < dp.d(414) {
            if x >= dp.x(120) && x < dp.x(180) {
                self.scroll = self.scroll.saturating_sub(ROWS_PER_PAGE);
                return RET_REDRAW;
            }
            if x >= dp.x(180) && x < dp.x(240) && self.scroll + ROWS_PER_PAGE < self.item_count {
                self.scroll += ROWS_PER_PAGE;
                return RET_REDRAW;
            }
        }

        // Alt eylem çubuğu (y: 416..452 dp)
        if y >= dp.d(414) && y < dp.d(454) {
            if self.is_writable_dir() {
                if x >= dp.x(16) && x < dp.x(122) {
                    self.create_new_folder(en);
                    return RET_REDRAW;
                }
                if x >= dp.x(128) && x < dp.x(234) {
                    self.create_new_note(en);
                    return RET_REDRAW;
                }
                if x >= dp.x(240) && x < dp.x(294) && self.selected < self.item_count {
                    self.open_item(self.selected);
                    return RET_REDRAW;
                }
                if x >= dp.x(298) && x < dp.x(344) && self.selected < self.item_count {
                    self.delete_current(en);
                    return RET_REDRAW;
                }
            } else if self.active_section() == 3 {
                // USB alt eylem cubugu: Yeniden Tara | Guvenle Cikar | Ac
                if x >= dp.x(16) && x < dp.x(122) {
                    let _ = storage::usb_rescan();
                    self.refresh_usb();
                    self.refresh_dir();
                    self.set_toast(if self.usb.mounted {
                        if en { "USB mounted" } else { "USB baglandi" }
                    } else {
                        if en { "No USB drive" } else { "USB bulunamadi" }
                    });
                    return RET_REDRAW;
                }
                if self.usb.mounted && x >= dp.x(128) && x < dp.x(234) {
                    if storage::usb_eject().is_ok() {
                        self.refresh_usb();
                        self.item_count = 0;
                        self.set_path("/storage/usb0");
                        self.set_toast(if en { "Safely ejected" } else { "Guvenle cikarildi" });
                    }
                    return RET_REDRAW;
                }
                if self.usb.mounted && x >= dp.x(240) && x < dp.x(344) && self.selected < self.item_count {
                    self.open_item(self.selected);
                    return RET_REDRAW;
                }
            } else if x >= dp.x(60) && x < dp.x(300) && self.selected < self.item_count {
                self.open_item(self.selected);
                return RET_REDRAW;
            }
        }

        RET_NONE
    }

    fn draw_inspect(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        let en = ev.lang == 1;

        if self.is_ntzc() {
            draw_card(gfx, dp.x(16), dp.d(104), dp.d(328), dp.d(280), dp.d(22));
            let icon_s = dp.d(64);
            let ix = dp.x(180) - icon_s / 2;
            let iy = dp.d(132);
            draw_item_icon(gfx, ix, iy, icon_s, wire::KIND_FILE, ".ntzc");

            let app_name = self.extract_ntzc_field("name=").unwrap_or(self.file_name_str());
            let app_ver = self.extract_ntzc_field("version=").unwrap_or("1.0.0");
            gfx.text_centered(dp.x(180), dp.d(212), dp.d(12), INK, 2, app_name);

            let mut sub = Line::new();
            sub.text(if en { "Version " } else { "Sürüm " })
                .text(app_ver)
                .text("  •  ");
            format_size(self.file_size, &mut sub);
            gfx.text_centered(dp.x(180), dp.d(234), dp.d(8), MUTED, 2, sub.as_str());

            gfx.text_centered(
                dp.x(180),
                dp.d(264),
                dp.d(7),
                SUBTLE,
                2,
                if en { "Verified NetizenCore Application" } else { "Doğrulanmış NetizenCore Uygulaması" },
            );

            draw_pill_btn(
                gfx,
                dp.x(56),
                dp.d(318),
                dp.d(248),
                dp.d(38),
                ev.accent,
                WHITE,
                dp.d(9),
                if en { "Open Application" } else { "Uygulamayı Aç" },
            );
            return;
        }

        // Belge / Not görünümü
        draw_card(gfx, dp.x(16), dp.d(98), dp.d(328), dp.d(304), dp.d(20));
        gfx.text(dp.x(30), dp.d(114), dp.d(10), INK, 2, self.file_name_str());

        let mut meta = Line::new();
        format_size(self.file_size, &mut meta);
        if self.dirty {
            meta.text(if en { "  •  Edited" } else { "  •  Düzenlendi" });
        }
        gfx.text(dp.x(30), dp.d(132), dp.d(7), MUTED, 1, meta.as_str());
        gfx.fill_rect(dp.x(28), dp.d(148), dp.d(304), 1, DIVIDER);

        if let Ok(text) = core::str::from_utf8(&self.preview[..self.preview_len]) {
            let mut y = dp.d(162);
            for line in text.lines().take(10) {
                let mut end = line.len().min(42);
                while end > 0 && !line.is_char_boundary(end) {
                    end -= 1;
                }
                gfx.text(dp.x(30), y, dp.d(8), INK, 1, &line[..end]);
                y += dp.d(20);
            }
            if !self.file_ro {
                gfx.text(
                    dp.x(30),
                    dp.d(376),
                    dp.d(7),
                    SUBTLE,
                    1,
                    if en { "Use keyboard to type" } else { "Klavye ile metin yazabilirsiniz" },
                );
            }
        } else {
            gfx.text_centered(
                dp.x(180),
                dp.d(230),
                dp.d(9),
                MUTED,
                2,
                if en { "Binary media file" } else { "Medya / Sistem Dosyası" },
            );
        }

        if !self.file_ro {
            draw_pill_btn(
                gfx,
                dp.x(16),
                dp.d(416),
                dp.d(150),
                dp.d(34),
                ev.accent,
                WHITE,
                dp.d(8),
                if en { "Save" } else { "Kaydet" },
            );
            draw_pill_btn(
                gfx,
                dp.x(174),
                dp.d(416),
                dp.d(92),
                dp.d(34),
                CARD,
                INK,
                dp.d(8),
                if en { "+ Line" } else { "+ Satır" },
            );
            draw_pill_btn(
                gfx,
                dp.x(272),
                dp.d(416),
                dp.d(72),
                dp.d(34),
                CORAL_SOFT,
                CORAL,
                dp.d(8),
                if en { "Delete" } else { "Sil" },
            );
        }
    }

    fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        let dp = Dp::new(ev.area_w, ev.area_h);
        let en = ev.lang == 1;
        let pal = Palette::for_dark(unpack_dark_theme(ev.flags));
        gfx.fill_rect(0, 0, ev.area_w, ev.area_h, pal.bg);

        // Minimalist Başlık
        let has_back = self.mode == Mode::Inspect || self.is_in_subfolder();
        if has_back {
            let bs = dp.d(30);
            gfx.panel(dp.x(16), dp.d(16), bs, bs, bs / 2, pal.card, 255, pal.divider, 255);
            draw_chevron_left(gfx, dp.x(31), dp.d(31), &dp, pal.ink);
            let title = if self.mode == Mode::Inspect {
                self.file_name_str()
            } else {
                self.current_folder_title(en)
            };
            gfx.text(dp.x(54), dp.d(23), dp.d(13), pal.ink, 2, title);
        } else {
            gfx.icon(ICON_FILES, dp.x(18), dp.d(14), dp.d(34), dp.d(34));
            gfx.text(dp.x(62), dp.d(20), dp.d(18), pal.ink, 3, if en { "Files" } else { "Dosyalar" });
        }

        if self.toast_len > 0 {
            gfx.panel(dp.x(210), dp.d(20), dp.d(134), dp.d(24), dp.d(12), pal.card, 255, pal.divider, 255);
            gfx.text_centered(dp.x(277), dp.d(28), dp.d(7), GREEN, 1, self.toast_str());
        }

        let sec = self.active_section();
        let tabs = if en {
            ["Docs", "Apps", "System", "USB"]
        } else {
            ["Belgeler", "Paketler", "Sistem", "USB"]
        };
        draw_segmented_p(gfx, &dp, 54, &tabs, self.seg_pos, ev.accent);

        if self.mode == Mode::Inspect {
            self.draw_inspect(ev, gfx, &dp);
            return;
        }

        // Ana Dosya Listesi Kartı
        draw_card(gfx, dp.x(16), dp.d(98), dp.d(328), dp.d(296), dp.d(20));

        if sec == 3 && !self.usb.mounted {
            let icon_s = dp.d(48);
            draw_item_icon(gfx, dp.x(180) - icon_s / 2, dp.d(184), icon_s, wire::KIND_DIR, "");
            gfx.text_centered(
                dp.x(180),
                dp.d(246),
                dp.d(10),
                INK,
                2,
                if en { "No USB Drive Connected" } else { "Takılı USB Bellek Yok" },
            );
            gfx.text_centered(
                dp.x(180),
                dp.d(268),
                dp.d(7),
                MUTED,
                1,
                if en { "Connect a USB drive and tap Rescan" } else { "Bir USB bellek takın ve Yeniden Tara'ya dokunun" },
            );
        } else if self.item_count == 0 {
            let icon_s = dp.d(48);
            draw_item_icon(gfx, dp.x(180) - icon_s / 2, dp.d(196), icon_s, wire::KIND_DIR, "");
            gfx.text_centered(
                dp.x(180),
                dp.d(258),
                dp.d(9),
                MUTED,
                2,
                if en { "No files yet" } else { "Henüz dosya yok" },
            );
        } else {
            let list_top = dp.d(103);
            let row_h = dp.d(57);
            let since = ev.uptime_ms.wrapping_sub(self.list_t0);
            // Seçim vurgusu: satırlar arasında kayar
            if self.selected >= self.scroll && self.selected < self.scroll + ROWS_PER_PAGE && self.selected < self.item_count {
                let hy = list_top + self.hl_pos * row_h / 256;
                gfx.panel(dp.x(22), hy + dp.d(3), dp.d(316), row_h - dp.d(6), dp.d(14), mix_color(WHITE, ev.accent, 30, 256), 255, 0, 0);
            }
            for row in 0..ROWS_PER_PAGE {
                let idx = self.scroll + row;
                if idx >= self.item_count {
                    break;
                }
                let item = &self.items[idx];
                // Kademeli yükselme + solma
                let local = ((since as i64 - row as i64 * ROW_STAGGER_MS as i64) * 256 / LIST_MS as i64).clamp(0, 256) as i32;
                let s = ease_out_cubic(local);
                let ry = list_top + (row as i32) * row_h + (256 - s) * dp.d(12) / 256;
                let is_sel = idx == self.selected;
                if !is_sel && row > 0 {
                    gfx.fill_rect(dp.x(74), ry, dp.d(254), 1, mix_color(CARD, DIVIDER, s, 256));
                }

                let icon_s = dp.d(38);
                draw_item_icon(gfx, dp.x(30), ry + (row_h - icon_s) / 2, icon_s, item.kind, item.name_str());

                gfx.text(dp.x(78), ry + dp.d(14), dp.d(9), mix_color(CARD, INK, s, 256), 2, item.name_str());

                let mut sub = Line::new();
                if item.kind == wire::KIND_DIR {
                    sub.text(if en { "Folder" } else { "Klasör" });
                } else if item.name_str().ends_with(".ntzc") {
                    sub.text(if en { "App Package  •  " } else { "Uygulama Paketi  •  " });
                    format_size(item.size, &mut sub);
                } else {
                    format_size(item.size, &mut sub);
                }
                gfx.text(dp.x(78), ry + dp.d(31), dp.d(7), mix_color(CARD, MUTED, s, 256), 1, sub.as_str());

                draw_chevron_right(gfx, dp.x(324), ry + row_h / 2, &dp, if is_sel { ev.accent } else { SUBTLE });
            }
        }

        // Sayfa göstergesi (birden fazla sayfa varsa)
        if self.item_count > ROWS_PER_PAGE {
            let pages = (self.item_count + ROWS_PER_PAGE - 1) / ROWS_PER_PAGE;
            let cur_page = self.scroll / ROWS_PER_PAGE;
            for p in 0..pages.min(6) {
                let dx = dp.x(180) - ((pages as i32 - 1) * dp.d(6)) + (p as i32) * dp.d(12);
                let col = if p == cur_page { ev.accent } else { TRACK };
                gfx.dot(dx, dp.d(403), dp.d(3).max(2), col, 1024);
            }
        }

        // USB birim ozet satiri (y: 396..412 dp)
        if sec == 3 && self.usb.mounted {
            let mut info = Line::new();
            info.text(self.usb.label_str())
                .text("  •  ")
                .text(self.usb.fs_type_str())
                .text("  •  ")
                .number((self.usb.total_kib / 1024) as u32, 1)
                .text(" MB  •  ")
                .text(if self.usb.read_only {
                    if en { "Read-Only" } else { "Salt-Okunur" }
                } else {
                    if en { "Read-Write" } else { "Yazilabilir" }
                });
            gfx.text_centered(dp.x(180), dp.d(399), dp.d(7), pal.muted, 1, info.as_str());
        }

        // Minimalist Alt Eylem Çubuğu
        if sec == 3 {
            draw_pill_btn(
                gfx,
                dp.x(16),
                dp.d(416),
                dp.d(104),
                dp.d(34),
                CARD,
                INK,
                dp.d(8),
                if en { "Rescan" } else { "Yeniden Tara" },
            );
            if self.usb.mounted {
                draw_pill_btn(
                    gfx,
                    dp.x(126),
                    dp.d(416),
                    dp.d(104),
                    dp.d(34),
                    CORAL_SOFT,
                    CORAL,
                    dp.d(8),
                    if en { "Eject" } else { "Güvenle Çıkar" },
                );
                if self.item_count > 0 {
                    draw_pill_btn(
                        gfx,
                        dp.x(236),
                        dp.d(416),
                        dp.d(108),
                        dp.d(34),
                        ev.accent,
                        WHITE,
                        dp.d(8),
                        if en { "Open" } else { "Aç" },
                    );
                }
            }
        } else if self.is_writable_dir() {
            draw_pill_btn(
                gfx,
                dp.x(16),
                dp.d(416),
                dp.d(104),
                dp.d(34),
                CARD,
                INK,
                dp.d(8),
                if en { "New Folder" } else { "Yeni Klasör" },
            );
            draw_pill_btn(
                gfx,
                dp.x(126),
                dp.d(416),
                dp.d(104),
                dp.d(34),
                ev.accent,
                WHITE,
                dp.d(8),
                if en { "New Note" } else { "Yeni Not" },
            );
            if self.item_count > 0 {
                draw_pill_btn(
                    gfx,
                    dp.x(236),
                    dp.d(416),
                    dp.d(56),
                    dp.d(34),
                    CARD,
                    INK,
                    dp.d(8),
                    if en { "Open" } else { "Aç" },
                );
                draw_pill_btn(
                    gfx,
                    dp.x(296),
                    dp.d(416),
                    dp.d(48),
                    dp.d(34),
                    CORAL_SOFT,
                    CORAL,
                    dp.d(8),
                    if en { "Del" } else { "Sil" },
                );
            }
        } else if self.item_count > 0 {
            draw_pill_btn(
                gfx,
                dp.x(80),
                dp.d(416),
                dp.d(200),
                dp.d(34),
                ev.accent,
                WHITE,
                dp.d(8),
                if en { "Open Selected" } else { "Seçileni Aç" },
            );
        }
    }
}

impl RingApp for FilesApp {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT => {
                self.now_ms = ev.uptime_ms;
                self.refresh_usb();
                if !self.initialized {
                    self.initialized = true;
                    self.set_path("/data/media");
                }
                self.seg_pos = self.active_section() as i32 * 256;
                RET_REDRAW | RET_ANIMATING
            }
            OP_RESUME => {
                self.refresh_usb();
                if self.mode == Mode::Browse {
                    self.refresh_dir();
                }
                RET_REDRAW
            }
            OP_PAUSE => RET_NONE,
            OP_UPDATE => {
                let mut ret = self.handle_key(ev.key);
                if self.tick(ev.uptime_ms) {
                    ret |= RET_REDRAW | RET_ANIMATING;
                }
                if ret & RET_CLOSE != 0 {
                    return ret;
                }
                if let Some(p) = pointer(ev) {
                    if p.clicked {
                        let dp = Dp::new(ev.area_w, ev.area_h);
                        ret |= self.handle_click(p.x, p.y, &dp, ev.lang);
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

static mut APP: FilesApp = FilesApp::new();
static mut COMMANDS: [u8; 16384] = [0; 16384];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
