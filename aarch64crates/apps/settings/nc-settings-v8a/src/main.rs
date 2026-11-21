//! Ayarlar (ARMv8-A / Ring 3 EL0): Tam kapsamli akilli telefon sistem ayarlari uygulamasi.
//!
//! Tum 11 kategori (Ag, Bagli Cihazlar, Ekran, Ses ve Titresim, Kilit Ekrani,
//! Guvenlik ve Gizlilik, Konum, Uygulamalar, Depolama, Gelistirici Secenekleri,
//! Sistem ve Cihaz Hakkinda) gercek cekirdek hizmetleri ve `nc-app-sdk` uzerinden calisir.
#![no_std]
#![no_main]

mod pages;

use nc_abi::event::{
    unpack_brightness, unpack_bt, unpack_dark_theme, unpack_wifi, EventBlock, OP_DRAW, OP_INIT,
    OP_PAUSE, OP_RESUME, OP_UPDATE, RET_CLOSE, RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_app_sdk::{apps, dispatch, perm, pointer, security, settings, storage, sys, Gfx, Line, RingApp};

pub const ACCENTS: [(u32, &str, &str); 5] = [
    (0x000A_59F7, "Mavi", "Blue"),
    (0x0007_C160, "Yeşil", "Green"),
    (0x00FA_7D00, "Turuncu", "Orange"),
    (0x007A_5AF8, "Mor", "Purple"),
    (0x00E1_1D48, "Mercan", "Coral"),
];

pub const MAX_APPS: usize = 24;
pub const WP_MAX: usize = 6;
pub const WP_NAME_MAX: usize = 40;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Network,
    Devices,
    Display,
    Sound,
    LockScreen,
    LockPinWizard,
    Security,
    Permissions,
    AppPermissions,
    Location,
    Apps,
    AppDetail,
    Storage,
    Developer,
    System,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PinWizardMode {
    SetPin,
    SetPassword,
    Disable,
}

pub struct SettingsApp {
    pub page: Page,
    pub main_scroll: usize,
    pub app_scroll: usize,
    pub selected_app: usize,
    pub confirm_clear: bool,
    pub confirm_uninstall: bool,
    pub last_flags: u32,

    // Cihaz & Sistem bilgisi
    pub device_name: [u8; 32],
    pub device_name_len: usize,
    pub device_model: [u8; 32],
    pub device_model_len: usize,
    pub soc: [u8; 32],
    pub soc_len: usize,
    pub build_number: [u8; 32],
    pub build_number_len: usize,
    pub kernel_ver: [u8; 36],
    pub kernel_ver_len: usize,
    pub sdk_ver: [u8; 28],
    pub sdk_ver_len: usize,
    pub screen: [u8; 24],
    pub screen_len: usize,
    pub net_mac: [u8; 20],
    pub net_mac_len: usize,
    pub net_dhcp: bool,
    pub net_ip: [u8; 20],
    pub net_ip_len: usize,
    pub net_gw: [u8; 20],
    pub net_gw_len: usize,
    pub net_dns: [u8; 20],
    pub net_dns_len: usize,
    pub wifi_radio: bool,
    pub wifi_scan_count: u32,

    pub ram_mib: u32,
    pub free_mib: u32,
    pub live_tasks: u32,
    pub disks: u32,
    pub sys_mib: u32,
    pub data_mib: u32,
    pub data_mounted: bool,
    pub data_writable: bool,
    pub used_kib: u32,
    pub apps_kib: u32,
    pub media_kib: u32,
    pub appdata_kib: u32,
    pub other_kib: u32,
    pub sys_used_kib: u32,

    // Donanim varligi (sahte anahtar yok!)
    pub wifi_hw: bool,
    pub bt_hw: bool,
    pub audio_hw: bool,
    pub vibrator_hw: bool,
    pub gps_hw: bool,

    // Kalici ayarlar
    pub accent: u32,
    pub wallpaper: u8,
    pub wp_names: [[u8; WP_NAME_MAX]; WP_MAX],
    pub wp_lens: [u8; WP_MAX],
    pub wp_count: usize,
    pub tz_hours: i32,
    pub lang: u32,
    pub wifi: bool,
    pub bt: bool,
    pub brightness: u8,
    pub dark_theme: bool,
    pub screen_timeout_s: u32,
    pub ui_scale: u8,
    pub dev_options: bool,
    pub anim_scale: u8,
    pub show_touch: bool,
    pub verbose_log: bool,
    pub build_taps: u8,

    // Kilit ekrani durumu & sihirbaz
    pub lock: security::LockStatus,
    pub wizard_mode: PinWizardMode,
    pub wizard_step: u8, // 0 = mevcut kimlik (eger kilit aciksa), 1 = yeni kimlik
    pub cur_secret: [u8; 32],
    pub cur_len: usize,
    pub new_secret: [u8; 32],
    pub new_len: usize,

    // USB durumu
    pub usb: storage::UsbStatus,

    // Dinamik uygulama kayit defteri
    pub apps: [apps::AppEntry; MAX_APPS],
    pub app_count: usize,

    // Calisma zamani izin kayitlari
    pub app_perms: [perm::AppPermSummary; MAX_APPS],
    pub perm_count: usize,
    pub perm_scroll: usize,
    pub app_perm_scroll: usize,
    pub selected_perm_app: usize,

    // Geri bildirim mesaji
    pub toast: [u8; 56],
    pub toast_len: usize,
}

static mut INFO_BUF: [u8; 4096] = [0; 4096];
static mut REG_BUF: [u8; 4096] = [0; 4096];
static mut PERM_BUF: [u8; 4096] = [0; 4096];
static mut COMMANDS: [u8; 16384] = [0; 16384];

pub fn copy_str(dst: &mut [u8], text: &str) -> usize {
    let bytes = text.as_bytes();
    let len = bytes.len().min(dst.len());
    dst[..len].copy_from_slice(&bytes[..len]);
    len
}

pub fn s_of(buf: &[u8], len: usize) -> &str {
    core::str::from_utf8(&buf[..len]).unwrap_or("")
}

pub fn put_size(line: &mut Line, kib: u32, comma: bool) {
    let sep = if comma { "," } else { "." };
    if kib >= 1024 * 1024 {
        let tenth = (kib as u64 * 10 / (1024 * 1024)) as u32;
        line.number(tenth / 10, 1).text(sep).number(tenth % 10, 1).text(" GB");
    } else if kib >= 1024 {
        let tenth = kib * 10 / 1024;
        line.number(tenth / 10, 1).text(sep).number(tenth % 10, 1).text(" MB");
    } else {
        line.number(kib, 1).text(" KB");
    }
}

fn parse_u32(text: &str) -> u32 {
    text.trim().parse::<u32>().unwrap_or(0)
}

impl SettingsApp {
    const fn new() -> Self {
        Self {
            page: Page::Home,
            main_scroll: 0,
            app_scroll: 0,
            selected_app: 0,
            confirm_clear: false,
            confirm_uninstall: false,
            last_flags: 0,
            device_name: [0; 32],
            device_name_len: 0,
            device_model: [0; 32],
            device_model_len: 0,
            soc: [0; 32],
            soc_len: 0,
            build_number: [0; 32],
            build_number_len: 0,
            kernel_ver: [0; 36],
            kernel_ver_len: 0,
            sdk_ver: [0; 28],
            sdk_ver_len: 0,
            screen: [0; 24],
            screen_len: 0,
            net_mac: [0; 20],
            net_mac_len: 0,
            net_dhcp: false,
            net_ip: [0; 20],
            net_ip_len: 0,
            net_gw: [0; 20],
            net_gw_len: 0,
            net_dns: [0; 20],
            net_dns_len: 0,
            wifi_radio: false,
            wifi_scan_count: 0,
            ram_mib: 4096,
            free_mib: 0,
            live_tasks: 0,
            disks: 0,
            sys_mib: 0,
            data_mib: 0,
            data_mounted: false,
            data_writable: false,
            used_kib: 0,
            apps_kib: 0,
            media_kib: 0,
            appdata_kib: 0,
            other_kib: 0,
            sys_used_kib: 0,
            wifi_hw: false,
            bt_hw: false,
            audio_hw: false,
            vibrator_hw: false,
            gps_hw: false,
            accent: 0x000A_59F7,
            wallpaper: 0,
            wp_names: [[0; WP_NAME_MAX]; WP_MAX],
            wp_lens: [0; WP_MAX],
            wp_count: 0,
            tz_hours: 3,
            lang: 0,
            wifi: true,
            bt: false,
            brightness: 100,
            dark_theme: false,
            screen_timeout_s: 60,
            ui_scale: 100,
            dev_options: false,
            anim_scale: 100,
            show_touch: false,
            verbose_log: false,
            build_taps: 0,
            lock: security::LockStatus {
                locked: false,
                enabled: false,
                kind: 0,
                fails: 0,
                lockout_s: 0,
                timeout_s: 60,
                lock_on_boot: true,
                show_info: true,
            },
            wizard_mode: PinWizardMode::SetPin,
            wizard_step: 0,
            cur_secret: [0; 32],
            cur_len: 0,
            new_secret: [0; 32],
            new_len: 0,
            usb: storage::UsbStatus::EMPTY,
            apps: [apps::AppEntry::EMPTY; MAX_APPS],
            app_count: 0,
            app_perms: [perm::AppPermSummary::EMPTY; MAX_APPS],
            perm_count: 0,
            perm_scroll: 0,
            app_perm_scroll: 0,
            selected_perm_app: 0,
            toast: [0; 56],
            toast_len: 0,
        }
    }

    pub fn set_toast(&mut self, msg: &str) {
        self.toast_len = copy_str(&mut self.toast, msg);
    }

    pub fn refresh(&mut self) {
        let buf = unsafe { &mut *core::ptr::addr_of_mut!(INFO_BUF) };
        if let Ok(text) = settings::device_info(buf) {
            self.wp_count = 0;
            for line in text.lines() {
                let Some((k, v)) = line.split_once('=') else { continue };
                match k {
                    "device_name" => self.device_name_len = copy_str(&mut self.device_name, v),
                    "device_model" => self.device_model_len = copy_str(&mut self.device_model, v),
                    "soc" => self.soc_len = copy_str(&mut self.soc, v),
                    "build_number" => self.build_number_len = copy_str(&mut self.build_number, v),
                    "kernel_version" => self.kernel_ver_len = copy_str(&mut self.kernel_ver, v),
                    "sdk_version" => self.sdk_ver_len = copy_str(&mut self.sdk_ver, v),
                    "screen" => self.screen_len = copy_str(&mut self.screen, v),
                    "net_mac" => self.net_mac_len = copy_str(&mut self.net_mac, v),
                    "net_dhcp" => self.net_dhcp = parse_u32(v) != 0,
                    "net_ip" => self.net_ip_len = copy_str(&mut self.net_ip, v),
                    "net_gw" => self.net_gw_len = copy_str(&mut self.net_gw, v),
                    "net_dns" => self.net_dns_len = copy_str(&mut self.net_dns, v),
                    "wifi_radio" => self.wifi_radio = parse_u32(v) != 0,
                    "wifi_scan_count" => self.wifi_scan_count = parse_u32(v),
                    "ram_mib" => self.ram_mib = parse_u32(v),
                    "free_mib" => self.free_mib = parse_u32(v),
                    "live_tasks" => self.live_tasks = parse_u32(v),
                    "disks" => self.disks = parse_u32(v),
                    "sys_mib" => self.sys_mib = parse_u32(v),
                    "data_mib" => self.data_mib = parse_u32(v),
                    "data_mounted" => self.data_mounted = parse_u32(v) != 0,
                    "data_writable" => self.data_writable = parse_u32(v) != 0,
                    "data_used_kib" => self.used_kib = parse_u32(v),
                    "data_apps_kib" => self.apps_kib = parse_u32(v),
                    "data_media_kib" => self.media_kib = parse_u32(v),
                    "data_appdata_kib" => self.appdata_kib = parse_u32(v),
                    "data_other_kib" => self.other_kib = parse_u32(v),
                    "sys_used_kib" => self.sys_used_kib = parse_u32(v),
                    "wifi_hw" => self.wifi_hw = parse_u32(v) != 0,
                    "bt_hw" => self.bt_hw = parse_u32(v) != 0,
                    "audio_hw" => self.audio_hw = parse_u32(v) != 0,
                    "vibrator_hw" => self.vibrator_hw = parse_u32(v) != 0,
                    "gps_hw" => self.gps_hw = parse_u32(v) != 0,
                    "accent" => {
                        if let Ok(val) = u32::from_str_radix(v.trim().trim_start_matches("0x"), 16) {
                            self.accent = val;
                        }
                    }
                    "wallpaper" => self.wallpaper = (parse_u32(v) as u8).min(5),
                    "wp_image" => {
                        if self.wp_count < WP_MAX {
                            let slot = self.wp_count;
                            self.wp_lens[slot] = copy_str(&mut self.wp_names[slot], v) as u8;
                            self.wp_count += 1;
                        }
                    }
                    "tz_hours" => self.tz_hours = v.trim().parse::<i32>().unwrap_or(3).clamp(-12, 14),
                    "lang" => self.lang = parse_u32(v).min(1),
                    "wifi" => self.wifi = parse_u32(v) != 0,
                    "bt" => self.bt = parse_u32(v) != 0,
                    "brightness" => self.brightness = (parse_u32(v) as u8).clamp(20, 100),
                    "dark_theme" => self.dark_theme = parse_u32(v) != 0,
                    "screen_timeout_s" => self.screen_timeout_s = parse_u32(v),
                    "ui_scale" => self.ui_scale = (parse_u32(v) as u8).clamp(85, 125),
                    "dev_options" => self.dev_options = parse_u32(v) != 0,
                    "anim_scale" => self.anim_scale = (parse_u32(v) as u8).min(200),
                    "show_touch" => self.show_touch = parse_u32(v) != 0,
                    "verbose_log" => self.verbose_log = parse_u32(v) != 0,
                    _ => {}
                }
            }
        }
        let rbuf = unsafe { &mut *core::ptr::addr_of_mut!(REG_BUF) };
        if let Ok((_, _, cnt)) = apps::query_registry(rbuf, &mut self.apps) {
            self.app_count = cnt;
        }
        let pbuf = unsafe { &mut *core::ptr::addr_of_mut!(PERM_BUF) };
        if let Ok(pcnt) = perm::list_all(pbuf, &mut self.app_perms) {
            self.perm_count = pcnt;
        }
        if let Some(st) = security::lock_status() {
            self.lock = st;
        }
        if let Some(u) = storage::usb_info() {
            self.usb = u;
        }
    }

    pub fn home_category_count(&self) -> usize {
        if self.dev_options { 11 } else { 10 }
    }

    pub fn home_category_page(&self, idx: usize) -> Page {
        match idx {
            0 => Page::Network,
            1 => Page::Devices,
            2 => Page::Display,
            3 => Page::Sound,
            4 => Page::LockScreen,
            5 => Page::Security,
            6 => Page::Location,
            7 => Page::Apps,
            8 => Page::Storage,
            9 => Page::System,
            _ => Page::Developer,
        }
    }

    pub fn start_pin_wizard(&mut self, mode: PinWizardMode) {
        self.wizard_mode = mode;
        self.cur_len = 0;
        self.new_len = 0;
        self.wizard_step = if self.lock.enabled { 0 } else { 1 };
        self.toast_len = 0;
        self.page = Page::LockPinWizard;
    }

    pub fn wizard_push(&mut self, b: u8, en: bool) {
        if self.wizard_step == 0 {
            if self.cur_len < self.cur_secret.len() {
                self.cur_secret[self.cur_len] = b;
                self.cur_len += 1;
            }
        } else if self.new_len < self.new_secret.len() {
            self.new_secret[self.new_len] = b;
            self.new_len += 1;
            if self.wizard_mode == PinWizardMode::SetPin && self.new_len == 4 {
                self.wizard_submit(en);
            }
        }
    }

    pub fn wizard_pop(&mut self) {
        if self.wizard_step == 0 {
            if self.cur_len > 0 {
                self.cur_len -= 1;
            }
        } else if self.new_len > 0 {
            self.new_len -= 1;
        }
    }

    pub fn wizard_submit(&mut self, en: bool) {
        if self.wizard_step == 0 {
            if self.wizard_mode == PinWizardMode::Disable {
                let mut cur = [0u8; 32];
                let clen = self.cur_len;
                cur[..clen].copy_from_slice(&self.cur_secret[..clen]);
                match security::configure_disable(&cur[..clen]) {
                    Ok(()) => {
                        self.refresh();
                        self.set_toast(if en { "Screen lock disabled" } else { "Ekran kilidi kapatildi" });
                        self.page = Page::LockScreen;
                    }
                    Err(_) => {
                        self.cur_len = 0;
                        self.set_toast(if en { "Wrong current credential" } else { "Mevcut sifre yanlis" });
                    }
                }
            } else {
                if self.cur_len == 0 {
                    return;
                }
                self.wizard_step = 1;
            }
            return;
        }

        let kind = if self.wizard_mode == PinWizardMode::SetPassword { 2 } else { 1 };
        let mut cur = [0u8; 32];
        let clen = self.cur_len;
        cur[..clen].copy_from_slice(&self.cur_secret[..clen]);
        let mut new_s = [0u8; 32];
        let nlen = self.new_len;
        new_s[..nlen].copy_from_slice(&self.new_secret[..nlen]);

        match security::configure_set(kind, &cur[..clen], &new_s[..nlen]) {
            Ok(()) => {
                self.refresh();
                self.set_toast(if en { "Lock credential saved" } else { "Kilit sifresi kaydedildi" });
                self.page = Page::LockScreen;
            }
            Err(_) => {
                self.new_len = 0;
                self.set_toast(if en { "Invalid format or wrong current PIN" } else { "Gecersiz bicim veya yanlis mevcut sifre" });
            }
        }
    }

    fn handle_key(&mut self, ev: &EventBlock) -> u32 {
        let k = ev.key;
        let en = ev.lang == 1;
        if k == key::NONE {
            return RET_NONE;
        }
        if k == key::ESCAPE {
            match self.page {
                Page::Home => return RET_CLOSE,
                Page::AppDetail => {
                    self.confirm_clear = false;
                    self.confirm_uninstall = false;
                    self.page = Page::Apps;
                }
                Page::AppPermissions => self.page = Page::Permissions,
                Page::Permissions => self.page = Page::Security,
                Page::LockPinWizard => self.page = Page::LockScreen,
                _ => self.page = Page::Home,
            }
            self.toast_len = 0;
            return RET_REDRAW;
        }
        if self.page == Page::LockPinWizard {
            if k == key::BACKSPACE {
                self.wizard_pop();
                return RET_REDRAW;
            }
            if k == key::ENTER {
                self.wizard_submit(en);
                return RET_REDRAW;
            }
            if let Some(c) = key::as_char(k) {
                if c.is_ascii() && !c.is_control() {
                    if self.wizard_step == 1 && self.wizard_mode == PinWizardMode::SetPin && !c.is_ascii_digit() {
                        return RET_NONE;
                    }
                    self.wizard_push(c as u8, en);
                    return RET_REDRAW;
                }
            }
        }
        RET_NONE
    }
}

impl RingApp for SettingsApp {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.refresh();
                let mut ibuf = [0u8; 64];
                if let Some(intent) = apps::take_intent(&mut ibuf) {
                    if intent == "permissions" {
                        self.page = Page::Permissions;
                        self.perm_scroll = 0;
                    }
                }
                self.last_flags = ev.flags;
                RET_REDRAW
            }
            OP_PAUSE => RET_NONE,
            OP_UPDATE => {
                let mut ret = self.handle_key(ev);
                if ret & RET_CLOSE != 0 {
                    return ret;
                }
                if ev.flags != self.last_flags {
                    self.last_flags = ev.flags;
                    self.wifi = unpack_wifi(ev.flags);
                    self.bt = unpack_bt(ev.flags);
                    self.brightness = unpack_brightness(ev.flags);
                    self.dark_theme = unpack_dark_theme(ev.flags);
                    ret |= RET_REDRAW;
                }
                if let Some(p) = pointer(ev) {
                    if p.clicked {
                        let dp = pages::settings_dp(ev);
                        ret |= self.handle_click(ev, &dp, p.x, p.y);
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

static mut APP: SettingsApp = SettingsApp::new();

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
