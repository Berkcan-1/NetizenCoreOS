// crates/nc-app-settings/src/lib.rs
#![no_std]

extern crate alloc;

use alloc::string::{String, ToString};
use nc_app::locale::{tr, KeyboardLayout, Language, TextId};
use nc_app::text::{draw_text_clipped, Line};
use nc_app::theme::{ACCENTS, ACCENT_COUNT, WALLPAPERS, WALLPAPER_COUNT};
use nc_app::{App, AppInfo, Context, Icon, Input, KeyCode, Update};
use nc_arch::{inl, outl};
use nc_driver::{
    EfraClass, EfraCompiler, EfraDriverPackage, EFRA_FLAG_CUSTOM_COMPILED, EFRA_FLAG_DMA_BURST,
    EFRA_FLAG_GENERIC_FALLBACK, EFRA_FLAG_WRITE_COMBINING,
};
use nc_font::{text_width, Style};
use nc_fs::VfsManager;
use nc_gfx::{lerp_color, rgb, Canvas, Rect};
use nc_net::wireless;
use nc_net::DhcpState;
use nc_ui::{draw_dot, draw_panel, draw_text_color, text_style, WHITE};

const PAGE_COUNT: usize = 6;

const PORCELAIN_BG: u32 = rgb(244, 249, 253);
const CARD_WHITE: u32 = rgb(253, 254, 255);
const CARD_ALT: u32 = rgb(238, 246, 251);
const CARD_HOVER: u32 = rgb(228, 241, 250);
const CARD_SELECT: u32 = rgb(216, 238, 249);
const BORDER_LIGHT: u32 = rgb(208, 224, 236);

const AZURE_BRIGHT: u32 = rgb(44, 138, 242);
const AZURE_DEEP: u32 = rgb(24, 98, 206);
const AQUA_MINT: u32 = rgb(26, 188, 168);
const AQUA_SOFT: u32 = rgb(156, 236, 222);
const AQUA_DEEP: u32 = rgb(16, 144, 130);
const WARN_AMBER: u32 = rgb(238, 154, 24);

const INK_DARK: u32 = rgb(16, 28, 44);
const INK_BODY: u32 = rgb(58, 76, 96);
const INK_MUTED: u32 = rgb(116, 134, 154);

#[derive(Clone, Copy)]
struct PciNicProbe {
    found: bool,
    bus: u8,
    dev: u8,
    irq: u8,
    model_name: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HoverTarget {
    None,
    Sidebar(usize),
    WallpaperSwatch(usize),
    AnimToggle,
    SpeedPill(u8),
    AccentDot(usize),
    BlurLevel(u8),
    DockModeToggle,
    EthDhcpBtn,
    EthArpBtn,
    WifiToggle,
    WifiScanBtn,
    WifiRow(usize),
    WifiModalConnect,
    WifiModalCancel,
    BtToggle,
    BtScanBtn,
    BtRow(usize),
    DrvRow(usize),
    DrvScanBtn,
    DrvUpgradeBtn,
    DrvCompileBtn,
    DrvModalBuild,
    DrvModalCancel,
    LaunchSpeed(u8),
    LangBtn(Language),
    KbdBtn(KeyboardLayout),
    ResetDefaultsBtn,
}

pub struct SettingsApp {
    info: AppInfo,
    page: usize,
    hover: HoverTarget,
    pci_nic: PciNicProbe,
    eth_status_note: String,
    password_prompt: Option<(String, String)>,
    selected_device: usize,
    efra_compile_prompt: Option<String>,
    efra_status_note: String,
    compiled_counter: u32,
}

fn pci_read_32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        outl(0xCF8, addr);
        inl(0xCFC)
    }
}

fn probe_pci_hardware() -> PciNicProbe {
    for bus in 0..=8u8 {
        for dev in 0..32u8 {
            let vd = pci_read_32(bus, dev, 0, 0x00);
            let vendor = (vd & 0xFFFF) as u16;
            let device = ((vd >> 16) & 0xFFFF) as u16;
            if vendor == 0xFFFF || vendor == 0x0000 {
                continue;
            }
            let class_rev = pci_read_32(bus, dev, 0, 0x08);
            let base_class = ((class_rev >> 24) & 0xFF) as u8;
            let sub_class = ((class_rev >> 16) & 0xFF) as u8;

            if (base_class == 0x02 && sub_class == 0x00) || (vendor == 0x8086 && device == 0x100E) {
                let irq = (pci_read_32(bus, dev, 0, 0x3C) & 0xFF) as u8;
                let model = match (vendor, device) {
                    (0x8086, 0x100E) => "Intel 82540EM Gigabit Ethernet (e1000)",
                    (0x8086, 0x100F) => "Intel 82545EM Copper Gigabit",
                    (0x8086, 0x10D3) => "Intel 82574L Gigabit Ethernet",
                    (0x1AF4, 0x1000) => "VirtIO Paravirtualized Network Adapter",
                    (0x10EC, 0x8139) => "Realtek RTL8139 Fast Ethernet",
                    (0x10EC, 0x8168) => "Realtek RTL8111/8168 Gigabit Ethernet",
                    (0x1022, 0x2000) => "AMD PCnet-PCI II (Am79C970A)",
                    _ => "PCI Ethernet Controller",
                };
                return PciNicProbe {
                    found: true,
                    bus,
                    dev,
                    irq,
                    model_name: model,
                };
            }
        }
    }
    PciNicProbe {
        found: false,
        bus: 0,
        dev: 0,
        irq: 0,
        model_name: "PCI Ethernet Not Detected",
    }
}

fn page_titles(lang: Language, index: usize) -> (&'static str, &'static str) {
    match index {
        0 => (tr(lang, TextId::PageAppearance), tr(lang, TextId::PageAppearanceSub)),
        1 => (tr(lang, TextId::PageEthernet), tr(lang, TextId::PageEthernetSub)),
        2 => (tr(lang, TextId::PageWifi), tr(lang, TextId::PageWifiSub)),
        3 => (tr(lang, TextId::PageBluetooth), tr(lang, TextId::PageBluetoothSub)),
        4 => (tr(lang, TextId::PageDrivers), tr(lang, TextId::PageDriversSub)),
        _ => (tr(lang, TextId::PageSystem), tr(lang, TextId::PageSystemSub)),
    }
}

pub fn save_locale_config(lang: Language, kbd: KeyboardLayout) {
    nc_fs::set_vfs_caller(nc_fs::CALLER_SETTINGS);
    let mut vfs = VfsManager::new();
    let _ = vfs.create_dir_path("/data/system");
    let content = alloc::format!("lang={}\nkbd={}\n", lang as u8, kbd as u8);
    if vfs.write_file_path("/data/system/locale.cfg", content.as_bytes()).is_err() {
        let _ = vfs.create_dir_path("/ram/system");
        let _ = vfs.write_file_path("/ram/system/locale.cfg", content.as_bytes());
    }
    nc_fs::set_vfs_caller(nc_fs::CALLER_KERNEL);
}

impl SettingsApp {
    pub fn new() -> SettingsApp {
        let _ = wireless::global();
        let _ = nc_driver::global();
        SettingsApp {
            info: AppInfo::new("nc.settings", "Ayarlar", "2.8.0", Icon::Sliders, 48, 31),
            page: 0,
            hover: HoverTarget::None,
            pci_nic: probe_pci_hardware(),
            eth_status_note: String::from("Donanim koprusu aktif ve paket trafigine hazir."),
            password_prompt: None,
            selected_device: 0,
            efra_compile_prompt: None,
            efra_status_note: String::from("Generic .efra surucu tabani ve PAT Write-Combining aktif."),
            compiled_counter: 0,
        }
    }

    fn save_efra_package_to_vfs(&mut self, pkg: &EfraDriverPackage) {
    nc_fs::set_vfs_caller(nc_fs::CALLER_SETTINGS);
    let mut vfs = VfsManager::new();
    let _ = vfs.create_dir_path("/data/drivers");
    let full_path = alloc::format!("/data/drivers/{}", pkg.manifest.file_name);
    let raw = pkg.serialize();
    if vfs.write_file_path(&full_path, &raw).is_ok() {
        self.efra_status_note = alloc::format!(
            "'{}' ({} B) -> /data/drivers [DysonSphere Onayli]",
            pkg.manifest.file_name,
            raw.len()
        );
    } else {
        let _ = vfs.create_dir_path("/ram/drivers");
        let ram_path = alloc::format!("/ram/drivers/{}", pkg.manifest.file_name);
        let _ = vfs.write_file_path(&ram_path, &raw);
        self.efra_status_note = alloc::format!(
            "'{}' -> /ram/drivers [DysonSphere Onayli]",
            pkg.manifest.file_name
        );
    }
    nc_fs::set_vfs_caller(nc_fs::CALLER_KERNEL);
}

