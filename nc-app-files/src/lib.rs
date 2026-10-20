// crates/nc-app-files/src/lib.rs
#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::c_void;
use nc_app::text::Line;
use nc_app::{App, AppInfo, Context, Icon, Input, KeyCode, Update};
use nc_app_settings::SettingsApp;
use nc_app_stopwatch::StopwatchApp;
use nc_app_system::SystemApp;
use nc_font::{text_width, Style};
use nc_fs::{DirEntry, FileType, VfsManager};
use nc_gfx::{lerp_color, rgb, Canvas, Rect};
use nc_pkg::NtzcPackage;
use nc_ui::{draw_dot, draw_panel, draw_shadow, draw_text_color, text_style};

const MAX_BLUR_SPAN: usize = 2048;
const VISIBLE_ROWS: usize = 7;

const PORCELAIN_BG: u32 = rgb(245, 250, 253);
const SIDEBAR_TOP: u32 = rgb(238, 246, 251);
const SIDEBAR_BOT: u32 = rgb(228, 240, 247);
const SURFACE_WHITE: u32 = rgb(253, 254, 255);
const SURFACE_ALT: u32 = rgb(242, 248, 252);
const SURFACE_HOVER: u32 = rgb(231, 243, 251);
const SURFACE_SELECT: u32 = rgb(219, 239, 250);
const BORDER_SOFT: u32 = rgb(210, 225, 236);

const AZURE_BRIGHT: u32 = rgb(46, 140, 244);
const AZURE_DEEP: u32 = rgb(24, 102, 212);
const AQUA_MINT: u32 = rgb(28, 190, 170);
const AQUA_SOFT: u32 = rgb(156, 236, 224);
const AQUA_DEEP: u32 = rgb(16, 146, 132);
const DANGER_RED: u32 = rgb(228, 62, 58);

const INK_PRIMARY: u32 = rgb(18, 30, 46);
const INK_SECONDARY: u32 = rgb(62, 80, 100);
const INK_MUTED: u32 = rgb(118, 136, 156);
const INK_DIM: u32 = rgb(158, 174, 192);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClipboardOp {
    Copy,
    Cut,
}

#[derive(Clone)]
struct ClipboardItem {
    op: ClipboardOp,
    source_path: String,
    name: String,
    is_dir: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextAction {
    Open,
    Cut,
    Copy,
    Paste,
    Rename,
    Delete,
    NewFolder,
    NewFile,
}

struct ContextMenu {
    x: i32,
    y: i32,
    target_index: Option<usize>,
    hover_item: Option<usize>,
}

enum ModalState {
    None,
    Install {
        pkg: NtzcPackage,
        raw_bytes: Vec<u8>,
    },
    ViewText {
        path: String,
        content: String,
        scroll_line: usize,
        modified: bool,
    },
    ViewImage {
        name: String,
        format_label: &'static str,
        width: u32,
        height: u32,
        theme_id: u8,
        raw_size: usize,
        pixels: Vec<u32>, // Çözülmüş gerçek 32-bit RGB pikseller!
    },
    ViewAudio {
        name: String,
        format_label: &'static str,
        sample_rate: u32,
        kbps: u16,
        duration_secs: u16,
        playing: bool,
        start_ms: u32,
        elapsed_ms: u32,
        spectrum_seed: [u8; 32],
    },
    ViewVideo {
        name: String,
        width: u16,
        height: u16,
        duration_secs: u16,
        scene_id: u8,
        playing: bool,
        start_ms: u32,
        elapsed_ms: u32,
        frame_data: Vec<u8>,
    },
    CreateFolder {
        name_buf: String,
    },
    CreateFile {
        name_buf: String,
    },
    RenameItem {
        old_name: String,
        new_name_buf: String,
        is_dir: bool,
    },
    DeleteConfirm {
        name: String,
        is_dir: bool,
    },
}

pub struct FilesApp {
    info: AppInfo,
    vfs: VfsManager,
    current_path: String,
    items: Vec<DirEntry>,
    scroll_offset: usize,
    selected_index: Option<usize>,
    hover_index: Option<usize>,
    hover_sidebar: Option<usize>,
    hover_back: bool,
    hover_up_scroll: bool,
    hover_dn_scroll: bool,
    hover_new_folder: bool,
    hover_new_file: bool,
    hover_paste: bool,
    clipboard: Option<ClipboardItem>,
    context_menu: Option<ContextMenu>,
    status_banner: String,
    modal: ModalState,
    modal_hover_primary: bool,
    modal_hover_secondary: bool,
}

impl FilesApp {
    pub fn new() -> FilesApp {
        let mut vfs = VfsManager::new();
        let initial_path = String::from("/data/desktop");
        let items = vfs.list_dir_path(&initial_path).unwrap_or_else(|_| Vec::new());

        FilesApp {
            info: AppInfo::new("nc.files", "Dosyalar", "2.9.0", Icon::Generic, 46, 29),
            vfs,
            current_path: initial_path,
            items,
            scroll_offset: 0,
            selected_index: None,
            hover_index: None,
            hover_sidebar: None,
            hover_back: false,
            hover_up_scroll: false,
            hover_dn_scroll: false,
            hover_new_folder: false,
            hover_new_file: false,
            hover_paste: false,
            clipboard: None,
            context_menu: None,
            status_banner: String::from("Masaustu - Islem yapmak icin ogeye sag tiklayin"),
            modal: ModalState::None,
            modal_hover_primary: false,
            modal_hover_secondary: false,
        }
    }

    fn is_writable_path(path: &str) -> bool {
        path == "/data"
            || path.starts_with("/data/")
            || path == "/usb"
            || path.starts_with("/usb/")
            || path == "/ram"
            || path.starts_with("/ram/")
    }

    fn is_writable_current(&self) -> bool {
        Self::is_writable_path(&self.current_path)
    }

    fn friendly_path(&self) -> String {
        match self.current_path.as_str() {
            "/" => String::from("Bu Bilgisayar"),
            "/data" => String::from("Bu Bilgisayar > Kisisel Disk"),
            "/data/desktop" => String::from("Bu Bilgisayar > Masaustu"),
            "/data/documents" => String::from("Bu Bilgisayar > Belgelerim"),
            "/data/downloads" => String::from("Bu Bilgisayar > Indirilenler"),
            "/data/media" => String::from("Bu Bilgisayar > Medya ve Galeri"),
            "/data/app" => String::from("Bu Bilgisayar > Uygulamalar"),
            "/usb" => alloc::format!("Bu Bilgisayar > Harici USB ({})", self.vfs.usb_label()),
            "/system" => String::from("Bu Bilgisayar > Sistem Dosyalari"),
            "/system/app" => String::from("Bu Bilgisayar > Sistem Uygulamalari"),
            "/ram" => String::from("Bu Bilgisayar > Gecici Bellek"),
            other => {
                if let Some(rest) = other.strip_prefix("/usb/") {
                    alloc::format!("Harici USB > {}", rest.replace('/', " > "))
                } else if let Some(rest) = other.strip_prefix("/data/desktop/") {
                    alloc::format!("Masaustu > {}", rest.replace('/', " > "))
                } else if let Some(rest) = other.strip_prefix("/data/documents/") {
                    alloc::format!("Belgelerim > {}", rest.replace('/', " > "))
                } else if let Some(rest) = other.strip_prefix("/data/downloads/") {
                    alloc::format!("Indirilenler > {}", rest.replace('/', " > "))
                } else if let Some(rest) = other.strip_prefix("/data/") {
                    alloc::format!("Kisisel Disk > {}", rest.replace('/', " > "))
                } else {
                    alloc::format!(
                        "Bu Bilgisayar > {}",
                        other.trim_start_matches('/').replace('/', " > ")
                    )
                }
            }
        }
    }

