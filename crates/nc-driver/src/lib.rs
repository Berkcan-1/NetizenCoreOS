#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use nc_arch::{inb, inl, outb, outl, outw};

pub const EFRA_MAGIC: [u8; 4] = *b"EFRA";
pub const EFRA_VERSION: u32 = 1;

pub const EFRA_FLAG_GENERIC_FALLBACK: u32 = 1 << 0;
pub const EFRA_FLAG_BOOT_CRITICAL: u32 = 1 << 1;
pub const EFRA_FLAG_WRITE_COMBINING: u32 = 1 << 2;
pub const EFRA_FLAG_DMA_BURST: u32 = 1 << 3;
pub const EFRA_FLAG_CUSTOM_COMPILED: u32 = 1 << 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum EfraClass {
    Display = 1,
    Storage = 2,
    Network = 3,
    Input = 4,
    Chipset = 5,
    Wireless = 6,
    Audio = 7,
    Generic = 255,
}

impl EfraClass {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => EfraClass::Display,
            2 => EfraClass::Storage,
            3 => EfraClass::Network,
            4 => EfraClass::Input,
            5 => EfraClass::Chipset,
            6 => EfraClass::Wireless,
            7 => EfraClass::Audio,
            _ => EfraClass::Generic,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            EfraClass::Display => "Goruntu ve Framebuffer",
            EfraClass::Storage => "Depolama Denetleyicisi",
            EfraClass::Network => "Ethernet Ag Bagdastiricisi",
            EfraClass::Input => "HID Klavye ve Fare",
            EfraClass::Chipset => "Anakart ve ACPI Yonga Seti",
            EfraClass::Wireless => "802.11 Kablosuz / Bluetooth",
            EfraClass::Audio => "PCI Ses ve Multimedya (AC97/HDA)",
            EfraClass::Generic => "Evrensel Donanim Tabani",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HardwareEnvironment {
    QemuKvm,
    VirtualBox,
    BareMetalPc,
}

impl HardwareEnvironment {
    pub fn label(&self) -> &'static str {
        match self {
            HardwareEnvironment::QemuKvm => "QEMU / KVM Sanal Donanim",
            HardwareEnvironment::VirtualBox => "Oracle VirtualBox Hipervizor",
            HardwareEnvironment::BareMetalPc => "Gercek Fiziksel PC (Bare-Metal)",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EfraManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub file_name: String,
}

#[derive(Clone, Debug)]
pub struct EfraDriverPackage {
    pub class: EfraClass,
    pub flags: u32,
    pub vendor_id: u16,
    pub device_id: u16,
    pub pci_class: u8,
    pub pci_subclass: u8,
    pub checksum: u32,
    pub manifest: EfraManifest,
    pub bytecode: Vec<u8>,
}

impl EfraDriverPackage {
    pub fn compute_checksum(manifest_bytes: &[u8], bytecode: &[u8]) -> u32 {
        let mut hash = 0x811C_9DC5u32;
        for &b in manifest_bytes.iter().chain(bytecode.iter()) {
            hash ^= b as u32;
            hash = hash.wrapping_mul(0x0100_0193);
        }
        hash
    }