    fn sync_efra_from_vfs(&mut self) {
        let mut vfs = VfsManager::new();
        let reg = nc_driver::global();
        let mut count = 0usize;
        if let Ok(list) = vfs.list_dir_path("/data/drivers") {
            for entry in list {
                if entry.name.ends_with(".efra") {
                    let p = alloc::format!("/data/drivers/{}", entry.name);
                    if let Ok(bytes) = vfs.read_file_path(&p) {
                        if let Some(pkg) = EfraDriverPackage::parse(&bytes) {
                            reg.register_or_update_package(pkg);
                            count += 1;
                        }
                    }
                }
            }
        }
        reg.optimize_and_scan();
        self.efra_status_note = alloc::format!(
            "PCI/ISA: {} donanim, {} .efra paketi eslestirildi.",
            reg.devices.len(),
            count
        );
    }

    fn sidebar_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(
            area.x + unit / 2,
            area.y + unit / 2,
            unit * 13,
            area.height - unit,
        )
    }

    fn sidebar_item(area: Rect, index: usize, unit: i32) -> Rect {
        let side = Self::sidebar_rect(area, unit);
        let top_offset = unit * 2;
        let avail_h = side.height - top_offset - unit / 2;
        let step = (avail_h / PAGE_COUNT as i32).max(unit * 3);
        let y = side.y + top_offset + (index as i32) * step;
        Rect::new(side.x + unit / 2, y, side.width - unit, (step - unit / 4).max(unit * 2))
    }

    fn main_pane(area: Rect, unit: i32) -> Rect {
        let side = Self::sidebar_rect(area, unit);
        Rect::new(
            side.x + side.width + unit / 2,
            area.y + unit / 2,
            area.width - side.width - unit * 3 / 2,
            area.height - unit,
        )
    }

    // Sayfa 0 (Görünüm) - 27.5 unit yüksekliğe tam sığan yerleşim
    fn swatch_rect(pane: Rect, index: usize, unit: i32) -> Rect {
        let col = (index % 3) as i32;
        let row = (index / 3) as i32;
        let gap = unit * 2 / 3;
        let w = (pane.width - unit * 2 - gap * 2) / 3;
        let h = unit * 3 + unit / 2;
        Rect::new(
            pane.x + unit + col * (w + gap),
            pane.y + unit * 7 / 2 + row * (h + unit / 2),
            w,
            h,
        )
    }

    fn anim_toggle_rect(pane: Rect, unit: i32) -> Rect {
        Rect::new(
            pane.x + unit,
            pane.y + unit * 12,
            pane.width - unit * 2,
            unit * 5 / 2,
        )
    }

    fn speed_pill_rect(pane: Rect, idx: u8, unit: i32) -> Rect {
        let base_y = pane.y + unit * 12 + unit / 3;
        let right = pane.x + pane.width - unit * 3 / 2;
        let w = unit * 4;
        Rect::new(right - (3 - idx as i32) * (w + unit / 4), base_y, w, unit * 11 / 6)
    }

    fn accent_dot_rect(pane: Rect, index: usize, unit: i32) -> Rect {
        Rect::new(
            pane.x + unit * 9 + (index as i32) * (unit * 5 / 2),
            pane.y + unit * 15 + unit / 3,
            unit * 2,
            unit * 2,
        )
    }

    fn blur_pill_rect(pane: Rect, idx: u8, unit: i32) -> Rect {
        let w = unit * 4 + unit / 4;
        Rect::new(
            pane.x + unit * 9 + (idx as i32) * (w + unit / 3),
            pane.y + unit * 18 + unit / 3,
            w,
            unit * 2,
        )
    }

    fn dock_toggle_rect(pane: Rect, unit: i32) -> Rect {
        Rect::new(
            pane.x + unit,
            pane.y + unit * 21 + unit / 3,
            pane.width - unit * 2,
            unit * 5 / 2,
        )
    }

    fn eth_action_rect(pane: Rect, idx: usize, unit: i32) -> Rect {
        let w = (pane.width - unit * 3) / 2;
        Rect::new(
            pane.x + unit + (idx as i32) * (w + unit),
            pane.y + pane.height - unit * 7 / 2,
            w,
            unit * 5 / 2,
        )
    }

    fn wifi_toggle_rect(pane: Rect, unit: i32) -> Rect {
        Rect::new(
            pane.x + unit,
            pane.y + unit * 7 / 2,
            pane.width - unit * 10,
            unit * 5 / 2,
        )
    }

    fn wifi_scan_rect(pane: Rect, unit: i32) -> Rect {
        Rect::new(
            pane.x + pane.width - unit * 8 - unit / 2,
            pane.y + unit * 7 / 2,
            unit * 7 + unit / 2,
            unit * 5 / 2,
        )
    }

    fn wifi_row_rect(pane: Rect, idx: usize, unit: i32) -> Rect {
        Rect::new(
            pane.x + unit,
            pane.y + unit * 7 + (idx as i32) * (unit * 3),
            pane.width - unit * 2,
            unit * 8 / 3,
        )
    }

    fn drv_row_rect(pane: Rect, idx: usize, unit: i32) -> Rect {
        Rect::new(
            pane.x + unit,
            pane.y + unit * 6 + (idx as i32) * (unit * 3),
            pane.width - unit * 2,
            unit * 8 / 3,
        )
    }

    fn drv_action_rect(pane: Rect, idx: usize, unit: i32) -> Rect {
        let w = (pane.width - unit * 4) / 3;
        Rect::new(
            pane.x + unit + (idx as i32) * (w + unit),
            pane.y + pane.height - unit * 7 / 2,
            w,
            unit * 5 / 2,
        )
    }

    fn pass_modal_rect(pane: Rect, unit: i32) -> Rect {
        Rect::new(
            pane.x + unit * 2,
            pane.y + unit * 6,
            pane.width - unit * 4,
            unit * 12,
        )
    }

    // Sayfa 5 (Sistem ve Dil) yerleşimi
    fn launch_speed_rect(pane: Rect, idx: u8, unit: i32) -> Rect {
        let w = (pane.width - unit * 4) / 3;
        Rect::new(
            pane.x + unit + (idx as i32) * (w + unit),
            pane.y + unit * 5,
            w,
            unit * 3 + unit / 3,
        )
    }

    fn lang_btn_rect(pane: Rect, idx: usize, unit: i32) -> Rect {
        let w = unit * 8;
        Rect::new(
            pane.x + unit * 9 + (idx as i32) * (w + unit / 2),
            pane.y + unit * 9 + unit / 2,
            w,
            unit * 2 + unit / 4,
        )
    }

    fn kbd_btn_rect(pane: Rect, idx: usize, unit: i32) -> Rect {
        let w = unit * 8;
        Rect::new(
            pane.x + unit * 9 + (idx as i32) * (w + unit / 2),
            pane.y + unit * 12 + unit / 2,
            w,
            unit * 2 + unit / 4,
        )
    }

    fn reset_btn_rect(pane: Rect, unit: i32) -> Rect {
        Rect::new(
            pane.x + unit,
            pane.y + pane.height - unit * 7 / 2,
            unit * 12,
            unit * 5 / 2,
        )
    }

    fn hit(&self, area: Rect, unit: i32, x: i32, y: i32) -> HoverTarget {
        let pane = Self::main_pane(area, unit);

        if self.password_prompt.is_some() {
            let m = Self::pass_modal_rect(pane, unit);
            let bw = unit * 7;
            let bh = unit * 2;
            let by = m.y + m.height - bh - unit;
            let conn_r = Rect::new(m.x + m.width - bw - unit, by, bw, bh);
            let canc_r = Rect::new(conn_r.x - bw - unit / 2, by, bw, bh);
            if inside(conn_r, x, y) {
                return HoverTarget::WifiModalConnect;
            }
            if inside(canc_r, x, y) {
                return HoverTarget::WifiModalCancel;
            }
            return HoverTarget::None;
        }

        if self.efra_compile_prompt.is_some() {
            let m = Self::pass_modal_rect(pane, unit);
            let bw = unit * 8;
            let bh = unit * 2;
            let by = m.y + m.height - bh - unit;
            let build_r = Rect::new(m.x + m.width - bw - unit, by, bw, bh);
            let canc_r = Rect::new(build_r.x - bw - unit / 2, by, bw, bh);
            if inside(build_r, x, y) {
                return HoverTarget::DrvModalBuild;
            }
            if inside(canc_r, x, y) {
                return HoverTarget::DrvModalCancel;
            }
            return HoverTarget::None;
        }

        for i in 0..PAGE_COUNT {
            if inside(Self::sidebar_item(area, i, unit), x, y) {
                return HoverTarget::Sidebar(i);
            }
        }

        let wl = wireless::global();

        match self.page {
            0 => {
                for i in 0..WALLPAPER_COUNT {
                    if inside(Self::swatch_rect(pane, i, unit), x, y) {
                        return HoverTarget::WallpaperSwatch(i);
                    }
                }
                for s in 1..=3u8 {
                    if inside(Self::speed_pill_rect(pane, s, unit), x, y) {
                        return HoverTarget::SpeedPill(s);
                    }
                }
                if inside(Self::anim_toggle_rect(pane, unit), x, y) {
                    return HoverTarget::AnimToggle;
                }
                for i in 0..ACCENT_COUNT {
                    if inside(Self::accent_dot_rect(pane, i, unit), x, y) {
                        return HoverTarget::AccentDot(i);
                    }
                }
                for b in 0..4u8 {
                    if inside(Self::blur_pill_rect(pane, b, unit), x, y) {
                        return HoverTarget::BlurLevel(b);
                    }
                }
                if inside(Self::dock_toggle_rect(pane, unit), x, y) {
                    return HoverTarget::DockModeToggle;
                }
            }
            1 => {
                if inside(Self::eth_action_rect(pane, 0, unit), x, y) {
                    return HoverTarget::EthDhcpBtn;
                }
                if inside(Self::eth_action_rect(pane, 1, unit), x, y) {
                    return HoverTarget::EthArpBtn;
                }
            }
            2 => {
                if inside(Self::wifi_toggle_rect(pane, unit), x, y) {
                    return HoverTarget::WifiToggle;
                }
                if inside(Self::wifi_scan_rect(pane, unit), x, y) {
                    return HoverTarget::WifiScanBtn;
                }
                for i in 0..wl.wifi_networks.len().min(5) {
                    if inside(Self::wifi_row_rect(pane, i, unit), x, y) {
                        return HoverTarget::WifiRow(i);
                    }
                }
            }
            3 => {
                if inside(Self::wifi_toggle_rect(pane, unit), x, y) {
                    return HoverTarget::BtToggle;
                }
                if inside(Self::wifi_scan_rect(pane, unit), x, y) {
                    return HoverTarget::BtScanBtn;
                }
                for i in 0..wl.bt_devices.len().min(5) {
                    if inside(Self::wifi_row_rect(pane, i, unit), x, y) {
                        return HoverTarget::BtRow(i);
                    }
                }
            }
            4 => {
                let drv_cnt = nc_driver::global().devices.len().min(4);
                for i in 0..drv_cnt {
                    if inside(Self::drv_row_rect(pane, i, unit), x, y) {
                        return HoverTarget::DrvRow(i);
                    }
                }
                if inside(Self::drv_action_rect(pane, 0, unit), x, y) {
                    return HoverTarget::DrvScanBtn;
                }
                if inside(Self::drv_action_rect(pane, 1, unit), x, y) {
                    return HoverTarget::DrvUpgradeBtn;
                }
                if inside(Self::drv_action_rect(pane, 2, unit), x, y) {
                    return HoverTarget::DrvCompileBtn;
                }
            }
            _ => {
                for s in 1..=3u8 {
                    if inside(Self::launch_speed_rect(pane, s - 1, unit), x, y) {
                        return HoverTarget::LaunchSpeed(s);
                    }
                }
                if inside(Self::lang_btn_rect(pane, 0, unit), x, y) {
                    return HoverTarget::LangBtn(Language::Turkish);
                }
                if inside(Self::lang_btn_rect(pane, 1, unit), x, y) {
                    return HoverTarget::LangBtn(Language::English);
                }
                if inside(Self::kbd_btn_rect(pane, 0, unit), x, y) {
                    return HoverTarget::KbdBtn(KeyboardLayout::TurkishQ);
                }
                if inside(Self::kbd_btn_rect(pane, 1, unit), x, y) {
                    return HoverTarget::KbdBtn(KeyboardLayout::UsQwerty);
                }
                if inside(Self::reset_btn_rect(pane, unit), x, y) {
                    return HoverTarget::ResetDefaultsBtn;
                }
            }
        }
        HoverTarget::None
    }
}

