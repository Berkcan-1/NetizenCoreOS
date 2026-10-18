// crates/nc-setup/src/lib.rs
#![no_std]

use core::sync::atomic::{AtomicU64, Ordering};
use nc_font::{text_width, Style};
use nc_gfx::{rgb, Canvas, Rect};
use nc_hal::{BlockDeviceInfo, DeviceType, KeyCode, MouseState};
use nc_ui::{draw_dot, draw_panel, draw_shadow, draw_text_color, text_style};

pub const MAX_SETUP_DISKS: usize = 8;
const MAX_PARTITIONS: usize = 8;

const PORCELAIN_TOP: u32 = rgb(244, 249, 253);
const PORCELAIN_BOT: u32 = rgb(230, 240, 248);
const CARD_WHITE: u32 = rgb(253, 254, 255);
const CARD_BORDER: u32 = rgb(210, 224, 236);
const SUBTLE_FILL: u32 = rgb(240, 246, 251);
const SUBTLE_HOVER: u32 = rgb(228, 240, 250);

const AZURE_BRIGHT: u32 = rgb(44, 138, 242);
const AZURE_DEEP: u32 = rgb(24, 98, 206);
const AQUA_MINT: u32 = rgb(26, 188, 168);
const AQUA_SOFT: u32 = rgb(156, 236, 222);
const AQUA_DEEP: u32 = rgb(16, 144, 130);
const TINT_SELECTED: u32 = rgb(224, 242, 252);

const INK_DARK: u32 = rgb(16, 28, 44);
const INK_BODY: u32 = rgb(58, 76, 96);
const INK_MUTED: u32 = rgb(116, 134, 154);
const INK_LIGHT: u32 = rgb(156, 172, 190);

static LAST_RENDER_HASH: AtomicU64 = AtomicU64::new(u64::MAX);

const LANGUAGES: [(&str, &str); 2] = [
    ("Turkce (Turkiye)", "Standart Turkce sistem arayuzu ve yerel bicimlendirme"),
    ("English (United States)", "International English system interface and messages"),
];

const TIMEZONES_TR: [&str; 3] = [
    "UTC+03:00  Istanbul, Turkiye (24 Saat)",
    "UTC+00:00  Evrensel Esgudumlu Saat (UTC)",
    "UTC-05:00  Kuzey Amerika Dogu Saati",
];

const TIMEZONES_EN: [&str; 3] = [
    "UTC+03:00  Istanbul, Turkey (24-Hour)",
    "UTC+00:00  Coordinated Universal Time (UTC)",
    "UTC-05:00  North America Eastern Time",
];

const KEYBOARDS_TR: [(&str, &str); 2] = [
    ("Turkce Q Klavye", "q w e r t y u i o p  |  a s d f g h j k l"),
    ("ABD Ingilizce (US QWERTY)", "q w e r t y u i o p [ ]  |  a s d f g h j k l ; '"),
];

const KEYBOARDS_EN: [(&str, &str); 2] = [
    ("Turkish Q Keyboard", "q w e r t y u i o p  |  a s d f g h j k l"),
    ("US English (QWERTY)", "q w e r t y u i o p [ ]  |  a s d f g h j k l ; '"),
];

// C-02: /data bolumu LBA 4_851_712'de baslar ve en az 2_097_152 sektor (1 GiB) gerektirir (~3393 MB)
pub const MIN_INSTALL_SECTORS: u64 = 6_948_864;
pub const MIN_INSTALL_MB: u64 = (MIN_INSTALL_SECTORS * 512) / (1024 * 1024);

const ARCH_NOTES_TR: [&str; 14] = [
    "NetizenCore Isletim Sistemi Mimarisi",
    "",
    "1. Bagimsiz Cekirdek Yapisi",
    "   NetizenCore; Unix veya Linux tabanli degildir. Onyukleyici NASM,",
    "   cekirdek ve arayuz bilesenleri ise tamamen Rust ile yazilmistir.",
    "",
    "2. Disk Bolumleme Semasi (MBR + Dual-Boot BIOS/UEFI)",
    "   - Onyukleme & ESP (64 MB) : LBA 2048 Stage2 + LBA 65536 FAT16 ESP",
    "   - Bolum 2 (2048 MB)       : Salt okunur sistem agaci (/system - EROFS)",
    "   - Bolum 3 (256 MB)        : Ayrilmis guvenlik bolumu (/security - 0x83)",
    "   - Bolum 4 (Kalan Alan)    : Kisisel kullanici diski (/data - exFAT)",
    "",
    "3. Moduler .efra Surucu Katmani",
    "   Donanim birimleri otomatik taranir ve .efra paketleriyle eslestirilir.",
];