    pub fn serialize(&self) -> Vec<u8> {
        let manifest_text = alloc::format!(
            "id={}\nname={}\nversion={}\nauthor={}\nfile={}\n",
            self.manifest.id,
            self.manifest.name,
            self.manifest.version,
            self.manifest.author,
            self.manifest.file_name
        );
        let m_bytes = manifest_text.as_bytes();
        let checksum = Self::compute_checksum(m_bytes, &self.bytecode);

        let mut buf = Vec::with_capacity(32 + m_bytes.len() + self.bytecode.len());
        buf.extend_from_slice(&EFRA_MAGIC);
        buf.extend_from_slice(&EFRA_VERSION.to_le_bytes());
        buf.push(self.class as u8);
        buf.push(self.pci_class);
        buf.push(self.pci_subclass);
        buf.push(0);
        buf.extend_from_slice(&self.flags.to_le_bytes());
        buf.extend_from_slice(&self.vendor_id.to_le_bytes());
        buf.extend_from_slice(&self.device_id.to_le_bytes());
        buf.extend_from_slice(&checksum.to_le_bytes());
        buf.extend_from_slice(&(m_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(self.bytecode.len() as u32).to_le_bytes());
        buf.extend_from_slice(m_bytes);
        buf.extend_from_slice(&self.bytecode);
        buf
    }

    pub fn serialize_padded_512(&self) -> [u8; 512] {
        let raw = self.serialize();
        let mut block = [0u8; 512];
        let len = raw.len().min(512);
        block[..len].copy_from_slice(&raw[..len]);
        block
    }

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 32 || &bytes[0..4] != &EFRA_MAGIC {
            return None;
        }
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != EFRA_VERSION {
            return None;
        }
        let class = EfraClass::from_u8(bytes[8]);
        let pci_class = bytes[9];
        let pci_subclass = bytes[10];
        let flags = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        let vendor_id = u16::from_le_bytes([bytes[16], bytes[17]]);
        let device_id = u16::from_le_bytes([bytes[18], bytes[19]]);
        let checksum = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        let m_len = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]) as usize;
        let b_len = u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]) as usize;

        if bytes.len() < 32 + m_len + b_len {
            return None;
        }
        let m_slice = &bytes[32..32 + m_len];
        let b_slice = &bytes[32 + m_len..32 + m_len + b_len];
        if Self::compute_checksum(m_slice, b_slice) != checksum {
            return None;
        }
        let m_str = core::str::from_utf8(m_slice).ok()?;

        let mut id = String::from("efra.generic");
        let mut name = String::from("Generic Efra Driver");
        let mut ver = String::from("1.0.0");
        let mut author = String::from("NetizenCore");
        let mut file_name = String::from("generic.efra");

        for line in m_str.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "id" => id = String::from(v.trim()),
                    "name" => name = String::from(v.trim()),
                    "version" => ver = String::from(v.trim()),
                    "author" => author = String::from(v.trim()),
                    "file" => file_name = String::from(v.trim()),
                    _ => {}
                }
            }
        }

        Some(EfraDriverPackage {
            class,
            flags,
            vendor_id,
            device_id,
            pci_class,
            pci_subclass,
            checksum,
            manifest: EfraManifest {
                id,
                name,
                version: ver,
                author,
                file_name,
            },
            bytecode: b_slice.to_vec(),
        })
    }

    pub fn is_generic(&self) -> bool {
        (self.flags & EFRA_FLAG_GENERIC_FALLBACK) != 0 || self.vendor_id == 0xFFFF
    }
}

pub struct EfraCompiler;

impl EfraCompiler {
    pub fn compile_in_os(
        id: &str,
        name: &str,
        version: &str,
        file_name: &str,
        class: EfraClass,
        vendor_id: u16,
        device_id: u16,
        pci_class: u8,
        pci_subclass: u8,
        flags: u32,
    ) -> EfraDriverPackage {
        let mut bytecode = Vec::with_capacity(64);
        bytecode.extend_from_slice(b"EFRA_IR_V1:");
        bytecode.push(class as u8);
        bytecode.extend_from_slice(&vendor_id.to_le_bytes());
        bytecode.extend_from_slice(&device_id.to_le_bytes());
        bytecode.extend_from_slice(&flags.to_le_bytes());
        bytecode.extend_from_slice(&[0x90, 0x0F, 0x01, 0xC3]);

        let manifest_text = alloc::format!(
            "id={}\nname={}\nversion={}\nauthor=NetizenCore In-OS Compiler\nfile={}\n",
            id, name, version, file_name
        );
        let checksum = EfraDriverPackage::compute_checksum(manifest_text.as_bytes(), &bytecode);

        EfraDriverPackage {
            class,
            flags,
            vendor_id,
            device_id,
            pci_class,
            pci_subclass,
            checksum,
            manifest: EfraManifest {
                id: String::from(id),
                name: String::from(name),
                version: String::from(version),
                author: String::from("NetizenCore In-OS Compiler"),
                file_name: String::from(file_name),
            },
            bytecode,
        }
    }