impl App for SettingsApp {
    fn info(&self) -> &AppInfo {
        &self.info
    }

    fn opened(&mut self, _ctx: &mut Context) {
        let wl = wireless::global();
        wl.send_command("CMD:INIT");
        if wl.wifi_enabled {
            wl.request_wifi_scan();
        }
        if wl.bt_enabled {
            wl.request_bt_scan();
        }
    }

    fn update(&mut self, ctx: &mut Context, input: &Input) -> Update {
        let wl = wireless::global();
        let prev_w_cnt = wl.wifi_networks.len();
        let prev_b_cnt = wl.bt_devices.len();
        let prev_w_scan = wl.wifi_scanning;
        let prev_b_scan = wl.bt_scanning;

        wl.poll();

        let mut redraw = wl.wifi_networks.len() != prev_w_cnt
            || wl.bt_devices.len() != prev_b_cnt
            || wl.wifi_scanning != prev_w_scan
            || wl.bt_scanning != prev_b_scan;

        if let Some((ref ssid, ref mut pass_buf)) = self.password_prompt {
            if let Some(k) = input.key {
                match k {
                    KeyCode::Escape => {
                        self.password_prompt = None;
                        redraw = true;
                    }
                    KeyCode::Enter => {
                        let target_ssid = ssid.clone();
                        let target_pass = pass_buf.clone();
                        self.password_prompt = None;
                        wl.connect_wifi(&target_ssid, &target_pass);
                        redraw = true;
                    }
                    KeyCode::Backspace => {
                        pass_buf.pop();
                        redraw = true;
                    }
                    KeyCode::Space => {
                        pass_buf.push(' ');
                        redraw = true;
                    }
                    KeyCode::Char(c) => {
                        if pass_buf.len() < 40 {
                            pass_buf.push(c);
                            redraw = true;
                        }
                    }
                    _ => {}
                }
            }
        } else if let Some(ref mut drv_buf) = self.efra_compile_prompt {
            if let Some(k) = input.key {
                match k {
                    KeyCode::Escape => {
                        self.efra_compile_prompt = None;
                        redraw = true;
                    }
                    KeyCode::Enter => {
                        let mut fname = drv_buf.trim().to_string();
                        self.efra_compile_prompt = None;
                        if !fname.ends_with(".efra") {
                            fname.push_str(".efra");
                        }
                        self.compiled_counter += 1;
                        let custom_id = alloc::format!("efra.custom.u{}", self.compiled_counter);
                        let pkg = EfraCompiler::compile_in_os(
                            &custom_id,
                            "NetizenCore Custom .efra",
                            "3.0.0",
                            &fname,
                            EfraClass::Generic,
                            0xFFFF,
                            0xFFFF,
                            0xFF,
                            0xFF,
                            EFRA_FLAG_GENERIC_FALLBACK
                                | EFRA_FLAG_WRITE_COMBINING
                                | EFRA_FLAG_DMA_BURST
                                | EFRA_FLAG_CUSTOM_COMPILED,
                        );
                        nc_driver::global().register_or_update_package(pkg.clone());
                        self.save_efra_package_to_vfs(&pkg);
                        redraw = true;
                    }
                    KeyCode::Backspace => {
                        drv_buf.pop();
                        redraw = true;
                    }
                    KeyCode::Char(c) => {
                        if c != '/' && c != '\\' && drv_buf.len() < 32 {
                            drv_buf.push(c);
                            redraw = true;
                        }
                    }
                    _ => {}
                }
            }
        }

        let hover = match input.pointer {
            Some(p) => self.hit(input.area, ctx.unit, p.x, p.y),
            None => HoverTarget::None,
        };
        if hover != self.hover {
            self.hover = hover;
            redraw = true;
        }

        if let Some(p) = input.pointer {
            if p.clicked {
                match hover {
                    HoverTarget::WifiModalCancel => {
                        self.password_prompt = None;
                    }
                    HoverTarget::WifiModalConnect => {
                        if let Some((ssid, pass)) = self.password_prompt.take() {
                            wl.connect_wifi(&ssid, &pass);
                        }
                    }
                    HoverTarget::DrvModalCancel => {
                        self.efra_compile_prompt = None;
                    }
                    HoverTarget::DrvModalBuild => {
                        if let Some(raw_name) = self.efra_compile_prompt.take() {
                            let mut fname = raw_name.trim().to_string();
                            if fname.is_empty() {
                                fname = String::from("generic-boost.efra");
                            } else if !fname.ends_with(".efra") {
                                fname.push_str(".efra");
                            }
                            self.compiled_counter += 1;
                            let custom_id = alloc::format!("efra.custom.u{}", self.compiled_counter);
                            let pkg = EfraCompiler::compile_in_os(
                                &custom_id,
                                "NetizenCore Custom .efra",
                                "3.0.0",
                                &fname,
                                EfraClass::Generic,
                                0xFFFF,
                                0xFFFF,
                                0xFF,
                                0xFF,
                                EFRA_FLAG_GENERIC_FALLBACK
                                    | EFRA_FLAG_WRITE_COMBINING
                                    | EFRA_FLAG_DMA_BURST
                                    | EFRA_FLAG_CUSTOM_COMPILED,
                            );
                            nc_driver::global().register_or_update_package(pkg.clone());
                            self.save_efra_package_to_vfs(&pkg);
                        }
                    }
                    HoverTarget::Sidebar(idx) => {
                        self.page = idx;
                        if idx == 1 {
                            self.pci_nic = probe_pci_hardware();
                        } else if idx == 2 && wl.wifi_networks.is_empty() && wl.wifi_enabled {
                            wl.request_wifi_scan();
                        } else if idx == 3 && wl.bt_devices.is_empty() && wl.bt_enabled {
                            wl.request_bt_scan();
                        }
                    }
                    HoverTarget::WallpaperSwatch(idx) => {
                        ctx.settings.wallpaper = idx;
                    }
                    HoverTarget::AnimToggle => {
                        ctx.settings.wallpaper_animated = !ctx.settings.wallpaper_animated;
                    }
                    HoverTarget::SpeedPill(s) => {
                        ctx.settings.wallpaper_speed = s;
                        ctx.settings.wallpaper_animated = true;
                    }
                    HoverTarget::AccentDot(idx) => {
                        ctx.settings.accent = idx;
                    }
                    HoverTarget::BlurLevel(b) => {
                        ctx.settings.glass_blur = b;
                    }
                    HoverTarget::DockModeToggle => {
                        ctx.settings.dock_auto_hide = !ctx.settings.dock_auto_hide;
                    }
                    HoverTarget::EthDhcpBtn => {
                        let net = nc_net::global();
                        if net.link_online {
                            let pkt = net.dhcp.build_discover(net.mac_addr);
                            if let Some(ref mut drv) = net.driver {
                                let _ = drv.send_packet(&pkt);
                                net.tx_packets += 1;
                            }
                            self.eth_status_note = String::from("DHCP Discover -> PCI e1000");
                        } else {
                            self.eth_status_note = String::from("No physical Ethernet link.");
                        }
                    }
                    HoverTarget::EthArpBtn => {
                        let net = nc_net::global();
                        net.resolve_gateway_mac();
                        self.eth_status_note = String::from("ARP Gateway MAC request sent.");
                    }
                    HoverTarget::WifiToggle => {
                        let next_state = !wl.wifi_enabled;
                        wl.set_wifi_enabled(next_state);
                        ctx.settings.wifi_enabled = next_state;
                    }
                    HoverTarget::WifiScanBtn => {
                        wl.request_wifi_scan();
                    }
                    HoverTarget::WifiRow(idx) => {
                        if idx < wl.wifi_networks.len() {
                            let net_item = wl.wifi_networks[idx].clone();
                            if net_item.connected {
                                wl.disconnect_wifi(&net_item.ssid);
                            } else if net_item.is_open() {
                                wl.connect_wifi(&net_item.ssid, "");
                            } else {
                                self.password_prompt = Some((net_item.ssid, String::new()));
                            }
                        }
                    }
                    HoverTarget::BtToggle => {
                        let next_state = !wl.bt_enabled;
                        wl.set_bt_enabled(next_state);
                        ctx.settings.bluetooth_enabled = next_state;
                    }
                    HoverTarget::BtScanBtn => {
                        wl.request_bt_scan();
                    }
                    HoverTarget::BtRow(idx) => {
                        wl.toggle_bt_device(idx);
                    }
                    HoverTarget::DrvRow(idx) => {
                        self.selected_device = idx;
                    }
                    HoverTarget::DrvScanBtn => {
                        self.sync_efra_from_vfs();
                    }
                    HoverTarget::DrvUpgradeBtn => {
                        if let Some(pkg) = nc_driver::global().upgrade_device_driver(self.selected_device) {
                            self.save_efra_package_to_vfs(&pkg);
                        }
                    }
                    HoverTarget::DrvCompileBtn => {
                        self.efra_compile_prompt = Some(String::from("generic-turbo-drv.efra"));
                    }
                    HoverTarget::LaunchSpeed(s) => {
                        ctx.settings.launch_anim_speed = s;
                    }
                    HoverTarget::LangBtn(lang) => {
                        ctx.settings.language = lang;
                        save_locale_config(ctx.settings.language, ctx.settings.keyboard);
                    }
                    HoverTarget::KbdBtn(kbd) => {
                        ctx.settings.keyboard = kbd;
                        save_locale_config(ctx.settings.language, ctx.settings.keyboard);
                    }
                    HoverTarget::ResetDefaultsBtn => {
                        ctx.settings = nc_app::Settings::new();
                        save_locale_config(ctx.settings.language, ctx.settings.keyboard);
                    }
                    HoverTarget::None => {}
                }
                redraw = true;
            }
        }

        Update {
            redraw,
            close: false,
            install_app: None,
        }
    }

    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        let unit = ctx.unit;
        let lang = ctx.settings.language;
        canvas.fill_rect(area.x, area.y, area.width, area.height, PORCELAIN_BG);