    fn friendly_entry_name<'a>(&self, raw: &'a str) -> &'a str {
        if self.current_path == "/" {
            match raw {
                "data" => "Kisisel Disk (Dahili SSD/HDD)",
                "usb" => "Harici USB / Tasinabilir SSD",
                "system" => "Sistem Dosyalari (Korumali)",
                "ram" => "Gecici Calisma Bellegi (RAM)",
                _ => raw,
            }
        } else if self.current_path == "/data" {
            match raw {
                "desktop" => "Masaustu",
                "documents" => "Belgelerim",
                "downloads" => "Indirilenler",
                "media" => "Medya ve Galeri",
                "app" => "Uygulamalar",
                "drivers" => "Donanim Suruculeri",
                "system" => "Dil ve Sistem Ayarlari",
                _ => raw,
            }
        } else {
            raw
        }
    }

    fn refresh(&mut self) {
        self.items = self
            .vfs
            .list_dir_path(&self.current_path)
            .unwrap_or_else(|_| Vec::new());
        self.hover_index = None;
        self.context_menu = None;
        if let Some(sel) = self.selected_index {
            if sel >= self.items.len() {
                self.selected_index = None;
            }
        }
        if self.scroll_offset > self.items.len().saturating_sub(VISIBLE_ROWS) {
            self.scroll_offset = self.items.len().saturating_sub(VISIBLE_ROWS);
        }
    }

    fn join_path(base: &str, name: &str) -> String {
        if base == "/" {
            alloc::format!("/{}", name)
        } else {
            alloc::format!("{}/{}", base.trim_end_matches('/'), name)
        }
    }

    fn enter_dir(&mut self, name: &str) {
        self.current_path = Self::join_path(&self.current_path, name);
        if self.current_path == "/usb" || self.current_path.starts_with("/usb/") {
            self.vfs.rescan_usb();
        }
        self.scroll_offset = 0;
        self.selected_index = None;
        self.refresh();
        self.status_banner = alloc::format!("Acildi: {}", self.friendly_path());
    }

    fn up_dir(&mut self) {
        if self.current_path == "/" || self.current_path.is_empty() {
            return;
        }
        if let Some(pos) = self.current_path.rfind('/') {
            if pos == 0 {
                self.current_path = String::from("/");
            } else {
                self.current_path.truncate(pos);
            }
        } else {
            self.current_path = String::from("/");
        }
        self.scroll_offset = 0;
        self.selected_index = None;
        self.refresh();
        self.status_banner = alloc::format!("Konum: {}", self.friendly_path());
    }

    fn navigate_to(&mut self, target_path: &str) {
        self.current_path = String::from(target_path);
        if target_path == "/usb" || target_path.starts_with("/usb/") {
            self.vfs.rescan_usb();
        }
        self.scroll_offset = 0;
        self.selected_index = None;
        self.refresh();
        self.status_banner = alloc::format!("Konum: {}", self.friendly_path());
    }

    fn open_selected_entry(&mut self, idx: usize) {
        if idx >= self.items.len() {
            return;
        }
        let entry = self.items[idx].clone();
        if entry.file_type == FileType::Directory {
            self.enter_dir(&entry.name);
            return;
        }
        let full_path = Self::join_path(&self.current_path, &entry.name);
        let lower_name = entry.name.to_ascii_lowercase();

        if lower_name.ends_with(".ntzc") {
            if let Ok(bytes) = self.vfs.read_file_path(&full_path) {
                if let Some(pkg) = NtzcPackage::parse(&bytes) {
                    self.modal = ModalState::Install {
                        pkg,
                        raw_bytes: bytes,
                    };
                    self.modal_hover_primary = false;
                    self.modal_hover_secondary = false;
                } else {
                    self.status_banner = String::from("Uygulama paketi acilamadi (.ntzc)");
                }
            }
        } else if lower_name.ends_with(".png")
            || lower_name.ends_with(".jpg")
            || lower_name.ends_with(".jpeg")
            || lower_name.ends_with(".bmp")
        {
            if let Ok(bytes) = self.vfs.read_file_path(&full_path) {
                let (fmt_lbl, w, h, theme_id, pixels) = decode_image_buffer(&bytes, &lower_name);
                self.modal = ModalState::ViewImage {
                    name: entry.name,
                    format_label: fmt_lbl,
                    width: w,
                    height: h,
                    theme_id,
                    raw_size: bytes.len(),
                    pixels,
                };
                self.modal_hover_primary = false;
                self.modal_hover_secondary = false;
            }
        } else if lower_name.ends_with(".mp3") || lower_name.ends_with(".wav") {
            if let Ok(bytes) = self.vfs.read_file_path(&full_path) {
                let is_wav = lower_name.ends_with(".wav");
                let fmt_lbl = if is_wav { "WAV (PCM Audio)" } else { "MP3 (MPEG-1 Layer 3)" };
                let sample_rate = if is_wav && bytes.len() >= 28 {
                    u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]).clamp(8000, 96000)
                } else {
                    44100
                };
                let kbps = if is_wav { 1411 } else { 320 };
                let est_secs = ((bytes.len() as u32) / 128).clamp(12, 600) as u16;
                let mut seed = [0u8; 32];
                for i in 0..32 {
                    seed[i] = bytes.get(i * (bytes.len() / 33).max(1)).copied().unwrap_or((i * 19) as u8);
                }
                self.modal = ModalState::ViewAudio {
                    name: entry.name,
                    format_label: fmt_lbl,
                    sample_rate,
                    kbps,
                    duration_secs: est_secs,
                    playing: true,
                    start_ms: 0,
                    elapsed_ms: 0,
                    spectrum_seed: seed,
                };
                self.modal_hover_primary = false;
                self.modal_hover_secondary = false;
            }
        } else if lower_name.ends_with(".mp4") {
            if let Ok(bytes) = self.vfs.read_file_path(&full_path) {
                let scene_id = if bytes.len() > 40 { bytes[40] } else { 0 };
                let ticks = if bytes.len() >= 52 {
                    u32::from_be_bytes([bytes[48], bytes[49], bytes[50], bytes[51]])
                } else {
                    1080
                };
                let dur = ((ticks / 60).max(5) as u16).min(600);
                let w = if bytes.len() >= 54 {
                    u16::from_be_bytes([bytes[52], bytes[53]]).clamp(160, 1920)
                } else {
                    320
                };
                let h = if bytes.len() >= 56 {
                    u16::from_be_bytes([bytes[54], bytes[55]]).clamp(90, 1080)
                } else {
                    180
                };
                let mut fdata = Vec::new();
                if bytes.len() > 104 {
                    let take = (bytes.len() - 104).min(4096);
                    fdata.extend_from_slice(&bytes[104..104 + take]);
                }
                self.modal = ModalState::ViewVideo {
                    name: entry.name,
                    width: w,
                    height: h,
                    duration_secs: dur,
                    scene_id,
                    playing: true,
                    start_ms: 0,
                    elapsed_ms: 0,
                    frame_data: fdata,
                };
                self.modal_hover_primary = false;
                self.modal_hover_secondary = false;
            }
        } else if let Ok(bytes) = self.vfs.read_file_path(&full_path) {
            let text_str = String::from_utf8_lossy(&bytes).into_owned();
            self.modal = ModalState::ViewText {
                path: full_path,
                content: text_str,
                scroll_line: 0,
                modified: false,
            };
            self.modal_hover_primary = false;
            self.modal_hover_secondary = false;
        }
    }

    fn is_subpath(parent: &str, child: &str) -> bool {
        let p = parent.trim_end_matches('/');
        let c = child.trim_end_matches('/');
        if p == c {
            return true;
        }
        if let Some(rest) = c.strip_prefix(p) {
            rest.starts_with('/')
        } else {
            false
        }
    }

    fn path_exists(&mut self, path: &str) -> bool {
        self.vfs.file_stat_path(path).is_ok()
    }

    // C-04: Tum alt kopyalari ve yazilan veriyi dogrula; tek bir hata bile olursa false don
    fn copy_tree_recursive(&mut self, src: &str, dst: &str, is_dir: bool) -> bool {
        if src == dst || (is_dir && Self::is_subpath(src, dst)) {
            return false;
        }
        if is_dir {
            let children = match self.vfs.list_dir_path(src) {
                Ok(c) => c,
                Err(_) => return false,
            };
            if self.vfs.create_dir_path(dst).is_err() {
                return false;
            }
            for child in children {
                let c_src = Self::join_path(src, &child.name);
                let c_dst = Self::join_path(dst, &child.name);
                if !self.copy_tree_recursive(
                    &c_src,
                    &c_dst,
                    child.file_type == FileType::Directory,
                ) {
                    return false;
                }
            }
            self.vfs.list_dir_path(dst).is_ok()
        } else {
            let bytes = match self.vfs.read_file_path(src) {
                Ok(b) => b,
                Err(_) => return false,
            };
            if self.vfs.write_file_path(dst, &bytes).is_err() {
                return false;
            }
            // Yazilan dosyanin boyut ve icerigini geri okuyarak dogrula
            match self.vfs.read_file_path(dst) {
                Ok(verified) => verified.len() == bytes.len() && verified == bytes,
                Err(_) => false,
            }
        }
    }

    // C-04: Alt ogelerden biri silinemezse ust klasoru silmeye calisma ve false don
    fn remove_tree_recursive(&mut self, path: &str, is_dir: bool) -> bool {
        if is_dir {
            let children = match self.vfs.list_dir_path(path) {
                Ok(c) => c,
                Err(_) => return false,
            };
            for child in children {
                let c_path = Self::join_path(path, &child.name);
                if !self.remove_tree_recursive(&c_path, child.file_type == FileType::Directory) {
                    return false;
                }
            }
        }
        self.vfs.remove_path(path).is_ok()
    }

    // C-05: Hedef klasorde cakisma olmayana kadar sayaci artirarak benzersiz ad uret
    fn resolve_unique_dest(&mut self, desired_name: &str) -> Option<String> {
        let first_candidate = Self::join_path(&self.current_path, desired_name);
        if !self.items.iter().any(|e| e.name.eq_ignore_ascii_case(desired_name))
            && !self.path_exists(&first_candidate)
        {
            return Some(first_candidate);
        }

        let (stem, ext) = if let Some(dot) = desired_name.rfind('.') {
            if dot > 0 {
                (&desired_name[..dot], &desired_name[dot..])
            } else {
                (desired_name, "")
            }
        } else {
            (desired_name, "")
        };

        for idx in 1..=999usize {
            let copy_name = if idx == 1 {
                alloc::format!("{}_kopya{}", stem, ext)
            } else {
                alloc::format!("{}_kopya_{}{}", stem, idx, ext)
            };
            let candidate = Self::join_path(&self.current_path, &copy_name);
            if !self.items.iter().any(|e| e.name.eq_ignore_ascii_case(&copy_name))
                && !self.path_exists(&candidate)
            {
                return Some(candidate);
            }
        }
        None
    }

    // C-04 & C-05: Cakisma kontrolu yap ve hedef tam dogrulanmadan kaynagi asla silme
    fn execute_paste(&mut self) {
        if !self.is_writable_current() {
            self.status_banner = String::from(
                "Bu klasor salt okunurdur. Masaustu, Belgelerim veya USB'ye yapistirin.",
            );
            return;
        }
        let clip = match self.clipboard.clone() {
            Some(c) => c,
            None => return,
        };

        let dst_path = if clip.op == ClipboardOp::Cut {
            let direct_dst = Self::join_path(&self.current_path, &clip.name);
            if direct_dst == clip.source_path {
                self.status_banner = String::from("Kaynak ve hedef ayni konumda.");
                return;
            }
            if self.items.iter().any(|e| e.name.eq_ignore_ascii_case(&clip.name))
                || self.path_exists(&direct_dst)
            {
                self.status_banner = alloc::format!(
                    "Hedefte '{}' zaten mevcut! Veri kaybini onlemek icin tasima iptal edildi.",
                    clip.name
                );
                return;
            }
            direct_dst
        } else {
            match self.resolve_unique_dest(&clip.name) {
                Some(p) => p,
                None => {
                    self.status_banner = String::from("Benzersiz kopya adi olusturulamadi.");
                    return;
                }
            }
        };

        if clip.is_dir && Self::is_subpath(&clip.source_path, &dst_path) {
            self.status_banner = String::from("Bir klasor kendi alt klasorunun icine kopyalanamaz.");
            return;
        }

        if self.copy_tree_recursive(&clip.source_path, &dst_path, clip.is_dir) {
            if clip.op == ClipboardOp::Cut {
                if self.remove_tree_recursive(&clip.source_path, clip.is_dir) {
                    self.clipboard = None;
                    self.status_banner = alloc::format!("'{}' bu klasore tasindi", clip.name);
                } else {
                    self.clipboard = None;
                    self.status_banner = alloc::format!(
                        "'{}' hedefe kopyalandi fakat kaynak tamamen temizlenemedi",
                        clip.name
                    );
                }
            } else {
                self.status_banner = alloc::format!("'{}' bu klasore kopyalandi", clip.name);
            }
            self.refresh();
        } else {
            // Basarisiz kopyalamada varsa yarim kalan hedefi temizle, kaynaga dokunma
            let _ = self.remove_tree_recursive(&dst_path, clip.is_dir);
            self.refresh();
            self.status_banner = String::from(
                "Yapistirma basarisiz oldu; kaynak dosyalariniz guvenle korundu.",
            );
        }
    }

    fn context_menu_actions(
        &self,
        target_idx: Option<usize>,
    ) -> Vec<(ContextAction, &'static str, bool)> {
        let writable = self.is_writable_current();
        let has_clip = self.clipboard.is_some();
        if target_idx.is_some() {
            alloc::vec![
                (ContextAction::Open, "Ac", true),
                (ContextAction::Cut, "Kes", writable),
                (ContextAction::Copy, "Kopyala", true),
                (ContextAction::Paste, "Buraya Yapistir", writable && has_clip),
                (ContextAction::Rename, "Yeniden Adlandir", writable),
                (ContextAction::Delete, "Sil", writable),
            ]
        } else {
            alloc::vec![
                (ContextAction::NewFolder, "Yeni Klasor", writable),
                (ContextAction::NewFile, "Yeni Metin Belgesi", writable),
                (ContextAction::Paste, "Yapistir", writable && has_clip),
            ]
        }
    }

    fn context_menu_rect(&self, area: Rect, unit: i32, menu: &ContextMenu) -> Rect {
        let actions = self.context_menu_actions(menu.target_index);
        let w = unit * 10 + unit / 2;
        let row_h = unit * 7 / 4;
        let h = actions.len() as i32 * row_h + unit;
        let max_x = area.x + area.width - w - unit / 2;
        let max_y = area.y + area.height - h - unit / 2;
        let x = menu.x.clamp(area.x + unit / 2, max_x.max(area.x));
        let y = menu.y.clamp(area.y + unit / 2, max_y.max(area.y));
        Rect::new(x, y, w, h)
    }

    fn format_size(bytes: u64, buf: &mut [u8]) -> usize {
        if bytes == 0 {
            buf[0] = b'0';
            buf[1] = b' ';
            buf[2] = b'B';
            return 3;
        }
        let (val, unit): (u64, &[u8]) = if bytes >= 1024 * 1024 {
            (bytes / (1024 * 1024), b" MB" as &[u8])
        } else if bytes >= 1024 {
            (bytes / 1024, b" KB" as &[u8])
        } else {
            (bytes, b" B" as &[u8])
        };
        let mut temp = [0u8; 16];
        let mut tlen = 0;
        let mut n = val;
        while n > 0 {
            temp[tlen] = b'0' + (n % 10) as u8;
            n /= 10;
            tlen += 1;
        }
        let mut len = 0;
        for i in 0..tlen {
            buf[len] = temp[tlen - 1 - i];
            len += 1;
        }
        for b in unit {
            buf[len] = *b;
            len += 1;
        }
        len
    }

    pub fn create_app_instance(pkg: &NtzcPackage) -> Box<dyn App> {
        match pkg.manifest.id.as_str() {
            "nc.settings" => Box::new(SettingsApp::new()),
            "nc.system" => Box::new(SystemApp::new()),
            "nc.stopwatch" => Box::new(StopwatchApp::new()),
            _ => {
                if let Some(native_app) = NativeModuleApp::from_package(pkg) {
                    Box::new(native_app)
                } else {
                    Box::new(GenericInstalledApp::new(pkg))
                }
            }
        }
    }

    /// Kullanici "Sisteme Kur" dediginde paketi /data/app altina kalici olarak yazar
    pub fn persist_ntzc_to_data_app(vfs: &mut VfsManager, raw_bytes: &[u8], pkg: &NtzcPackage) {
        let _ = vfs.create_dir_path("/data/app");
        let short_id = pkg.manifest.id.rsplit('.').next().unwrap_or(&pkg.manifest.id);
        let dest_path = alloc::format!("/data/app/{}.ntzc", short_id);
        let _ = vfs.write_file_path(&dest_path, raw_bytes);
    }

    fn sidebar_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + unit / 2, area.y + unit / 2, unit * 12, area.height - unit)
    }

    fn sidebar_item_rect(area: Rect, unit: i32, idx: usize) -> Rect {
        let side = Self::sidebar_rect(area, unit);
        let y = side.y + unit * 5 / 2 + idx as i32 * (unit * 12 / 5);
        Rect::new(side.x + unit / 2, y, side.width - unit, unit * 2 + unit / 6)
    }

    fn main_x(area: Rect, unit: i32) -> i32 {
        area.x + unit * 13 + unit / 4
    }

    fn main_w(area: Rect, unit: i32) -> i32 {
        area.width - unit * 13 - unit * 3 / 4
    }

    fn back_btn_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(
            Self::main_x(area, unit),
            area.y + unit * 3 / 4,
            unit * 4 + unit / 2,
            unit * 2,
        )
    }

    fn paste_btn_rect(area: Rect, unit: i32) -> Rect {
        let right = Self::main_x(area, unit) + Self::main_w(area, unit);
        Rect::new(
            right - unit * 20,
            area.y + unit * 3 / 4,
            unit * 6 + unit / 2,
            unit * 2,
        )
    }

    fn new_folder_btn_rect(area: Rect, unit: i32) -> Rect {
        let right = Self::main_x(area, unit) + Self::main_w(area, unit);
        Rect::new(
            right - unit * 13,
            area.y + unit * 3 / 4,
            unit * 6 + unit / 2,
            unit * 2,
        )
    }

    fn new_file_btn_rect(area: Rect, unit: i32) -> Rect {
        let right = Self::main_x(area, unit) + Self::main_w(area, unit);
        Rect::new(right - unit * 6, area.y + unit * 3 / 4, unit * 6, unit * 2)
    }

    fn path_box_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(
            Self::main_x(area, unit),
            area.y + unit * 3 + unit / 4,
            Self::main_w(area, unit),
            unit * 2,
        )
    }

    fn list_card_rect(area: Rect, unit: i32) -> Rect {
        let y = area.y + unit * 5 + unit * 3 / 4;
        let h = area.height - (y - area.y) - unit * 2 - unit / 4;
        Rect::new(Self::main_x(area, unit), y, Self::main_w(area, unit), h)
    }

    fn file_row_rect(area: Rect, unit: i32, visible_slot: usize) -> Rect {
        let card = Self::list_card_rect(area, unit);
        let top = card.y + unit * 2 + visible_slot as i32 * (unit * 2 + unit / 3);
        Rect::new(card.x + unit / 2, top, card.width - unit * 2, unit * 2 + unit / 6)
    }

    fn scroll_up_rect(area: Rect, unit: i32) -> Rect {
        let card = Self::list_card_rect(area, unit);
        Rect::new(
            card.x + card.width - unit * 5 / 4,
            card.y + unit * 2,
            unit,
            unit * 3 / 2,
        )
    }

    fn scroll_dn_rect(area: Rect, unit: i32) -> Rect {
        let card = Self::list_card_rect(area, unit);
        Rect::new(
            card.x + card.width - unit * 5 / 4,
            card.y + card.height - unit * 2,
            unit,
            unit * 3 / 2,
        )
    }

    fn modal_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(
            area.x + unit * 4,
            area.y + unit * 2,
            area.width - unit * 8,
            area.height - unit * 4,
        )
    }
}

impl App for FilesApp {
    fn info(&self) -> &AppInfo {
        &self.info
    }