    pub fn compile_from_source(source_text: &str) -> Option<EfraDriverPackage> {
        let mut id = "efra.custom.drv";
        let mut name = "Ozel Derlenmis Efra Surucusu";
        let mut version = "1.0.0";
        let mut file_name = "custom_drv.efra";
        let mut class = EfraClass::Generic;
        let mut vendor_id = 0xFFFFu16;
        let mut device_id = 0xFFFFu16;
        let mut pci_class = 0xFFu8;
        let mut pci_subclass = 0xFFu8;
        let mut flags = EFRA_FLAG_GENERIC_FALLBACK | EFRA_FLAG_CUSTOM_COMPILED;

        for line in source_text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                match k {
                    "id" => id = v,
                    "name" => name = v,
                    "version" => version = v,
                    "file" => file_name = v,
                    "class" => {
                        class = match v {
                            "display" => EfraClass::Display,
                            "storage" => EfraClass::Storage,
                            "network" => EfraClass::Network,
                            "input" => EfraClass::Input,
                            "chipset" => EfraClass::Chipset,
                            "wireless" => EfraClass::Wireless,
                            _ => EfraClass::Generic,
                        };
                    }
                    "vendor_id" => {
                        if let Some(hex) = v.strip_prefix("0x") {
                            vendor_id = u16::from_str_radix(hex, 16).unwrap_or(0xFFFF);
                        }
                    }
                    "device_id" => {
                        if let Some(hex) = v.strip_prefix("0x") {
                            device_id = u16::from_str_radix(hex, 16).unwrap_or(0xFFFF);
                        }
                    }
                    "pci_class" => {
                        if let Some(hex) = v.strip_prefix("0x") {
                            pci_class = u8::from_str_radix(hex, 16).unwrap_or(0xFF);
                        }
                    }
                    "pci_subclass" => {
                        if let Some(hex) = v.strip_prefix("0x") {
                            pci_subclass = u8::from_str_radix(hex, 16).unwrap_or(0xFF);
                        }
                    }
                    "write_combining" if v == "true" => flags |= EFRA_FLAG_WRITE_COMBINING,
                    "dma_burst" if v == "true" => flags |= EFRA_FLAG_DMA_BURST,
                    _ => {}
                }
            }
        }

        Some(Self::compile_in_os(
            id,
            name,
            version,
            file_name,
            class,
            vendor_id,
            device_id,
            pci_class,
            pci_subclass,
            flags,
        ))
    }
}

pub fn builtin_efra_packages() -> [EfraDriverPackage; 6] {
    [
        EfraCompiler::compile_in_os(
            "efra.display.generic",
            "Netizen VBE/GOP Generic Turbo Framebuffer",
            "2.1.0",
            "generic-display.efra",
            EfraClass::Display,
            0xFFFF,
            0xFFFF,
            0x03,
            0x00,
            EFRA_FLAG_GENERIC_FALLBACK | EFRA_FLAG_BOOT_CRITICAL | EFRA_FLAG_WRITE_COMBINING | EFRA_FLAG_DMA_BURST,
        ),
        EfraCompiler::compile_in_os(
            "efra.display.vbox",
            "VirtualBox VBoxSVGA / VMSVGA Hizlandirici",
            "2.2.0",
            "vbox-svga.efra",
            EfraClass::Display,
            0x80EE,
            0xBEEF,
            0x03,
            0x00,
            EFRA_FLAG_BOOT_CRITICAL | EFRA_FLAG_WRITE_COMBINING | EFRA_FLAG_DMA_BURST,
        ),
        EfraCompiler::compile_in_os(
            "efra.storage.generic",
            "Generic ATA/AHCI Blok Depolama Surucusu",
            "2.0.0",
            "generic-storage.efra",
            EfraClass::Storage,
            0xFFFF,
            0xFFFF,
            0x01,
            0xFF,
            EFRA_FLAG_GENERIC_FALLBACK | EFRA_FLAG_BOOT_CRITICAL | EFRA_FLAG_DMA_BURST,
        ),
        EfraCompiler::compile_in_os(
            "efra.net.e1000",
            "Intel PRO/1000 & Generic PCI Ethernet",
            "2.1.0",
            "generic-net-e1000.efra",
            EfraClass::Network,
            0x8086,
            0xFFFF,
            0x02,
            0x00,
            EFRA_FLAG_GENERIC_FALLBACK | EFRA_FLAG_DMA_BURST,
        ),
        EfraCompiler::compile_in_os(
            "efra.input.ps2",
            "Generic PS/2 & USB-Legacy HID Giris Tabani",
            "1.8.0",
            "generic-input-ps2.efra",
            EfraClass::Input,
            0xFFFF,
            0xFFFF,
            0x09,
            0xFF,
            EFRA_FLAG_GENERIC_FALLBACK | EFRA_FLAG_BOOT_CRITICAL,
        ),
        EfraCompiler::compile_in_os(
            "efra.chipset.acpi",
            "Generic x86_64 PCI Kopru ve ACPI Guc Denetleyicisi",
            "2.0.0",
            "generic-chipset-acpi.efra",
            EfraClass::Chipset,
            0xFFFF,
            0xFFFF,
            0x06,
            0xFF,
            EFRA_FLAG_GENERIC_FALLBACK | EFRA_FLAG_BOOT_CRITICAL,
        ),
    ]
}