        let side = Self::sidebar_rect(area, unit);
        draw_panel(canvas, side, unit / 2, CARD_ALT, 255, BORDER_LIGHT, 255);

        let title_st = text_style((unit * 76 / 100).max(11), 15);
        let label_st = text_style((unit * 52 / 100).max(9), 14);
        let sub_st = text_style((unit * 42 / 100).max(8), 14);

        draw_dot(canvas, side.x + unit, side.y + unit, 5, AQUA_MINT, 255);
        draw_text_clipped(
            canvas,
            tr(lang, TextId::SettingsTitle),
            side.x + unit * 7 / 4,
            side.y + unit * 3 / 5,
            side.width - unit * 2,
            &label_st,
            INK_DARK,
        );

        for i in 0..PAGE_COUNT {
            let item = Self::sidebar_item(area, i, unit);
            let selected = self.page == i;
            let hovered = self.hover == HoverTarget::Sidebar(i);

            if selected {
                draw_panel(canvas, item, unit / 3, AZURE_BRIGHT, 255, AZURE_DEEP, 255);
            } else if hovered {
                draw_panel(canvas, item, unit / 3, CARD_HOVER, 255, BORDER_LIGHT, 255);
            } else {
                draw_panel(canvas, item, unit / 3, CARD_WHITE, 220, BORDER_LIGHT, 180);
            }

            let fg_main = if selected { WHITE } else { INK_DARK };
            let fg_sub = if selected { AQUA_SOFT } else { INK_MUTED };
            let (p_title, p_sub) = page_titles(lang, i);
            let max_tw = item.width - unit * 5 / 4;

            draw_text_clipped(
                canvas,
                p_title,
                item.x + unit * 2 / 3,
                item.y + unit / 4,
                max_tw,
                &label_st,
                fg_main,
            );
            draw_text_clipped(
                canvas,
                p_sub,
                item.x + unit * 2 / 3,
                item.y + unit * 4 / 3,
                max_tw,
                &sub_st,
                fg_sub,
            );
        }

        let pane = Self::main_pane(area, unit);
        draw_panel(canvas, pane, unit / 2, CARD_WHITE, 255, BORDER_LIGHT, 255);

        let (cur_title, cur_sub) = page_titles(lang, self.page);
        draw_text_clipped(
            canvas,
            cur_title,
            pane.x + unit,
            pane.y + unit * 3 / 4,
            pane.width - unit * 2,
            &title_st,
            INK_DARK,
        );
        draw_text_clipped(
            canvas,
            cur_sub,
            pane.x + unit,
            pane.y + unit * 2,
            pane.width - unit * 2,
            &sub_st,
            INK_MUTED,
        );

        match self.page {
            0 => self.draw_page_appearance(canvas, pane, ctx, &label_st, &sub_st),
            1 => self.draw_page_ethernet(canvas, pane, ctx, &label_st, &sub_st),
            2 => self.draw_page_wifi(canvas, pane, ctx, &label_st, &sub_st),
            3 => self.draw_page_bluetooth(canvas, pane, ctx, &label_st, &sub_st),
            4 => self.draw_page_drivers(canvas, pane, ctx, &label_st, &sub_st),
            _ => self.draw_page_system(canvas, pane, ctx, &label_st, &sub_st),
        }
    }
}

