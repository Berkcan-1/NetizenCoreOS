#![no_std]
extern crate alloc;

pub mod ac97;
pub mod dma;
pub mod pci;
pub mod ps2;

use alloc::string::String;
use alloc::vec::Vec;
use nc_hal::{AudioOutput, PcmFormat};
use pci::{enumerate_pci_bus, DeviceLifecycleState, PciBar, PciEndpoint, PciEcam};

pub use nc_efra::*;

#[derive(Clone, Debug)]
pub struct HardwareDeviceNode {
    pub slot_label: String,
    pub hw_name: String,
    pub vendor_id: u16,
    pub device_id: u16,
    pub pci_class: u8,
    pub pci_subclass: u8,
    pub prog_if: u8,
    pub revision_id: u8,
    pub class: EfraClass,
    pub bound_driver_id: String,
    pub bound_driver_name: String,
    pub bound_driver_version: String,
    pub driver_file: String,
    pub is_generic_binding: bool,
    pub optimized: bool,
    pub lifecycle: DeviceLifecycleState,
    pub bars: [PciBar; 6],
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
    pub pci_endpoints: Vec<PciEndpoint>,
    pub write_combining_active: bool,
    pub status_log: String,
}

impl DriverRegistry {
    fn new_unscanned() -> Self {
        let mut reg = DriverRegistry {
            environment: HardwareEnvironment::BareMetalPc,
            drivers: Vec::new(),
            devices: Vec::new(),
            pci_endpoints: Vec::new(),
            write_combining_active: false,
            status_log: String::from("Generic .efra surucu ve PCI/DMA tabani hazirlaniyor..."),
        };
        for pkg in builtin_efra_packages() {
            reg.drivers.push(pkg);
        }
        reg
    }

    /// Legacy PCI configuration mechanism 1 (0xCF8/0xCFC), suitable for
    /// conventional x86 PCs and common QEMU/VirtualBox machine profiles.
    pub fn new() -> Self {
        let mut reg = Self::new_unscanned();
        reg.optimize_and_scan();
        reg
    }

    /// PCIe-native initialization after platform code has parsed ACPI MCFG
    /// and created a UC mapping for the matching ECAM allocation.
    pub fn new_with_ecam(ecam: &PciEcam) -> Self {
        let mut reg = Self::new_unscanned();
        reg.optimize_and_scan_ecam(ecam);
        reg
    }

    /// AHCI (01:06), NVMe (01:08) veya Ethernet (02:00) sürücülerinin PCI cihazlarını bulması için
    pub fn find_pci_by_class(&self, class_code: u8, subclass: u8) -> Vec<PciEndpoint> {
        self.pci_endpoints
            .iter()
            .copied()
            .filter(|ep| ep.class_code == class_code && ep.subclass == subclass)
            .collect()
    }

    pub fn optimize_and_scan(&mut self) {
        self.bind_pci_endpoints(enumerate_pci_bus());
    }

    /// Use this entry point on platforms where ACPI MCFG has already been parsed
    /// and its ECAM range safely mapped by the memory manager.
    pub fn optimize_and_scan_ecam(&mut self, ecam: &pci::PciEcam) {
        self.bind_pci_endpoints(ecam.enumerate());
    }

    fn bind_pci_endpoints(&mut self, endpoints: Vec<PciEndpoint>) {
        self.write_combining_active = false;
        self.devices.clear();
        self.pci_endpoints = endpoints;

        let mut found_vbox = false;
        let mut found_qemu = false;
        let mut found_virtio = false;

        let total_endpoints = self.pci_endpoints.len();
        for i in 0..total_endpoints {
            let ep_loc = self.pci_endpoints[i].loc;
            let vendor_id = self.pci_endpoints[i].vendor_id;
            let device_id = self.pci_endpoints[i].device_id;
            let class_code = self.pci_endpoints[i].class_code;
            let subclass = self.pci_endpoints[i].subclass;
            let prog_if = self.pci_endpoints[i].prog_if;
            let revision_id = self.pci_endpoints[i].revision_id;
            let bars = self.pci_endpoints[i].bars;

            if vendor_id == 0x80EE {
                found_vbox = true;
            }
            if vendor_id == 0x1234 {
                found_qemu = true;
            }
            if vendor_id == 0x1AF4 {
                found_virtio = true;
            }

            let efra_class = classify_pci(class_code, subclass);
            let (drv_id, drv_name, drv_ver, drv_file, is_gen) = self.match_driver(
                efra_class,
                vendor_id,
                device_id,
                class_code,
                subclass,
            );

            self.pci_endpoints[i].state = DeviceLifecycleState::DriverMatched;

            if self.devices.len() < 24 {
                let hw_name = String::from(describe_pci_device(
                    vendor_id,
                    device_id,
                    class_code,
                    subclass,
                ));
                let slot_label = alloc::format!(
                    "PCI {:02X}:{:02X}.{}",
                    ep_loc.bus,
                    ep_loc.dev,
                    ep_loc.func
                );
                self.devices.push(HardwareDeviceNode {
                    slot_label,
                    hw_name,
                    vendor_id,
                    device_id,
                    pci_class: class_code,
                    pci_subclass: subclass,
                    prog_if,
                    revision_id,
                    class: efra_class,
                    bound_driver_id: drv_id,
                    bound_driver_name: drv_name,
                    bound_driver_version: drv_ver,
                    driver_file: drv_file,
                    is_generic_binding: is_gen,
                    // A matched EFRA package is not proof that a native driver was loaded.
                    optimized: false,
                    lifecycle: DeviceLifecycleState::DriverMatched,
                    bars,
                });
            }
        }

        // Legacy PS/2 input is provided by `ps2::Ps2Input`, but it is not listed as
        // present/operational until the platform input layer actually polls it.

        self.environment = if found_vbox {
            HardwareEnvironment::VirtualBox
        } else if found_qemu {
            HardwareEnvironment::QemuKvm
        } else if found_virtio {
            HardwareEnvironment::VirtualizedVirtIo
        } else {
            HardwareEnvironment::BareMetalPc
        };

        self.status_log = alloc::format!(
            "{} | {} PCI fonksiyonu kesfedildi. Eslesme yapildi; native baslatma yalnizca aygit surucusu tarafindan dogrulanir.",
            self.environment.label(),
            self.pci_endpoints.len()
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
            "'{}' icin {} paket tanimi olusturuldu. Bu EFRA prototipi calistirilabilir native surucu degildir.",
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

pub fn init_with_ecam(ecam: &PciEcam) -> &'static mut DriverRegistry {
    unsafe {
        let slot = core::ptr::addr_of_mut!(GLOBAL_DRIVERS);
        let magic = core::ptr::addr_of_mut!(DRIVERS_MAGIC);
        core::ptr::write(slot, None);
        core::ptr::write(slot, Some(DriverRegistry::new_with_ecam(ecam)));
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

/// Volume control is performed on the registered `AudioOutput` instance.
/// This legacy global helper intentionally performs no PCI scan or guessed BAR I/O.
pub fn set_hardware_volume(_percent: u8) {
    // Call AudioOutput::set_volume_percent on the initialized backend instead.
}