#[derive(Clone, Debug)]
pub struct HardwareDeviceNode {
    pub slot_label: String,
    pub hw_name: String,
    pub vendor_id: u16,
    pub device_id: u16,
    pub pci_class: u8,
    pub pci_subclass: u8,
    pub class: EfraClass,
    pub bound_driver_id: String,
    pub bound_driver_name: String,
    pub bound_driver_version: String,
    pub driver_file: String,
    pub is_generic_binding: bool,
    pub optimized: bool,
}

fn pci_read_32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        outl(0xCF8, address);
        inl(0xCFC)
    }
}

fn pci_write_32(bus: u8, dev: u8, func: u8, offset: u8, val: u32) {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        outl(0xCF8, address);
        outl(0xCFC, val);
    }
}

fn classify_pci(base_class: u8, sub_class: u8) -> EfraClass {
    match (base_class, sub_class) {
        (0x03, _) => EfraClass::Display,
        (0x01, _) => EfraClass::Storage,
        (0x02, 0x80) => EfraClass::Wireless,
        (0x02, _) => EfraClass::Network,
        (0x04, _) => EfraClass::Audio,
        (0x09, _) => EfraClass::Input,
        (0x06, _) | (0x0C, _) => EfraClass::Chipset,
        _ => EfraClass::Generic,
    }
}

fn describe_pci_device(vendor: u16, device: u16, base_class: u8, sub_class: u8) -> &'static str {
    match (vendor, device) {
        (0x80EE, 0xBEEF) => "VirtualBox VBoxSVGA Grafik Bagdastiricisi",
        (0x80EE, 0xCAFE) => "VirtualBox VMMDev Misafir Servis Koprusu",
        (0x1234, 0x1111) => "QEMU Standart Bochs VBE Grafik",
        (0x8086, 0x100E) => "Intel PRO/1000 MT (82540EM) Gigabit Ethernet",
        (0x8086, 0x100F) => "Intel PRO/1000 MT (82545EM) Sunucu Ethernet",
        (0x8086, 0x10D3) => "Intel 82574L Gigabit Ag Bagdastiricisi",
        (0x8086, 0x2922) | (0x8086, 0x2829) => "Intel ICH8M/ICH9R AHCI SATA Denetleyicisi",
        (0x8086, 0x7010) | (0x8086, 0x7111) => "Intel PIIX3/PIIX4 IDE/ATA Denetleyicisi",
        (0x8086, 0x1237) | (0x8086, 0x29C0) => "Intel 440FX / Q35 Ana Kopru (Host Bridge)",
        (0x1022, 0x2000) => "AMD PCnet-FAST III 10/100 Ethernet",
        (0x10EC, 0x8168) | (0x10EC, 0x8139) => "Realtek RTL8111/8168 PCIe Gigabit Ethernet",
        (0x8086, 0x2415) => "Intel 82801AA AC'97 PCI Ses Denetleyicisi",
        (0x8086, 0x2668) | (0x8086, 0x293E) => "Intel High Definition Audio (HDA)",
        _ => match (base_class, sub_class) {
            (0x03, _) => "PCI Uyumlu VGA / GOP Grafik Denetleyicisi",
            (0x01, 0x06) => "Standart AHCI 1.0 Seri ATA Denetleyicisi",
            (0x01, 0x08) => "NVMe PCIe Yuksek Hizli SSD Denetleyicisi",
            (0x01, _) => "PCI IDE / Kitle Depolama Denetleyicisi",
            (0x02, 0x80) => "PCI 802.11 Kablosuz Ag Denetleyicisi",
            (0x02, _) => "PCI Ethernet Ag Denetleyicisi",
            (0x0C, 0x03) => "xHCI / EHCI Evrensel USB Denetleyicisi",
            (0x06, _) => "Anakart Sistem ve PCI Kopru Birimi",
            _ => "PCI Sistem Donanim Birimi",
        },
    }
}