impl SettingsApp {
    fn draw_page_appearance(
        &self,
        canvas: &mut Canvas<'_>,
        pane: Rect,
        ctx: &Context,
        label_st: &Style,
        sub_st: &Style,
    ) {
        let unit = ctx.unit;
        let lang = ctx.settings.language;
        let accent = ctx.settings.accent_color();

        for i in 0..WALLPAPER_COUNT {
            let cell = Self::swatch_rect(pane, i, unit);
            let wp = &WALLPAPERS[i];
            for r in 0..cell.height {
                let t = (r * 255 / (cell.height - 1).max(1)) as u32;
                canvas.fill_rect(
                    cell.x + 2,
                    cell.y + r,
                    cell.width - 4,
                    1,
                    lerp_color(wp.top, wp.bottom, t),
                );
            }
            draw_dot(
                canvas,
                cell.x + cell.width * 3 / 4,
                cell.y + cell.height / 3,
                unit / 2,
                wp.glows[0].color,
                190,
            );
            let selected = ctx.settings.wallpaper == i;
            let brd = if selected {
                accent
            } else if self.hover == HoverTarget::WallpaperSwatch(i) {
                AZURE_BRIGHT
            } else {
                BORDER_LIGHT
            };
            draw_panel(canvas, cell, unit / 3, WHITE, 0, brd, 255);
            if selected {
                let outer = Rect::new(cell.x - 2, cell.y - 2, cell.width + 4, cell.height + 4);
                draw_panel(canvas, outer, unit / 3 + 1, WHITE, 0, accent, 255);
            }

            let name_pill = Rect::new(
                cell.x + 4,
                cell.y + cell.height - unit * 6 / 5,
                cell.width - 8,
                unit,
            );
            draw_panel(canvas, name_pill, 4, CARD_WHITE, 215, BORDER_LIGHT, 0);
            let nw = text_width(wp.name, sub_st).min(name_pill.width - 6);
            draw_text_clipped(
                canvas,
                wp.name,
                name_pill.x + (name_pill.width - nw) / 2,
                name_pill.y + (name_pill.height - sub_st.cap_height) / 2,
                name_pill.width - 6,
                sub_st,
                INK_DARK,
            );
        }

        let anim_row = Self::anim_toggle_rect(pane, unit);
        draw_panel(canvas, anim_row, unit / 3, CARD_ALT, 255, BORDER_LIGHT, 255);
        draw_switch(
            canvas,
            anim_row.x + unit / 2,
            anim_row.y + (anim_row.height - 18) / 2,
            ctx.settings.wallpaper_animated,
        );

        let first_pill_x = Self::speed_pill_rect(pane, 1, unit).x;
        let max_anim_tw = (first_pill_x - (anim_row.x + unit * 5 / 2) - unit / 2).max(40);
        draw_text_clipped(
            canvas,
            tr(lang, TextId::LiveWallpaperToggle),
            anim_row.x + unit * 5 / 2,
            anim_row.y + (anim_row.height - label_st.cap_height) / 2,
            max_anim_tw,
            label_st,
            INK_DARK,
        );

        let speeds = [
            (1u8, tr(lang, TextId::SpeedSlow)),
            (2u8, tr(lang, TextId::SpeedBalanced)),
            (3u8, tr(lang, TextId::SpeedFluid)),
        ];
        for (s_val, s_lbl) in speeds {
            let pr = Self::speed_pill_rect(pane, s_val, unit);
            let sel = ctx.settings.wallpaper_animated && ctx.settings.wallpaper_speed == s_val;
            let hov = self.hover == HoverTarget::SpeedPill(s_val);
            draw_pill_btn(canvas, pr, s_lbl, sel, hov, sub_st);
        }

        let acc_y = pane.y + unit * 15 + unit / 3;
        draw_text_clipped(
            canvas,
            tr(lang, TextId::AccentColorLabel),
            pane.x + unit,
            acc_y + (unit * 2 - label_st.cap_height) / 2,
            unit * 8,
            label_st,
            INK_DARK,
        );
        for i in 0..ACCENT_COUNT {
            let r = Self::accent_dot_rect(pane, i, unit);
            let cx = r.x + r.width / 2;
            let cy = r.y + r.height / 2;
            if ctx.settings.accent == i {
                draw_dot(canvas, cx, cy, unit * 3 / 4 + 3, ACCENTS[i], 255);
                draw_dot(canvas, cx, cy, unit * 3 / 4 + 1, WHITE, 255);
            }
            draw_dot(canvas, cx, cy, unit * 3 / 4, ACCENTS[i], 255);
        }

        let blur_y = pane.y + unit * 18 + unit / 3;
        draw_text_clipped(
            canvas,
            tr(lang, TextId::BlurLabel),
            pane.x + unit,
            blur_y + (unit * 2 - label_st.cap_height) / 2,
            unit * 8,
            label_st,
            INK_DARK,
        );
        let blurs = [
            (0u8, tr(lang, TextId::BlurOff)),
            (1u8, tr(lang, TextId::BlurLight)),
            (2u8, tr(lang, TextId::BlurStandard)),
            (3u8, tr(lang, TextId::BlurDeep)),
        ];
        for (b_val, b_lbl) in blurs {
            let br = Self::blur_pill_rect(pane, b_val, unit);
            let sel = ctx.settings.glass_blur == b_val;
            let hov = self.hover == HoverTarget::BlurLevel(b_val);
            draw_pill_btn(canvas, br, b_lbl, sel, hov, sub_st);
        }

        let dock_row = Self::dock_toggle_rect(pane, unit);
        draw_panel(canvas, dock_row, unit / 3, CARD_ALT, 255, BORDER_LIGHT, 255);
        draw_switch(
            canvas,
            dock_row.x + unit / 2,
            dock_row.y + (dock_row.height - 18) / 2,
            ctx.settings.dock_auto_hide,
        );
        let dock_txt = if ctx.settings.dock_auto_hide {
            tr(lang, TextId::DockAutoHideOn)
        } else {
            tr(lang, TextId::DockAutoHideOff)
        };
        draw_text_clipped(
            canvas,
            dock_txt,
            dock_row.x + unit * 5 / 2,
            dock_row.y + (dock_row.height - label_st.cap_height) / 2,
            dock_row.width - unit * 3,
            label_st,
            INK_DARK,
        );
    }