const ARCH_NOTES_EN: [&str; 14] = [
    "NetizenCore Operating System Architecture",
    "",
    "1. Independent Kernel Design",
    "   NetizenCore is not based on Unix or Linux. The bootloader is written",
    "   in NASM Assembly and the kernel/UI stack is 100% no_std Rust.",
    "",
    "2. Disk Partition Layout (MBR + Dual-Boot BIOS/UEFI)",
    "   - Boot & ESP (64 MB)      : LBA 2048 Stage2 + LBA 65536 FAT16 ESP",
    "   - Partition 2 (2048 MB)   : Read-only system tree (/system - EROFS)",
    "   - Partition 3 (256 MB)    : Reserved security partition (/security - 0x83)",
    "   - Partition 4 (Remaining) : Personal user storage (/data - exFAT)",
    "",
    "3. Modular .efra Driver Layer",
    "   Hardware buses are probed and bound to optimized .efra packages.",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SetupAction {
    None,
    Shutdown,
    Reboot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepPhase {
    Welcome,
    Architecture,
    Storage,
    Installing,
    Completed,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VolumeRole {
    Unallocated,
    BootCore,
    SystemErofs,
    SecurityVault,
    DataExfat,
    OpticalIso,
}

#[derive(Clone, Copy)]
pub struct VolumeEntry {
    pub disk_index: usize,
    pub index_on_disk: u8,
    pub role: VolumeRole,
    pub start_lba: u64,
    pub size_mb: u64,
    pub free_mb: u64,
}

impl VolumeEntry {
    const EMPTY: VolumeEntry = VolumeEntry {
        disk_index: 0,
        index_on_disk: 0,
        role: VolumeRole::Unallocated,
        start_lba: 0,
        size_mb: 0,
        free_mb: 0,
    };
}

pub struct SetupState {
    pub step: usize,
    phase: StepPhase,
    pub selected_disk: usize,
    pub disks: [BlockDeviceInfo; MAX_SETUP_DISKS],
    pub disk_count: usize,
    pub volumes: [VolumeEntry; MAX_PARTITIONS],
    pub volume_count: usize,
    pub selected_volume: usize,
    pub layout_prepared: bool,
    pub progress: u32,
    pub changed: bool,
    pub install_error: Option<&'static str>,
    lang_index: usize,
    tz_index: usize,
    kbd_index: usize,
    terms_accepted: bool,
    doc_scroll: usize,
    profile_mode: usize,
    install_ticks: u32,
    reboot_ticks: u32,
    inspector_open: bool,
    banner_state: u8,
}

struct ShellLayout {
    top_pill: Rect,
    probe_btn: Rect,
    main_card: Rect,
    left_pane: Rect,
    right_pane: Rect,
    bottom_bar: Rect,
    back_btn: Rect,
    next_btn: Rect,
    power_btn: Rect,
}

fn compute_shell(width: i32, height: i32) -> ShellLayout {
    let shell_w = (width * 84 / 100).clamp(680, 920).min(width - 28);
    let shell_h = (height * 82 / 100).clamp(470, 600).min(height - 28);
    let shell_x = (width - shell_w) / 2;
    let shell_y = (height - shell_h) / 2;

    let top_pill = Rect::new(shell_x, shell_y, shell_w, 46);
    let probe_btn = Rect::new(top_pill.x + top_pill.width - 154, top_pill.y + 8, 142, 30);

    let main_y = top_pill.y + top_pill.height + 10;
    let bottom_h = 52;
    let main_h = shell_h - top_pill.height - bottom_h - 20;
    let main_card = Rect::new(shell_x, main_y, shell_w, main_h);

    let left_w = (shell_w * 33 / 100).clamp(210, 290);
    let left_pane = Rect::new(main_card.x + 12, main_card.y + 12, left_w, main_card.height - 24);
    let right_pane = Rect::new(
        left_pane.x + left_pane.width + 14,
        main_card.y + 12,
        main_card.width - left_w - 38,
        main_card.height - 24,
    );

    let bottom_bar = Rect::new(shell_x, main_card.y + main_card.height + 10, shell_w, bottom_h);
    let next_btn = Rect::new(bottom_bar.x + bottom_bar.width - 168, bottom_bar.y + 9, 156, 34);
    let back_btn = Rect::new(next_btn.x - 110, bottom_bar.y + 9, 100, 34);
    let power_btn = Rect::new(bottom_bar.x + 12, bottom_bar.y + 9, 140, 34);

    ShellLayout {
        top_pill,
        probe_btn,
        main_card,
        left_pane,
        right_pane,
        bottom_bar,
        back_btn,
        next_btn,
        power_btn,
    }
}

fn hit_rect(x: i32, y: i32, r: Rect) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

fn sel<'a>(state: &SetupState, tr: &'a str, en: &'a str) -> &'a str {
    if state.lang_index == 0 {
        tr
    } else {
        en
    }
}

fn draw_clipped(canvas: &mut Canvas<'_>, text: &str, x: i32, y: i32, max_w: i32, style: &Style, col: u32) {
    if max_w <= 8 {
        return;
    }
    if text_width(text, style) <= max_w {
        draw_text_color(canvas, text, x, y, style, col);
        return;
    }
    let mut end_byte = 0usize;
    for (idx, ch) in text.char_indices() {
        let next_idx = idx + ch.len_utf8();
        if text_width(&text[..next_idx], style) > max_w {
            break;
        }
        end_byte = next_idx;
    }
    if end_byte > 0 {
        draw_text_color(canvas, &text[..end_byte], x, y, style, col);
    }
}

impl SetupState {
    pub fn new(disks_slice: &[BlockDeviceInfo]) -> SetupState {
        let mut disks = [BlockDeviceInfo::EMPTY; MAX_SETUP_DISKS];
        let count = disks_slice.len().min(MAX_SETUP_DISKS);
        for i in 0..count {
            disks[i] = disks_slice[i];
        }

        // Gercek PC'de ilk uygun sabit diski otomatik sec
        let mut initial_disk = 0usize;
        for i in 0..count {
            if disks[i].device_type == DeviceType::HardDisk
                && disks[i].block_size == 512
                && disks[i].block_count >= MIN_INSTALL_SECTORS
            {
                initial_disk = i;
                break;
            }
        }

        let mut state = SetupState {
            step: 0,
            phase: StepPhase::Welcome,
            selected_disk: initial_disk,
            disks,
            disk_count: count,
            volumes: [VolumeEntry::EMPTY; MAX_PARTITIONS],
            volume_count: 0,
            selected_volume: 0,
            layout_prepared: true,
            progress: 0,
            changed: true,
            install_error: None,
            lang_index: 0,
            tz_index: 0,
            kbd_index: 0,
            terms_accepted: true,
            doc_scroll: 0,
            profile_mode: 0,
            install_ticks: 0,
            reboot_ticks: 0,
            inspector_open: false,
            banner_state: 0,
        };
        state.sync_volumes();
        LAST_RENDER_HASH.store(u64::MAX, Ordering::Relaxed);
        state
    }

    pub fn selected_language_code(&self) -> u8 {
        if self.lang_index == 0 { 0 } else { 1 }
    }

    pub fn selected_keyboard_code(&self) -> u8 {
        if self.kbd_index == 0 { 0 } else { 1 }
    }

    // C-01 & C-02: Secili diskin kurulum sartlarini gercekten sagladigini dogrula
    pub fn is_disk_eligible(&self, disk_idx: usize) -> bool {
        if disk_idx >= self.disk_count {
            return false;
        }
        let d = &self.disks[disk_idx];
        d.device_type == DeviceType::HardDisk
            && d.block_size == 512
            && d.block_count >= MIN_INSTALL_SECTORS
            && d.channel < 4
    }

    // C-01: UI liste indeksini degil, dogrudan secilen diskin fiziksel ATA kanal numarasini dondur
    pub fn selected_disk_channel(&self) -> Option<u8> {
        if self.is_disk_eligible(self.selected_disk) {
            Some(self.disks[self.selected_disk].channel)
        } else {
            None
        }
    }

    pub fn set_progress(&mut self, pct: u32, _stage_text: &'static str) {
        self.progress = (pct.min(100) * 256) / 100;
        self.changed = true;
    }

    // H-01: Yalnizca cekirdek yazma ve dogrulama islemini bitirdiginde cagrilir
    pub fn complete_install(&mut self) {
        self.progress = 256;
        self.phase = StepPhase::Completed;
        self.reboot_ticks = 0;
        self.step = 3;
        self.install_error = None;
        self.changed = true;
    }

    // H-01: Kurulum basarisiz oldugunda sahte basari yerine hata ekranina gec
    pub fn fail_install(&mut self, error_msg: &'static str) {
        self.phase = StepPhase::Failed;
        self.step = 1;
        self.install_error = Some(error_msg);
        self.changed = true;
    }

    fn sync_volumes(&mut self) {
        self.volume_count = 0;
        if self.disk_count == 0 {
            return;
        }
        if self.selected_disk >= self.disk_count {
            self.selected_disk = 0;
        }

        // Ham disk gorunumu: Sistemdeki tum fiziksel diskleri gercek kapasiteleriyle listele
        if !self.layout_prepared {
            for (d_idx, dev) in self.disks[..self.disk_count].iter().enumerate() {
                if self.volume_count >= MAX_PARTITIONS {
                    break;
                }
                let real_mb = dev.size_mb();
                let role = if dev.device_type == DeviceType::OpticalDisc {
                    VolumeRole::OpticalIso
                } else {
                    VolumeRole::Unallocated
                };
                self.volumes[self.volume_count] = VolumeEntry {
                    disk_index: d_idx,
                    index_on_disk: 0,
                    role,
                    start_lba: 0,
                    size_mb: real_mb,
                    free_mb: if role == VolumeRole::OpticalIso { 0 } else { real_mb },
                };
                if d_idx == self.selected_disk {
                    self.selected_volume = self.volume_count;
                }
                self.volume_count += 1;
            }
            return;
        }

        // Bolumlenmis gorunum: Secili diskin gercek kapasitesine gore MBR tablosunu olustur
        let d_idx = self.selected_disk;
        let dev = &self.disks[d_idx];
        let real_mb = dev.size_mb();

        if dev.device_type == DeviceType::OpticalDisc {
            self.volumes[0] = VolumeEntry {
                disk_index: d_idx,
                index_on_disk: 1,
                role: VolumeRole::OpticalIso,
                start_lba: 0,
                size_mb: real_mb,
                free_mb: 0,
            };
            self.volume_count = 1;
            self.selected_volume = 0;
        } else if !self.is_disk_eligible(d_idx) {
            // 3.4 GB altindaki diskleri sahte 4096 MB olarak gosterme; gercek boyutunu goster
            self.volumes[0] = VolumeEntry {
                disk_index: d_idx,
                index_on_disk: 0,
                role: VolumeRole::Unallocated,
                start_lba: 0,
                size_mb: real_mb,
                free_mb: real_mb,
            };
            self.volume_count = 1;
            self.selected_volume = 0;
        } else {
            let boot_mb = 32u64; // LBA 65536, 65536 sektor (32 MiB FAT16 ESP + LBA 2048 Stage2/Kernel)
            let sys_mb = 2048u64; // LBA 133120, 4194304 sektor (2 GiB EROFS)
            let sec_mb = 256u64; // LBA 4327424, 524288 sektor (256 MiB Reserved)
            let data_sectors = dev.block_count.saturating_sub(4_851_712);
            let data_mb = (data_sectors * 512) / (1024 * 1024);

            let entries = [
                VolumeEntry {
                    disk_index: d_idx,
                    index_on_disk: 1,
                    role: VolumeRole::BootCore,
                    start_lba: 65_536,
                    size_mb: boot_mb,
                    free_mb: 24,
                },
                VolumeEntry {
                    disk_index: d_idx,
                    index_on_disk: 2,
                    role: VolumeRole::SystemErofs,
                    start_lba: 133_120,
                    size_mb: sys_mb,
                    free_mb: 1620,
                },
                VolumeEntry {
                    disk_index: d_idx,
                    index_on_disk: 3,
                    role: VolumeRole::SecurityVault,
                    start_lba: 4_327_424,
                    size_mb: sec_mb,
                    free_mb: 256,
                },
                VolumeEntry {
                    disk_index: d_idx,
                    index_on_disk: 4,
                    role: VolumeRole::DataExfat,
                    start_lba: 4_851_712,
                    size_mb: data_mb,
                    free_mb: data_mb.saturating_sub(16),
                },
            ];
            for e in entries {
                self.volumes[self.volume_count] = e;
                self.volume_count += 1;
            }
            self.selected_volume = 3; // Varsayilan olarak /data bolumunu vurgula
        }
    }

    fn can_continue(&self) -> bool {
        match self.phase {
            StepPhase::Welcome => true,
            StepPhase::Architecture => self.terms_accepted,
            StepPhase::Storage => self.is_disk_eligible(self.selected_disk),
            StepPhase::Installing => false,
            StepPhase::Completed => true,
            StepPhase::Failed => true,
        }
    }

    fn step_forward(&mut self) -> SetupAction {
        match self.phase {
            StepPhase::Welcome => {
                self.phase = StepPhase::Architecture;
                self.step = 0;
                self.changed = true;
            }
            StepPhase::Architecture => {
                if self.terms_accepted {
                    self.phase = StepPhase::Storage;
                    self.step = 1;
                    self.changed = true;
                }
            }
            StepPhase::Storage => {
                if self.can_continue() {
                    if !self.layout_prepared {
                        self.layout_prepared = true;
                        self.sync_volumes();
                    }
                    self.phase = StepPhase::Installing;
                    self.install_ticks = 0;
                    self.progress = 24;
                    self.install_error = None;
                    self.step = 1;
                    self.changed = true;
                }
            }
            StepPhase::Installing => {}
            StepPhase::Completed => {
                return SetupAction::Reboot;
            }
            StepPhase::Failed => {
                // Hata ekraninda "Yeniden Dene / Disk Sec" tiklandiginda Depolama adimina don
                self.phase = StepPhase::Storage;
                self.step = 1;
                self.progress = 0;
                self.install_error = None;
                self.changed = true;
            }
        }
        SetupAction::None
    }

    fn step_backward(&mut self) {
        match self.phase {
            StepPhase::Architecture => {
                self.phase = StepPhase::Welcome;
                self.step = 0;
                self.changed = true;
            }
            StepPhase::Storage => {
                self.phase = StepPhase::Architecture;
                self.step = 0;
                self.changed = true;
            }
            StepPhase::Failed => {
                self.phase = StepPhase::Storage;
                self.step = 1;
                self.progress = 0;
                self.install_error = None;
                self.changed = true;
            }
            _ => {}
        }
    }

    pub fn update(&mut self, mouse: &MouseState, key: Option<KeyCode>, width: i32, height: i32) -> SetupAction {
        let shell = compute_shell(width, height);

        if self.inspector_open {
            let modal = inspector_rect(width, height);
            let close_btn = Rect::new(modal.x + modal.width - 116, modal.y + modal.height - 44, 100, 30);
            if key == Some(KeyCode::Escape) || key == Some(KeyCode::Enter) {
                self.inspector_open = false;
                self.changed = true;
            } else if mouse.left_clicked && hit_rect(mouse.x, mouse.y, close_btn) {
                self.inspector_open = false;
                self.changed = true;
            }
            return SetupAction::None;
        }

        if let Some(k) = key {
            match k {
                KeyCode::Enter => {
                    let act = self.step_forward();
                    if act != SetupAction::None {
                        return act;
                    }
                }
                KeyCode::Space => {
                    if self.phase == StepPhase::Architecture {
                        self.terms_accepted = !self.terms_accepted;
                        self.changed = true;
                    } else {
                        let act = self.step_forward();
                        if act != SetupAction::None {
                            return act;
                        }
                    }
                }
                KeyCode::Escape => {
                    self.step_backward();
                }
                KeyCode::Up => match self.phase {
                    StepPhase::Welcome => {
                        self.lang_index = (self.lang_index + LANGUAGES.len() - 1) % LANGUAGES.len();
                        self.kbd_index = self.lang_index;
                        self.changed = true;
                    }
                    StepPhase::Architecture => {
                        if self.doc_scroll > 0 {
                            self.doc_scroll -= 1;
                            self.changed = true;
                        }
                    }
                    StepPhase::Storage => {
                        if self.selected_volume > 0 {
                            self.selected_volume -= 1;
                            self.selected_disk = self.volumes[self.selected_volume].disk_index;
                            self.changed = true;
                        }
                    }
                    _ => {}
                },
                KeyCode::Down => match self.phase {
                    StepPhase::Welcome => {
                        self.lang_index = (self.lang_index + 1) % LANGUAGES.len();
                        self.kbd_index = self.lang_index;
                        self.changed = true;
                    }
                    StepPhase::Architecture => {
                        if self.doc_scroll + 7 < ARCH_NOTES_TR.len() {
                            self.doc_scroll += 1;
                            self.changed = true;
                        }
                    }
                    StepPhase::Storage => {
                        if self.selected_volume + 1 < self.volume_count {
                            self.selected_volume += 1;
                            self.selected_disk = self.volumes[self.selected_volume].disk_index;
                            self.changed = true;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        if mouse.left_clicked {
            if hit_rect(mouse.x, mouse.y, shell.probe_btn) {
                self.inspector_open = true;
                self.changed = true;
                return SetupAction::None;
            }
            if hit_rect(mouse.x, mouse.y, shell.power_btn) && self.phase != StepPhase::Installing {
                return SetupAction::Reboot;
            }
            if hit_rect(mouse.x, mouse.y, shell.next_btn) && self.can_continue() {
                let act = self.step_forward();
                if act != SetupAction::None {
                    return act;
                }
            } else if (self.phase == StepPhase::Architecture
                || self.phase == StepPhase::Storage
                || self.phase == StepPhase::Failed)
                && hit_rect(mouse.x, mouse.y, shell.back_btn)
            {
                self.step_backward();
            } else {
                match self.phase {
                    StepPhase::Welcome => {
                        for i in 0..LANGUAGES.len() {
                            let r = welcome_lang_card(shell.right_pane, i);
                            if hit_rect(mouse.x, mouse.y, r) {
                                self.lang_index = i;
                                self.kbd_index = i;
                                self.changed = true;
                            }
                        }
                        let tz_r = welcome_selector_row(shell.right_pane, 0);
                        let kb_r = welcome_selector_row(shell.right_pane, 1);
                        if hit_rect(mouse.x, mouse.y, tz_r) {
                            self.tz_index = (self.tz_index + 1) % TIMEZONES_TR.len();
                            self.changed = true;
                        } else if hit_rect(mouse.x, mouse.y, kb_r) {
                            self.kbd_index = (self.kbd_index + 1) % KEYBOARDS_TR.len();
                            self.changed = true;
                        }
                    }
                    StepPhase::Architecture => {
                        let doc_r = arch_doc_rect(shell.right_pane);
                        let up_r = Rect::new(doc_r.x + doc_r.width - 28, doc_r.y + 6, 22, 24);
                        let dn_r = Rect::new(doc_r.x + doc_r.width - 28, doc_r.y + doc_r.height - 30, 22, 24);
                        if hit_rect(mouse.x, mouse.y, up_r) && self.doc_scroll > 0 {
                            self.doc_scroll -= 1;
                            self.changed = true;
                        } else if hit_rect(mouse.x, mouse.y, dn_r) && self.doc_scroll + 7 < ARCH_NOTES_TR.len() {
                            self.doc_scroll += 1;
                            self.changed = true;
                        }
                        let m0 = arch_mode_card(shell.right_pane, 0);
                        let m1 = arch_mode_card(shell.right_pane, 1);
                        if hit_rect(mouse.x, mouse.y, m0) {
                            self.profile_mode = 0;
                            self.changed = true;
                        } else if hit_rect(mouse.x, mouse.y, m1) {
                            self.profile_mode = 1;
                            self.changed = true;
                        }
                        let toggle_r = arch_consent_rect(shell.right_pane);
                        if hit_rect(mouse.x, mouse.y, toggle_r) {
                            self.terms_accepted = !self.terms_accepted;
                            self.changed = true;
                        }
                    }
                    StepPhase::Storage => {
                        for i in 0..self.volume_count.min(5) {
                            let vr = storage_row_rect(shell.right_pane, i);
                            if hit_rect(mouse.x, mouse.y, vr) {
                                self.selected_volume = i;
                                self.selected_disk = self.volumes[i].disk_index;
                                self.changed = true;
                            }
                        }
                        for t in 0..4 {
                            let tr = storage_action_pill(shell.right_pane, t);
                            if hit_rect(mouse.x, mouse.y, tr) {
                                match t {
                                    0 => {
                                        // Coklu disk varsa sonraki fiziksel diske gec
                                        if self.disk_count > 1 {
                                            self.selected_disk = (self.selected_disk + 1) % self.disk_count;
                                        }
                                        self.sync_volumes();
                                        self.banner_state = 1;
                                    }
                                    1 => {
                                        // Ham disk listesini goster
                                        self.layout_prepared = false;
                                        self.sync_volumes();
                                        self.banner_state = 2;
                                    }
                                    2 => {
                                        // Secili diski otomatik bolumle
                                        self.layout_prepared = true;
                                        self.sync_volumes();
                                        self.banner_state = 3;
                                    }
                                    _ => {
                                        // Donanim ve kanal denetcisini ac
                                        self.inspector_open = true;
                                        self.banner_state = 4;
                                    }
                                }
                                self.changed = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // H-01: Zamanlayici kurulumu kendi kendine Completed yapmaz; cekirdegin yazma adimini tetikler
        if self.phase == StepPhase::Installing {
            self.install_ticks += 1;
            if self.install_ticks == 2 {
                self.step = 2; // Cekirdege kurulumu baslatmasi icin sinyal ver
                self.progress = 96;
                self.changed = true;
            } else if self.install_ticks > 2 && self.progress < 220 {
                self.progress = (self.progress + 4).min(220);
                self.changed = true;
            }
        } else if self.phase == StepPhase::Completed {
            self.reboot_ticks += 1;
            if self.reboot_ticks % 60 == 0 {
                self.changed = true;
            }
            if self.reboot_ticks >= 600 {
                return SetupAction::Reboot;
            }
        }

        SetupAction::None
    }
}

fn welcome_lang_card(pane: Rect, idx: usize) -> Rect {
    Rect::new(pane.x + 14, pane.y + 56 + idx as i32 * 56, pane.width - 28, 48)
}

fn welcome_selector_row(pane: Rect, idx: usize) -> Rect {
    Rect::new(pane.x + 14, pane.y + 180 + idx as i32 * 46, pane.width - 28, 38)
}

fn arch_doc_rect(pane: Rect) -> Rect {
    Rect::new(pane.x + 14, pane.y + 54, pane.width - 28, 150)
}

fn arch_mode_card(pane: Rect, idx: usize) -> Rect {
    let w = (pane.width - 38) / 2;
    Rect::new(pane.x + 14 + idx as i32 * (w + 10), pane.y + 214, w, 50)
}

fn arch_consent_rect(pane: Rect) -> Rect {
    Rect::new(pane.x + 14, pane.y + 274, pane.width - 28, 36)
}

fn storage_row_rect(pane: Rect, idx: usize) -> Rect {
    Rect::new(pane.x + 14, pane.y + 54 + idx as i32 * 35, pane.width - 28, 31)
}

fn storage_action_pill(pane: Rect, idx: usize) -> Rect {
    let w = (pane.width - 46) / 4;
    Rect::new(pane.x + 14 + idx as i32 * (w + 6), pane.y + 238, w, 28)
}

fn inspector_rect(width: i32, height: i32) -> Rect {
    let w = 540.min(width - 40);
    let h = 330.min(height - 40);
    Rect::new((width - w) / 2, (height - h) / 2, w, h)
}

fn compute_render_signature(width: i32, height: i32, state: &SetupState, mouse: &MouseState) -> u64 {
    let shell = compute_shell(width, height);
    let mut hover_mask = 0u64;
    if hit_rect(mouse.x, mouse.y, shell.probe_btn) {
        hover_mask |= 1 << 0;
    }
    if hit_rect(mouse.x, mouse.y, shell.power_btn) {
        hover_mask |= 1 << 1;
    }
    if hit_rect(mouse.x, mouse.y, shell.back_btn) {
        hover_mask |= 1 << 2;
    }
    if hit_rect(mouse.x, mouse.y, shell.next_btn) {
        hover_mask |= 1 << 3;
    }
    if state.inspector_open {
        let modal = inspector_rect(width, height);
        let close_btn = Rect::new(modal.x + modal.width - 116, modal.y + modal.height - 44, 100, 30);
        if hit_rect(mouse.x, mouse.y, close_btn) {
            hover_mask |= 1 << 4;
        }
    }
    match state.phase {
        StepPhase::Welcome => {
            for i in 0..2 {
                if hit_rect(mouse.x, mouse.y, welcome_lang_card(shell.right_pane, i)) {
                    hover_mask |= 1 << (5 + i);
                }
            }
            for i in 0..2 {
                if hit_rect(mouse.x, mouse.y, welcome_selector_row(shell.right_pane, i)) {
                    hover_mask |= 1 << (8 + i);
                }
            }
        }
        StepPhase::Architecture => {
            for i in 0..2 {
                if hit_rect(mouse.x, mouse.y, arch_mode_card(shell.right_pane, i)) {
                    hover_mask |= 1 << (10 + i);
                }
            }
            if hit_rect(mouse.x, mouse.y, arch_consent_rect(shell.right_pane)) {
                hover_mask |= 1 << 12;
            }
        }
        StepPhase::Storage => {
            for i in 0..state.volume_count.min(5) {
                if hit_rect(mouse.x, mouse.y, storage_row_rect(shell.right_pane, i)) {
                    hover_mask |= 1 << (13 + i);
                }
            }
            for i in 0..4 {
                if hit_rect(mouse.x, mouse.y, storage_action_pill(shell.right_pane, i)) {
                    hover_mask |= 1 << (18 + i);
                }
            }
        }
        _ => {}
    }

    let phase_code = match state.phase {
        StepPhase::Welcome => 1u64,
        StepPhase::Architecture => 2,
        StepPhase::Storage => 3,
        StepPhase::Installing => 4,
        StepPhase::Completed => 5,
        StepPhase::Failed => 6,
    };
    let mut sig = hover_mask;
    sig ^= phase_code << 24;
    sig ^= (state.selected_disk as u64) << 21;
    sig ^= (state.lang_index as u64) << 28;
    sig ^= (state.tz_index as u64) << 30;
    sig ^= (state.kbd_index as u64) << 32;
    sig ^= (state.doc_scroll as u64) << 34;
    sig ^= (state.profile_mode as u64) << 40;
    sig ^= (state.terms_accepted as u64) << 42;
    sig ^= (state.selected_volume as u64) << 43;
    sig ^= (state.layout_prepared as u64) << 47;
    sig ^= (state.banner_state as u64) << 48;
    sig ^= (state.inspector_open as u64) << 51;
    sig ^= ((state.progress * 100 / 256) as u64) << 52;
    sig ^= ((state.reboot_ticks / 60) as u64) << 60;
    sig
}

fn draw_clean_backdrop(canvas: &mut Canvas<'_>) {
    canvas.vertical_gradient(PORCELAIN_TOP, PORCELAIN_BOT);
    let w = canvas.width() as i32;
    let h = canvas.height() as i32;
    canvas.blend_glow(w * 20 / 100, h * 22 / 100, h / 4, AQUA_SOFT, 65);
    canvas.blend_glow(w * 82 / 100, h * 78 / 100, h / 4, rgb(180, 224, 255), 65);
}

fn draw_surface_card(canvas: &mut Canvas<'_>, rect: Rect, radius: i32, shadow_strength: u32) {
    if shadow_strength > 0 {
        draw_shadow(canvas, rect, radius, 4, 12, rgb(20, 42, 66), shadow_strength);
    }
    draw_panel(canvas, rect, radius, CARD_WHITE, 248, CARD_BORDER, 255);
}

fn draw_pill_button(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    label: &str,
    hovered: bool,
    primary: bool,
    enabled: bool,
) {
    let radius = rect.height / 2;
    let st = text_style(10, 15);
    let max_tw = (rect.width - 12).max(12);
    let tw = text_width(label, &st).min(max_tw);
    let tx = rect.x + (rect.width - tw) / 2;
    let ty = rect.y + (rect.height - st.cap_height) / 2;

    if !enabled {
        draw_panel(canvas, rect, radius, SUBTLE_FILL, 255, CARD_BORDER, 255);
        draw_clipped(canvas, label, tx, ty, max_tw, &st, INK_LIGHT);
        return;
    }

    if primary {
        let fill = if hovered { AZURE_DEEP } else { AZURE_BRIGHT };
        draw_panel(canvas, rect, radius, fill, 255, AZURE_DEEP, 255);
        draw_clipped(canvas, label, tx, ty, max_tw, &st, rgb(255, 255, 255));
    } else {
        let fill = if hovered { SUBTLE_HOVER } else { CARD_WHITE };
        let brd = if hovered { AZURE_BRIGHT } else { CARD_BORDER };
        draw_panel(canvas, rect, radius, fill, 255, brd, 255);
        draw_clipped(canvas, label, tx, ty, max_tw, &st, INK_DARK);
    }
}

fn draw_radio_dot(canvas: &mut Canvas<'_>, cx: i32, cy: i32, selected: bool) {
    if selected {
        draw_dot(canvas, cx, cy, 8, AZURE_BRIGHT, 255);
        draw_dot(canvas, cx, cy, 3, rgb(255, 255, 255), 255);
    } else {
        draw_dot(canvas, cx, cy, 8, CARD_BORDER, 255);
        draw_dot(canvas, cx, cy, 6, CARD_WHITE, 255);
    }
}

fn draw_switch(canvas: &mut Canvas<'_>, x: i32, y: i32, active: bool) {
    let pill = Rect::new(x, y, 36, 18);
    let fill = if active { AQUA_MINT } else { CARD_BORDER };
    draw_panel(canvas, pill, 9, fill, 255, fill, 0);
    let kx = if active { x + 26 } else { x + 10 };
    draw_dot(canvas, kx, y + 9, 6, rgb(255, 255, 255), 255);
}

fn format_capacity(mb: u64, buf: &mut [u8]) -> usize {
    let mut pos = 0usize;
    let push_u64 = |mut v: u64, out: &mut [u8], p: &mut usize| {
        let mut tmp = [0u8; 20];
        let mut t = 0;
        if v == 0 {
            tmp[0] = b'0';
            t = 1;
        } else {
            while v > 0 {
                tmp[t] = b'0' + (v % 10) as u8;
                v /= 10;
                t += 1;
            }
        }
        for i in (0..t).rev() {
            if *p < out.len() {
                out[*p] = tmp[i];
                *p += 1;
            }
        }
    };
    if mb >= 1024 {
        let gb = mb / 1024;
        let dec = (mb % 1024) * 10 / 1024;
        push_u64(gb, buf, &mut pos);
        buf[pos] = b'.';
        pos += 1;
        push_u64(dec, buf, &mut pos);
        for b in b" GB" {
            buf[pos] = *b;
            pos += 1;
        }
    } else {
        push_u64(mb, buf, &mut pos);
        for b in b" MB" {
            buf[pos] = *b;
            pos += 1;
        }
    }
    pos
}

fn format_number(mut val: u64, buf: &mut [u8]) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 20];
    let mut t = 0;
    while val > 0 {
        tmp[t] = b'0' + (val % 10) as u8;
        val /= 10;
        t += 1;
    }
    for i in 0..t {
        buf[i] = tmp[t - 1 - i];
    }
    t
}

fn draw_top_navigation_pill(canvas: &mut Canvas<'_>, shell: &ShellLayout, state: &SetupState, mouse: &MouseState) {
    draw_surface_card(canvas, shell.top_pill, 14, 10);

    let orb_x = shell.top_pill.x + 22;
    let orb_y = shell.top_pill.y + 23;
    draw_dot(canvas, orb_x, orb_y, 9, AZURE_BRIGHT, 255);
    draw_dot(canvas, orb_x, orb_y, 3, rgb(255, 255, 255), 255);

    let brand_st = text_style(12, 15);
    let sub_st = text_style(9, 15);
    draw_text_color(canvas, "NetizenCore", orb_x + 16, shell.top_pill.y + 10, &brand_st, INK_DARK);
    draw_text_color(
        canvas,
        sel(state, "Kurulum Sihirbazi", "System Installer"),
        orb_x + 16,
        shell.top_pill.y + 26,
        &sub_st,
        INK_MUTED,
    );

    let active_step: usize = match state.phase {
        StepPhase::Welcome => 0,
        StepPhase::Architecture => 1,
        StepPhase::Storage => 2,
        StepPhase::Installing | StepPhase::Completed | StepPhase::Failed => 3,
    };

    let steps = if state.lang_index == 0 {
        ["1. Dil & Bolge", "2. Mimari", "3. Depolama", "4. Kurulum"]
    } else {
        ["1. Language", "2. Architecture", "3. Storage", "4. Install"]
    };

    let step_w = 102;
    let total_steps_w = step_w * 4 + 18;
    let start_x = shell.top_pill.x + (shell.top_pill.width - total_steps_w) / 2 + 10;
    let pill_y = shell.top_pill.y + 10;
    let st_font = text_style(9, 14);

    for (idx, title) in steps.iter().enumerate() {
        let r = Rect::new(start_x + idx as i32 * (step_w + 6), pill_y, step_w, 26);
        let (fill, brd, fg) = if idx == active_step {
            (AZURE_BRIGHT, AZURE_DEEP, rgb(255, 255, 255))
        } else if idx < active_step {
            (TINT_SELECTED, AQUA_MINT, AQUA_DEEP)
        } else {
            (SUBTLE_FILL, CARD_BORDER, INK_MUTED)
        };
        draw_panel(canvas, r, 13, fill, 255, brd, 255);
        let tw = text_width(title, &st_font).min(r.width - 8);
        draw_clipped(
            canvas,
            title,
            r.x + (r.width - tw) / 2,
            r.y + (r.height - st_font.cap_height) / 2,
            r.width - 8,
            &st_font,
            fg,
        );
    }

    let probe_hov = hit_rect(mouse.x, mouse.y, shell.probe_btn);
    draw_pill_button(
        canvas,
        shell.probe_btn,
        sel(state, "Donanim Raporu", "Hardware Info"),
        probe_hov,
        false,
        true,
    );
}

fn draw_left_showcase(canvas: &mut Canvas<'_>, pane: Rect, platform: &str, state: &SetupState) {
    draw_panel(canvas, pane, 14, SUBTLE_FILL, 255, CARD_BORDER, 255);

    let cx = pane.x + pane.width / 2;
    let art_cy = pane.y + 78;
    let orb_col = if state.phase == StepPhase::Failed {
        rgb(224, 68, 62)
    } else {
        AZURE_BRIGHT
    };
    draw_dot(canvas, cx, art_cy, 38, TINT_SELECTED, 255);
    draw_dot(canvas, cx, art_cy, 26, orb_col, 255);
    draw_dot(canvas, cx, art_cy, 10, rgb(255, 255, 255), 255);

    let title_st = text_style(13, 15);
    let body_st = text_style(9, 14);
    let cap_st = text_style(9, 16);

    let (headline, line1, line2) = match state.phase {
        StepPhase::Welcome => (
            sel(state, "Temiz Baslangic", "Clean Beginning"),
            sel(state, "Sade, hizli ve moduler", "Minimal, fast and modular"),
            sel(state, "yeni nesil isletim sistemi.", "next-gen operating system."),
        ),
        StepPhase::Architecture => (
            sel(state, "Hibrit Cekirdek", "Hybrid Architecture"),
            sel(state, "Salt okunur sistem agaci", "Read-only system tree and"),
            sel(state, "ve kalici kisisel depolama.", "persistent personal disk."),
        ),
        StepPhase::Storage => (
            sel(state, "Gercek Disk Semasi", "Real Disk Layout"),
            sel(state, "Donanim kanali dogrulanmis", "Hardware-verified MBR and"),
            sel(state, "MBR ve dosya sistemi plani.", "filesystem partition plan."),
        ),
        StepPhase::Installing => (
            sel(state, "Diske Yaziliyor", "Writing to Disk"),
            sel(state, "Sektorler ve sistem dosyalari", "Sectors and system packages"),
            sel(state, "sabit diske aktariliyor.", "are being written to disk."),
        ),
        StepPhase::Completed => (
            sel(state, "NetizenCore Hazir", "NetizenCore Ready"),
            sel(state, "Sabit disk onyukleme kaydi", "Hard disk boot record has"),
            sel(state, "dogrulandi ve etkinlestirildi.", "been verified and activated."),
        ),
        StepPhase::Failed => (
            sel(state, "Kurulum Durduruldu", "Installation Halted"),
            sel(state, "Veri guvenligi icin islem", "Operation stopped to protect"),
            sel(state, "guvenli sekilde iptal edildi.", "disk data integrity."),
        ),
    };

    let max_w = pane.width - 20;
    let hw = text_width(headline, &title_st).min(max_w);
    draw_clipped(canvas, headline, cx - hw / 2, pane.y + 136, max_w, &title_st, INK_DARK);

    let l1w = text_width(line1, &body_st).min(max_w);
    draw_clipped(canvas, line1, cx - l1w / 2, pane.y + 160, max_w, &body_st, INK_BODY);

    let l2w = text_width(line2, &body_st).min(max_w);
    draw_clipped(canvas, line2, cx - l2w / 2, pane.y + 176, max_w, &body_st, INK_BODY);

    let spec_card = Rect::new(pane.x + 12, pane.y + pane.height - 130, pane.width - 24, 118);
    draw_panel(canvas, spec_card, 10, CARD_WHITE, 255, CARD_BORDER, 255);

    draw_clipped(
        canvas,
        sel(state, "SISTEM OZETI", "SYSTEM SUMMARY"),
        spec_card.x + 12,
        spec_card.y + 12,
        spec_card.width - 24,
        &cap_st,
        AZURE_DEEP,
    );

    let disk_model = if state.selected_disk < state.disk_count {
        state.disks[state.selected_disk].model_str()
    } else {
        sel(state, "Disk Yok", "No Disk")
    };

    let rows = [
        ("Platform:", platform),
        (sel(state, "Dil:", "Language:"), if state.lang_index == 0 { "Turkce" } else { "English" }),
        (sel(state, "Klavye:", "Keyboard:"), if state.kbd_index == 0 { "TR-Q" } else { "US" }),
        (sel(state, "Hedef Disk:", "Target Disk:"), disk_model),
    ];

    for (i, (k, v)) in rows.iter().enumerate() {
        let ry = spec_card.y + 34 + (i as i32) * 20;
        draw_text_color(canvas, k, spec_card.x + 12, ry, &body_st, INK_MUTED);
        draw_clipped(
            canvas,
            v,
            spec_card.x + 92,
            ry,
            spec_card.width - 102,
            &body_st,
            INK_DARK,
        );
    }
}

fn draw_step_welcome(canvas: &mut Canvas<'_>, pane: Rect, state: &SetupState, mouse: &MouseState) {
    let h_st = text_style(15, 15);
    let sub_st = text_style(9, 14);
    draw_clipped(
        canvas,
        sel(state, "Dil, Bolge ve Klavye Tercihleri", "Language, Region & Keyboard Preferences"),
        pane.x + 14,
        pane.y + 8,
        pane.width - 28,
        &h_st,
        INK_DARK,
    );
    draw_clipped(
        canvas,
        sel(
            state,
            "Sectiginiz dil kurulum sihirbazina ve masaustune aninda uygulanir.",
            "Your selected language applies immediately to setup and desktop.",
        ),
        pane.x + 14,
        pane.y + 30,
        pane.width - 28,
        &sub_st,
        INK_MUTED,
    );

    let title_st = text_style(11, 14);
    let desc_st = text_style(9, 14);

    for (idx, (lang_name, lang_desc)) in LANGUAGES.iter().enumerate() {
        let card = welcome_lang_card(pane, idx);
        let selected = state.lang_index == idx;
        let hovered = hit_rect(mouse.x, mouse.y, card);
        let (fill, brd) = if selected {
            (TINT_SELECTED, AZURE_BRIGHT)
        } else if hovered {
            (SUBTLE_HOVER, AZURE_BRIGHT)
        } else {
            (CARD_WHITE, CARD_BORDER)
        };
        draw_panel(canvas, card, 10, fill, 255, brd, 255);
        draw_radio_dot(canvas, card.x + 20, card.y + card.height / 2, selected);
        draw_clipped(canvas, lang_name, card.x + 38, card.y + 9, card.width - 50, &title_st, INK_DARK);
        draw_clipped(canvas, lang_desc, card.x + 38, card.y + 26, card.width - 50, &desc_st, INK_MUTED);
    }

    let tz_row = welcome_selector_row(pane, 0);
    let kb_row = welcome_selector_row(pane, 1);

    let tz_val = if state.lang_index == 0 {
        TIMEZONES_TR[state.tz_index]
    } else {
        TIMEZONES_EN[state.tz_index]
    };
    let (kb_name, kb_preview) = if state.lang_index == 0 {
        KEYBOARDS_TR[state.kbd_index]
    } else {
        KEYBOARDS_EN[state.kbd_index]
    };

    for (row_rect, label, val) in [
        (tz_row, sel(state, "Saat Dilimi:", "Time Zone:"), tz_val),
        (kb_row, sel(state, "Klavye Duzeni:", "Keyboard:"), kb_name),
    ] {
        let hov = hit_rect(mouse.x, mouse.y, row_rect);
        let brd = if hov { AQUA_MINT } else { CARD_BORDER };
        draw_panel(canvas, row_rect, 9, CARD_WHITE, 255, brd, 255);
        draw_text_color(canvas, label, row_rect.x + 14, row_rect.y + 13, &sub_st, INK_MUTED);

        let badge = Rect::new(row_rect.x + row_rect.width - 78, row_rect.y + 7, 68, 24);
        draw_clipped(
            canvas,
            val,
            row_rect.x + 110,
            row_rect.y + 12,
            badge.x - (row_rect.x + 116),
            &title_st,
            INK_DARK,
        );

        draw_panel(canvas, badge, 12, TINT_SELECTED, 255, AQUA_MINT, 255);
        let chg_txt = sel(state, "Degistir", "Change");
        let cw = text_width(chg_txt, &desc_st);
        draw_text_color(
            canvas,
            chg_txt,
            badge.x + (badge.width - cw) / 2,
            badge.y + (badge.height - desc_st.cap_height) / 2,
            &desc_st,
            AQUA_DEEP,
        );
    }

    let preview_bar = Rect::new(pane.x + 14, kb_row.y + 48, pane.width - 28, 32);
    draw_panel(canvas, preview_bar, 8, SUBTLE_FILL, 255, CARD_BORDER, 255);
    draw_text_color(
        canvas,
        sel(state, "Tus Dizilimi:", "Key Preview:"),
        preview_bar.x + 12,
        preview_bar.y + 11,
        &desc_st,
        AZURE_DEEP,
    );
    draw_clipped(
        canvas,
        kb_preview,
        preview_bar.x + 102,
        preview_bar.y + 11,
        preview_bar.width - 114,
        &desc_st,
        INK_BODY,
    );
}

fn draw_step_architecture(canvas: &mut Canvas<'_>, pane: Rect, state: &SetupState, mouse: &MouseState) {
    let h_st = text_style(15, 15);
    let sub_st = text_style(9, 14);
    draw_clipped(
        canvas,
        sel(state, "Sistem Mimarisi ve Surucu Profili", "System Architecture & Driver Profile"),
        pane.x + 14,
        pane.y + 8,
        pane.width - 28,
        &h_st,
        INK_DARK,
    );
    draw_clipped(
        canvas,
        sel(
            state,
            "Dosya sistemi yapisini inceleyin ve calisma profilini secin.",
            "Review the partition layout and select the hardware profile.",
        ),
        pane.x + 14,
        pane.y + 30,
        pane.width - 28,
        &sub_st,
        INK_MUTED,
    );

    let doc_r = arch_doc_rect(pane);
    draw_panel(canvas, doc_r, 10, CARD_WHITE, 255, CARD_BORDER, 255);

    let notes = if state.lang_index == 0 { &ARCH_NOTES_TR } else { &ARCH_NOTES_EN };
    let line_st = text_style(9, 14);
    let visible = 7usize;
    for i in 0..visible {
        let idx = state.doc_scroll + i;
        if idx < notes.len() {
            let ly = doc_r.y + 12 + i as i32 * 18;
            let is_header = idx == 0 || idx == 2 || idx == 6 || idx == 12;
            let color = if is_header { AZURE_DEEP } else { INK_BODY };
            draw_clipped(canvas, notes[idx], doc_r.x + 14, ly, doc_r.width - 42, &line_st, color);
        }
    }

    let title_st = text_style(10, 14);
    let desc_st = text_style(8, 14);
    let modes = if state.lang_index == 0 {
        [
            ("Standart Evrensel Profil", "QEMU, VBox ve PC tam uyum"),
            ("Maksimum Performans", "PAT Write-Combining + 64-Bit"),
        ]
    } else {
        [
            ("Universal Standard Profile", "QEMU, VBox and PC compatibility"),
            ("Maximum Performance", "PAT Write-Combining + 64-Bit"),
        ]
    };

    for (idx, (m_title, m_desc)) in modes.iter().enumerate() {
        let mr = arch_mode_card(pane, idx);
        let sel_mode = state.profile_mode == idx;
        let hov = hit_rect(mouse.x, mouse.y, mr);
        let (fill, brd) = if sel_mode {
            (TINT_SELECTED, AQUA_MINT)
        } else if hov {
            (SUBTLE_HOVER, AZURE_BRIGHT)
        } else {
            (CARD_WHITE, CARD_BORDER)
        };
        draw_panel(canvas, mr, 10, fill, 255, brd, 255);
        draw_radio_dot(canvas, mr.x + 16, mr.y + mr.height / 2, sel_mode);
        draw_clipped(canvas, m_title, mr.x + 32, mr.y + 10, mr.width - 40, &title_st, INK_DARK);
        draw_clipped(canvas, m_desc, mr.x + 32, mr.y + 27, mr.width - 40, &desc_st, INK_MUTED);
    }

    let consent = arch_consent_rect(pane);
    let c_hov = hit_rect(mouse.x, mouse.y, consent);
    let c_brd = if state.terms_accepted {
        AQUA_MINT
    } else if c_hov {
        AZURE_BRIGHT
    } else {
        CARD_BORDER
    };
    draw_panel(canvas, consent, 9, CARD_WHITE, 255, c_brd, 255);
    draw_switch(canvas, consent.x + 12, consent.y + 9, state.terms_accepted);
    draw_clipped(
        canvas,
        sel(
            state,
            "Mimari yapilandirmayi ve disk bolumleme islemini onayliyorum",
            "I accept the architecture configuration and disk partitioning",
        ),
        consent.x + 58,
        consent.y + 13,
        consent.width - 68,
        &line_st,
        INK_DARK,
    );
}

fn draw_step_storage(canvas: &mut Canvas<'_>, pane: Rect, state: &SetupState, mouse: &MouseState) {
    let h_st = text_style(15, 15);
    let sub_st = text_style(9, 14);
    draw_clipped(
        canvas,
        sel(state, "Hedef Disk ve MBR Bolumleme Tablosu", "Target Disk & MBR Partition Table"),
        pane.x + 14,
        pane.y + 8,
        pane.width - 28,
        &h_st,
        INK_DARK,
    );

    // Secili fiziksel diskin kanalini, modelini ve gercek kapasitesini alt baslikta goster
    if state.selected_disk < state.disk_count {
        let d = &state.disks[state.selected_disk];
        let ch_label = match d.channel {
            0 => "ATA0-Master",
            1 => "ATA0-Slave",
            2 => "ATA1-Master",
            3 => "ATA1-Slave",
            _ => "ATA-Bilinmeyen",
        };
        draw_clipped(
            canvas,
            ch_label,
            pane.x + 14,
            pane.y + 30,
            90,
            &sub_st,
            AZURE_DEEP,
        );
        draw_clipped(
            canvas,
            d.model_str(),
            pane.x + 106,
            pane.y + 30,
            pane.width - 210,
            &sub_st,
            INK_DARK,
        );
        let mut d_cap = [0u8; 24];
        let d_len = format_capacity(d.size_mb(), &mut d_cap);
        if let Ok(cs) = core::str::from_utf8(&d_cap[..d_len]) {
            let cw = text_width(cs, &sub_st);
            draw_text_color(canvas, cs, pane.x + pane.width - cw - 16, pane.y + 30, &sub_st, AQUA_DEEP);
        }
    } else {
        draw_clipped(
            canvas,
            sel(state, "Hicbir depolama aygiti algilanamadi.", "No storage device detected."),
            pane.x + 14,
            pane.y + 30,
            pane.width - 28,
            &sub_st,
            rgb(220, 62, 58),
        );
    }

    let row_st = text_style(9, 14);
    let small_st = text_style(8, 14);

    for i in 0..state.volume_count.min(5) {
        let vol = &state.volumes[i];
        let r = storage_row_rect(pane, i);
        let selected = state.selected_volume == i;
        let hov = hit_rect(mouse.x, mouse.y, r);
        let (fill, brd) = if selected {
            (TINT_SELECTED, AZURE_BRIGHT)
        } else if hov {
            (SUBTLE_HOVER, AZURE_BRIGHT)
        } else {
            (CARD_WHITE, CARD_BORDER)
        };
        draw_panel(canvas, r, 8, fill, 255, brd, 255);

        let badge_col = match vol.role {
            VolumeRole::Unallocated => INK_MUTED,
            VolumeRole::BootCore => AZURE_DEEP,
            VolumeRole::SystemErofs => AQUA_MINT,
            VolumeRole::SecurityVault => rgb(98, 112, 228),
            VolumeRole::DataExfat => AZURE_BRIGHT,
            VolumeRole::OpticalIso => rgb(218, 146, 36),
        };
        draw_dot(canvas, r.x + 14, r.y + r.height / 2, 4, badge_col, 255);

        let (title, fs_tag) = match vol.role {
            VolumeRole::Unallocated => {
                if !state.layout_prepared && vol.disk_index < state.disk_count {
                    (
                        state.disks[vol.disk_index].model_str(),
                        sel(state, "Fiziksel Disk (Ham)", "Physical Disk (Raw)"),
                    )
                } else {
                    (
                        sel(state, "Ayrilmamis Disk Alani (Yetersiz < 3.4 GB)", "Unallocated (< 3.4 GB Min)"),
                        "RAW",
                    )
                }
            }
            VolumeRole::BootCore => (
                sel(state, "Bolum 1: ESP & Onyukleme (/boot)", "Partition 1: ESP & Boot (/boot)"),
                "0xEF FAT16 (LBA 65536)",
            ),
            VolumeRole::SystemErofs => (
                sel(state, "Bolum 2: Sistem Agaci (/system)", "Partition 2: System (/system)"),
                "0x83 EROFS (LBA 133120)",
            ),
            VolumeRole::SecurityVault => (
                sel(state, "Bolum 3: Ayrilmis Alan (/security)", "Partition 3: Reserved (/security)"),
                "0x83 RAW (LBA 4327424)",
            ),
            VolumeRole::DataExfat => (
                sel(state, "Bolum 4: Kisisel Disk (/data)", "Partition 4: Personal (/data)"),
                "0x83 exFAT (LBA 4851712)",
            ),
            VolumeRole::OpticalIso => (
                sel(state, "Optik Kurulum Medyasi (Salt Okunur)", "Optical Media (Read-Only)"),
                "ATAPI ISO9660",
            ),
        };

        draw_clipped(canvas, title, r.x + 26, r.y + 10, 215, &row_st, INK_DARK);
        draw_clipped(canvas, fs_tag, r.x + 245, r.y + 11, 130, &small_st, badge_col);

        let mut cap_buf = [0u8; 24];
        let cap_len = format_capacity(vol.size_mb, &mut cap_buf);
        if let Ok(cap_str) = core::str::from_utf8(&cap_buf[..cap_len]) {
            let cw = text_width(cap_str, &row_st);
            draw_text_color(canvas, cap_str, r.x + r.width - cw - 12, r.y + 10, &row_st, INK_BODY);
        }
    }

    let actions = if state.lang_index == 0 {
        ["Disk Degistir", "Ham Diskler", "Otomatik Bolumle", "Donanim Detayi"]
    } else {
        ["Switch Disk", "Raw Disks", "Auto Partition", "Disk Details"]
    };
    for (idx, act_label) in actions.iter().enumerate() {
        let ar = storage_action_pill(pane, idx);
        let hov = hit_rect(mouse.x, mouse.y, ar);
        draw_pill_button(canvas, ar, act_label, hov, false, true);
    }

    let bar_rect = Rect::new(pane.x + 14, pane.y + 274, pane.width - 28, 34);
    draw_panel(canvas, bar_rect, 8, CARD_WHITE, 255, CARD_BORDER, 255);

    let seg_y = bar_rect.y + 6;
    let total_w = bar_rect.width - 20;
    let sx = bar_rect.x + 10;

    let eligible = state.is_disk_eligible(state.selected_disk);
    if state.layout_prepared && eligible {
        let w_boot = (total_w * 6 / 100).max(12);
        let w_sys = (total_w * 32 / 100).max(40);
        let w_sec = (total_w * 10 / 100).max(20);
        let w_dat = (total_w - w_boot - w_sys - w_sec).max(20);

        canvas.fill_rect(sx, seg_y, w_boot - 2, 7, AZURE_DEEP);
        canvas.fill_rect(sx + w_boot, seg_y, w_sys - 2, 7, AQUA_MINT);
        canvas.fill_rect(sx + w_boot + w_sys, seg_y, w_sec - 2, 7, rgb(110, 124, 232));
        canvas.fill_rect(sx + w_boot + w_sys + w_sec, seg_y, w_dat, 7, AZURE_BRIGHT);
    } else {
        let warn_col = if eligible { rgb(186, 198, 210) } else { rgb(232, 142, 138) };
        canvas.fill_rect(sx, seg_y, total_w, 7, warn_col);
    }

    let (status_note, note_col) = if state.disk_count == 0 {
        (
            sel(state, "Kurulum yapilacak sabit disk bulunamadi.", "No hard disk found for installation."),
            rgb(220, 62, 58),
        )
    } else if state.disks[state.selected_disk].device_type == DeviceType::OpticalDisc {
        (
            sel(
                state,
                "Optik surucuye kurulum yapilamaz; 'Disk Degistir' ile sabit disk secin.",
                "Cannot install to optical drive; click 'Switch Disk' to select HDD/SSD.",
            ),
            rgb(220, 62, 58),
        )
    } else if !eligible {
        (
            sel(
                state,
                "Secili disk cok kucuk! NetizenCore bolum semasi en az 3.4 GB sabit disk gerektirir.",
                "Selected disk is too small! NetizenCore requires at least 3.4 GB hard disk.",
            ),
            rgb(220, 62, 58),
        )
    } else {
        (
            sel(
                state,
                "Donanim dogrulandi: MBR + ESP + /system (2 GB) + /security (256 MB) + /data (exFAT).",
                "Hardware verified: MBR + ESP + /system (2 GB) + /security (256 MB) + /data (exFAT).",
            ),
            AQUA_DEEP,
        )
    };
    draw_clipped(canvas, status_note, sx, bar_rect.y + 19, total_w, &small_st, note_col);
}

fn draw_step_installing(canvas: &mut Canvas<'_>, pane: Rect, state: &SetupState) {
    let h_st = text_style(15, 15);
    let sub_st = text_style(9, 14);
    draw_clipped(
        canvas,
        sel(state, "NetizenCore Diske Kuruluyor...", "Installing NetizenCore to Disk..."),
        pane.x + 14,
        pane.y + 8,
        pane.width - 28,
        &h_st,
        INK_DARK,
    );
    draw_clipped(
        canvas,
        sel(
            state,
            "Lutfen bekleyin, bolum tablosu ve sistem goruntuleri dogrulanarak yaziliyor.",
            "Please wait while partitions and system images are written and verified.",
        ),
        pane.x + 14,
        pane.y + 30,
        pane.width - 28,
        &sub_st,
        INK_MUTED,
    );

    let pct = (state.progress * 100 / 256).min(100);
    let prog_box = Rect::new(pane.x + 14, pane.y + 58, pane.width - 28, 52);
    draw_panel(canvas, prog_box, 10, CARD_WHITE, 255, CARD_BORDER, 255);

    let pct_st = text_style(14, 15);
    let mut p_buf = [0u8; 12];
    p_buf[0] = b'%';
    let plen = 1 + format_number(pct as u64, &mut p_buf[1..]);
    if let Ok(ps) = core::str::from_utf8(&p_buf[..plen]) {
        draw_text_color(canvas, ps, prog_box.x + 16, prog_box.y + 10, &pct_st, AZURE_DEEP);
    }

    let track = Rect::new(prog_box.x + 16, prog_box.y + 31, prog_box.width - 32, 10);
    draw_panel(canvas, track, 5, SUBTLE_FILL, 255, CARD_BORDER, 255);
    let fill_w = (track.width as u32 * state.progress / 256) as i32;
    if fill_w > 6 {
        let fill_r = Rect::new(track.x, track.y, fill_w, track.height);
        draw_panel(canvas, fill_r, 5, AZURE_BRIGHT, 255, AQUA_MINT, 0);
    }

    let current_stage = match pct {
        0..=19 => 0,
        20..=44 => 1,
        45..=72 => 2,
        73..=91 => 3,
        _ => 4,
    };

    let milestones = if state.lang_index == 0 {
        [
            "Hedef disk kapasitesi ve LBA sinirlari dogrulaniyor",
            "Stage2, 64-bit Cekirdek ve UEFI ESP (FAT16) yaziliyor",
            "Salt okunur sistem agaci (/system - EROFS) olusturuluyor",
            "Kisisel disk (/data - exFAT) ve dil ayarlari yaziliyor",
            "MBR yaziliyor, onbellek (flush) ve sektorler dogrulaniyor",
        ]
    } else {
        [
            "Verifying target disk capacity and LBA bounds",
            "Writing Stage2, 64-bit Kernel and UEFI ESP (FAT16)",
            "Building read-only system tree (/system - EROFS)",
            "Formatting personal disk (/data - exFAT) & locale",
            "Committing MBR, flushing cache and verifying sectors",
        ]
    };

    let item_st = text_style(9, 14);
    for (idx, text) in milestones.iter().enumerate() {
        let iy = pane.y + 128 + idx as i32 * 30;
        let cx = pane.x + 26;
        let cy = iy + 5;
        let (dot_col, txt_col) = if idx < current_stage {
            (AQUA_MINT, INK_BODY)
        } else if idx == current_stage {
            (AZURE_BRIGHT, INK_DARK)
        } else {
            (CARD_BORDER, INK_LIGHT)
        };
        draw_dot(canvas, cx, cy, 6, dot_col, 255);
        draw_clipped(canvas, text, pane.x + 42, iy, pane.width - 56, &item_st, txt_col);
    }
}

fn draw_step_completed(canvas: &mut Canvas<'_>, pane: Rect, state: &SetupState) {
    let h_st = text_style(15, 15);
    let sub_st = text_style(9, 14);
    draw_clipped(
        canvas,
        sel(state, "Kurulum Basariyla Tamamlandi!", "Installation Completed Successfully!"),
        pane.x + 14,
        pane.y + 8,
        pane.width - 28,
        &h_st,
        INK_DARK,
    );
    draw_clipped(
        canvas,
        sel(
            state,
            "NetizenCore diskinize kuruldu, MBR dogrulandi ve baslatilmaya hazir.",
            "NetizenCore is installed, MBR verified and ready to boot.",
        ),
        pane.x + 14,
        pane.y + 30,
        pane.width - 28,
        &sub_st,
        INK_MUTED,
    );

    let card = Rect::new(pane.x + 14, pane.y + 66, pane.width - 28, 170);
    draw_panel(canvas, card, 12, CARD_WHITE, 255, AQUA_MINT, 255);

    let orb_cx = card.x + card.width / 2;
    let orb_cy = card.y + 48;
    draw_dot(canvas, orb_cx, orb_cy, 24, AQUA_MINT, 255);
    draw_dot(canvas, orb_cx, orb_cy, 8, rgb(255, 255, 255), 255);

    let secs = (10u32).saturating_sub(state.reboot_ticks / 60);
    let mut msg = [0u8; 64];
    let pfx = if state.lang_index == 0 {
        b"Sistem " as &[u8]
    } else {
        b"Restarting in " as &[u8]
    };
    msg[..pfx.len()].copy_from_slice(pfx);
    let mut len = pfx.len();
    len += format_number(secs as u64, &mut msg[len..]);
    let sfx = if state.lang_index == 0 {
        b" saniye icinde yeniden baslatilacak..." as &[u8]
    } else {
        b" seconds..." as &[u8]
    };
    msg[len..len + sfx.len()].copy_from_slice(sfx);
    len += sfx.len();

    let mid_st = text_style(11, 14);
    if let Ok(s) = core::str::from_utf8(&msg[..len]) {
        let sw = text_width(s, &mid_st);
        draw_text_color(canvas, s, orb_cx - sw / 2, card.y + 92, &mid_st, INK_DARK);
    }

    let bar = Rect::new(card.x + 36, card.y + 124, card.width - 72, 10);
    draw_panel(canvas, bar, 5, SUBTLE_FILL, 255, CARD_BORDER, 255);
    let fill_w = (bar.width as u32 * state.reboot_ticks.min(600) / 600) as i32;
    if fill_w > 6 {
        let fr = Rect::new(bar.x, bar.y, fill_w, bar.height);
        draw_panel(canvas, fr, 5, AZURE_BRIGHT, 255, AQUA_MINT, 0);
    }
}

fn draw_step_failed(canvas: &mut Canvas<'_>, pane: Rect, state: &SetupState) {
    let h_st = text_style(15, 15);
    let sub_st = text_style(9, 14);
    draw_clipped(
        canvas,
        sel(state, "Kurulum Basarisiz Oldu", "Installation Failed"),
        pane.x + 14,
        pane.y + 8,
        pane.width - 28,
        &h_st,
        rgb(220, 62, 58),
    );
    draw_clipped(
        canvas,
        sel(
            state,
            "Hedef disk uzerinde islem tamamlanamadi. Lutfen asagidaki hatayi inceleyin:",
            "Could not complete installation on target disk. Please review the error below:",
        ),
        pane.x + 14,
        pane.y + 30,
        pane.width - 28,
        &sub_st,
        INK_MUTED,
    );

    let card = Rect::new(pane.x + 14, pane.y + 66, pane.width - 28, 170);
    draw_panel(canvas, card, 12, CARD_WHITE, 255, rgb(220, 62, 58), 255);

    let orb_cx = card.x + card.width / 2;
    let orb_cy = card.y + 48;
    draw_dot(canvas, orb_cx, orb_cy, 22, rgb(220, 62, 58), 255);
    draw_dot(canvas, orb_cx, orb_cy, 7, rgb(255, 255, 255), 255);

    let err_txt = state.install_error.unwrap_or("Disk G/C veya dogrulama hatasi olustu.");
    let mid_st = text_style(10, 14);
    let sw = text_width(err_txt, &mid_st).min(card.width - 28);
    draw_clipped(
        canvas,
        err_txt,
        orb_cx - sw / 2,
        card.y + 92,
        card.width - 28,
        &mid_st,
        INK_DARK,
    );

    let hint = sel(
        state,
        "Depolama adimina donup baska bir fiziksel disk secebilir veya tekrar deneyebilirsiniz.",
        "Return to Storage step to select another physical disk or retry.",
    );
    let hw = text_width(hint, &sub_st).min(card.width - 28);
    draw_clipped(
        canvas,
        hint,
        orb_cx - hw / 2,
        card.y + 122,
        card.width - 28,
        &sub_st,
        INK_MUTED,
    );
}

fn draw_hardware_inspector(
    canvas: &mut Canvas<'_>,
    width: i32,
    height: i32,
    platform: &str,
    state: &SetupState,
    mouse: &MouseState,
) {
    let modal = inspector_rect(width, height);
    draw_surface_card(canvas, modal, 14, 32);

    let h_st = text_style(13, 15);
    let sub_st = text_style(9, 14);
    draw_clipped(
        canvas,
        sel(state, "Donanim ve Fiziksel Kanal Raporu", "Hardware & Physical Channel Report"),
        modal.x + 20,
        modal.y + 16,
        modal.width - 40,
        &h_st,
        INK_DARK,
    );

    let env_label = nc_driver::global().environment.label();
    draw_clipped(canvas, env_label, modal.x + 20, modal.y + 38, modal.width - 40, &sub_st, AZURE_DEEP);

    let list_box = Rect::new(modal.x + 20, modal.y + 60, modal.width - 40, modal.height - 114);
    draw_panel(canvas, list_box, 10, SUBTLE_FILL, 255, CARD_BORDER, 255);

    draw_text_color(canvas, "Platform:", list_box.x + 14, list_box.y + 12, &sub_st, INK_MUTED);
    draw_text_color(canvas, platform, list_box.x + 90, list_box.y + 12, &sub_st, AZURE_DEEP);

    for i in 0..state.disk_count.min(4) {
        let d = &state.disks[i];
        let ry = list_box.y + 34 + i as i32 * 28;
        let row_r = Rect::new(list_box.x + 12, ry, list_box.width - 24, 24);
        draw_panel(canvas, row_r, 6, CARD_WHITE, 255, CARD_BORDER, 255);
        let eligible = state.is_disk_eligible(i);
        let dot_c = if eligible { AQUA_MINT } else { rgb(218, 146, 36) };
        draw_dot(canvas, row_r.x + 12, row_r.y + 12, 4, dot_c, 255);

        let ch_tag = match d.channel {
            0 => "CH0",
            1 => "CH1",
            2 => "CH2",
            3 => "CH3",
            _ => "DEV",
        };
        draw_text_color(canvas, ch_tag, row_r.x + 24, row_r.y + 7, &sub_st, AZURE_DEEP);
        draw_text_color(canvas, d.type_label(), row_r.x + 60, row_r.y + 7, &sub_st, AZURE_BRIGHT);
        draw_clipped(canvas, d.model_str(), row_r.x + 98, row_r.y + 7, row_r.width - 190, &sub_st, INK_DARK);

        let mut cbuf = [0u8; 24];
        let clen = format_capacity(d.size_mb(), &mut cbuf);
        if let Ok(cs) = core::str::from_utf8(&cbuf[..clen]) {
            let cw = text_width(cs, &sub_st);
            draw_text_color(canvas, cs, row_r.x + row_r.width - cw - 10, row_r.y + 7, &sub_st, INK_BODY);
        }
    }

    let close_btn = Rect::new(modal.x + modal.width - 116, modal.y + modal.height - 42, 100, 30);
    let hov = hit_rect(mouse.x, mouse.y, close_btn);
    draw_pill_button(canvas, close_btn, sel(state, "Kapat", "Close"), hov, true, true);
}

pub fn draw(canvas: &mut Canvas<'_>, platform: &str, state: &mut SetupState, mouse: &MouseState) -> bool {
    let width = canvas.width() as i32;
    let height = canvas.height() as i32;
    let sig = compute_render_signature(width, height, state, mouse);
    if !state.changed && LAST_RENDER_HASH.load(Ordering::Relaxed) == sig {
        return false;
    }
    state.changed = false;
    LAST_RENDER_HASH.store(sig, Ordering::Relaxed);

    let shell = compute_shell(width, height);

    draw_clean_backdrop(canvas);
    draw_top_navigation_pill(canvas, &shell, state, mouse);

    draw_surface_card(canvas, shell.main_card, 16, 16);
    draw_left_showcase(canvas, shell.left_pane, platform, state);

    match state.phase {
        StepPhase::Welcome => draw_step_welcome(canvas, shell.right_pane, state, mouse),
        StepPhase::Architecture => draw_step_architecture(canvas, shell.right_pane, state, mouse),
        StepPhase::Storage => draw_step_storage(canvas, shell.right_pane, state, mouse),
        StepPhase::Installing => draw_step_installing(canvas, shell.right_pane, state),
        StepPhase::Completed => draw_step_completed(canvas, shell.right_pane, state),
        StepPhase::Failed => draw_step_failed(canvas, shell.right_pane, state),
    }

    draw_surface_card(canvas, shell.bottom_bar, 14, 10);

    if state.phase != StepPhase::Installing {
        let p_hov = hit_rect(mouse.x, mouse.y, shell.power_btn);
        draw_pill_button(
            canvas,
            shell.power_btn,
            sel(state, "Yeniden Baslat", "Restart"),
            p_hov,
            false,
            true,
        );
    } else {
        let st = text_style(9, 14);
        draw_clipped(
            canvas,
            sel(
                state,
                "Disk yazma islemi surerken sistemi kapatmayin...",
                "Do not power off while disk installation is in progress...",
            ),
            shell.bottom_bar.x + 18,
            shell.bottom_bar.y + 20,
            shell.bottom_bar.width - 36,
            &st,
            INK_MUTED,
        );
    }

    if state.phase == StepPhase::Architecture
        || state.phase == StepPhase::Storage
        || state.phase == StepPhase::Failed
    {
        let b_hov = hit_rect(mouse.x, mouse.y, shell.back_btn);
        draw_pill_button(
            canvas,
            shell.back_btn,
            sel(state, "Geri", "Back"),
            b_hov,
            false,
            true,
        );
    }

    if state.phase != StepPhase::Installing {
        let next_label = match state.phase {
            StepPhase::Welcome => sel(state, "Devam Et", "Continue"),
            StepPhase::Architecture => sel(state, "Onayla ve Gec", "Accept & Next"),
            StepPhase::Storage => sel(state, "Kuruluma Basla", "Start Install"),
            StepPhase::Installing => "",
            StepPhase::Completed => sel(state, "Simdi Yeniden Baslat", "Restart Now"),
            StepPhase::Failed => sel(state, "Disk Secimine Don", "Back to Disks"),
        };
        let n_hov = hit_rect(mouse.x, mouse.y, shell.next_btn);
        draw_pill_button(
            canvas,
            shell.next_btn,
            next_label,
            n_hov,
            true,
            state.can_continue(),
        );
    }

    if state.inspector_open {
        draw_hardware_inspector(canvas, width, height, platform, state, mouse);
    }

    true
}