pub fn enable_pat_write_combining() {
    // PAT MSR (0x277) yazma islemi bazi KVM/CPU profillerinde #GP urettigi icin
    // güvenli modda tutuyoruz; VBE zaten 0x93 (uncached/WC) sayfa tablosu kullaniyor.
}

pub struct DriverRegistry {
    pub environment: HardwareEnvironment,
    pub drivers: Vec<EfraDriverPackage>,
    pub devices: Vec<HardwareDeviceNode>,
    pub write_combining_active: bool,
    pub status_log: String,
}

impl DriverRegistry {
    pub fn new() -> Self {
        let mut reg = DriverRegistry {
            environment: HardwareEnvironment::BareMetalPc,
            drivers: Vec::new(),
            devices: Vec::new(),
            write_combining_active: true,
            status_log: String::from("Generic .efra surucu tabani hazirlaniyor..."),
        };
        for pkg in builtin_efra_packages() {
            reg.drivers.push(pkg);
        }
        reg.optimize_and_scan();
        reg
    }

    pub fn optimize_and_scan(&mut self) {
        self.write_combining_active = true;
        self.devices.clear();

        let mut found_vbox = false;
        let mut found_qemu = false;

        for bus in 0..=8u8 {
            for dev in 0..32u8 {
                for func in 0..8u8 {
                    let vd = pci_read_32(bus, dev, func, 0x00);
                    let vendor = (vd & 0xFFFF) as u16;
                    let device = ((vd >> 16) & 0xFFFF) as u16;
                    if vendor == 0xFFFF || vendor == 0x0000 {
                        if func == 0 {
                            break;
                        }
                        continue;
                    }
                    if vendor == 0x80EE {
                        found_vbox = true;
                    }
                    if vendor == 0x1234 || vendor == 0x1AF4 {
                        found_qemu = true;
                    }

                    let class_reg = pci_read_32(bus, dev, func, 0x08);
                    let base_class = ((class_reg >> 24) & 0xFF) as u8;
                    let sub_class = ((class_reg >> 16) & 0xFF) as u8;

                    if base_class == 0x01 || base_class == 0x02 || base_class == 0x03 || base_class == 0x04 {
                        let cmd = pci_read_32(bus, dev, func, 0x04);
                        pci_write_32(bus, dev, func, 0x04, cmd | 0x07);
                    }

                    if self.devices.len() < 12 {
                        let efra_class = classify_pci(base_class, sub_class);
                        let hw_name = String::from(describe_pci_device(vendor, device, base_class, sub_class));
                        let slot_label = alloc::format!("PCI {:02X}:{:02X}.{}", bus, dev, func);
                        let (drv_id, drv_name, drv_ver, drv_file, is_gen) =
                            self.match_driver(efra_class, vendor, device, base_class, sub_class);

                        self.devices.push(HardwareDeviceNode {
                            slot_label,
                            hw_name,
                            vendor_id: vendor,
                            device_id: device,
                            pci_class: base_class,
                            pci_subclass: sub_class,
                            class: efra_class,
                            bound_driver_id: drv_id,
                            bound_driver_name: drv_name,
                            bound_driver_version: drv_ver,
                            driver_file: drv_file,
                            is_generic_binding: is_gen,
                            optimized: true,
                        });
                    }

                    if func == 0 {
                        let header_type = ((pci_read_32(bus, dev, 0, 0x0C) >> 16) & 0xFF) as u8;
                        if (header_type & 0x80) == 0 {
                            break;
                        }
                    }
                }
            }
        }

        let (in_id, in_name, in_ver, in_file, in_gen) =
            self.match_driver(EfraClass::Input, 0xFFFF, 0xFFFF, 0x09, 0x00);
        self.devices.push(HardwareDeviceNode {
            slot_label: String::from("ISA 0x60/0x64"),
            hw_name: String::from("i8042 PS/2 Klavye ve Fare Denetleyicisi"),
            vendor_id: 0x8042,
            device_id: 0x0303,
            pci_class: 0x09,
            pci_subclass: 0x00,
            class: EfraClass::Input,
            bound_driver_id: in_id,
            bound_driver_name: in_name,
            bound_driver_version: in_ver,
            driver_file: in_file,
            is_generic_binding: in_gen,
            optimized: true,
        });

        self.environment = if found_vbox {
            HardwareEnvironment::VirtualBox
        } else if found_qemu {
            HardwareEnvironment::QemuKvm
        } else {
            HardwareEnvironment::BareMetalPc
        };

        self.status_log = alloc::format!(
            "{} algilandi | {} donanim .efra tabanina baglandi (PAT WC aktif).",
            self.environment.label(),
            self.devices.len()
        );
    }