    fn draw_page_ethernet(
        &self,
        canvas: &mut Canvas<'_>,
        pane: Rect,
        ctx: &Context,
        label_st: &Style,
        sub_st: &Style,
    ) {
        let unit = ctx.unit;
        let lang = ctx.settings.language;
        let net = nc_net::global();

        let hw_card = Rect::new(
            pane.x + unit,
            pane.y + unit * 7 / 2,
            pane.width - unit * 2,
            unit * 7 + unit / 2,
        );
        draw_panel(canvas, hw_card, unit / 2, CARD_ALT, 255, BORDER_LIGHT, 255);

        draw_dot(
            canvas,
            hw_card.x + unit,
            hw_card.y + unit,
            5,
            if self.pci_nic.found {
                AQUA_MINT
            } else {
                WARN_AMBER
            },
            255,
        );
        draw_text_clipped(
            canvas,
            self.pci_nic.model_name,
            hw_card.x + unit * 2,
            hw_card.y + unit * 2 / 3,
            hw_card.width - unit * 3,
            label_st,
            INK_DARK,
        );

        let mut pci_line = Line::new();
        pci_line
            .text("Platform: ")
            .text(ctx.platform)
            .text("  |  PCI ")
            .number(self.pci_nic.bus as u32, 1)
            .text(":")
            .number(self.pci_nic.dev as u32, 1)
            .text("  |  IRQ ")
            .number(self.pci_nic.irq as u32, 1);
        draw_text_clipped(
            canvas,
            pci_line.as_str(),
            hw_card.x + unit * 2,
            hw_card.y + unit * 9 / 4,
            hw_card.width - unit * 3,
            sub_st,
            INK_BODY,
        );

        let mut mac_line = Line::new();
        mac_line
            .text("MAC: ")
            .number(net.mac_addr[0] as u32, 2)
            .text(":")
            .number(net.mac_addr[1] as u32, 2)
            .text(":")
            .number(net.mac_addr[2] as u32, 2)
            .text(":")
            .number(net.mac_addr[3] as u32, 2)
            .text(":")
            .number(net.mac_addr[4] as u32, 2)
            .text(":")
            .number(net.mac_addr[5] as u32, 2);
        draw_text_clipped(
            canvas,
            mac_line.as_str(),
            hw_card.x + unit * 2,
            hw_card.y + unit * 4,
            hw_card.width - unit * 3,
            sub_st,
            AZURE_DEEP,
        );

        let dhcp_str = match net.dhcp.state {
            DhcpState::Bound => "DHCP: Bound (IP Assigned)",
            DhcpState::RequestSent => "DHCP: Waiting Server ACK",
            DhcpState::DiscoverSent => "DHCP: Searching Server",
            DhcpState::Init => "DHCP: Initializing",
        };
        draw_text_clipped(
            canvas,
            dhcp_str,
            hw_card.x + unit * 2,
            hw_card.y + unit * 11 / 2,
            hw_card.width - unit * 3,
            sub_st,
            AQUA_DEEP,
        );

        let ip_card = Rect::new(
            pane.x + unit,
            pane.y + unit * 12,
            pane.width - unit * 2,
            unit * 8 + unit / 2,
        );
        draw_panel(
            canvas,
            ip_card,
            unit / 2,
            CARD_WHITE,
            255,
            BORDER_LIGHT,
            255,
        );

        let mut ip_l = Line::new();
        ip_l.text("IPv4 :  ")
            .number(net.ip_addr[0] as u32, 1)
            .text(".")
            .number(net.ip_addr[1] as u32, 1)
            .text(".")
            .number(net.ip_addr[2] as u32, 1)
            .text(".")
            .number(net.ip_addr[3] as u32, 1);
        draw_text_color(
            canvas,
            ip_l.as_str(),
            ip_card.x + unit,
            ip_card.y + unit * 3 / 4,
            label_st,
            INK_DARK,
        );

        let mut gw_l = Line::new();
        gw_l.text("GW   :  ")
            .number(net.gateway_ip[0] as u32, 1)
            .text(".")
            .number(net.gateway_ip[1] as u32, 1)
            .text(".")
            .number(net.gateway_ip[2] as u32, 1)
            .text(".")
            .number(net.gateway_ip[3] as u32, 1);
        draw_text_color(
            canvas,
            gw_l.as_str(),
            ip_card.x + unit,
            ip_card.y + unit * 5 / 2,
            label_st,
            INK_BODY,
        );

        let mut dns_l = Line::new();
        dns_l
            .text("DNS  :  ")
            .number(net.dns_ip[0] as u32, 1)
            .text(".")
            .number(net.dns_ip[1] as u32, 1)
            .text(".")
            .number(net.dns_ip[2] as u32, 1)
            .text(".")
            .number(net.dns_ip[3] as u32, 1);
        draw_text_color(
            canvas,
            dns_l.as_str(),
            ip_card.x + unit,
            ip_card.y + unit * 17 / 4,
            label_st,
            INK_BODY,
        );

        let mut pkt_l = Line::new();
        pkt_l
            .text("TX: ")
            .number(net.tx_packets as u32, 1)
            .text("   |   RX: ")
            .number(net.rx_packets as u32, 1);
        draw_text_color(
            canvas,
            pkt_l.as_str(),
            ip_card.x + unit,
            ip_card.y + unit * 6,
            sub_st,
            AZURE_BRIGHT,
        );

        draw_text_clipped(
            canvas,
            &self.eth_status_note,
            pane.x + unit,
            pane.y + pane.height - unit * 5,
            pane.width - unit * 2,
            sub_st,
            INK_MUTED,
        );

        let btn0 = Self::eth_action_rect(pane, 0, unit);
        let btn1 = Self::eth_action_rect(pane, 1, unit);
        draw_pill_btn(
            canvas,
            btn0,
            tr(lang, TextId::EthRenewDhcp),
            true,
            self.hover == HoverTarget::EthDhcpBtn,
            label_st,
        );
        draw_pill_btn(
            canvas,
            btn1,
            tr(lang, TextId::EthArpTest),
            false,
            self.hover == HoverTarget::EthArpBtn,
            label_st,
        );
    }

    fn draw_page_wifi(
        &self,
        canvas: &mut Canvas<'_>,
        pane: Rect,
        ctx: &Context,
        label_st: &Style,
        sub_st: &Style,
    ) {
        let unit = ctx.unit;
        let lang = ctx.settings.language;
        let wl = wireless::global();

        let top_bar = Self::wifi_toggle_rect(pane, unit);
        draw_panel(canvas, top_bar, unit / 3, CARD_ALT, 255, BORDER_LIGHT, 255);
        draw_switch(
            canvas,
            top_bar.x + unit / 2,
            top_bar.y + (top_bar.height - 18) / 2,
            wl.wifi_enabled,
        );
        draw_text_clipped(
            canvas,
            &wl.wifi_adapter_name,
            top_bar.x + unit * 5 / 2,
            top_bar.y + (top_bar.height - label_st.cap_height) / 2,
            top_bar.width - unit * 3,
            label_st,
            INK_DARK,
        );

        let scan_btn = Self::wifi_scan_rect(pane, unit);
        let scan_lbl = if wl.wifi_scanning {
            tr(lang, TextId::WifiScanning)
        } else {
            tr(lang, TextId::WifiScanBtn)
        };
        draw_pill_btn(
            canvas,
            scan_btn,
            scan_lbl,
            wl.wifi_scanning,
            self.hover == HoverTarget::WifiScanBtn,
            label_st,
        );

        if !wl.wifi_enabled {
            draw_text_color(
                canvas,
                tr(lang, TextId::WifiRadioOff),
                pane.x + unit,
                pane.y + unit * 9,
                label_st,
                INK_MUTED,
            );
            return;
        }

        for (idx, net) in wl.wifi_networks.iter().take(5).enumerate() {
            let row = Self::wifi_row_rect(pane, idx, unit);
            let hov = self.hover == HoverTarget::WifiRow(idx);
            let fill = if net.connected {
                CARD_SELECT
            } else if hov {
                CARD_HOVER
            } else {
                CARD_WHITE
            };
            let brd = if net.connected {
                AQUA_MINT
            } else if hov {
                AZURE_BRIGHT
            } else {
                BORDER_LIGHT
            };
            draw_panel(canvas, row, unit / 3, fill, 255, brd, 255);

            let bars = net.bars();
            for b in 0..4u8 {
                let bh = 4 + (b as i32) * 3;
                let bx = row.x + unit * 2 / 3 + (b as i32) * 5;
                let by = row.y + row.height / 2 + 6 - bh;
                let b_col = if b < bars {
                    if net.connected {
                        AQUA_DEEP
                    } else {
                        AZURE_BRIGHT
                    }
                } else {
                    BORDER_LIGHT
                };
                canvas.fill_rect(bx, by, 3, bh, b_col);
            }

            let badge = Rect::new(
                row.x + row.width - unit * 6 - unit / 2,
                row.y + unit / 4,
                unit * 6,
                row.height - unit / 2,
            );
            let text_max = (badge.x - (row.x + unit * 5 / 2) - unit / 2).max(40);

            draw_text_clipped(
                canvas,
                &net.ssid,
                row.x + unit * 5 / 2,
                row.y + unit / 4,
                text_max,
                label_st,
                INK_DARK,
            );

            let mut detail = Line::new();
            detail
                .text(&net.freq_label)
                .text(" | Ch ")
                .number(net.channel as u32, 1)
                .text(" | %")
                .number(net.signal_pct as u32, 1)
                .text(" | ")
                .text(&net.security);
            draw_text_clipped(
                canvas,
                detail.as_str(),
                row.x + unit * 5 / 2,
                row.y + unit * 7 / 5,
                text_max,
                sub_st,
                INK_MUTED,
            );

            let status_txt = if net.connected {
                tr(lang, TextId::WifiConnectedBtn)
            } else {
                tr(lang, TextId::WifiConnectBtn)
            };
            draw_pill_btn(canvas, badge, status_txt, net.connected, hov, sub_st);
        }

        draw_text_clipped(
            canvas,
            &wl.status_message,
            pane.x + unit,
            pane.y + pane.height - unit * 2,
            pane.width - unit * 2,
            sub_st,
            AQUA_DEEP,
        );

        if let Some((ref ssid, ref pass_buf)) = self.password_prompt {
            let m = Self::pass_modal_rect(pane, unit);
            draw_panel(canvas, m, unit / 2, CARD_WHITE, 252, AZURE_BRIGHT, 255);

            let mut t_line = Line::new();
            t_line.text("Wi-Fi: ").text(ssid);
            draw_text_clipped(
                canvas,
                t_line.as_str(),
                m.x + unit,
                m.y + unit,
                m.width - unit * 2,
                label_st,
                INK_DARK,
            );

            let inp = Rect::new(m.x + unit, m.y + unit * 4, m.width - unit * 2, unit * 5 / 2);
            draw_panel(canvas, inp, unit / 3, CARD_ALT, 255, AZURE_BRIGHT, 255);
            let ty = inp.y + (inp.height - label_st.cap_height) / 2;
            draw_text_clipped(canvas, pass_buf, inp.x + 10, ty, inp.width - 24, label_st, INK_DARK);
            let pw_w = text_width(pass_buf, label_st).min(inp.width - 24);
            canvas.fill_rect(inp.x + 12 + pw_w, ty - 1, 2, label_st.cap_height + 2, AQUA_MINT);

            let bw = unit * 7;
            let bh = unit * 2;
            let by = m.y + m.height - bh - unit;
            let conn_r = Rect::new(m.x + m.width - bw - unit, by, bw, bh);
            let canc_r = Rect::new(conn_r.x - bw - unit / 2, by, bw, bh);

            draw_pill_btn(
                canvas,
                canc_r,
                tr(lang, TextId::Cancel),
                false,
                self.hover == HoverTarget::WifiModalCancel,
                label_st,
            );
            draw_pill_btn(
                canvas,
                conn_r,
                tr(lang, TextId::WifiConnectBtn),
                true,
                self.hover == HoverTarget::WifiModalConnect,
                label_st,
            );
        }
    }

