// crates/nc-net/src/wireless.rs
extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use nc_arch::{inb, outb};
use nc_driver::pci::enumerate_pci_bus;

const COM2_PORT: u16 = 0x02F8;

/// Teknik bağlam (Bölüm 7) uyarınca Kablosuz ve Bluetooth durum makinesi
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RadioLifecycleState {
    NotPresent,
    Discovered,
    DriverMatched,
    FirmwareRequired(&'static str),
    BridgeTransportActive,
    NativeOperational,
}

impl RadioLifecycleState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotPresent => "Donanim Bulunamadi",
            Self::Discovered => "PCIe/USB Kesfedildi (Discovered)",
            Self::DriverMatched => "Yonga Surucusu Eslesti (DriverMatched)",
            Self::FirmwareRequired(fw) => fw,
            Self::BridgeTransportActive => "Harici HCI/802.11 Koprusu Aktif",
            Self::NativeOperational => "Native Sürücü Aktif (Operational)",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WifiChipsetFamily {
    None,
    IntelIwlwifi,
    RealtekRtw88,
    RealtekRtw89,
    MediaTekMt76,
    QualcommAtheros,
    BroadcomFullMac,
    VirtioOrGeneric,
}

#[derive(Clone, Debug)]
pub struct WifiNetworkInfo {
    pub ssid: String,
    pub channel: u8,
    pub freq_label: String,
    pub signal_pct: u8,
    pub security: String,
    pub connected: bool,
}

impl WifiNetworkInfo {
    pub fn bars(&self) -> u8 {
        match self.signal_pct {
            0..=24 => 1,
            25..=49 => 2,
            50..=74 => 3,
            _ => 4,
        }
    }

    pub fn is_open(&self) -> bool {
        self.security.is_empty()
            || self.security == "--"
            || self.security.eq_ignore_ascii_case("none")
            || self.security.eq_ignore_ascii_case("acik")
    }
}

#[derive(Clone, Debug)]
pub struct BluetoothDeviceInfo {
    pub mac: String,
    pub name: String,
    pub kind: String,
    pub battery: u8,
    pub connected: bool,
}

#[derive(Clone, Copy)]
pub struct PciRadioHardware {
    pub wifi_pci_found: bool,
    pub wifi_vendor: u16,
    pub wifi_device: u16,
    pub wifi_subsys_vendor: u16,
    pub wifi_subsys_device: u16,
    pub wifi_revision: u8,
    pub wifi_bus: u8,
    pub wifi_dev: u8,
    pub wifi_irq: u8,
    pub wifi_bar0: u32,
    pub wifi_family: WifiChipsetFamily,
    pub wifi_state: RadioLifecycleState,
    pub bt_controller_found: bool,
    pub bt_vendor: u16,
    pub bt_device: u16,
    pub bt_is_usb_xhci: bool,
    pub bt_state: RadioLifecycleState,
}

fn classify_wifi_chipset(
    vendor: u16,
    device: u16,
) -> (&'static str, WifiChipsetFamily, &'static str) {
    match (vendor, device) {
        (0x8086, 0x2723) => (
            "Intel Wi-Fi 6 AX200 160MHz",
            WifiChipsetFamily::IntelIwlwifi,
            "iwlwifi-cc-a0.ucode",
        ),
        (0x8086, 0x06F0) | (0x8086, 0xA0F0) | (0x8086, 0x51F0) | (0x8086, 0x51F1) => (
            "Intel Wi-Fi 6 AX201 / AX211 CNVi",
            WifiChipsetFamily::IntelIwlwifi,
            "iwlwifi-QuZ-a0.ucode",
        ),
        (0x8086, 0x2725) => (
            "Intel Wi-Fi 6E AX210 160MHz",
            WifiChipsetFamily::IntelIwlwifi,
            "iwlwifi-ty-a0.ucode",
        ),
        (0x8086, 0x2526) | (0x8086, 0x9DF0) => (
            "Intel Wireless-AC 9260 / 9560",
            WifiChipsetFamily::IntelIwlwifi,
            "iwlwifi-9260.ucode",
        ),
        (0x10EC, 0xC821) | (0x10EC, 0xB822) | (0x10EC, 0xC822) => (
            "Realtek RTL8821CE / RTL8822CE 802.11ac",
            WifiChipsetFamily::RealtekRtw88,
            "rtw88/rtw8822c_fw.bin",
        ),
        (0x10EC, 0x8852) | (0x10EC, 0xB852) | (0x10EC, 0xA85A) => (
            "Realtek RTL8852AE / RTL8852BE Wi-Fi 6",
            WifiChipsetFamily::RealtekRtw89,
            "rtw89/rtw8852a_fw.bin",
        ),
        (0x14C3, 0x7961) | (0x14C3, 0x0608) | (0x14C3, 0x7922) => (
            "MediaTek MT7921 / MT7922 (RZ608) Wi-Fi 6E",
            WifiChipsetFamily::MediaTekMt76,
            "mediatek/WIFI_MT7961_patch.bin",
        ),
        (0x168C, _) => (
            "Qualcomm Atheros QCA9377 / QCA6174 802.11ac",
            WifiChipsetFamily::QualcommAtheros,
            "ath10k/QCA6174/hw3.0/firmware.bin",
        ),
        (0x14E4, _) => (
            "Broadcom BCM43xx 802.11ac FullMAC",
            WifiChipsetFamily::BroadcomFullMac,
            "brcm/brcmfmac43602-pcie.bin",
        ),
        _ => (
            "Generic PCIe 802.11 Kablosuz Denetleyicisi",
            WifiChipsetFamily::VirtioOrGeneric,
            "generic-80211.fw",
        ),
    }
}

pub fn scan_pci_radio_hardware() -> PciRadioHardware {
    let mut hw = PciRadioHardware {
        wifi_pci_found: false,
        wifi_vendor: 0,
        wifi_device: 0,
        wifi_subsys_vendor: 0,
        wifi_subsys_device: 0,
        wifi_revision: 0,
        wifi_bus: 0,
        wifi_dev: 0,
        wifi_irq: 0,
        wifi_bar0: 0,
        wifi_family: WifiChipsetFamily::None,
        wifi_state: RadioLifecycleState::NotPresent,
        bt_controller_found: false,
        bt_vendor: 0,
        bt_device: 0,
        bt_is_usb_xhci: false,
        bt_state: RadioLifecycleState::NotPresent,
    };

    for ep in enumerate_pci_bus() {
        // 1. PCIe 802.11 Kablosuz Ağ Denetleyicisi (Class 0x02, Subclass 0x80)
        if ep.class_code == 0x02 && ep.subclass == 0x80 && !hw.wifi_pci_found {
            let (_, family, fw_name) = classify_wifi_chipset(ep.vendor_id, ep.device_id);
            let bar0 = ep.bar(0).mmio_base().unwrap_or(0) as u32;

            hw.wifi_pci_found = true;
            hw.wifi_vendor = ep.vendor_id;
            hw.wifi_device = ep.device_id;
            hw.wifi_subsys_vendor = ep.subsys_vendor_id;
            hw.wifi_subsys_device = ep.subsys_id;
            hw.wifi_revision = ep.revision_id;
            hw.wifi_bus = ep.loc.bus;
            hw.wifi_dev = ep.loc.dev;
            hw.wifi_irq = ep.irq_line;
            hw.wifi_bar0 = bar0;
            hw.wifi_family = family;
            hw.wifi_state = if bar0 != 0 {
                RadioLifecycleState::FirmwareRequired(fw_name)
            } else {
                RadioLifecycleState::Discovered
            };
        }

        // 2. Bluetooth HCI: Doğrudan Wireless/BT PCI (0x0D) veya Combo kartın bağlı olduğu USB xHCI (0x0C:0x03)
        if (ep.class_code == 0x0D || (ep.class_code == 0x0C && ep.subclass == 0x03))
            && !hw.bt_controller_found
        {
            hw.bt_controller_found = true;
            hw.bt_vendor = ep.vendor_id;
            hw.bt_device = ep.device_id;
            hw.bt_is_usb_xhci = ep.class_code == 0x0C;
            hw.bt_state = RadioLifecycleState::DriverMatched;
        }
    }

    hw
}

/// Parola ve gizli anahtarları loglarda maskeler (Örn: "gizlisifre" -> "********")
pub fn mask_secret(secret: &str) -> String {
    if secret.is_empty() {
        String::from("<acik-ag>")
    } else {
        let count = secret.chars().count().clamp(4, 12);
        let mut s = String::with_capacity(count);
        for _ in 0..count {
            s.push('*');
        }
        s
    }
}

pub struct WirelessManager {
    pub pci_hw: PciRadioHardware,
    pub uart_present: bool,
    pub bridge_online: bool,
    pub wifi_adapter_name: String,
    pub bt_adapter_name: String,
    pub wifi_enabled: bool,
    pub bt_enabled: bool,
    pub wifi_scanning: bool,
    pub bt_scanning: bool,
    pub wifi_networks: Vec<WifiNetworkInfo>,
    pub bt_devices: Vec<BluetoothDeviceInfo>,
    pub status_message: String,
    rx_line: Vec<u8>,
    staging_wifi: Vec<WifiNetworkInfo>,
    staging_bt: Vec<BluetoothDeviceInfo>,
    poll_ticks: u32,
}

impl WirelessManager {
    pub fn new() -> Self {
        let mut pci_hw = scan_pci_radio_hardware();
        let uart_present = Self::init_com2_uart();

        if uart_present {
            pci_hw.wifi_state = RadioLifecycleState::BridgeTransportActive;
            pci_hw.bt_state = RadioLifecycleState::BridgeTransportActive;
        }

        let wifi_name = if pci_hw.wifi_pci_found {
            let (model, _, _) = classify_wifi_chipset(pci_hw.wifi_vendor, pci_hw.wifi_device);
            alloc::format!(
                "{} [{:04X}:{:04X} rev {:02X}]",
                model,
                pci_hw.wifi_vendor,
                pci_hw.wifi_device,
                pci_hw.wifi_revision
            )
        } else if uart_present {
            String::from("Harici 802.11 Radyo Transport Koprusu")
        } else {
            String::from("Fiziksel 802.11 PCIe bagdastirici bulunamadi")
        };

        let bt_name = if pci_hw.bt_controller_found {
            if pci_hw.bt_is_usb_xhci {
                alloc::format!(
                    "USB xHCI Bluetooth HCI Transport [{:04X}:{:04X}]",
                    pci_hw.bt_vendor,
                    pci_hw.bt_device
                )
            } else {
                alloc::format!(
                    "PCI Bluetooth HCI Denetleyicisi [{:04X}:{:04X}]",
                    pci_hw.bt_vendor,
                    pci_hw.bt_device
                )
            }
        } else if uart_present {
            String::from("Harici Bluetooth HCI Transport Koprusu")
        } else {
            String::from("Bluetooth HCI denetleyicisi bulunamadi")
        };

        let initial_status = if uart_present {
            String::from("Radyo donanimi ve HCI transport koprusu sorgulaniyor...")
        } else if let RadioLifecycleState::FirmwareRequired(fw) = pci_hw.wifi_state {
            alloc::format!(
                "Kesfedildi (DriverMatched) — Native 802.11 icin '/system/firmware/{}' gerekli.",
                fw
            )
        } else {
            String::from("Bu makinede kablosuz donanim veya radyo koprusu algilanmadi.")
        };

        let mut mgr = WirelessManager {
            pci_hw,
            uart_present,
            bridge_online: false,
            wifi_adapter_name: wifi_name,
            bt_adapter_name: bt_name,
            wifi_enabled: true,
            bt_enabled: true,
            wifi_scanning: false,
            bt_scanning: false,
            wifi_networks: Vec::new(),
            bt_devices: Vec::new(),
            status_message: initial_status,
            rx_line: Vec::with_capacity(256),
            staging_wifi: Vec::new(),
            staging_bt: Vec::new(),
            poll_ticks: 0,
        };

        if uart_present {
            mgr.send_command("CMD:INIT");
            mgr.send_command("CMD:WIFI_SCAN");
            mgr.send_command("CMD:BT_SCAN");
            mgr.wifi_scanning = true;
            mgr.bt_scanning = true;
        }
        mgr
    }

    fn init_com2_uart() -> bool {
        unsafe {
            outb(COM2_PORT + 1, 0x00);
            outb(COM2_PORT + 3, 0x80);
            outb(COM2_PORT + 0, 0x01);
            outb(COM2_PORT + 1, 0x00);
            outb(COM2_PORT + 3, 0x03);
            outb(COM2_PORT + 2, 0xC7);
            outb(COM2_PORT + 4, 0x0B);
            outb(COM2_PORT + 7, 0xA5);
            inb(COM2_PORT + 7) == 0xA5
        }
    }

    pub fn send_command(&mut self, cmd: &str) {
        if !self.uart_present {
            return;
        }
        for b in cmd.bytes() {
            Self::write_com2_byte(b);
        }
        Self::write_com2_byte(b'\n');
    }

    fn write_com2_byte(byte: u8) {
        unsafe {
            let mut spins = 0u32;
            while (inb(COM2_PORT + 5) & 0x20) == 0 {
                spins += 1;
                if spins > 50_000 {
                    return;
                }
            }
            outb(COM2_PORT, byte);
        }
    }

    pub fn request_wifi_scan(&mut self) {
        if !self.wifi_enabled {
            self.status_message = String::from("Kablosuz alici kapali.");
            return;
        }
        if !self.uart_present {
            if let RadioLifecycleState::FirmwareRequired(fw) = self.pci_hw.wifi_state {
                self.status_message = alloc::format!(
                    "Donanim kesfedildi ({:04X}:{:04X}); tarama icin '{}' firmware bekleniyor.",
                    self.pci_hw.wifi_vendor,
                    self.pci_hw.wifi_device,
                    fw
                );
            }
            return;
        }
        self.wifi_scanning = true;
        self.status_message = String::from("Cevredeki 802.11 aglari taraniyor...");
        self.send_command("CMD:WIFI_SCAN");
    }

    pub fn request_bt_scan(&mut self) {
        if !self.bt_enabled {
            self.status_message = String::from("Bluetooth denetleyicisi kapali.");
            return;
        }
        if !self.uart_present {
            self.status_message = String::from(
                "Bluetooth denetleyicisi kesfedildi (DriverMatched); USB HCI enumeration bekleniyor.",
            );
            return;
        }
        self.bt_scanning = true;
        self.status_message = String::from("Bluetooth HCI sorgusu (Inquiry) yurutuluyor...");
        self.send_command("CMD:BT_SCAN");
    }

    pub fn set_wifi_enabled(&mut self, enabled: bool) {
        self.wifi_enabled = enabled;
        if enabled {
            self.send_command("CMD:WIFI_ON");
            self.request_wifi_scan();
        } else {
            self.send_command("CMD:WIFI_OFF");
            self.wifi_networks.clear();
            self.wifi_scanning = false;
            self.status_message = String::from("802.11 radyo vericisi kapatildi.");
        }
    }

    pub fn set_bt_enabled(&mut self, enabled: bool) {
        self.bt_enabled = enabled;
        if enabled {
            self.send_command("CMD:BT_ON");
            self.request_bt_scan();
        } else {
            self.send_command("CMD:BT_OFF");
            self.bt_devices.clear();
            self.bt_scanning = false;
            self.status_message = String::from("Bluetooth HCI radyosu kapatildi.");
        }
    }

    pub fn connect_wifi(&mut self, ssid: &str, password: &str) {
        let masked = mask_secret(password);
        self.status_message = alloc::format!("'{}' agina baglaniliyor (Kimlik: {})...", ssid, masked);
        let cmd = alloc::format!("CMD:WIFI_CONN|{}|{}", ssid, password);
        self.send_command(&cmd);
    }

    pub fn disconnect_wifi(&mut self, ssid: &str) {
        self.status_message = alloc::format!("'{}' ag baglantisi kesiliyor...", ssid);
        let cmd = alloc::format!("CMD:WIFI_DISC|{}", ssid);
        self.send_command(&cmd);
    }

    pub fn toggle_bt_device(&mut self, idx: usize) {
        if idx >= self.bt_devices.len() {
            return;
        }
        let mac = self.bt_devices[idx].mac.clone();
        let name = self.bt_devices[idx].name.clone();
        let was_conn = self.bt_devices[idx].connected;
        if was_conn {
            self.status_message = alloc::format!("'{}' baglantisi kesiliyor...", name);
            let cmd = alloc::format!("CMD:BT_DISC|{}", mac);
            self.send_command(&cmd);
        } else {
            self.status_message = alloc::format!("'{}' ile HCI baglantisi kuruluyor...", name);
            let cmd = alloc::format!("CMD:BT_CONN|{}", mac);
            self.send_command(&cmd);
        }
    }

    pub fn poll(&mut self) {
        if !self.uart_present {
            return;
        }
        self.poll_ticks = self.poll_ticks.wrapping_add(1);
        let mut budget = 2048usize;
        while budget > 0 {
            budget -= 1;
            let lsr = unsafe { inb(COM2_PORT + 5) };
            if (lsr & 0x01) == 0 {
                break;
            }
            let byte = unsafe { inb(COM2_PORT) };
            if byte == b'\r' {
                continue;
            }
            if byte == b'\n' {
                if !self.rx_line.is_empty() {
                    let line_bytes = core::mem::take(&mut self.rx_line);
                    if let Ok(line_str) = core::str::from_utf8(&line_bytes) {
                        self.process_line(line_str.trim());
                    }
                }
            } else if self.rx_line.len() < 512 {
                self.rx_line.push(byte);
            }
        }
    }

    fn process_line(&mut self, line: &str) {
        if line.is_empty() {
            return;
        }
        self.bridge_online = true;
        if let Some(rest) = line.strip_prefix("H|") {
            let mut parts = rest.split('|');
            if let Some(w_hw) = parts.next() {
                if !w_hw.trim().is_empty() {
                    self.wifi_adapter_name = String::from(w_hw.trim());
                }
            }
            if let Some(b_hw) = parts.next() {
                if !b_hw.trim().is_empty() {
                    self.bt_adapter_name = String::from(b_hw.trim());
                }
            }
            if let Some(w_on) = parts.next() {
                self.wifi_enabled = w_on.trim() == "1";
            }
            if let Some(b_on) = parts.next() {
                self.bt_enabled = b_on.trim() == "1";
            }
        } else if line == "W_CLEAR" {
            self.staging_wifi.clear();
        } else if let Some(rest) = line.strip_prefix("W|") {
            let mut parts = rest.split('|');
            let in_use = parts.next().unwrap_or("0").trim() == "1";
            let ssid = parts.next().unwrap_or("").trim();
            let chan = parts
                .next()
                .and_then(|s| s.trim().parse::<u8>().ok())
                .unwrap_or(1);
            let freq = parts.next().unwrap_or("2.4 GHz").trim();
            let signal = parts
                .next()
                .and_then(|s| s.trim().parse::<u8>().ok())
                .unwrap_or(50)
                .min(100);
            let sec = parts.next().unwrap_or("WPA2").trim();
            if !ssid.is_empty() && ssid != "--" {
                if let Some(existing) = self.staging_wifi.iter_mut().find(|n| n.ssid == ssid) {
                    if in_use {
                        existing.connected = true;
                    }
                    if signal > existing.signal_pct {
                        existing.signal_pct = signal;
                    }
                } else if self.staging_wifi.len() < 8 {
                    self.staging_wifi.push(WifiNetworkInfo {
                        ssid: String::from(ssid),
                        channel: chan,
                        freq_label: String::from(freq),
                        signal_pct: signal,
                        security: String::from(if sec.is_empty() { "Acik" } else { sec }),
                        connected: in_use,
                    });
                }
            }
        } else if line == "W_END" {
            self.wifi_networks = self.staging_wifi.clone();
            self.wifi_scanning = false;
            self.status_message = alloc::format!(
                "Tarama tamamlandi: {} kablosuz ag bulundu.",
                self.wifi_networks.len()
            );
        } else if line == "B_CLEAR" {
            self.staging_bt.clear();
        } else if let Some(rest) = line.strip_prefix("B|") {
            let mut parts = rest.split('|');
            let conn = parts.next().unwrap_or("0").trim() == "1";
            let mac = parts.next().unwrap_or("").trim();
            let kind = parts.next().unwrap_or("Bluetooth Cihazi").trim();
            let batt = parts
                .next()
                .and_then(|s| s.trim().parse::<u8>().ok())
                .unwrap_or(0);
            let name = parts.next().unwrap_or("").trim();
            if !mac.is_empty() && !name.is_empty() && self.staging_bt.len() < 8 {
                self.staging_bt.push(BluetoothDeviceInfo {
                    mac: String::from(mac),
                    name: String::from(name),
                    kind: String::from(kind),
                    battery: batt,
                    connected: conn,
                });
            }
        } else if line == "B_END" {
            self.bt_devices = self.staging_bt.clone();
            self.bt_scanning = false;
            self.status_message = alloc::format!(
                "HCI sorgusu bitti: {} Bluetooth cihazi listelendi.",
                self.bt_devices.len()
            );
        } else if let Some(msg) = line.strip_prefix("MSG|") {
            self.status_message = String::from(msg.trim());
        }
    }
}

static mut GLOBAL_WIRELESS: Option<WirelessManager> = None;
static mut WL_MAGIC: u64 = 0;

pub fn global() -> &'static mut WirelessManager {
    unsafe {
        let slot = core::ptr::addr_of_mut!(GLOBAL_WIRELESS);
        let magic = core::ptr::addr_of_mut!(WL_MAGIC);
        if core::ptr::read_volatile(magic) != 0x574C_5F49_4E49_5431 {
            core::ptr::write(slot, None);
            core::ptr::write(slot, Some(WirelessManager::new()));
            core::ptr::write(magic, 0x574C_5F49_4E49_5431);
        }
        (*slot).as_mut().unwrap()
    }
}