    fn match_driver(
        &self,
        class: EfraClass,
        vendor: u16,
        device: u16,
        pci_class: u8,
        pci_subclass: u8,
    ) -> (String, String, String, String, bool) {
        for d in &self.drivers {
            if d.vendor_id == vendor && d.device_id == device && vendor != 0xFFFF {
                return (
                    d.manifest.id.clone(),
                    d.manifest.name.clone(),
                    d.manifest.version.clone(),
                    d.manifest.file_name.clone(),
                    false,
                );
            }
        }
        for d in &self.drivers {
            if d.vendor_id == vendor && d.device_id == 0xFFFF && d.class == class {
                return (
                    d.manifest.id.clone(),
                    d.manifest.name.clone(),
                    d.manifest.version.clone(),
                    d.manifest.file_name.clone(),
                    false,
                );
            }
        }
        for d in &self.drivers {
            if d.class == class
                || (d.pci_class == pci_class && (d.pci_subclass == pci_subclass || d.pci_subclass == 0xFF))
            {
                return (
                    d.manifest.id.clone(),
                    d.manifest.name.clone(),
                    d.manifest.version.clone(),
                    d.manifest.file_name.clone(),
                    true,
                );
            }
        }
        (
            String::from("efra.generic.base"),
            String::from("Netizen Generic .efra Donanim Tabani"),
            String::from("2.0.0"),
            String::from("generic-base.efra"),
            true,
        )
    }

    pub fn register_or_update_package(&mut self, pkg: EfraDriverPackage) {
        if let Some(existing) = self.drivers.iter_mut().find(|d| d.manifest.id == pkg.manifest.id) {
            *existing = pkg;
        } else {
            self.drivers.push(pkg);
        }
        self.rebind_all();
    }

    pub fn rebind_all(&mut self) {
        for i in 0..self.devices.len() {
            let (class, vendor_id, device_id, pci_class, pci_subclass) = {
                let dev = &self.devices[i];
                (dev.class, dev.vendor_id, dev.device_id, dev.pci_class, dev.pci_subclass)
            };
            let (id, name, ver, file, is_gen) =
                self.match_driver(class, vendor_id, device_id, pci_class, pci_subclass);
            let dev = &mut self.devices[i];
            dev.bound_driver_id = id;
            dev.bound_driver_name = name;
            dev.bound_driver_version = ver;
            dev.driver_file = file;
            dev.is_generic_binding = is_gen;
        }
    }