    fn draw_page_bluetooth(
        &self,
        canvas: &mut Canvas<'_>,
        pane: Rect,
        ctx: &Context,
        label_st: &Style,
        sub_st: &Style,
    ) {
        let unit = ctx.unit;
        let lang = ctx.settings.language;
        let wl = wireless::global();

        let top_bar = Self::wifi_toggle_rect(pane, unit);
        draw_panel(canvas, top_bar, unit / 3, CARD_ALT, 255, BORDER_LIGHT, 255);
        draw_switch(
            canvas,
            top_bar.x + unit / 2,
            top_bar.y + (top_bar.height - 18) / 2,
            wl.bt_enabled,
        );
        draw_text_clipped(
            canvas,
            &wl.bt_adapter_name,
            top_bar.x + unit * 5 / 2,
            top_bar.y + (top_bar.height - label_st.cap_height) / 2,
            top_bar.width - unit * 3,
            label_st,
            INK_DARK,
        );

        let scan_btn = Self::wifi_scan_rect(pane, unit);
        let scan_lbl = if wl.bt_scanning {
            tr(lang, TextId::BtSearching)
        } else {
            tr(lang, TextId::BtScanBtn)
        };
        draw_pill_btn(
            canvas,
            scan_btn,
            scan_lbl,
            wl.bt_scanning,
            self.hover == HoverTarget::BtScanBtn,
            label_st,
        );

        if !wl.bt_enabled {
            draw_text_color(
                canvas,
                tr(lang, TextId::BtRadioOff),
                pane.x + unit,
                pane.y + unit * 9,
                label_st,
                INK_MUTED,
            );
            return;
        }

        for (idx, dev) in wl.bt_devices.iter().take(5).enumerate() {
            let row = Self::wifi_row_rect(pane, idx, unit);
            let hov = self.hover == HoverTarget::BtRow(idx);
            let fill = if dev.connected {
                CARD_SELECT
            } else if hov {
                CARD_HOVER
            } else {
                CARD_WHITE
            };
            let brd = if dev.connected {
                AZURE_BRIGHT
            } else if hov {
                AQUA_MINT
            } else {
                BORDER_LIGHT
            };
            draw_panel(canvas, row, unit / 3, fill, 255, brd, 255);

            draw_dot(
                canvas,
                row.x + unit,
                row.y + row.height / 2,
                5,
                if dev.connected { AZURE_BRIGHT } else { INK_MUTED },
                255,
            );

            let badge = Rect::new(
                row.x + row.width - unit * 6 - unit / 2,
                row.y + unit / 4,
                unit * 6,
                row.height - unit / 2,
            );
            let text_max = (badge.x - (row.x + unit * 2) - unit / 2).max(40);

            draw_text_clipped(
                canvas,
                &dev.name,
                row.x + unit * 2,
                row.y + unit / 4,
                text_max,
                label_st,
                INK_DARK,
            );

            let mut info_l = Line::new();
            info_l.text(&dev.kind).text(" | ").text(&dev.mac);
            draw_text_clipped(
                canvas,
                info_l.as_str(),
                row.x + unit * 2,
                row.y + unit * 7 / 5,
                text_max,
                sub_st,
                INK_MUTED,
            );

            let act_lbl = if dev.connected {
                tr(lang, TextId::BtConnectedBtn)
            } else {
                tr(lang, TextId::WifiConnectBtn)
            };
            draw_pill_btn(canvas, badge, act_lbl, dev.connected, hov, sub_st);
        }

        draw_text_clipped(
            canvas,
            &wl.status_message,
            pane.x + unit,
            pane.y + pane.height - unit * 2,
            pane.width - unit * 2,
            sub_st,
            AZURE_DEEP,
        );
    }

    fn draw_page_drivers(
        &self,
        canvas: &mut Canvas<'_>,
        pane: Rect,
        ctx: &Context,
        label_st: &Style,
        sub_st: &Style,
    ) {
        let unit = ctx.unit;
        let lang = ctx.settings.language;
        let drv_reg = nc_driver::global();

        let top_card = Rect::new(
            pane.x + unit,
            pane.y + unit * 7 / 2,
            pane.width - unit * 2,
            unit * 2,
        );
        draw_panel(canvas, top_card, unit / 3, CARD_ALT, 255, BORDER_LIGHT, 255);

        let mut env_line = Line::new();
        env_line
            .text(drv_reg.environment.label())
            .text("  |  PAT Write-Combining");
        draw_text_clipped(
            canvas,
            env_line.as_str(),
            top_card.x + unit,
            top_card.y + (top_card.height - label_st.cap_height) / 2,
            top_card.width - unit * 2,
            label_st,
            INK_DARK,
        );

        for (idx, dev) in drv_reg.devices.iter().take(4).enumerate() {
            let row = Self::drv_row_rect(pane, idx, unit);
            let selected = self.selected_device == idx;
            let hov = self.hover == HoverTarget::DrvRow(idx);
            let fill = if selected {
                CARD_SELECT
            } else if hov {
                CARD_HOVER
            } else {
                CARD_WHITE
            };
            let brd = if selected {
                AZURE_BRIGHT
            } else if hov {
                AQUA_MINT
            } else {
                BORDER_LIGHT
            };
            draw_panel(canvas, row, unit / 3, fill, 255, brd, 255);

            draw_dot(
                canvas,
                row.x + unit * 3 / 4,
                row.y + row.height / 2,
                5,
                if dev.is_generic_binding {
                    AZURE_BRIGHT
                } else {
                    AQUA_MINT
                },
                255,
            );

            let badge = Rect::new(
                row.x + row.width - unit * 6 - unit / 2,
                row.y + unit / 4,
                unit * 6,
                row.height - unit / 2,
            );
            let max_tw = (badge.x - (row.x + unit * 7 / 4) - unit / 2).max(40);

            draw_text_clipped(
                canvas,
                &dev.hw_name,
                row.x + unit * 7 / 4,
                row.y + unit / 4,
                max_tw,
                label_st,
                INK_DARK,
            );

            let mut detail = Line::new();
            detail
                .text("[")
                .text(&dev.slot_label)
                .text("] ")
                .text(&dev.driver_file);
            draw_text_clipped(
                canvas,
                detail.as_str(),
                row.x + unit * 7 / 4,
                row.y + unit * 7 / 5,
                max_tw,
                sub_st,
                INK_MUTED,
            );

            let status_txt = if dev.is_generic_binding {
                "Generic"
            } else {
                "Optimized"
            };
            draw_pill_btn(canvas, badge, status_txt, !dev.is_generic_binding, hov, sub_st);
        }

        draw_text_clipped(
            canvas,
            &self.efra_status_note,
            pane.x + unit,
            pane.y + pane.height - unit * 5,
            pane.width - unit * 2,
            sub_st,
            AQUA_DEEP,
        );

        let b0 = Self::drv_action_rect(pane, 0, unit);
        let b1 = Self::drv_action_rect(pane, 1, unit);
        let b2 = Self::drv_action_rect(pane, 2, unit);

        draw_pill_btn(
            canvas,
            b0,
            tr(lang, TextId::DrvScanBtn),
            false,
            self.hover == HoverTarget::DrvScanBtn,
            sub_st,
        );
        draw_pill_btn(
            canvas,
            b1,
            tr(lang, TextId::DrvOptimizeBtn),
            true,
            self.hover == HoverTarget::DrvUpgradeBtn,
            sub_st,
        );
        draw_pill_btn(
            canvas,
            b2,
            tr(lang, TextId::DrvCompileBtn),
            false,
            self.hover == HoverTarget::DrvCompileBtn,
            sub_st,
        );

        if let Some(ref drv_buf) = self.efra_compile_prompt {
            let m = Self::pass_modal_rect(pane, unit);
            draw_panel(canvas, m, unit / 2, CARD_WHITE, 252, AZURE_BRIGHT, 255);
            draw_text_clipped(
                canvas,
                "EfraCompiler (.efra)",
                m.x + unit,
                m.y + unit,
                m.width - unit * 2,
                label_st,
                INK_DARK,
            );

            let inp = Rect::new(m.x + unit, m.y + unit * 4, m.width - unit * 2, unit * 5 / 2);
            draw_panel(canvas, inp, unit / 3, CARD_ALT, 255, AZURE_BRIGHT, 255);
            let ty = inp.y + (inp.height - label_st.cap_height) / 2;
            draw_text_clipped(canvas, drv_buf, inp.x + 10, ty, inp.width - 24, label_st, INK_DARK);

            let bw = unit * 8;
            let bh = unit * 2;
            let by = m.y + m.height - bh - unit;
            let build_r = Rect::new(m.x + m.width - bw - unit, by, bw, bh);
            let canc_r = Rect::new(build_r.x - bw - unit / 2, by, bw, bh);

            draw_pill_btn(
                canvas,
                canc_r,
                tr(lang, TextId::Cancel),
                false,
                self.hover == HoverTarget::DrvModalCancel,
                label_st,
            );
            draw_pill_btn(
                canvas,
                build_r,
                tr(lang, TextId::DrvBuildInstallBtn),
                true,
                self.hover == HoverTarget::DrvModalBuild,
                label_st,
            );
        }
    }

