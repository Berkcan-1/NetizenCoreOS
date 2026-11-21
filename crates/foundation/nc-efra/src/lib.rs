#![no_std]
extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

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
    VirtualizedVirtIo,
    BareMetalPc,
}

impl HardwareEnvironment {
    pub fn label(&self) -> &'static str {
        match self {
            HardwareEnvironment::QemuKvm => "QEMU / KVM Sanal Donanim",
            HardwareEnvironment::VirtualBox => "Oracle VirtualBox Hipervizor",
            HardwareEnvironment::VirtualizedVirtIo => "VirtIO destekli sanal ortam (hipervizor kesin degil)",
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