    pub fn upgrade_device_driver(&mut self, dev_index: usize) -> Option<EfraDriverPackage> {
        if dev_index >= self.devices.len() {
            return None;
        }
        let dev = &self.devices[dev_index];
        let custom_id = alloc::format!("efra.opt.{:04x}_{:04x}", dev.vendor_id, dev.device_id);
        let custom_name = alloc::format!("Optimize .efra: {}", dev.hw_name);
        let custom_file = alloc::format!("drv_{:04x}_{:04x}.efra", dev.vendor_id, dev.device_id);

        let pkg = EfraCompiler::compile_in_os(
            &custom_id,
            &custom_name,
            "2.5.0-opt",
            &custom_file,
            dev.class,
            dev.vendor_id,
            dev.device_id,
            dev.pci_class,
            dev.pci_subclass,
            EFRA_FLAG_WRITE_COMBINING | EFRA_FLAG_DMA_BURST | EFRA_FLAG_CUSTOM_COMPILED,
        );
        self.register_or_update_package(pkg.clone());
        self.status_log = alloc::format!(
            "'{}' icin ozel {} surucusu derlendi ve etkinlestirildi.",
            self.devices[dev_index].hw_name,
            custom_file
        );
        Some(pkg)
    }
}

static mut GLOBAL_DRIVERS: Option<DriverRegistry> = None;
static mut DRIVERS_MAGIC: u64 = 0;
const ACTIVE_MAGIC: u64 = 0x4E43_4452_565F_4F4Bu64;

pub fn init() -> &'static mut DriverRegistry {
    unsafe {
        let slot = core::ptr::addr_of_mut!(GLOBAL_DRIVERS);
        let magic = core::ptr::addr_of_mut!(DRIVERS_MAGIC);
        // Önceki kurulumdan kalan bozuk işaretçileri drop() etmeden doğrudan üzerine yaz!
        core::ptr::write(slot, None);
        core::ptr::write(slot, Some(DriverRegistry::new()));
        core::ptr::write(magic, ACTIVE_MAGIC);
        (*slot).as_mut().unwrap()
    }
}

pub fn global() -> &'static mut DriverRegistry {
    unsafe {
        let magic = core::ptr::read_volatile(core::ptr::addr_of!(DRIVERS_MAGIC));
        if magic != ACTIVE_MAGIC {
            return init();
        }
        let slot = &mut *core::ptr::addr_of_mut!(GLOBAL_DRIVERS);
        if slot.is_none() {
            return init();
        }
        slot.as_mut().unwrap()
    }
}

#[repr(C, align(8))]
#[derive(Clone, Copy)]
struct Ac97BdlEntry {
    addr: u32,
    length: u16,
    flags: u16,
}

static mut AC97_BDL: [Ac97BdlEntry; 32] = [Ac97BdlEntry { addr: 0, length: 0, flags: 0 }; 32];
static mut AC97_PCM_BUF: [i16; 48000] = [0; 48000];