    fn draw_page_system(
        &self,
        canvas: &mut Canvas<'_>,
        pane: Rect,
        ctx: &Context,
        label_st: &Style,
        sub_st: &Style,
    ) {
        let unit = ctx.unit;
        let lang = ctx.settings.language;

        draw_text_clipped(
            canvas,
            tr(lang, TextId::LaunchSpeedLabel),
            pane.x + unit,
            pane.y + unit * 7 / 2,
            pane.width - unit * 2,
            label_st,
            INK_DARK,
        );

        let modes = [
            (1u8, tr(lang, TextId::ModeCinematic), tr(lang, TextId::ModeCinematicSub)),
            (2u8, tr(lang, TextId::ModeBalanced), tr(lang, TextId::ModeBalancedSub)),
            (3u8, tr(lang, TextId::ModeInstant), tr(lang, TextId::ModeInstantSub)),
        ];
        for (m_val, m_title, m_desc) in modes {
            let card = Self::launch_speed_rect(pane, m_val - 1, unit);
            let sel = ctx.settings.launch_anim_speed == m_val;
            let hov = self.hover == HoverTarget::LaunchSpeed(m_val);
            let fill = if sel {
                CARD_SELECT
            } else if hov {
                CARD_HOVER
            } else {
                CARD_ALT
            };
            let brd = if sel { AZURE_BRIGHT } else { BORDER_LIGHT };
            draw_panel(canvas, card, unit / 3, fill, 255, brd, 255);
            draw_text_clipped(
                canvas,
                m_title,
                card.x + unit / 2,
                card.y + unit / 2,
                card.width - unit,
                label_st,
                INK_DARK,
            );
            draw_text_clipped(
                canvas,
                m_desc,
                card.x + unit / 2,
                card.y + unit * 7 / 4,
                card.width - unit,
                sub_st,
                INK_MUTED,
            );
        }

        // Dil Seçimi Satırı
        let lang_y = pane.y + unit * 9 + unit / 2;
        draw_text_clipped(
            canvas,
            tr(lang, TextId::SystemLanguageLabel),
            pane.x + unit,
            lang_y + (unit * 9 / 4 - label_st.cap_height) / 2,
            unit * 8,
            label_st,
            INK_DARK,
        );
        let l0 = Self::lang_btn_rect(pane, 0, unit);
        let l1 = Self::lang_btn_rect(pane, 1, unit);
        draw_pill_btn(
            canvas,
            l0,
            tr(lang, TextId::LangNameTr),
            ctx.settings.language == Language::Turkish,
            self.hover == HoverTarget::LangBtn(Language::Turkish),
            sub_st,
        );
        draw_pill_btn(
            canvas,
            l1,
            tr(lang, TextId::LangNameEn),
            ctx.settings.language == Language::English,
            self.hover == HoverTarget::LangBtn(Language::English),
            sub_st,
        );

        // Klavye Düzeni Satırı
        let kbd_y = pane.y + unit * 12 + unit / 2;
        draw_text_clipped(
            canvas,
            tr(lang, TextId::KeyboardLayoutLabel),
            pane.x + unit,
            kbd_y + (unit * 9 / 4 - label_st.cap_height) / 2,
            unit * 8,
            label_st,
            INK_DARK,
        );
        let k0 = Self::kbd_btn_rect(pane, 0, unit);
        let k1 = Self::kbd_btn_rect(pane, 1, unit);
        draw_pill_btn(
            canvas,
            k0,
            tr(lang, TextId::KbdNameTrQ),
            ctx.settings.keyboard == KeyboardLayout::TurkishQ,
            self.hover == HoverTarget::KbdBtn(KeyboardLayout::TurkishQ),
            sub_st,
        );
        draw_pill_btn(
            canvas,
            k1,
            tr(lang, TextId::KbdNameUs),
            ctx.settings.keyboard == KeyboardLayout::UsQwerty,
            self.hover == HoverTarget::KbdBtn(KeyboardLayout::UsQwerty),
            sub_st,
        );

        let sys_box = Rect::new(
            pane.x + unit,
            pane.y + unit * 15 + unit * 3 / 4,
            pane.width - unit * 2,
            unit * 5,
        );
        draw_panel(canvas, sys_box, unit / 2, CARD_ALT, 255, BORDER_LIGHT, 255);

        let mut res_l = Line::new();
        res_l
            .text("Framebuffer: ")
            .number(ctx.screen_width as u32, 1)
            .text("x")
            .number(ctx.screen_height as u32, 1)
            .text(" (32-bit PAT WC)");
        draw_text_clipped(
            canvas,
            res_l.as_str(),
            sys_box.x + unit,
            sys_box.y + unit * 3 / 4,
            sys_box.width - unit * 2,
            label_st,
            INK_DARK,
        );

        let mut ram_l = Line::new();
        ram_l
            .text("RAM: ")
            .number(ctx.total_memory_mb, 1)
            .text(" MB   |   Platform: ")
            .text(ctx.platform);
        draw_text_clipped(
            canvas,
            ram_l.as_str(),
            sys_box.x + unit,
            sys_box.y + unit * 5 / 2,
            sys_box.width - unit * 2,
            sub_st,
            INK_BODY,
        );

        let reset_r = Self::reset_btn_rect(pane, unit);
        draw_pill_btn(
            canvas,
            reset_r,
            tr(lang, TextId::ResetDefaults),
            false,
            self.hover == HoverTarget::ResetDefaultsBtn,
            sub_st,
        );
    }
}

fn draw_switch(canvas: &mut Canvas<'_>, x: i32, y: i32, on: bool) {
    let pill = Rect::new(x, y, 36, 18);
    let fill = if on { AQUA_MINT } else { BORDER_LIGHT };
    draw_panel(canvas, pill, 9, fill, 255, fill, 0);
    let kx = if on { x + 26 } else { x + 10 };
    draw_dot(canvas, kx, y + 9, 6, WHITE, 255);
}

fn draw_pill_btn(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    text: &str,
    active: bool,
    hovered: bool,
    style: &Style,
) {
    let (fill, brd, fg) = if active {
        (
            if hovered { AZURE_DEEP } else { AZURE_BRIGHT },
            AZURE_DEEP,
            WHITE,
        )
    } else if hovered {
        (CARD_HOVER, AZURE_BRIGHT, INK_DARK)
    } else {
        (CARD_WHITE, BORDER_LIGHT, INK_BODY)
    };
    draw_panel(canvas, rect, rect.height / 2, fill, 255, brd, 255);
    let max_tw = (rect.width - 8).max(8);
    let tw = text_width(text, style).min(max_tw);
    draw_text_clipped(
        canvas,
        text,
        rect.x + (rect.width - tw) / 2,
        rect.y + (rect.height - style.cap_height) / 2,
        max_tw,
        style,
        fg,
    );
}

fn inside(area: Rect, x: i32, y: i32) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}