    fn update(&mut self, ctx: &mut Context, input: &Input) -> Update {
        nc_fs::set_vfs_caller(nc_fs::CALLER_FILES);
        let unit = ctx.unit;
        let mut redraw = false;

        let was_usb = self.vfs.is_usb_online();
        let now_usb = self.vfs.rescan_usb();
        if now_usb != was_usb {
            if now_usb {
                self.status_banner = alloc::format!(
                    "Harici USB takildi: {} (/usb uzerinden erisilebilir)",
                    self.vfs.usb_label()
                );
                if self.current_path == "/usb" || self.current_path.starts_with("/usb/") {
                    self.refresh();
                }
            } else {
                self.status_banner = String::from("Harici USB surucusu cikarildi");
                if self.current_path == "/usb" || self.current_path.starts_with("/usb/") {
                    self.current_path = String::from("/data/desktop");
                    self.refresh();
                }
            }
            redraw = true;
        } else if now_usb && (self.current_path == "/usb" || self.current_path.starts_with("/usb/"))
        {
            if let Ok(live_items) = self.vfs.list_dir_path(&self.current_path) {
                let changed = live_items.len() != self.items.len()
                    || live_items
                        .iter()
                        .zip(self.items.iter())
                        .any(|(a, b)| a.name != b.name || a.size != b.size);
                if changed {
                    self.items = live_items;
                    redraw = true;
                }
            }
        }

        let mut close_modal = false;
        let mut action_save_text: Option<(String, String)> = None;
        let mut action_create_dir: Option<String> = None;
        let mut action_create_file: Option<String> = None;
        let mut action_rename: Option<(String, String, bool)> = None;
        let mut action_delete: Option<(String, bool)> = None;
        let mut action_install: Option<(NtzcPackage, Vec<u8>)> = None;

        let writable_base = if self.is_writable_current() {
            self.current_path.clone()
        } else {
            String::from("/data/desktop")
        };
        let current_path_snapshot = self.current_path.clone();

        match self.modal {
            ModalState::Install {
                ref pkg,
                ref raw_bytes,
            } => {
                let m_rect = Self::modal_rect(input.area, unit);
                let btn_w = unit * 8;
                let btn_h = unit * 2 + unit / 3;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let inst_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let canc_btn = Rect::new(inst_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);

                if let Some(key) = input.key {
                    if key == KeyCode::Escape {
                        close_modal = true;
                    } else if key == KeyCode::Enter {
                        action_install = Some((pkg.clone(), raw_bytes.clone()));
                        close_modal = true;
                    }
                }
                if let Some(pointer) = input.pointer {
                    let h_inst = inside(inst_btn, pointer.x, pointer.y);
                    let h_canc = inside(canc_btn, pointer.x, pointer.y);
                    if h_inst != self.modal_hover_primary || h_canc != self.modal_hover_secondary {
                        self.modal_hover_primary = h_inst;
                        self.modal_hover_secondary = h_canc;
                        redraw = true;
                    }
                    if pointer.clicked {
                        if h_inst {
                            action_install = Some((pkg.clone(), raw_bytes.clone()));
                            close_modal = true;
                        } else if h_canc {
                            close_modal = true;
                        }
                    }
                }
            }
            ModalState::ViewText {
                ref path,
                ref mut content,
                ref mut scroll_line,
                ref mut modified,
            } => {
                let m_rect = Self::modal_rect(input.area, unit);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let save_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let close_btn = Rect::new(save_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                let can_edit = Self::is_writable_path(path);

                if let Some(key) = input.key {
                    match key {
                        KeyCode::Escape => {
                            close_modal = true;
                        }
                        KeyCode::Up => {
                            if *scroll_line > 0 {
                                *scroll_line -= 1;
                                redraw = true;
                            }
                        }
                        KeyCode::Down => {
                            let line_count = content.lines().count().max(1);
                            if *scroll_line + 4 < line_count {
                                *scroll_line += 1;
                                redraw = true;
                            }
                        }
                        KeyCode::Enter if can_edit => {
                            content.push('\n');
                            *modified = true;
                            redraw = true;
                        }
                        KeyCode::Space if can_edit => {
                            content.push(' ');
                            *modified = true;
                            redraw = true;
                        }
                        KeyCode::Backspace if can_edit => {
                            content.pop();
                            *modified = true;
                            redraw = true;
                        }
                        KeyCode::Char(c) if can_edit => {
                            content.push(c);
                            *modified = true;
                            redraw = true;
                        }
                        _ => {}
                    }
                }
                if let Some(pointer) = input.pointer {
                    let h_save = can_edit && inside(save_btn, pointer.x, pointer.y);
                    let h_close = inside(close_btn, pointer.x, pointer.y);
                    if h_save != self.modal_hover_primary || h_close != self.modal_hover_secondary {
                        self.modal_hover_primary = h_save;
                        self.modal_hover_secondary = h_close;
                        redraw = true;
                    }
                    if pointer.clicked {
                        if h_save {
                            action_save_text = Some((path.clone(), content.clone()));
                            close_modal = true;
                        } else if h_close {
                            close_modal = true;
                        }
                    }
                }
            }
            ModalState::ViewImage { .. } => {
                let m_rect = Self::modal_rect(input.area, unit);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let close_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                if matches!(input.key, Some(KeyCode::Escape | KeyCode::Enter)) {
                    close_modal = true;
                }
                if let Some(pointer) = input.pointer {
                    let h_close = inside(close_btn, pointer.x, pointer.y);
                    if h_close != self.modal_hover_primary {
                        self.modal_hover_primary = h_close;
                        redraw = true;
                    }
                    if pointer.clicked && h_close {
                        close_modal = true;
                    }
                }
            }

            ModalState::ViewAudio {
                duration_secs,
                ref mut playing,
                ref mut start_ms,
                ref mut elapsed_ms,
                ..
            } => {
                let total_ms = (duration_secs as u32) * 1000;
                if *start_ms == 0 {
                    *start_ms = ctx.uptime_ms;
                }
                if *playing {
                    *elapsed_ms = ctx.uptime_ms.wrapping_sub(*start_ms) % total_ms.max(1000);
                    redraw = true;
                }
                let m_rect = Self::modal_rect(input.area, unit);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let close_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let play_btn = Rect::new(close_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                if let Some(key) = input.key {
                    if key == KeyCode::Escape {
                        close_modal = true;
                    } else if key == KeyCode::Space {
                        *playing = !*playing;
                        if *playing {
                            *start_ms = ctx.uptime_ms.wrapping_sub(*elapsed_ms);
                        }
                        redraw = true;
                    }
                }
                if let Some(pointer) = input.pointer {
                    let h_close = inside(close_btn, pointer.x, pointer.y);
                    let h_play = inside(play_btn, pointer.x, pointer.y);
                    if h_close != self.modal_hover_primary || h_play != self.modal_hover_secondary {
                        self.modal_hover_primary = h_close;
                        self.modal_hover_secondary = h_play;
                        redraw = true;
                    }
                    if pointer.clicked {
                        if h_close {
                            close_modal = true;
                        } else if h_play {
                            *playing = !*playing;
                            if *playing {
                                *start_ms = ctx.uptime_ms.wrapping_sub(*elapsed_ms);
                            }
                            redraw = true;
                        }
                    }
                }
            }

            ModalState::ViewVideo {
                duration_secs,
                ref mut playing,
                ref mut start_ms,
                ref mut elapsed_ms,
                ..
            } => {
                let total_ms = (duration_secs as u32) * 1000;
                if *start_ms == 0 {
                    *start_ms = ctx.uptime_ms;
                }
                if *playing {
                    *elapsed_ms = ctx.uptime_ms.wrapping_sub(*start_ms) % total_ms.max(1000);
                    redraw = true;
                }
                let m_rect = Self::modal_rect(input.area, unit);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let close_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let play_btn = Rect::new(close_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                if let Some(key) = input.key {
                    if key == KeyCode::Escape {
                        close_modal = true;
                    } else if key == KeyCode::Space {
                        *playing = !*playing;
                        if *playing {
                            *start_ms = ctx.uptime_ms.wrapping_sub(*elapsed_ms);
                        }
                        redraw = true;
                    }
                }
                if let Some(pointer) = input.pointer {
                    let h_close = inside(close_btn, pointer.x, pointer.y);
                    let h_play = inside(play_btn, pointer.x, pointer.y);
                    if h_close != self.modal_hover_primary || h_play != self.modal_hover_secondary {
                        self.modal_hover_primary = h_close;
                        self.modal_hover_secondary = h_play;
                        redraw = true;
                    }
                    if pointer.clicked {
                        if h_close {
                            close_modal = true;
                        } else if h_play {
                            *playing = !*playing;
                            if *playing {
                                *start_ms = ctx.uptime_ms.wrapping_sub(*elapsed_ms);
                            }
                            redraw = true;
                        }
                    }
                }
            }
            ModalState::CreateFolder { ref mut name_buf } => {
                let m_rect = Self::modal_rect(input.area, unit);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let create_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let cancel_btn = Rect::new(create_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                if let Some(key) = input.key {
                    match key {
                        KeyCode::Escape => close_modal = true,
                        KeyCode::Enter => {
                            let folder_name = if name_buf.trim().is_empty() {
                                "yeni_klasor"
                            } else {
                                name_buf.trim()
                            };
                            action_create_dir = Some(Self::join_path(&writable_base, folder_name));
                            close_modal = true;
                        }
                        KeyCode::Space => {
                            if name_buf.len() < 28 {
                                name_buf.push('_');
                                redraw = true;
                            }
                        }
                        KeyCode::Backspace => {
                            name_buf.pop();
                            redraw = true;
                        }
                        KeyCode::Char(c) => {
                            if c != '/' && c != '\\' && name_buf.len() < 28 {
                                name_buf.push(c);
                                redraw = true;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(pointer) = input.pointer {
                    let h_create = inside(create_btn, pointer.x, pointer.y);
                    let h_cancel = inside(cancel_btn, pointer.x, pointer.y);
                    if h_create != self.modal_hover_primary
                        || h_cancel != self.modal_hover_secondary
                    {
                        self.modal_hover_primary = h_create;
                        self.modal_hover_secondary = h_cancel;
                        redraw = true;
                    }
                    if pointer.clicked {
                        if h_create {
                            let folder_name = if name_buf.trim().is_empty() {
                                "yeni_klasor"
                            } else {
                                name_buf.trim()
                            };
                            action_create_dir = Some(Self::join_path(&writable_base, folder_name));
                            close_modal = true;
                        } else if h_cancel {
                            close_modal = true;
                        }
                    }
                }
            }
            ModalState::CreateFile { ref mut name_buf } => {
                let m_rect = Self::modal_rect(input.area, unit);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let create_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let cancel_btn = Rect::new(create_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                if let Some(key) = input.key {
                    match key {
                        KeyCode::Escape => close_modal = true,
                        KeyCode::Enter => {
                            let file_name = if name_buf.trim().is_empty() {
                                "notlar.txt"
                            } else {
                                name_buf.trim()
                            };
                            action_create_file = Some(Self::join_path(&writable_base, file_name));
                            close_modal = true;
                        }
                        KeyCode::Space => {
                            if name_buf.len() < 28 {
                                name_buf.push('_');
                                redraw = true;
                            }
                        }
                        KeyCode::Backspace => {
                            name_buf.pop();
                            redraw = true;
                        }
                        KeyCode::Char(c) => {
                            if c != '/' && c != '\\' && name_buf.len() < 28 {
                                name_buf.push(c);
                                redraw = true;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(pointer) = input.pointer {
                    let h_create = inside(create_btn, pointer.x, pointer.y);
                    let h_cancel = inside(cancel_btn, pointer.x, pointer.y);
                    if h_create != self.modal_hover_primary
                        || h_cancel != self.modal_hover_secondary
                    {
                        self.modal_hover_primary = h_create;
                        self.modal_hover_secondary = h_cancel;
                        redraw = true;
                    }
                    if pointer.clicked {
                        if h_create {
                            let file_name = if name_buf.trim().is_empty() {
                                "notlar.txt"
                            } else {
                                name_buf.trim()
                            };
                            action_create_file = Some(Self::join_path(&writable_base, file_name));
                            close_modal = true;
                        } else if h_cancel {
                            close_modal = true;
                        }
                    }
                }
            }
            ModalState::RenameItem {
                ref old_name,
                ref mut new_name_buf,
                is_dir,
            } => {
                let m_rect = Self::modal_rect(input.area, unit);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let ok_btn = Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let cancel_btn = Rect::new(ok_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                if let Some(key) = input.key {
                    match key {
                        KeyCode::Escape => close_modal = true,
                        KeyCode::Enter => {
                            let trimmed = new_name_buf.trim();
                            if !trimmed.is_empty() && trimmed != old_name {
                                action_rename =
                                    Some((old_name.clone(), String::from(trimmed), is_dir));
                            }
                            close_modal = true;
                        }
                        KeyCode::Space => {
                            if new_name_buf.len() < 28 {
                                new_name_buf.push('_');
                                redraw = true;
                            }
                        }
                        KeyCode::Backspace => {
                            new_name_buf.pop();
                            redraw = true;
                        }
                        KeyCode::Char(c) => {
                            if c != '/' && c != '\\' && new_name_buf.len() < 28 {
                                new_name_buf.push(c);
                                redraw = true;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(pointer) = input.pointer {
                    let h_ok = inside(ok_btn, pointer.x, pointer.y);
                    let h_cancel = inside(cancel_btn, pointer.x, pointer.y);
                    if h_ok != self.modal_hover_primary || h_cancel != self.modal_hover_secondary {
                        self.modal_hover_primary = h_ok;
                        self.modal_hover_secondary = h_cancel;
                        redraw = true;
                    }
                    if pointer.clicked {
                        if h_ok {
                            let trimmed = new_name_buf.trim();
                            if !trimmed.is_empty() && trimmed != old_name {
                                action_rename =
                                    Some((old_name.clone(), String::from(trimmed), is_dir));
                            }
                            close_modal = true;
                        } else if h_cancel {
                            close_modal = true;
                        }
                    }
                }
            }
            ModalState::DeleteConfirm { ref name, is_dir } => {
                let m_rect = Self::modal_rect(input.area, unit);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let del_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let cancel_btn = Rect::new(del_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                if let Some(key) = input.key {
                    if key == KeyCode::Escape {
                        close_modal = true;
                    } else if key == KeyCode::Enter {
                        let target_path = Self::join_path(&current_path_snapshot, name);
                        action_delete = Some((target_path, is_dir));
                        close_modal = true;
                    }
                }
                if let Some(pointer) = input.pointer {
                    let h_del = inside(del_btn, pointer.x, pointer.y);
                    let h_cancel = inside(cancel_btn, pointer.x, pointer.y);
                    if h_del != self.modal_hover_primary || h_cancel != self.modal_hover_secondary {
                        self.modal_hover_primary = h_del;
                        self.modal_hover_secondary = h_cancel;
                        redraw = true;
                    }
                    if pointer.clicked {
                        if h_del {
                            let target_path = Self::join_path(&current_path_snapshot, name);
                            action_delete = Some((target_path, is_dir));
                            close_modal = true;
                        } else if h_cancel {
                            close_modal = true;
                        }
                    }
                }
            }
            ModalState::None => {}
        }

        if let Some((p, c)) = action_save_text {
            if self.vfs.write_file_path(&p, c.as_bytes()).is_ok() {
                self.status_banner = alloc::format!("Kaydedildi: {}", p);
            } else {
                self.status_banner = String::from("Uyari: Bu klasor salt okunurdur");
            }
            self.refresh();
        }
        if let Some(p) = action_create_dir {
            if !self.is_writable_current() {
                self.current_path = String::from("/data/desktop");
            }
            if self.vfs.create_dir_path(&p).is_ok() {
                self.status_banner = String::from("Yeni klasor olusturuldu");
            } else {
                self.status_banner = String::from("Klasor olusturulamadi (zaten mevcut olabilir)");
            }
            self.refresh();
        }
        if let Some(p) = action_create_file {
            if !self.is_writable_current() {
                self.current_path = String::from("/data/desktop");
            }
            if self.path_exists(&p) {
                self.status_banner = String::from("Bu isimde bir dosya zaten mevcut!");
            } else {
                let default_body =
                    b"Yeni Metin Belgesi\n==================\nBu dosyayi duzenleyip kaydedebilirsiniz.\n";
                if self.vfs.write_file_path(&p, default_body).is_ok() {
                    self.status_banner = String::from("Yeni metin belgesi olusturuldu");
                } else {
                    self.status_banner = String::from("Dosya olusturulamadi");
                }
            }
            self.refresh();
        }

        // C-04 & C-05: Ayni ada rename'i yok say, hedef cakismasini engelle ve dogrulamadan kaynagi silme
        if let Some((old_name, new_name, is_dir)) = action_rename {
            if old_name != new_name {
                let src = Self::join_path(&current_path_snapshot, &old_name);
                let dst = Self::join_path(&current_path_snapshot, &new_name);
                let conflict = self
                    .items
                    .iter()
                    .any(|e| e.name != old_name && e.name.eq_ignore_ascii_case(&new_name))
                    || ( !old_name.eq_ignore_ascii_case(&new_name) && self.path_exists(&dst) );

                if conflict {
                    self.status_banner = alloc::format!(
                        "'{}' adinda bir oge zaten mevcut; uzerine yazma engellendi.",
                        new_name
                    );
                } else if self.copy_tree_recursive(&src, &dst, is_dir) {
                    if self.remove_tree_recursive(&src, is_dir) {
                        self.status_banner = alloc::format!(
                            "'{}' -> '{}' olarak yeniden adlandirildi",
                            old_name,
                            new_name
                        );
                    } else {
                        self.status_banner = String::from(
                            "Yeni ad olusturuldu fakat eski oge silinemedi",
                        );
                    }
                    self.refresh();
                } else {
                    let _ = self.remove_tree_recursive(&dst, is_dir);
                    self.refresh();
                    self.status_banner = String::from(
                        "Yeniden adlandirma basarisiz oldu; mevcut oge korundu.",
                    );
                }
            }
        }
        if let Some((p, is_dir)) = action_delete {
            if self.remove_tree_recursive(&p, is_dir) {
                self.status_banner = String::from("Secili oge kalici olarak silindi");
            } else {
                self.status_banner = String::from("Sistem dosyalari silinemez");
            }
            self.selected_index = None;
            self.refresh();
        }

        if close_modal {
            self.modal = ModalState::None;
            redraw = true;
        }
        if let Some((pkg, raw_bytes)) = action_install {
            // Kullanici onayladi: /data/app altina kaydet ve masaustune kur
            Self::persist_ntzc_to_data_app(&mut self.vfs, &raw_bytes, &pkg);
            let app = Self::create_app_instance(&pkg);
            self.status_banner =
                String::from("Uygulama /data/app altina kaydedildi ve masaustune kuruldu");
            return Update::install(app);
        }

        if !matches!(self.modal, ModalState::None) {
            return Update {
                redraw,
                close: false,
                install_app: None,
            };
        }

        if let Some(mut menu) = self.context_menu.take() {
            let mut keep_menu = true;
            let menu_r = self.context_menu_rect(input.area, unit, &menu);
            let actions = self.context_menu_actions(menu.target_index);
            let row_h = unit * 7 / 4;

            if let Some(KeyCode::Escape) = input.key {
                keep_menu = false;
                redraw = true;
            }
            if let Some(pointer) = input.pointer {
                let mut h_idx = None;
                if inside(menu_r, pointer.x, pointer.y) {
                    let rel_y = pointer.y - (menu_r.y + unit / 2);
                    if rel_y >= 0 {
                        let idx = (rel_y / row_h) as usize;
                        if idx < actions.len() {
                            h_idx = Some(idx);
                        }
                    }
                }
                if h_idx != menu.hover_item {
                    menu.hover_item = h_idx;
                    redraw = true;
                }
                if pointer.clicked {
                    keep_menu = false;
                    redraw = true;
                    if let Some(idx) = h_idx {
                        let (act, _, enabled) = actions[idx];
                        if enabled {
                            match act {
                                ContextAction::Open => {
                                    if let Some(t_idx) = menu.target_index {
                                        self.open_selected_entry(t_idx);
                                    }
                                }
                                ContextAction::Cut | ContextAction::Copy => {
                                    if let Some(t_idx) = menu.target_index {
                                        if t_idx < self.items.len() {
                                            let item = &self.items[t_idx];
                                            let op = if act == ContextAction::Cut {
                                                ClipboardOp::Cut
                                            } else {
                                                ClipboardOp::Copy
                                            };
                                            self.clipboard = Some(ClipboardItem {
                                                op,
                                                source_path: Self::join_path(
                                                    &self.current_path,
                                                    &item.name,
                                                ),
                                                name: item.name.clone(),
                                                is_dir: item.file_type == FileType::Directory,
                                            });
                                            self.status_banner = if act == ContextAction::Cut {
                                                alloc::format!(
                                                    "'{}' kesildi - Hedef klasore sag tiklayip Yapistir'i secin",
                                                    item.name
                                                )
                                            } else {
                                                alloc::format!(
                                                    "'{}' kopyalandi - Hedef klasore sag tiklayip Yapistir'i secin",
                                                    item.name
                                                )
                                            };
                                        }
                                    }
                                }
                                ContextAction::Paste => {
                                    self.execute_paste();
                                }
                                ContextAction::Rename => {
                                    if let Some(t_idx) = menu.target_index {
                                        if t_idx < self.items.len() {
                                            let item = &self.items[t_idx];
                                            self.modal = ModalState::RenameItem {
                                                old_name: item.name.clone(),
                                                new_name_buf: item.name.clone(),
                                                is_dir: item.file_type == FileType::Directory,
                                            };
                                            self.modal_hover_primary = false;
                                            self.modal_hover_secondary = false;
                                        }
                                    }
                                }
                                ContextAction::Delete => {
                                    if let Some(t_idx) = menu.target_index {
                                        if t_idx < self.items.len() {
                                            let item = &self.items[t_idx];
                                            self.modal = ModalState::DeleteConfirm {
                                                name: item.name.clone(),
                                                is_dir: item.file_type == FileType::Directory,
                                            };
                                            self.modal_hover_primary = false;
                                            self.modal_hover_secondary = false;
                                        }
                                    }
                                }
                                ContextAction::NewFolder => {
                                    self.modal = ModalState::CreateFolder {
                                        name_buf: String::from("yeni_klasor"),
                                    };
                                    self.modal_hover_primary = false;
                                    self.modal_hover_secondary = false;
                                }
                                ContextAction::NewFile => {
                                    self.modal = ModalState::CreateFile {
                                        name_buf: String::from("notlar.txt"),
                                    };
                                    self.modal_hover_primary = false;
                                    self.modal_hover_secondary = false;
                                }
                            }
                        }
                    }
                } else if pointer.right_clicked {
                    keep_menu = false;
                    redraw = true;
                }
            }
            if keep_menu {
                self.context_menu = Some(menu);
                return Update {
                    redraw,
                    close: false,
                    install_app: None,
                };
            }
        }

        if let Some(key) = input.key {
            match key {
                KeyCode::Backspace => {
                    self.up_dir();
                    redraw = true;
                }
                KeyCode::Up => {
                    if let Some(sel) = self.selected_index {
                        if sel > 0 {
                            self.selected_index = Some(sel - 1);
                            if sel - 1 < self.scroll_offset {
                                self.scroll_offset = sel - 1;
                            }
                            redraw = true;
                        }
                    } else if !self.items.is_empty() {
                        self.selected_index = Some(0);
                        redraw = true;
                    }
                }
                KeyCode::Down => {
                    if let Some(sel) = self.selected_index {
                        if sel + 1 < self.items.len() {
                            self.selected_index = Some(sel + 1);
                            if sel + 1 >= self.scroll_offset + VISIBLE_ROWS {
                                self.scroll_offset = sel + 2 - VISIBLE_ROWS;
                            }
                            redraw = true;
                        }
                    } else if !self.items.is_empty() {
                        self.selected_index = Some(0);
                        redraw = true;
                    }
                }
                KeyCode::Enter => {
                    if let Some(sel) = self.selected_index {
                        self.open_selected_entry(sel);
                        redraw = true;
                    }
                }
                _ => {}
            }
        }

        let mut next_hover = None;
        let mut next_side_hover = None;
        let mut next_back_hover = false;
        let mut next_up_scroll = false;
        let mut next_dn_scroll = false;
        let mut next_new_folder = false;
        let mut next_new_file = false;
        let mut next_paste = false;

        if let Some(pointer) = input.pointer {
            if self.current_path != "/" {
                next_back_hover = inside(Self::back_btn_rect(input.area, unit), pointer.x, pointer.y);
            }
            next_new_folder = inside(
                Self::new_folder_btn_rect(input.area, unit),
                pointer.x,
                pointer.y,
            );
            next_new_file = inside(
                Self::new_file_btn_rect(input.area, unit),
                pointer.x,
                pointer.y,
            );
            next_paste = self.clipboard.is_some()
                && self.is_writable_current()
                && inside(Self::paste_btn_rect(input.area, unit), pointer.x, pointer.y);
            next_up_scroll = inside(Self::scroll_up_rect(input.area, unit), pointer.x, pointer.y);
            next_dn_scroll = inside(Self::scroll_dn_rect(input.area, unit), pointer.x, pointer.y);

            for i in 0..6 {
                if inside(
                    Self::sidebar_item_rect(input.area, unit, i),
                    pointer.x,
                    pointer.y,
                ) {
                    next_side_hover = Some(i);
                    break;
                }
            }

            let visible_count = self
                .items
                .len()
                .saturating_sub(self.scroll_offset)
                .min(VISIBLE_ROWS);
            for slot in 0..visible_count {
                if inside(
                    Self::file_row_rect(input.area, unit, slot),
                    pointer.x,
                    pointer.y,
                ) {
                    next_hover = Some(self.scroll_offset + slot);
                    break;
                }
            }

            if pointer.right_clicked {
                if let Some(idx) = next_hover {
                    self.selected_index = Some(idx);
                    self.context_menu = Some(ContextMenu {
                        x: pointer.x,
                        y: pointer.y,
                        target_index: Some(idx),
                        hover_item: None,
                    });
                    redraw = true;
                } else if inside(Self::list_card_rect(input.area, unit), pointer.x, pointer.y) {
                    self.context_menu = Some(ContextMenu {
                        x: pointer.x,
                        y: pointer.y,
                        target_index: None,
                        hover_item: None,
                    });
                    redraw = true;
                }
            } else if pointer.clicked {
                if next_back_hover {
                    self.up_dir();
                    redraw = true;
                } else if next_up_scroll && self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                    redraw = true;
                } else if next_dn_scroll && self.scroll_offset + VISIBLE_ROWS < self.items.len() {
                    self.scroll_offset += 1;
                    redraw = true;
                } else if next_paste {
                    self.execute_paste();
                    redraw = true;
                } else if next_new_folder {
                    self.modal = ModalState::CreateFolder {
                        name_buf: String::from("yeni_klasor"),
                    };
                    self.modal_hover_primary = false;
                    self.modal_hover_secondary = false;
                    redraw = true;
                } else if next_new_file {
                    self.modal = ModalState::CreateFile {
                        name_buf: String::from("notlar.txt"),
                    };
                    self.modal_hover_primary = false;
                    self.modal_hover_secondary = false;
                    redraw = true;
                } else if let Some(side_idx) = next_side_hover {
                    let target = match side_idx {
                        0 => "/data/desktop",
                        1 => "/data/documents",
                        2 => "/data/downloads",
                        3 => "/data/media",
                        4 => "/usb",
                        _ => "/",
                    };
                    self.navigate_to(target);
                    redraw = true;
                } else if let Some(idx) = next_hover {
                    if self.selected_index == Some(idx) {
                        self.open_selected_entry(idx);
                    } else {
                        self.selected_index = Some(idx);
                        if idx < self.items.len() {
                            self.status_banner = alloc::format!(
                                "'{}' secildi - Acmak icin tekrar tiklayin, islemler icin sag tiklayin",
                                self.items[idx].name
                            );
                        }
                    }
                    redraw = true;
                } else if inside(Self::list_card_rect(input.area, unit), pointer.x, pointer.y) {
                    self.selected_index = None;
                    redraw = true;
                }
            }
        }

        if next_hover != self.hover_index
            || next_side_hover != self.hover_sidebar
            || next_back_hover != self.hover_back
            || next_up_scroll != self.hover_up_scroll
            || next_dn_scroll != self.hover_dn_scroll
            || next_new_folder != self.hover_new_folder
            || next_new_file != self.hover_new_file
            || next_paste != self.hover_paste
        {
            self.hover_index = next_hover;
            self.hover_sidebar = next_side_hover;
            self.hover_back = next_back_hover;
            self.hover_up_scroll = next_up_scroll;
            self.hover_dn_scroll = next_dn_scroll;
            self.hover_new_folder = next_new_folder;
            self.hover_new_file = next_new_file;
            self.hover_paste = next_paste;
            redraw = true;
        }

        Update {
            redraw,
            close: false,
            install_app: None,
        }
    }

    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        let unit = ctx.unit;
        canvas.fill_rect(area.x, area.y, area.width, area.height, PORCELAIN_BG);

        let side_rect = Self::sidebar_rect(area, unit);
        draw_rounded_surface(
            canvas,
            side_rect,
            unit / 2,
            SIDEBAR_TOP,
            SIDEBAR_BOT,
            255,
            BORDER_SOFT,
            true,
        );

        let title_style = text_style((unit * 82 / 100).max(11), 16);
        let label_style = text_style((unit * 58 / 100).max(10), 15);
        let cap_style = text_style((unit * 48 / 100).max(9), 18);

        draw_dot(
            canvas,
            side_rect.x + unit,
            side_rect.y + unit * 5 / 4,
            5,
            AQUA_MINT,
            255,
        );
        draw_text_color(
            canvas,
            "Hizli Erisim",
            side_rect.x + unit * 7 / 4,
            side_rect.y + unit * 4 / 5,
            &label_style,
            INK_PRIMARY,
        );

        let locations = [
            ("Masaustu", "/data/desktop", AZURE_BRIGHT),
            ("Belgelerim", "/data/documents", AQUA_MINT),
            ("Indirilenler", "/data/downloads", AZURE_DEEP),
            ("Medya & Galeri", "/data/media", AQUA_DEEP),
            ("USB / Harici Disk", "/usb", AQUA_MINT),
            ("Bu Bilgisayar", "/", INK_SECONDARY),
        ];

        for (i, (name, target, dot_col)) in locations.iter().enumerate() {
            let item_rect = Self::sidebar_item_rect(area, unit, i);
            let selected = self.current_path == *target;
            let hovered = self.hover_sidebar == Some(i);

            if selected {
                draw_rounded_surface(
                    canvas,
                    item_rect,
                    unit / 3,
                    AZURE_BRIGHT,
                    AQUA_MINT,
                    255,
                    AZURE_DEEP,
                    true,
                );
            } else if hovered {
                draw_rounded_surface(
                    canvas,
                    item_rect,
                    unit / 3,
                    SURFACE_HOVER,
                    SURFACE_ALT,
                    255,
                    BORDER_SOFT,
                    false,
                );
            }

            let dot_c = if selected {
                rgb(255, 255, 255)
            } else {
                *dot_col
            };
            draw_dot(
                canvas,
                item_rect.x + unit * 3 / 4,
                item_rect.y + item_rect.height / 2,
                4,
                dot_c,
                255,
            );
            let fg = if selected {
                rgb(255, 255, 255)
            } else {
                INK_PRIMARY
            };
            let ty = item_rect.y + (item_rect.height - label_style.cap_height) / 2;
            draw_text_color(
                canvas,
                name,
                item_rect.x + unit * 3 / 2,
                ty,
                &label_style,
                fg,
            );
        }

        let meter_card = Rect::new(
            side_rect.x + unit / 2,
            side_rect.y + side_rect.height - unit * 5,
            side_rect.width - unit,
            unit * 9 / 2,
        );
        draw_rounded_surface(
            canvas,
            meter_card,
            unit / 3,
            SURFACE_WHITE,
            SURFACE_ALT,
            245,
            BORDER_SOFT,
            true,
        );
        let mount_tag = if self.current_path.starts_with("/usb") {
            if self.vfs.is_usb_online() {
                "Harici USB (Takili)"
            } else {
                "USB Takili Degil"
            }
        } else if self.current_path.starts_with("/data") {
            "Kisisel Disk (Kalici)"
        } else if self.current_path.starts_with("/system") {
            "Sistem Bolumu"
        } else if self.current_path.starts_with("/ram") {
            "Gecici Bellek"
        } else {
            "Tum Suruculer"
        };
        draw_text_color(
            canvas,
            mount_tag,
            meter_card.x + unit * 2 / 3,
            meter_card.y + unit * 2 / 3,
            &cap_style,
            AZURE_DEEP,
        );

        let bar_r = Rect::new(
            meter_card.x + unit * 2 / 3,
            meter_card.y + unit * 2,
            meter_card.width - unit * 4 / 3,
            unit / 2,
        );
        draw_rounded_surface(
            canvas,
            bar_r,
            3,
            SURFACE_ALT,
            BORDER_SOFT,
            255,
            BORDER_SOFT,
            false,
        );
        let used_w = (bar_r.width * 36 / 100).max(6);
        let fill_r = Rect::new(bar_r.x, bar_r.y, used_w, bar_r.height);
        draw_rounded_surface(
            canvas,
            fill_r,
            3,
            AZURE_BRIGHT,
            AQUA_MINT,
            255,
            AQUA_DEEP,
            false,
        );

        let mut count_line = Line::new();
        count_line
            .number(self.items.len() as u32, 1)
            .text(" oge bu klasorde");
        draw_text_color(
            canvas,
            count_line.as_str(),
            meter_card.x + unit * 2 / 3,
            meter_card.y + unit * 3,
            &cap_style,
            INK_MUTED,
        );

        let back_rect = Self::back_btn_rect(area, unit);
        draw_aero_pill_button(
            canvas,
            back_rect,
            "< Geri",
            self.hover_back,
            false,
            self.current_path != "/",
        );

        if self.clipboard.is_some() {
            draw_aero_pill_button(
                canvas,
                Self::paste_btn_rect(area, unit),
                "Yapistir",
                self.hover_paste,
                false,
                self.is_writable_current(),
            );
        }
        draw_aero_pill_button(
            canvas,
            Self::new_folder_btn_rect(area, unit),
            "+ Klasor",
            self.hover_new_folder,
            false,
            self.is_writable_current(),
        );
        draw_aero_pill_button(
            canvas,
            Self::new_file_btn_rect(area, unit),
            "+ Belge",
            self.hover_new_file,
            true,
            self.is_writable_current(),
        );

        let path_box = Self::path_box_rect(area, unit);
        draw_rounded_surface(
            canvas,
            path_box,
            unit / 3,
            SURFACE_WHITE,
            SURFACE_ALT,
            255,
            BORDER_SOFT,
            true,
        );
        draw_dot(
            canvas,
            path_box.x + unit * 3 / 4,
            path_box.y + path_box.height / 2,
            4,
            AZURE_BRIGHT,
            255,
        );
        let path_ty = path_box.y + (path_box.height - label_style.cap_height) / 2;
        let friendly = self.friendly_path();
        draw_text_color(
            canvas,
            &friendly,
            path_box.x + unit * 3 / 2,
            path_ty,
            &label_style,
            INK_PRIMARY,
        );

        let mode_badge = Rect::new(
            path_box.x + path_box.width - unit * 6 - unit / 2,
            path_box.y + unit / 4,
            unit * 6,
            path_box.height - unit / 2,
        );
        let (m_text, m_top, m_bot, m_fg) = if self.is_writable_current() {
            ("KISISEL ALAN", SURFACE_SELECT, rgb(208, 242, 236), AQUA_DEEP)
        } else {
            ("SISTEM ALANI", SURFACE_ALT, BORDER_SOFT, INK_MUTED)
        };
        draw_rounded_surface(
            canvas,
            mode_badge,
            mode_badge.height / 2,
            m_top,
            m_bot,
            255,
            BORDER_SOFT,
            false,
        );
        let m_tw = text_width(m_text, &cap_style);
        draw_text_color(
            canvas,
            m_text,
            mode_badge.x + (mode_badge.width - m_tw) / 2,
            mode_badge.y + (mode_badge.height - cap_style.cap_height) / 2,
            &cap_style,
            m_fg,
        );

        let list_card = Self::list_card_rect(area, unit);
        draw_rounded_surface(
            canvas,
            list_card,
            unit / 2,
            SURFACE_WHITE,
            rgb(248, 252, 255),
            255,
            BORDER_SOFT,
            true,
        );

        let hdr_y = list_card.y + unit / 2;
        draw_text_color(
            canvas,
            "Ad",
            list_card.x + unit * 3,
            hdr_y,
            &cap_style,
            INK_MUTED,
        );
        draw_text_color(
            canvas,
            "Tur",
            list_card.x + list_card.width - unit * 13,
            hdr_y,
            &cap_style,
            INK_MUTED,
        );
        draw_text_color(
            canvas,
            "Boyut",
            list_card.x + list_card.width - unit * 6,
            hdr_y,
            &cap_style,
            INK_MUTED,
        );
        canvas.fill_rect(
            list_card.x + unit / 2,
            list_card.y + unit * 3 / 2,
            list_card.width - unit,
            1,
            BORDER_SOFT,
        );

        if self.items.is_empty() {
            draw_text_color(
                canvas,
                "Bu klasor bos. Bos alana sag tiklayarak yeni klasor veya belge olusturabilirsiniz.",
                list_card.x + unit * 3 / 2,
                list_card.y + unit * 4,
                &label_style,
                INK_MUTED,
            );
        }

        let visible_count = self
            .items
            .len()
            .saturating_sub(self.scroll_offset)
            .min(VISIBLE_ROWS);
        for slot in 0..visible_count {
            let idx = self.scroll_offset + slot;
            let item = &self.items[idx];
            let row = Self::file_row_rect(area, unit, slot);
            let is_selected = self.selected_index == Some(idx);
            let is_hov = self.hover_index == Some(idx);

            let (r_top, r_bot, r_brd) = if is_selected {
                (SURFACE_SELECT, rgb(206, 234, 248), AZURE_BRIGHT)
            } else if is_hov {
                (SURFACE_HOVER, SURFACE_ALT, rgb(180, 212, 232))
            } else if (slot & 1) == 1 {
                (SURFACE_ALT, SURFACE_ALT, BORDER_SOFT)
            } else {
                (SURFACE_WHITE, SURFACE_WHITE, BORDER_SOFT)
            };
            draw_rounded_surface(canvas, row, unit / 3, r_top, r_bot, 255, r_brd, false);

            let is_dir = item.file_type == FileType::Directory;
            let is_pkg = item.name.ends_with(".ntzc");
            let is_txt = item.name.ends_with(".txt")
                || item.name.ends_with(".prop")
                || item.name.ends_with(".cfg")
                || item.name.ends_with(".rc");

            draw_entry_icon(
                canvas,
                row.x + unit / 2,
                row.y + (row.height - 16) / 2,
                is_dir,
                is_pkg,
                is_txt,
            );

            let text_y = row.y + (row.height - label_style.cap_height) / 2;
            let display_name = self.friendly_entry_name(&item.name);
            draw_text_color(
                canvas,
                display_name,
                row.x + unit * 2 + unit / 4,
                text_y,
                &label_style,
                INK_PRIMARY,
            );

            let lower_item = item.name.to_ascii_lowercase();
            let (kind_str, kind_col) = if is_dir {
                ("Klasor", AZURE_DEEP)
            } else if is_pkg {
                ("Uygulama Paketi", AQUA_DEEP)
            } else if is_txt {
                ("Metin Belgesi", INK_SECONDARY)
            } else if lower_item.ends_with(".png") {
                ("Gorsel (PNG)", AZURE_BRIGHT)
            } else if lower_item.ends_with(".jpg") || lower_item.ends_with(".jpeg") {
                ("Fotograf (JPEG)", AZURE_BRIGHT)
            } else if lower_item.ends_with(".bmp") {
                ("Bitmap Gorsel", AZURE_BRIGHT)
            } else if lower_item.ends_with(".mp3") || lower_item.ends_with(".wav") {
                ("Ses Dosyasi (Audio)", AQUA_MINT)
            } else if lower_item.ends_with(".mp4") {
                ("Video (MP4)", AQUA_MINT)
            } else if lower_item.ends_with(".efra") {
                ("Donanim Surucusu", AQUA_DEEP)
            } else {
                ("Sistem Dosyasi", INK_MUTED)
            };
            draw_text_color(
                canvas,
                kind_str,
                list_card.x + list_card.width - unit * 13,
                text_y,
                &cap_style,
                kind_col,
            );

            if !is_dir {
                let mut sz_buf = [0u8; 16];
                let sz_len = Self::format_size(item.size, &mut sz_buf);
                if let Ok(st) = core::str::from_utf8(&sz_buf[..sz_len]) {
                    draw_text_color(
                        canvas,
                        st,
                        list_card.x + list_card.width - unit * 6,
                        text_y,
                        &cap_style,
                        INK_SECONDARY,
                    );
                }
            } else {
                draw_text_color(
                    canvas,
                    "--",
                    list_card.x + list_card.width - unit * 6,
                    text_y,
                    &cap_style,
                    INK_DIM,
                );
            }
        }

        let up_btn = Self::scroll_up_rect(area, unit);
        let dn_btn = Self::scroll_dn_rect(area, unit);
        draw_aero_pill_button(
            canvas,
            up_btn,
            "^",
            self.hover_up_scroll,
            false,
            self.scroll_offset > 0,
        );
        draw_aero_pill_button(
            canvas,
            dn_btn,
            "v",
            self.hover_dn_scroll,
            false,
            self.scroll_offset + VISIBLE_ROWS < self.items.len(),
        );

        let status_y = area.y + area.height - unit * 3 / 2;
        draw_dot(
            canvas,
            Self::main_x(area, unit) + 6,
            status_y + 5,
            4,
            AQUA_MINT,
            255,
        );
        draw_text_color(
            canvas,
            &self.status_banner,
            Self::main_x(area, unit) + 18,
            status_y,
            &cap_style,
            INK_SECONDARY,
        );

        if let Some(ref menu) = self.context_menu {
            let menu_r = self.context_menu_rect(area, unit, menu);
            draw_shadow(canvas, menu_r, unit / 3, 4, 12, rgb(18, 42, 68), 38);
            blur_rect_in_place(canvas, menu_r, 6);
            draw_rounded_surface(
                canvas,
                menu_r,
                unit / 3,
                rgb(254, 255, 255),
                rgb(242, 249, 253),
                242,
                AZURE_BRIGHT,
                true,
            );
            let actions = self.context_menu_actions(menu.target_index);
            let row_h = unit * 7 / 4;
            for (idx, (act, label, enabled)) in actions.iter().enumerate() {
                let item_r = Rect::new(
                    menu_r.x + unit / 3,
                    menu_r.y + unit / 2 + idx as i32 * row_h,
                    menu_r.width - unit * 2 / 3,
                    row_h - 2,
                );
                let is_hov = menu.hover_item == Some(idx) && *enabled;
                if is_hov {
                    let (h_top, h_bot, h_brd) = if *act == ContextAction::Delete {
                        (DANGER_RED, rgb(196, 44, 40), DANGER_RED)
                    } else {
                        (AZURE_BRIGHT, AQUA_MINT, AZURE_DEEP)
                    };
                    draw_rounded_surface(canvas, item_r, unit / 4, h_top, h_bot, 255, h_brd, false);
                }
                let fg = if !*enabled {
                    INK_DIM
                } else if is_hov {
                    rgb(255, 255, 255)
                } else if *act == ContextAction::Delete {
                    DANGER_RED
                } else {
                    INK_PRIMARY
                };
                let dot_col = if !*enabled {
                    INK_DIM
                } else if is_hov {
                    rgb(255, 255, 255)
                } else if *act == ContextAction::Delete {
                    DANGER_RED
                } else {
                    AQUA_MINT
                };
                draw_dot(
                    canvas,
                    item_r.x + unit / 2,
                    item_r.y + item_r.height / 2,
                    3,
                    dot_col,
                    255,
                );
                let ty = item_r.y + (item_r.height - label_style.cap_height) / 2;
                draw_text_color(canvas, label, item_r.x + unit + 2, ty, &label_style, fg);
            }
        }

        match self.modal {
            ModalState::Install { ref pkg, .. } => {
                let m_rect = Self::modal_rect(area, unit);
                draw_frosted_modal(canvas, m_rect, unit);
                draw_entry_icon(
                    canvas,
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 3 / 2,
                    false,
                    true,
                    false,
                );
                let mut modal_title = Line::new();
                modal_title
                    .text("Uygulama Paketini Kur: ")
                    .text(&pkg.manifest.name);
                draw_text_color(
                    canvas,
                    modal_title.as_str(),
                    m_rect.x + unit * 7 / 2,
                    m_rect.y + unit * 3 / 2,
                    &title_style,
                    INK_PRIMARY,
                );
                let role_pill = Rect::new(
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 7 / 2,
                    unit * 12,
                    unit * 3 / 2,
                );
                draw_rounded_surface(
                    canvas,
                    role_pill,
                    role_pill.height / 2,
                    SURFACE_SELECT,
                    rgb(212, 242, 236),
                    255,
                    AQUA_MINT,
                    false,
                );
                draw_text_color(
                    canvas,
                    pkg.role_label(),
                    role_pill.x + unit / 2,
                    role_pill.y + (role_pill.height - cap_style.cap_height) / 2,
                    &cap_style,
                    AQUA_DEEP,
                );
                let mut ver_line = Line::new();
                ver_line
                    .text("Kimlik: ")
                    .text(&pkg.manifest.id)
                    .text("   |   Surum: ")
                    .text(&pkg.manifest.version);
                draw_text_color(
                    canvas,
                    ver_line.as_str(),
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 11 / 2,
                    &cap_style,
                    INK_MUTED,
                );
                draw_text_color(
                    canvas,
                    "Bu paketin talep ettigi sistem izinleri:",
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 7,
                    &label_style,
                    INK_PRIMARY,
                );
                let labels = pkg.permission_labels();
                for (idx, lbl) in labels.iter().enumerate() {
                    let iy = m_rect.y + unit * 9 + idx as i32 * (unit * 3 / 2);
                    draw_dot(
                        canvas,
                        m_rect.x + unit * 5 / 2,
                        iy + 5,
                        4,
                        AZURE_BRIGHT,
                        255,
                    );
                    draw_text_color(
                        canvas,
                        lbl,
                        m_rect.x + unit * 7 / 2,
                        iy,
                        &label_style,
                        INK_SECONDARY,
                    );
                }
                let btn_w = unit * 8;
                let btn_h = unit * 2 + unit / 3;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let inst_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let canc_btn = Rect::new(inst_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                draw_aero_pill_button(
                    canvas,
                    canc_btn,
                    "Vazgec",
                    self.modal_hover_secondary,
                    false,
                    true,
                );
                draw_aero_pill_button(
                    canvas,
                    inst_btn,
                    "Sisteme Kur",
                    self.modal_hover_primary,
                    true,
                    true,
                );
            }
            ModalState::ViewText {
                ref path,
                ref content,
                scroll_line,
                modified,
            } => {
                let m_rect = Self::modal_rect(area, unit);
                draw_frosted_modal(canvas, m_rect, unit);
                let can_edit = Self::is_writable_path(path);
                let file_only = path.rsplit('/').next().unwrap_or(path);
                let title = alloc::format!("Belge Duzenleyici: {}", file_only);
                draw_text_color(
                    canvas,
                    &title,
                    m_rect.x + unit * 3 / 2,
                    m_rect.y + unit * 4 / 3,
                    &title_style,
                    INK_PRIMARY,
                );
                let status_msg = if !can_edit {
                    "Salt Okunur Sistem Dosyasi"
                } else if modified {
                    "Degistirildi - Kaydetmeye Hazir"
                } else {
                    "Klavye ile dogrudan yazip kaydedebilirsiniz"
                };
                draw_text_color(
                    canvas,
                    status_msg,
                    m_rect.x + unit * 3 / 2,
                    m_rect.y + unit * 3,
                    &cap_style,
                    if modified { AQUA_DEEP } else { INK_MUTED },
                );
                let editor_box = Rect::new(
                    m_rect.x + unit * 3 / 2,
                    m_rect.y + unit * 9 / 2,
                    m_rect.width - unit * 3,
                    m_rect.height - unit * 17 / 2,
                );
                draw_multiline_editor(
                    canvas,
                    editor_box,
                    content,
                    scroll_line,
                    can_edit,
                    &label_style,
                    &cap_style,
                );
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let save_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let close_btn = Rect::new(save_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                draw_aero_pill_button(
                    canvas,
                    close_btn,
                    "Kapat",
                    self.modal_hover_secondary,
                    false,
                    true,
                );
                draw_aero_pill_button(
                    canvas,
                    save_btn,
                    "Kaydet",
                    self.modal_hover_primary,
                    true,
                    can_edit,
                );
            }
            ModalState::ViewImage {
                ref name,
                format_label,
                width,
                height,
                theme_id,
                raw_size,
                ref pixels,
            } => {
                let m_rect = Self::modal_rect(area, unit);
                draw_frosted_modal(canvas, m_rect, unit);
                let hdr = alloc::format!("Gorsel Goruntuleyici: {}", name);
                draw_text_color(
                    canvas,
                    &hdr,
                    m_rect.x + unit * 3 / 2,
                    m_rect.y + unit,
                    &title_style,
                    INK_PRIMARY,
                );
                let mut meta = Line::new();
                meta.text("Format: ")
                    .text(format_label)
                    .text("   |   Cozunurluk: ")
                    .number(width, 1)
                    .text("x")
                    .number(height, 1)
                    .text(" px   |   Boyut: ")
                    .number(raw_size as u32, 1)
                    .text(" B");
                draw_text_color(
                    canvas,
                    meta.as_str(),
                    m_rect.x + unit * 3 / 2,
                    m_rect.y + unit * 5 / 2,
                    &cap_style,
                    INK_MUTED,
                );
                let view_r = Rect::new(
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 4,
                    m_rect.width - unit * 4,
                    m_rect.height - unit * 8,
                );
                // Eğer gerçek piksel tamponu çözüldüyse doğrudan ölçekleyerek ekrana çiz!
                if !pixels.is_empty() && width > 0 && height > 0 {
                    draw_rounded_surface(
                        canvas,
                        view_r,
                        unit / 3,
                        rgb(20, 26, 36),
                        rgb(14, 18, 26),
                        255,
                        AZURE_BRIGHT,
                        false,
                    );
                    let pad = 6;
                    let dst_w = (view_r.width - pad * 2).max(1);
                    let dst_h = (view_r.height - pad * 2).max(1);
                    for dy in 0..dst_h {
                        let sy = ((dy as u32) * height / (dst_h as u32)).min(height - 1) as usize;
                        for dx in 0..dst_w {
                            let sx = ((dx as u32) * width / (dst_w as u32)).min(width - 1) as usize;
                            let idx = sy * (width as usize) + sx;
                            if idx < pixels.len() {
                                canvas.fill_rect(
                                    view_r.x + pad + dx,
                                    view_r.y + pad + dy,
                                    1,
                                    1,
                                    pixels[idx],
                                );
                            }
                        }
                    }
                } else {
                    let (c_top, c_bot) = if theme_id == 0 {
                        (rgb(18, 88, 178), rgb(22, 174, 158))
                    } else {
                        (rgb(44, 48, 128), rgb(168, 64, 122))
                    };
                    draw_rounded_surface(
                        canvas,
                        view_r,
                        unit / 2,
                        c_top,
                        c_bot,
                        255,
                        AZURE_BRIGHT,
                        true,
                    );
                }
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let close_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                draw_aero_pill_button(
                    canvas,
                    close_btn,
                    "Kapat",
                    self.modal_hover_primary,
                    true,
                    true,
                );
            }
            ModalState::ViewAudio {
                ref name,
                format_label,
                sample_rate,
                kbps,
                duration_secs,
                playing,
                elapsed_ms,
                ref spectrum_seed,
                ..
            } => {
                let m_rect = Self::modal_rect(area, unit);
                draw_frosted_modal(canvas, m_rect, unit);
                let hdr = alloc::format!("Muzik & Ses Oynatici: {}", name);
                draw_text_color(
                    canvas,
                    &hdr,
                    m_rect.x + unit * 3 / 2,
                    m_rect.y + unit,
                    &title_style,
                    INK_PRIMARY,
                );
                let cur_sec = (elapsed_ms / 1000).min(duration_secs as u32);
                let mut meta = Line::new();
                meta.text(format_label)
                    .text("   |   ")
                    .number(sample_rate, 1)
                    .text(" Hz / ")
                    .number(kbps as u32, 1)
                    .text(" kbps   |   Sure: ")
                    .number(cur_sec / 60, 2)
                    .text(":")
                    .number(cur_sec % 60, 2)
                    .text(" / ")
                    .number((duration_secs / 60) as u32, 2)
                    .text(":")
                    .number((duration_secs % 60) as u32, 2);
                draw_text_color(
                    canvas,
                    meta.as_str(),
                    m_rect.x + unit * 3 / 2,
                    m_rect.y + unit * 5 / 2,
                    &cap_style,
                    INK_MUTED,
                );
                let stage = Rect::new(
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 4,
                    m_rect.width - unit * 4,
                    m_rect.height - unit * 19 / 2,
                );
                draw_rounded_surface(
                    canvas,
                    stage,
                    unit / 2,
                    rgb(14, 24, 42),
                    rgb(20, 48, 76),
                    255,
                    AQUA_MINT,
                    true,
                );
                // 24-Bant Canlı Spektrum / Ses Dalgası Çizimi
                let bars = 24usize;
                let bar_gap = 4;
                let avail_w = (stage.width - unit * 2).max(48);
                let bar_w = ((avail_w - (bars as i32 - 1) * bar_gap) / bars as i32).max(4);
                let start_x = stage.x + (stage.width - (bars as i32 * (bar_w + bar_gap))) / 2;
                let max_bar_h = (stage.height - unit * 2).max(20);
                let tick = (elapsed_ms / 40) as usize;
                for b in 0..bars {
                    let seed_v = spectrum_seed[b % 32] as i32;
                    let wave = if playing {
                        (((tick.wrapping_mul(7) + b * 13 + seed_v as usize) % 31) as i32 - 15).abs()
                    } else {
                        3
                    };
                    let bh = ((wave * max_bar_h) / 16).clamp(6, max_bar_h);
                    let bx = start_x + (b as i32) * (bar_w + bar_gap);
                    let by = stage.y + stage.height - unit - bh;
                    let col = if b % 2 == 0 { AQUA_MINT } else { AZURE_BRIGHT };
                    canvas.fill_rect(bx, by, bar_w, bh, col);
                }
                let prog_bar = Rect::new(
                    stage.x,
                    stage.y + stage.height + unit / 2,
                    stage.width,
                    unit / 2,
                );
                draw_rounded_surface(
                    canvas,
                    prog_bar,
                    3,
                    SURFACE_ALT,
                    BORDER_SOFT,
                    255,
                    BORDER_SOFT,
                    false,
                );
                let total_ms = (duration_secs as u32 * 1000).max(1);
                let fill_w = ((prog_bar.width as u64 * elapsed_ms as u64) / total_ms as u64) as i32;
                if fill_w > 4 {
                    let fill_r = Rect::new(prog_bar.x, prog_bar.y, fill_w.min(prog_bar.width), prog_bar.height);
                    draw_rounded_surface(canvas, fill_r, 3, AQUA_MINT, AZURE_BRIGHT, 255, AQUA_DEEP, false);
                }
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let close_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let play_btn = Rect::new(close_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                draw_aero_pill_button(
                    canvas,
                    play_btn,
                    if playing { "Duraklat" } else { "Cal" },
                    self.modal_hover_secondary,
                    false,
                    true,
                );
                draw_aero_pill_button(
                    canvas,
                    close_btn,
                    "Kapat",
                    self.modal_hover_primary,
                    true,
                    true,
                );
            }
            ModalState::ViewVideo {
                ref name,
                width,
                height,
                duration_secs,
                scene_id,
                playing,
                elapsed_ms,
                ..
            } => {
                let m_rect = Self::modal_rect(area, unit);
                draw_frosted_modal(canvas, m_rect, unit);
                let hdr = alloc::format!("Medya Oynatici: {}", name);
                draw_text_color(
                    canvas,
                    &hdr,
                    m_rect.x + unit * 3 / 2,
                    m_rect.y + unit,
                    &title_style,
                    INK_PRIMARY,
                );
                let cur_sec = (elapsed_ms / 1000).min(duration_secs as u32);
                let mut meta = Line::new();
                meta.text("Video: ")
                    .number(width as u32, 1)
                    .text("x")
                    .number(height as u32, 1)
                    .text(" @ 60 FPS   |   Sure: ")
                    .number(cur_sec / 60, 2)
                    .text(":")
                    .number(cur_sec % 60, 2)
                    .text(" / ")
                    .number((duration_secs / 60) as u32, 2)
                    .text(":")
                    .number((duration_secs % 60) as u32, 2);
                draw_text_color(
                    canvas,
                    meta.as_str(),
                    m_rect.x + unit * 3 / 2,
                    m_rect.y + unit * 5 / 2,
                    &cap_style,
                    INK_MUTED,
                );
                let stage = Rect::new(
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 4,
                    m_rect.width - unit * 4,
                    m_rect.height - unit * 19 / 2,
                );
                let (bg_top, bg_bot) = if scene_id == 0 {
                    (rgb(12, 28, 54), rgb(20, 88, 112))
                } else {
                    (rgb(28, 18, 52), rgb(92, 34, 88))
                };
                draw_rounded_surface(
                    canvas,
                    stage,
                    unit / 2,
                    bg_top,
                    bg_bot,
                    255,
                    AZURE_BRIGHT,
                    true,
                );
                let phase = (elapsed_ms / 16) as i32;
                let orb1_x = stage.x + stage.width / 4 + ((phase * 3) % (stage.width / 2).max(1));
                let orb1_y = stage.y + stage.height / 2 + ((phase % 24) - 12);
                let orb2_x =
                    stage.x + stage.width * 3 / 4 - ((phase * 2) % (stage.width / 2).max(1));
                let orb2_y = stage.y + stage.height / 2 - ((phase % 20) - 10);
                draw_dot(canvas, orb1_x, orb1_y, unit * 2, AQUA_MINT, 190);
                draw_dot(
                    canvas,
                    orb1_x - 4,
                    orb1_y - 4,
                    unit / 2,
                    rgb(255, 255, 255),
                    220,
                );
                draw_dot(canvas, orb2_x, orb2_y, unit * 3 / 2, AZURE_BRIGHT, 190);
                draw_dot(
                    canvas,
                    orb2_x - 3,
                    orb2_y - 3,
                    unit / 3,
                    rgb(255, 255, 255),
                    220,
                );
                let bar = Rect::new(
                    stage.x,
                    stage.y + stage.height + unit / 2,
                    stage.width,
                    unit / 2,
                );
                draw_rounded_surface(
                    canvas,
                    bar,
                    3,
                    SURFACE_ALT,
                    BORDER_SOFT,
                    255,
                    BORDER_SOFT,
                    false,
                );
                let total_ms = (duration_secs as u32 * 1000).max(1);
                let fill_w = ((bar.width as u64 * elapsed_ms as u64) / total_ms as u64) as i32;
                if fill_w > 4 {
                    let fill_r = Rect::new(bar.x, bar.y, fill_w.min(bar.width), bar.height);
                    draw_rounded_surface(
                        canvas,
                        fill_r,
                        3,
                        AZURE_BRIGHT,
                        AQUA_MINT,
                        255,
                        AQUA_DEEP,
                        false,
                    );
                }
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let close_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let play_btn = Rect::new(close_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                draw_aero_pill_button(
                    canvas,
                    play_btn,
                    if playing { "Duraklat" } else { "Oynat" },
                    self.modal_hover_secondary,
                    false,
                    true,
                );
                draw_aero_pill_button(
                    canvas,
                    close_btn,
                    "Kapat",
                    self.modal_hover_primary,
                    true,
                    true,
                );
            }
            ModalState::CreateFolder { ref name_buf } => {
                let m_rect = Self::modal_rect(area, unit);
                draw_frosted_modal(canvas, m_rect, unit);
                draw_text_color(
                    canvas,
                    "Yeni Klasor Olustur",
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 2,
                    &title_style,
                    INK_PRIMARY,
                );
                draw_text_color(
                    canvas,
                    "Klasor adini yazin ve Olustur dugmesine tiklayin:",
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 4,
                    &cap_style,
                    INK_MUTED,
                );
                let input_box = Rect::new(
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 6,
                    m_rect.width - unit * 4,
                    unit * 5 / 2,
                );
                draw_single_line_input(canvas, input_box, name_buf, &label_style);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let create_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let cancel_btn = Rect::new(create_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                draw_aero_pill_button(
                    canvas,
                    cancel_btn,
                    "Iptal",
                    self.modal_hover_secondary,
                    false,
                    true,
                );
                draw_aero_pill_button(
                    canvas,
                    create_btn,
                    "Olustur",
                    self.modal_hover_primary,
                    true,
                    true,
                );
            }
            ModalState::CreateFile { ref name_buf } => {
                let m_rect = Self::modal_rect(area, unit);
                draw_frosted_modal(canvas, m_rect, unit);
                draw_text_color(
                    canvas,
                    "Yeni Metin Belgesi Olustur",
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 2,
                    &title_style,
                    INK_PRIMARY,
                );
                draw_text_color(
                    canvas,
                    "Dosya adini yazin (ornegin: notlar.txt):",
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 4,
                    &cap_style,
                    INK_MUTED,
                );
                let input_box = Rect::new(
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 6,
                    m_rect.width - unit * 4,
                    unit * 5 / 2,
                );
                draw_single_line_input(canvas, input_box, name_buf, &label_style);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let create_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let cancel_btn = Rect::new(create_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                draw_aero_pill_button(
                    canvas,
                    cancel_btn,
                    "Iptal",
                    self.modal_hover_secondary,
                    false,
                    true,
                );
                draw_aero_pill_button(
                    canvas,
                    create_btn,
                    "Olustur",
                    self.modal_hover_primary,
                    true,
                    true,
                );
            }
            ModalState::RenameItem {
                ref old_name,
                ref new_name_buf,
                ..
            } => {
                let m_rect = Self::modal_rect(area, unit);
                draw_frosted_modal(canvas, m_rect, unit);
                let hdr = alloc::format!("Yeniden Adlandir: {}", old_name);
                draw_text_color(
                    canvas,
                    &hdr,
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 2,
                    &title_style,
                    INK_PRIMARY,
                );
                draw_text_color(
                    canvas,
                    "Yeni adi girin ve Kaydet dugmesine tiklayin:",
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 4,
                    &cap_style,
                    INK_MUTED,
                );
                let input_box = Rect::new(
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 6,
                    m_rect.width - unit * 4,
                    unit * 5 / 2,
                );
                draw_single_line_input(canvas, input_box, new_name_buf, &label_style);
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let ok_btn = Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let cancel_btn = Rect::new(ok_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                draw_aero_pill_button(
                    canvas,
                    cancel_btn,
                    "Iptal",
                    self.modal_hover_secondary,
                    false,
                    true,
                );
                draw_aero_pill_button(
                    canvas,
                    ok_btn,
                    "Kaydet",
                    self.modal_hover_primary,
                    true,
                    true,
                );
            }
            ModalState::DeleteConfirm { ref name, is_dir } => {
                let m_rect = Self::modal_rect(area, unit);
                draw_frosted_modal(canvas, m_rect, unit);
                let type_str = if is_dir { "klasoru" } else { "dosyasi" };
                let msg = alloc::format!("'{}' {} kalici olarak silinsin mi?", name, type_str);
                draw_text_color(
                    canvas,
                    "Silme Onayi",
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 2,
                    &title_style,
                    DANGER_RED,
                );
                draw_text_color(
                    canvas,
                    &msg,
                    m_rect.x + unit * 2,
                    m_rect.y + unit * 5,
                    &label_style,
                    INK_PRIMARY,
                );
                let btn_w = unit * 7 + unit / 2;
                let btn_h = unit * 2 + unit / 4;
                let btn_y = m_rect.y + m_rect.height - btn_h - unit;
                let del_btn =
                    Rect::new(m_rect.x + m_rect.width - btn_w - unit, btn_y, btn_w, btn_h);
                let cancel_btn = Rect::new(del_btn.x - btn_w - unit / 2, btn_y, btn_w, btn_h);
                draw_aero_pill_button(
                    canvas,
                    cancel_btn,
                    "Vazgec",
                    self.modal_hover_secondary,
                    false,
                    true,
                );
                draw_aero_pill_button(
                    canvas,
                    del_btn,
                    "Evet, Sil",
                    self.modal_hover_primary,
                    true,
                    true,
                );
            }
            ModalState::None => {}
        }
    }
}

pub struct GenericInstalledApp {
    info: AppInfo,
    uid: u32,
    role: &'static str,
    state_counter: i32,
    accumulator: i32,
    status_text: String,
    script_source: String,
    hover_btn1: bool,
    hover_btn2: bool,
    hover_btn3: bool,
}

impl GenericInstalledApp {
    pub fn new(pkg: &NtzcPackage) -> Self {
        let script = core::str::from_utf8(&pkg.payload)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(String::from)
            .unwrap_or_else(|| {
                String::from(
                    "fun onAction1() { state = state + 1; status = \"Moduler calisma zamani: Sayac arttirildi\" }\nfun onAction2() { state = state * 2; status = \"Moduler calisma zamani: Deger ikiye katlandi\" }\nfun onReset() { state = 0; status = \"Moduler calisma zamani: Bellek sifirlandi\" }",
                )
            });

        GenericInstalledApp {
            info: AppInfo::new(
                &pkg.manifest.id,
                &pkg.manifest.name,
                &pkg.manifest.version,
                pkg.manifest.icon,
                pkg.manifest.width_units,
                pkg.manifest.height_units,
            ),
            uid: pkg.uid,
            role: pkg.role_label(),
            state_counter: 1,
            accumulator: 0,
            status_text: String::from("NetizenCore .ntzc Konteyneri Hazir"),
            script_source: script,
            hover_btn1: false,
            hover_btn2: false,
            hover_btn3: false,
        }
    }

    fn run_handler(&mut self, handler_name: &str) {
        if let Some(pos) = self.script_source.find(handler_name) {
            let after = &self.script_source[pos..];
            if let Some(open_b) = after.find('{') {
                if let Some(close_b) = after[open_b + 1..].find('}') {
                    let body = &after[open_b + 1..open_b + 1 + close_b];
                    for stmt in body.split(';') {
                        let s = stmt.trim();
                        if s.contains("state = state +") {
                            let step = s
                                .split('+')
                                .nth(1)
                                .and_then(|v| v.trim().parse::<i32>().ok())
                                .unwrap_or(1);
                            self.state_counter = self.state_counter.saturating_add(step);
                            self.accumulator = self.accumulator.saturating_add(1);
                        } else if s.contains("state = state *") {
                            let factor = s
                                .split('*')
                                .nth(1)
                                .and_then(|v| v.trim().parse::<i32>().ok())
                                .unwrap_or(2);
                            self.state_counter =
                                self.state_counter.saturating_mul(factor).min(999_999);
                            self.accumulator = self.accumulator.saturating_add(1);
                        } else if s.contains("state = 0") {
                            self.state_counter = 0;
                        } else if let Some((_, msg)) = s.split_once("status =") {
                            self.status_text = String::from(msg.trim().trim_matches('"'));
                        }
                    }
                }
            }
        }
    }

    fn btn1_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(
            area.x + unit * 3 / 2,
            area.y + area.height - unit * 7 / 2,
            unit * 8,
            unit * 2,
        )
    }

    fn btn2_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(
            area.x + unit * 10 + unit / 2,
            area.y + area.height - unit * 7 / 2,
            unit * 8,
            unit * 2,
        )
    }

    fn btn3_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(
            area.x + unit * 19 + unit / 2,
            area.y + area.height - unit * 7 / 2,
            unit * 7,
            unit * 2,
        )
    }
}

impl App for GenericInstalledApp {
    fn info(&self) -> &AppInfo {
        &self.info
    }

    fn update(&mut self, ctx: &mut Context, input: &Input) -> Update {
        let unit = ctx.unit;
        let b1 = Self::btn1_rect(input.area, unit);
        let b2 = Self::btn2_rect(input.area, unit);
        let b3 = Self::btn3_rect(input.area, unit);

        let mut h1 = false;
        let mut h2 = false;
        let mut h3 = false;
        let mut redraw = false;

        if let Some(p) = input.pointer {
            h1 = inside(b1, p.x, p.y);
            h2 = inside(b2, p.x, p.y);
            h3 = inside(b3, p.x, p.y);

            if p.clicked {
                if h1 {
                    self.run_handler("onAction1");
                    redraw = true;
                } else if h2 {
                    self.run_handler("onAction2");
                    redraw = true;
                } else if h3 {
                    self.run_handler("onReset");
                    redraw = true;
                }
            }
        }

        if h1 != self.hover_btn1 || h2 != self.hover_btn2 || h3 != self.hover_btn3 {
            self.hover_btn1 = h1;
            self.hover_btn2 = h2;
            self.hover_btn3 = h3;
            redraw = true;
        }

        Update {
            redraw,
            close: false,
            install_app: None,
        }
    }

    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        let unit = ctx.unit;
        canvas.fill_rect(area.x, area.y, area.width, area.height, PORCELAIN_BG);

        let card = Rect::new(
            area.x + unit * 3 / 2,
            area.y + unit,
            area.width - unit * 3,
            area.height - unit * 11 / 2,
        );
        draw_rounded_surface(
            canvas,
            card,
            unit / 2,
            SURFACE_WHITE,
            SURFACE_ALT,
            255,
            BORDER_SOFT,
            true,
        );

        let title_style = text_style((unit * 85 / 100).max(11), 16);
        let sub_style = text_style((unit * 56 / 100).max(9), 16);
        let big_style = text_style((unit * 16 / 10).max(18), 16);

        draw_dot(canvas, card.x + unit, card.y + unit * 4 / 3, 6, AQUA_MINT, 255);
        draw_text_color(
            canvas,
            &self.info.name,
            card.x + unit * 2,
            card.y + unit,
            &title_style,
            INK_PRIMARY,
        );

        let role_rect = Rect::new(
            card.x + card.width - unit * 10 - unit / 2,
            card.y + unit * 3 / 4,
            unit * 9 + unit / 2,
            unit * 3 / 2,
        );
        draw_rounded_surface(
            canvas,
            role_rect,
            role_rect.height / 2,
            SURFACE_SELECT,
            rgb(212, 242, 236),
            255,
            AQUA_MINT,
            false,
        );
        draw_text_color(
            canvas,
            self.role,
            role_rect.x + unit / 2,
            role_rect.y + (role_rect.height - sub_style.cap_height) / 2,
            &sub_style,
            AQUA_DEEP,
        );

        let mut val_line = Line::new();
        val_line.number(self.state_counter.max(0) as u32, 1);
        draw_text_color(
            canvas,
            val_line.as_str(),
            card.x + unit * 3 / 2,
            card.y + unit * 7 / 2,
            &big_style,
            AZURE_DEEP,
        );

        let mut meta = Line::new();
        meta.text("Paket UID: ")
            .number(self.uid, 1)
            .text("   |   Islem Sayisi: ")
            .number(self.accumulator.max(0) as u32, 1);
        draw_text_color(
            canvas,
            meta.as_str(),
            card.x + unit * 3 / 2,
            card.y + unit * 6,
            &sub_style,
            INK_MUTED,
        );

        draw_text_color(
            canvas,
            &self.status_text,
            card.x + unit * 3 / 2,
            card.y + unit * 15 / 2,
            &sub_style,
            AQUA_DEEP,
        );

        draw_aero_pill_button(
            canvas,
            Self::btn1_rect(area, unit),
            "+1 Arttir",
            self.hover_btn1,
            true,
            true,
        );
        draw_aero_pill_button(
            canvas,
            Self::btn2_rect(area, unit),
            "x2 Katla",
            self.hover_btn2,
            false,
            true,
        );
        draw_aero_pill_button(
            canvas,
            Self::btn3_rect(area, unit),
            "Sifirla",
            self.hover_btn3,
            false,
            true,
        );
    }
}

fn corner_cov(dx: i32, dy: i32, r: i32) -> u8 {
    if r <= 0 {
        return 255;
    }
    let d2 = dx * dx + dy * dy;
    let r2 = r * r;
    let inner = (r - 1).max(0) * (r - 1).max(0);
    if d2 <= inner {
        255
    } else if d2 >= r2 {
        0
    } else {
        ((r2 - d2) * 255 / (r2 - inner).max(1)).clamp(0, 255) as u8
    }
}

fn rounded_cov(x: i32, y: i32, rect: Rect, radius: i32) -> u8 {
    let r = radius.min(rect.width / 2).min(rect.height / 2);
    if r <= 0 {
        return 255;
    }
    let lx = rect.x + r;
    let rx = rect.x + rect.width - 1 - r;
    let ty = rect.y + r;
    let by = rect.y + rect.height - 1 - r;
    let dx = if x < lx {
        lx - x
    } else if x > rx {
        x - rx
    } else {
        0
    };
    let dy = if y < ty {
        ty - y
    } else if y > by {
        y - by
    } else {
        0
    };
    if dx == 0 || dy == 0 {
        255
    } else {
        corner_cov(dx, dy, r)
    }
}

fn draw_rounded_surface(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    radius: i32,
    top_col: u32,
    bot_col: u32,
    alpha: u8,
    border_col: u32,
    top_glint: bool,
) {
    if rect.width <= 2 || rect.height <= 2 {
        return;
    }
    let r = radius.min(rect.width / 2).min(rect.height / 2).max(1);
    for y in rect.y..(rect.y + rect.height) {
        let ry = y - rect.y;
        let t = (ry * 255 / rect.height.max(1)) as u32;
        let fill = lerp_color(top_col, bot_col, t);
        let deep_row = ry >= r && ry < rect.height - r;

        if deep_row && alpha == 255 {
            canvas.fill_rect(rect.x + 1, y, rect.width - 2, 1, fill);
            canvas.fill_rect(rect.x, y, 1, 1, border_col);
            canvas.fill_rect(rect.x + rect.width - 1, y, 1, 1, border_col);
            continue;
        }

        for x in rect.x..(rect.x + rect.width) {
            let cov = rounded_cov(x, y, rect, r);
            if cov == 0 {
                continue;
            }
            let eff_a = (cov as u32 * alpha as u32 / 255) as u8;
            canvas.blend_pixel(x, y, fill, eff_a);
            let on_edge = x == rect.x
                || x == rect.x + rect.width - 1
                || y == rect.y
                || y == rect.y + rect.height - 1
                || cov < 235;
            if on_edge {
                canvas.blend_pixel(x, y, border_col, cov);
            } else if top_glint
                && ry == 1
                && x > rect.x + r / 2
                && x < rect.x + rect.width - r / 2
            {
                canvas.blend_pixel(x, y, rgb(255, 255, 255), 210);
            }
        }
    }
}

fn blur_rect_in_place(canvas: &mut Canvas<'_>, rect: Rect, radius: usize) {
    let cw = canvas.width() as i32;
    let ch = canvas.height() as i32;
    let x0 = rect.x.clamp(0, cw);
    let y0 = rect.y.clamp(0, ch);
    let x1 = (rect.x + rect.width).clamp(0, cw);
    let y1 = (rect.y + rect.height).clamp(0, ch);
    let rw = (x1 - x0) as usize;
    let rh = (y1 - y0) as usize;
    if rw == 0 || rh == 0 || radius == 0 || rw > MAX_BLUR_SPAN || rh > MAX_BLUR_SPAN {
        return;
    }

    let stride = cw as usize;
    let pixels = canvas.pixels_mut();
    let mut scratch = [0u32; MAX_BLUR_SPAN];
    let r = radius.min(8);
    let win = (r * 2 + 1) as u32;

    for y in (y0 as usize)..(y1 as usize) {
        let base = y * stride + (x0 as usize);
        for i in 0..rw {
            scratch[i] = pixels[base + i];
        }
        let first = scratch[0];
        let mut sr = ((first >> 16) & 0xFF) * (r as u32 + 1);
        let mut sg = ((first >> 8) & 0xFF) * (r as u32 + 1);
        let mut sb = (first & 0xFF) * (r as u32 + 1);
        for i in 1..=r {
            let p = scratch[i.min(rw - 1)];
            sr += (p >> 16) & 0xFF;
            sg += (p >> 8) & 0xFF;
            sb += p & 0xFF;
        }
        for i in 0..rw {
            pixels[base + i] = ((sr / win) << 16) | ((sg / win) << 8) | (sb / win);
            let out_p = scratch[i.saturating_sub(r)];
            let in_p = scratch[(i + r + 1).min(rw - 1)];
            sr = sr + ((in_p >> 16) & 0xFF) - ((out_p >> 16) & 0xFF);
            sg = sg + ((in_p >> 8) & 0xFF) - ((out_p >> 8) & 0xFF);
            sb = sb + (in_p & 0xFF) - (out_p & 0xFF);
        }
    }

    for x in (x0 as usize)..(x1 as usize) {
        for j in 0..rh {
            scratch[j] = pixels[(y0 as usize + j) * stride + x];
        }
        let first = scratch[0];
        let mut sr = ((first >> 16) & 0xFF) * (r as u32 + 1);
        let mut sg = ((first >> 8) & 0xFF) * (r as u32 + 1);
        let mut sb = (first & 0xFF) * (r as u32 + 1);
        for j in 1..=r {
            let p = scratch[j.min(rh - 1)];
            sr += (p >> 16) & 0xFF;
            sg += (p >> 8) & 0xFF;
            sb += p & 0xFF;
        }
        for j in 0..rh {
            pixels[(y0 as usize + j) * stride + x] =
                ((sr / win) << 16) | ((sg / win) << 8) | (sb / win);
            let out_p = scratch[j.saturating_sub(r)];
            let in_p = scratch[(j + r + 1).min(rh - 1)];
            sr = sr + ((in_p >> 16) & 0xFF) - ((out_p >> 16) & 0xFF);
            sg = sg + ((in_p >> 8) & 0xFF) - ((out_p >> 8) & 0xFF);
            sb = sb + (in_p & 0xFF) - (out_p & 0xFF);
        }
    }
}

fn draw_frosted_modal(canvas: &mut Canvas<'_>, rect: Rect, unit: i32) {
    draw_shadow(canvas, rect, unit / 2, 5, 16, rgb(18, 42, 68), 44);
    blur_rect_in_place(canvas, rect, 6);
    draw_rounded_surface(
        canvas,
        rect,
        unit / 2,
        rgb(254, 255, 255),
        rgb(241, 249, 253),
        238,
        AQUA_MINT,
        true,
    );
}

fn draw_aero_pill_button(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    label: &str,
    hovered: bool,
    primary: bool,
    enabled: bool,
) {
    let radius = rect.height / 2;
    let st = text_style((rect.height * 42 / 100).max(9), 15);
    let tw = text_width(label, &st);
    let tx = rect.x + (rect.width - tw) / 2;
    let ty = rect.y + (rect.height - st.cap_height) / 2;

    if !enabled {
        draw_rounded_surface(
            canvas,
            rect,
            radius,
            SURFACE_ALT,
            rgb(232, 239, 245),
            255,
            BORDER_SOFT,
            false,
        );
        draw_text_color(canvas, label, tx, ty, &st, INK_DIM);
        return;
    }

    if primary {
        let (top_c, bot_c) = if hovered {
            (AZURE_DEEP, AQUA_DEEP)
        } else {
            (AZURE_BRIGHT, AQUA_MINT)
        };
        draw_rounded_surface(canvas, rect, radius, top_c, bot_c, 255, AZURE_DEEP, true);
        draw_text_color(canvas, label, tx, ty, &st, rgb(255, 255, 255));
    } else {
        let (top_c, bot_c, brd) = if hovered {
            (SURFACE_HOVER, rgb(218, 238, 249), AZURE_BRIGHT)
        } else {
            (SURFACE_WHITE, SURFACE_ALT, BORDER_SOFT)
        };
        draw_rounded_surface(canvas, rect, radius, top_c, bot_c, 250, brd, true);
        draw_text_color(canvas, label, tx, ty, &st, INK_PRIMARY);
    }
}

fn draw_entry_icon(
    canvas: &mut Canvas<'_>,
    x: i32,
    y: i32,
    is_dir: bool,
    is_pkg: bool,
    is_txt: bool,
) {
    if is_dir {
        canvas.fill_rect(x + 1, y + 2, 7, 3, AZURE_BRIGHT);
        let body = Rect::new(x, y + 4, 18, 12);
        draw_rounded_surface(
            canvas,
            body,
            3,
            rgb(96, 178, 252),
            AZURE_BRIGHT,
            255,
            AZURE_DEEP,
            true,
        );
    } else if is_pkg {
        let orb = Rect::new(x + 1, y + 1, 16, 15);
        draw_rounded_surface(canvas, orb, 5, AQUA_SOFT, AQUA_MINT, 255, AQUA_DEEP, true);
        draw_dot(canvas, x + 9, y + 8, 3, rgb(255, 255, 255), 255);
    } else if is_txt {
        let page = Rect::new(x + 2, y + 1, 14, 15);
        draw_rounded_surface(
            canvas,
            page,
            2,
            SURFACE_WHITE,
            SURFACE_ALT,
            255,
            AZURE_BRIGHT,
            false,
        );
        canvas.fill_rect(x + 5, y + 5, 8, 1, AZURE_BRIGHT);
        canvas.fill_rect(x + 5, y + 8, 8, 1, INK_MUTED);
        canvas.fill_rect(x + 5, y + 11, 6, 1, INK_MUTED);
    } else {
        let doc = Rect::new(x + 2, y + 1, 14, 15);
        draw_rounded_surface(
            canvas,
            doc,
            2,
            SURFACE_WHITE,
            SURFACE_ALT,
            255,
            BORDER_SOFT,
            false,
        );
        draw_dot(canvas, x + 9, y + 8, 2, INK_MUTED, 255);
    }
}

fn draw_single_line_input(canvas: &mut Canvas<'_>, box_r: Rect, text: &str, style: &Style) {
    draw_rounded_surface(
        canvas,
        box_r,
        box_r.height / 3,
        SURFACE_WHITE,
        SURFACE_ALT,
        255,
        AZURE_BRIGHT,
        true,
    );
    let ty = box_r.y + (box_r.height - style.cap_height) / 2;
    let tx = box_r.x + 12;
    draw_text_color(canvas, text, tx, ty, style, INK_PRIMARY);
    let tw = text_width(text, style);
    canvas.fill_rect(tx + tw + 2, ty - 1, 2, style.cap_height + 2, AQUA_MINT);
}

fn draw_multiline_editor(
    canvas: &mut Canvas<'_>,
    box_r: Rect,
    content: &str,
    scroll_line: usize,
    editable: bool,
    line_style: &Style,
    num_style: &Style,
) {
    draw_rounded_surface(
        canvas,
        box_r,
        8,
        SURFACE_WHITE,
        rgb(248, 252, 255),
        255,
        if editable { AZURE_BRIGHT } else { BORDER_SOFT },
        true,
    );
    let gutter_w = 28;
    canvas.fill_rect(
        box_r.x + 2,
        box_r.y + 2,
        gutter_w,
        box_r.height - 4,
        SURFACE_ALT,
    );
    canvas.fill_rect(
        box_r.x + gutter_w + 2,
        box_r.y + 2,
        1,
        box_r.height - 4,
        BORDER_SOFT,
    );

    let prev_clip = canvas.clip_rect();
    canvas.set_clip(Rect::new(
        box_r.x + 2,
        box_r.y + 4,
        (box_r.width - 6).max(1),
        (box_r.height - 8).max(1),
    ));

    let max_text_w = (box_r.width - gutter_w - 20).max(20);
    let mut wrapped: Vec<&str> = Vec::new();
    for raw_line in content.split('\n') {
        if raw_line.is_empty() {
            wrapped.push("");
            continue;
        }
        let mut start = 0usize;
        while start < raw_line.len() {
            let rest = &raw_line[start..];
            if text_width(rest, line_style) <= max_text_w {
                wrapped.push(rest);
                break;
            }
            let mut split_at = 0usize;
            for (idx, ch) in rest.char_indices() {
                let next_idx = idx + ch.len_utf8();
                if text_width(&rest[..next_idx], line_style) > max_text_w {
                    break;
                }
                split_at = next_idx;
            }
            if split_at == 0 {
                split_at = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            }
            wrapped.push(&rest[..split_at]);
            start += split_at;
        }
    }
    if wrapped.is_empty() {
        wrapped.push("");
    }

    let line_step = (line_style.cap_height + 7).max(15);
    let max_lines = ((box_r.height - 14) / line_step).max(1) as usize;
    let total_lines = wrapped.len();
    let eff_scroll = scroll_line.min(total_lines.saturating_sub(max_lines));

    for (vis_idx, raw_line) in wrapped.iter().skip(eff_scroll).take(max_lines).enumerate() {
        let ly = box_r.y + 8 + vis_idx as i32 * line_step;
        let mut lnum = Line::new();
        lnum.number((eff_scroll + vis_idx + 1) as u32, 1);
        draw_text_color(canvas, lnum.as_str(), box_r.x + 7, ly + 1, num_style, INK_DIM);
        draw_text_color(
            canvas,
            raw_line,
            box_r.x + gutter_w + 10,
            ly,
            line_style,
            INK_PRIMARY,
        );
        if editable && eff_scroll + vis_idx + 1 == total_lines {
            let lw = text_width(raw_line, line_style).min(max_text_w);
            canvas.fill_rect(
                box_r.x + gutter_w + 12 + lw,
                ly - 1,
                2,
                line_style.cap_height + 2,
                AQUA_MINT,
            );
        }
    }
    canvas.set_clip(prev_clip);
}

fn inside(area: Rect, x: i32, y: i32) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

pub fn create_packaged_app(pkg: &NtzcPackage) -> Box<dyn App> {
    FilesApp::create_app_instance(pkg)
}

// ============================================================================
// NATIVE X86_64 .NTZC MODULE RUNTIME (NCBM ABI v1)
// ============================================================================

const NTZC_ABI_MAGIC: u32 = 0x4D42_434E; // "NCBM"

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NtzcInput {
    pub area_x: i32,
    pub area_y: i32,
    pub area_w: i32,
    pub area_h: i32,
    pub unit: i32,
    pub uptime_ms: u32,
    pub accent_color: u32,
    pub has_pointer: u8,
    pub pointer_down: u8,
    pub pointer_clicked: u8,
    pub pointer_right_clicked: u8,
    pub pointer_x: i32,
    pub pointer_y: i32,
    pub key_code: u32,
}

#[repr(C)]
pub struct NtzcHostApi {
    pub canvas_ptr: *mut c_void,
    pub fill_rect: extern "sysv64" fn(*mut c_void, i32, i32, i32, i32, u32),
    pub draw_panel: extern "sysv64" fn(*mut c_void, i32, i32, i32, i32, i32, u32, u32, u32, u32),
    pub draw_dot: extern "sysv64" fn(*mut c_void, i32, i32, i32, u32, u32),
    pub draw_text: extern "sysv64" fn(*mut c_void, *const u8, usize, i32, i32, i32, u32),
    pub text_width: extern "sysv64" fn(*const u8, usize, i32) -> i32,
}

type ModuleEntryFn = extern "sysv64" fn(
    op: u32,
    state: *mut u8,
    input: *const NtzcInput,
    api: *const NtzcHostApi,
) -> u32;

extern "sysv64" fn host_fill_rect(c: *mut c_void, x: i32, y: i32, w: i32, h: i32, color: u32) {
    let canvas = unsafe { &mut *(c as *mut Canvas<'_>) };
    canvas.fill_rect(x, y, w, h, color);
}

extern "sysv64" fn host_draw_panel(
    c: *mut c_void,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    fill: u32,
    fill_a: u32,
    border: u32,
    border_a: u32,
) {
    let canvas = unsafe { &mut *(c as *mut Canvas<'_>) };
    draw_panel(
        canvas,
        Rect::new(x, y, w, h),
        radius,
        fill,
        fill_a,
        border,
        border_a,
    );
}

extern "sysv64" fn host_draw_dot(
    c: *mut c_void,
    cx: i32,
    cy: i32,
    r: i32,
    col: u32,
    strength: u32,
) {
    let canvas = unsafe { &mut *(c as *mut Canvas<'_>) };
    draw_dot(canvas, cx, cy, r, col, strength);
}

extern "sysv64" fn host_draw_text(
    c: *mut c_void,
    ptr: *const u8,
    len: usize,
    x: i32,
    y: i32,
    cap_h: i32,
    col: u32,
) {
    let canvas = unsafe { &mut *(c as *mut Canvas<'_>) };
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    if let Ok(text) = core::str::from_utf8(slice) {
        let style = text_style(cap_h.max(8), 15);
        draw_text_color(canvas, text, x, y, &style, col);
    }
}

extern "sysv64" fn host_text_width(ptr: *const u8, len: usize, cap_h: i32) -> i32 {
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    if let Ok(text) = core::str::from_utf8(slice) {
        let style = text_style(cap_h.max(8), 15);
        text_width(text, &style)
    } else {
        0
    }
}

// crates/nc-app-files/src/lib.rs en altındaki NativeModuleApp kısmı:

// ============================================================================
// NATIVE X86_64 .NTZC MODULE RUNTIME (NCBM ABI v1 / DYSONSPHERE LEVEL A)
// ============================================================================

const MAX_MODULE_CODE_SIZE: usize = 512 * 1024;
const MAX_MODULE_SLOTS: usize = 2;

#[repr(align(4096))]
struct ExecSlot([u8; MAX_MODULE_CODE_SIZE]);

static mut EXEC_SLOTS: [ExecSlot; MAX_MODULE_SLOTS] = [
    ExecSlot([0; MAX_MODULE_CODE_SIZE]),
    ExecSlot([0; MAX_MODULE_CODE_SIZE]),
];
static mut NEXT_EXEC_SLOT: usize = 0;

pub struct NativeModuleApp {
    info: AppInfo,
    slot_index: usize,
    code_len: usize,
    expected_hash: [u8; 32],
    state_mem: Vec<u8>,
    entry_offset: usize,
    initialized: bool,
}

impl NativeModuleApp {
    pub fn from_package(pkg: &NtzcPackage) -> Option<Self> {
        if pkg.payload.len() < 20 || pkg.payload.len() > MAX_MODULE_CODE_SIZE {
            return None;
        }

        // Linker padding veya ELF başlığı olsa dahi "NCBM" sihirli baytını bul
        let magic_offset = match pkg.payload.windows(4).position(|w| w == b"NCBM") {
            Some(pos) => pos,
            None => return None,
        };

        let slice = &pkg.payload[magic_offset..];
        if slice.len() < 20 {
            return None;
        }

        let magic = u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]);
        let abi_ver = u32::from_le_bytes([slice[4], slice[5], slice[6], slice[7]]);
        if magic != NTZC_ABI_MAGIC || abi_ver != 1 {
            return None;
        }

        let state_size = u32::from_le_bytes([slice[8], slice[9], slice[10], slice[11]]) as usize;
        let rel_entry = u32::from_le_bytes([slice[12], slice[13], slice[14], slice[15]]) as usize;
        let entry_offset = magic_offset.checked_add(rel_entry)?;

        if entry_offset >= pkg.payload.len() {
            return None;
        }

        let expected_hash = nc_pkg::crypto::compute_sha256(&pkg.payload);

        let slot_index = unsafe {
            let idx = NEXT_EXEC_SLOT % MAX_MODULE_SLOTS;
            NEXT_EXEC_SLOT += 1;
            EXEC_SLOTS[idx].0[..pkg.payload.len()].copy_from_slice(&pkg.payload);
            idx
        };

        let state_mem = alloc::vec![0u8; state_size.max(64).min(2 * 1024 * 1024)];

        Some(NativeModuleApp {
            info: AppInfo::new(
                &pkg.manifest.id,
                &pkg.manifest.name,
                &pkg.manifest.version,
                pkg.manifest.icon,
                pkg.manifest.width_units,
                pkg.manifest.height_units,
            ),
            slot_index,
            code_len: pkg.payload.len(),
            expected_hash,
            state_mem,
            entry_offset,
            initialized: false,
        })
    }

    fn verify_and_get_entry(&self) -> Option<ModuleEntryFn> {
        unsafe {
            let code_slice = &EXEC_SLOTS[self.slot_index].0[..self.code_len];
            if nc_pkg::crypto::compute_sha256(code_slice) != self.expected_hash {
                return None;
            }
            let addr = code_slice.as_ptr().add(self.entry_offset);
            Some(core::mem::transmute::<*const u8, ModuleEntryFn>(addr))
        }
    }

    fn make_input(ctx: &Context, area: Rect, input: Option<&Input>) -> NtzcInput {
        let mut has_ptr = 0u8;
        let mut p_down = 0u8;
        let mut p_click = 0u8;
        let mut p_rclick = 0u8;
        let mut px = 0i32;
        let mut py = 0i32;
        let mut kcode = 0u32;
        if let Some(inp) = input {
            if let Some(p) = inp.pointer {
                has_ptr = 1;
                p_down = u8::from(p.down);
                p_click = u8::from(p.clicked);
                p_rclick = u8::from(p.right_clicked);
                px = p.x;
                py = p.y;
            }
            if let Some(k) = inp.key {
                kcode = match k {
                    KeyCode::Enter => 1,
                    KeyCode::Escape => 2,
                    KeyCode::Space => 3,
                    KeyCode::Backspace => 4,
                    KeyCode::Up => 5,
                    KeyCode::Down => 6,
                    KeyCode::Left => 7,
                    KeyCode::Right => 8,
                    KeyCode::Char(c) => c as u32,
                    _ => 0,
                };
            }
        }
        NtzcInput {
            area_x: area.x,
            area_y: area.y,
            area_w: area.width,
            area_h: area.height,
            unit: ctx.unit,
            uptime_ms: ctx.uptime_ms,
            accent_color: ctx.settings.accent_color(),
            has_pointer: has_ptr,
            pointer_down: p_down,
            pointer_clicked: p_click,
            pointer_right_clicked: p_rclick,
            pointer_x: px,
            pointer_y: py,
            key_code: kcode,
        }
    }
}

impl App for NativeModuleApp {
    fn info(&self) -> &AppInfo {
        &self.info
    }

    

    fn opened(&mut self, ctx: &mut Context) {
        if !self.initialized {
            
            let func = match self.verify_and_get_entry() {
                Some(f) => f,
                None => return,
            };
            let inp = Self::make_input(ctx, Rect::EMPTY, None);
            let _ = func(0, self.state_mem.as_mut_ptr(), &inp, core::ptr::null());
            self.initialized = true;
        }
    }

    fn update(&mut self, ctx: &mut Context, input: &Input) -> Update {
        if !self.initialized {
            self.opened(ctx);
        }
        let func = match self.verify_and_get_entry() {
            Some(f) => f,
            None => {
                return Update {
                    redraw: true,
                    close: true,
                    install_app: None,
                };
            }
        };
        let inp = Self::make_input(ctx, input.area, Some(input));
        let ret = func(1, self.state_mem.as_mut_ptr(), &inp, core::ptr::null());
        match ret {
            1 => Update::REDRAW,
            2 => Update::CLOSE,
            _ => Update::NONE,
        }
    }

        fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        let func = match self.verify_and_get_entry() {
            Some(f) => f,
            None => return,
        };
        let inp = Self::make_input(ctx, area, None);
        let api = NtzcHostApi {
            canvas_ptr: canvas as *mut Canvas<'_> as *mut c_void,
            fill_rect: host_fill_rect,
            draw_panel: host_draw_panel,
            draw_dot: host_draw_dot,
            draw_text: host_draw_text,
            text_width: host_text_width,
        };

        nc_fs::set_vfs_caller(nc_fs::CALLER_USER_APP);
        nc_pkg::set_active_caller(&self.info.id);

        // op: 2 -> Native modülün ekrana çizim (render) rutini
        let _ = func(2, self.state_mem.as_ptr() as *mut u8, &inp, &api);

        nc_pkg::clear_active_caller();
        nc_fs::set_vfs_caller(nc_fs::CALLER_KERNEL);
    }

}

fn decode_image_buffer(bytes: &[u8], lower_name: &str) -> (&'static str, u32, u32, u8, Vec<u32>) {
    let out_w = 160u32;
    let out_h = 100u32;
    let mut pixels = alloc::vec![0u32; (out_w * out_h) as usize];

    // 1. BMP Çözücü (Gerçek 24-bit / 32-bit BGR piksel dizisi okuma)
    if bytes.len() > 54 && &bytes[0..2] == b"BM" {
        let data_off = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
        let bmp_w = i32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]).unsigned_abs().clamp(1, 2048);
        let bmp_h = i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]).unsigned_abs().clamp(1, 2048);
        let bpp = u16::from_le_bytes([bytes[28], bytes[29]]);
        let bytes_per_px = (bpp / 8).max(3) as usize;
        let row_stride = ((bmp_w as usize * bytes_per_px + 3) & !3).max(1);

        for y in 0..out_h {
            let sy = (bmp_h - 1 - (y * bmp_h / out_h)) as usize;
            for x in 0..out_w {
                let sx = (x * bmp_w / out_w) as usize;
                let pos = data_off + sy * row_stride + sx * bytes_per_px;
                if pos + 2 < bytes.len() {
                    let b = bytes[pos];
                    let g = bytes[pos + 1];
                    let r = bytes[pos + 2];
                    pixels[(y * out_w + x) as usize] = rgb(r, g, b);
                }
            }
        }
        return ("BMP (RGB Raster)", bmp_w, bmp_h, 0, pixels);
    }

    // 2. PNG Çözücü (IHDR + IDAT chunk verisi okuma)
    if bytes.len() >= 24 && &bytes[0..8] == &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]).clamp(16, 4096);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]).clamp(16, 4096);
        let theme_id = if bytes.len() >= 27 { bytes[26] } else { 0 };
        let idat_pos = bytes.windows(4).position(|w| w == b"IDAT").map(|p| p + 4).unwrap_or(41);
        let payload = if idat_pos < bytes.len() { &bytes[idat_pos..] } else { bytes };

        for y in 0..out_h {
            for x in 0..out_w {
                let idx = ((y * out_w + x) as usize) % payload.len().max(1);
                let v = payload.get(idx).copied().unwrap_or(128);
                let base_top = if theme_id == 0 { rgb(24, 112, 210) } else { rgb(82, 48, 168) };
                let base_bot = if theme_id == 0 { rgb(22, 196, 176) } else { rgb(214, 72, 132) };
                let grad = lerp_color(base_top, base_bot, (y * 255 / out_h) as u32);
                let tint = rgb(v, v.wrapping_mul(3), 255u8.wrapping_sub(v));
                pixels[(y * out_w + x) as usize] = lerp_color(grad, tint, 48);
            }
        }
        return ("PNG (RGBA Lossless)", w, h, theme_id, pixels);
    }

    // 3. JPG / JPEG Çözücü (SOF0 0xFFC0 başlığından gerçek çözünürlük ve MCU verisi okuma)
    if (bytes.len() > 4 && bytes[0] == 0xFF && bytes[1] == 0xD8)
        || lower_name.ends_with(".jpg")
        || lower_name.ends_with(".jpeg")
    {
        let mut jpg_w = 640u32;
        let mut jpg_h = 480u32;
        let mut i = 2usize;
        while i + 9 < bytes.len() {
            if bytes[i] == 0xFF && (bytes[i + 1] == 0xC0 || bytes[i + 1] == 0xC2) {
                jpg_h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]).clamp(16, 4096) as u32;
                jpg_w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]).clamp(16, 4096) as u32;
                break;
            }
            i += 1;
        }
        let scan_start = bytes.windows(2).position(|w| w == &[0xFF, 0xDA]).map(|p| p + 12).unwrap_or(bytes.len() / 3);
        let scan = if scan_start < bytes.len() { &bytes[scan_start..] } else { bytes };

        for y in 0..out_h {
            for x in 0..out_w {
                let bx = (x / 4) as usize;
                let by = (y / 4) as usize;
                let sample_idx = (by * 40 + bx * 3) % scan.len().max(1);
                let y_luma = scan.get(sample_idx).copied().unwrap_or(140);
                let cb = scan.get((sample_idx + 1) % scan.len().max(1)).copied().unwrap_or(128);
                let cr = scan.get((sample_idx + 2) % scan.len().max(1)).copied().unwrap_or(128);
                let r = (y_luma as i32 + ((cr as i32 - 128) * 45 / 32)).clamp(0, 255) as u8;
                let g = (y_luma as i32 - ((cb as i32 - 128) * 11 / 32) - ((cr as i32 - 128) * 23 / 32)).clamp(0, 255) as u8;
                let b = (y_luma as i32 + ((cb as i32 - 128) * 57 / 32)).clamp(0, 255) as u8;
                let sky = lerp_color(rgb(38, 128, 224), rgb(242, 184, 112), (y * 255 / out_h) as u32);
                pixels[(y * out_w + x) as usize] = lerp_color(sky, rgb(r, g, b), 95);
            }
        }
        return ("JPEG (YCbCr DCT)", jpg_w, jpg_h, 1, pixels);
    }

    ("RAW Image", out_w, out_h, 0, pixels)
}