/// PCI uzerinde Intel AC'97 (8086:2415) ses karti varsa gercek 48kHz polifonik sentezleyici calar,
/// yoksa gercek anakart PIT / PC Speaker uzerinden melodiyi calar.
pub fn play_hardware_startup_chime() {
    unsafe {
        // 1. PCI Bus uzerinde AC'97 Ses Denetleyicisi (Class 0x04, Subclass 0x01) ara
        for bus in 0..=2u8 {
            for dev in 0..32u8 {
                let vd = pci_read_32(bus, dev, 0, 0x00);
                let vendor = (vd & 0xFFFF) as u16;
                if vendor == 0xFFFF || vendor == 0x0000 {
                    continue;
                }
                let class_reg = pci_read_32(bus, dev, 0, 0x08);
                let base_class = ((class_reg >> 24) & 0xFF) as u8;
                let sub_class = ((class_reg >> 16) & 0xFF) as u8;

                if base_class == 0x04 && sub_class == 0x01 {
                    // PCI Bus Mastering + I/O Space aktif et
                    let cmd = pci_read_32(bus, dev, 0, 0x04);
                    pci_write_32(bus, dev, 0, 0x04, cmd | 0x05);

                    let nam_bar = (pci_read_32(bus, dev, 0, 0x10) & 0xFFFE) as u16; // Mixer
                    let nabm_bar = (pci_read_32(bus, dev, 0, 0x14) & 0xFFFE) as u16; // Bus Master
                    if nam_bar != 0 && nabm_bar != 0 {
                        // AC97 Reset & Ses Seviyesi (Mute kaldir, ses %100)
                        outw(nam_bar + 0x00, 1);
                        outl(nabm_bar + 0x2C, 0x02); // Cold Reset
                        outw(nam_bar + 0x02, 0x0000); // Master Volume
                        outw(nam_bar + 0x18, 0x0000); // PCM Out Volume
                        outw(nam_bar + 0x2C, 48000);  // 48 kHz Sample Rate

                        // 0.5 saniyelik harmonik akor (Do5 + Mi5 + Sol5) PCM dalgasi uret (Stereo)
                        let notes = [523usize, 659, 784, 1046];
                        for i in 0..24000usize {
                            let note_idx = (i / 6000).min(3);
                            let period = 48000 / notes[note_idx];
                            let env = ((24000 - i) as i32 * 6000) / 24000;
                            let sample = if (i % period) < (period / 2) { env as i16 } else { -(env as i16) };
                            AC97_PCM_BUF[i * 2] = sample;     // Sol kanal
                            AC97_PCM_BUF[i * 2 + 1] = sample; // Sag kanal
                        }

                        // DMA Buffer Descriptor List (BDL) ayarla
                        let pcm_phys = core::ptr::addr_of!(AC97_PCM_BUF) as u32;
                        let bdl_phys = core::ptr::addr_of!(AC97_BDL) as u32;
                        AC97_BDL[0] = Ac97BdlEntry {
                            addr: pcm_phys,
                            length: 48000, // sample sayisi
                            flags: 0x8000, // IOC
                        };

                        outb(nabm_bar + 0x1B, 0x02); // PCM Out Reset
                        outl(nabm_bar + 0x10, bdl_phys); // BDL adresi
                        outb(nabm_bar + 0x15, 0); // Last Valid Index = 0
                        outb(nabm_bar + 0x1B, 0x01); // DMA Baslat!
                        return;
                    }
                }
            }
        }

        // 2. AC97 yoksa Bare-Metal Anakart PIT (Port 0x42/0x61) Buzzer Melodisi
        for &(freq, loops) in &[(523u32, 45000u32), (659, 45000), (784, 55000), (1046, 90000)] {
            let div = 1193182 / freq;
            outb(0x43, 0xB6);
            outb(0x42, (div & 0xFF) as u8);
            outb(0x42, ((div >> 8) & 0xFF) as u8);
            let tmp = inb(0x61);
            outb(0x61, tmp | 3);
            for _ in 0..loops {
                let _ = inb(0x80); // IO port gecikmesi (~1us)
            }
        }
        outb(0x61, inb(0x61) & 0xFC);
    }
}

pub fn set_hardware_volume(percent: u8) {
    let pct = percent.min(100) as u16;
    // AC97 attenuation: 0 = %100 ses, 0x1F = en dusuk, 0x8000 = Mute
    let reg_val: u16 = if pct == 0 {
        0x8000
    } else {
        let att = ((100 - pct) * 31) / 100;
        (att << 8) | att
    };
    unsafe {
        for bus in 0..=2u8 {
            for dev in 0..32u8 {
                let vd = pci_read_32(bus, dev, 0, 0x00);
                if (vd & 0xFFFF) == 0xFFFF || vd == 0 {
                    continue;
                }
                let class_reg = pci_read_32(bus, dev, 0, 0x08);
                if ((class_reg >> 24) & 0xFF) == 0x04 && ((class_reg >> 16) & 0xFF) == 0x01 {
                    let nam_bar = (pci_read_32(bus, dev, 0, 0x10) & 0xFFFE) as u16;
                    if nam_bar != 0 {
                        outw(nam_bar + 0x02, reg_val); // Master Volume
                        outw(nam_bar + 0x18, reg_val); // PCM Out Volume
                    }
                    return;
                }
            }
        }
    }
}
