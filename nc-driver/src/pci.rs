// crates/nc-driver/src/pci.rs
extern crate alloc;

use alloc::vec::Vec;
use nc_arch::{inl, outl};

const PCI_CONFIG_ADDRESS: u16 = 0x0CF8;
const PCI_CONFIG_DATA: u16 = 0x0CFC;

pub const PCI_CMD_IO_SPACE: u16 = 1 << 0;
pub const PCI_CMD_MEM_SPACE: u16 = 1 << 1;
pub const PCI_CMD_BUS_MASTER: u16 = 1 << 2;
pub const PCI_CMD_INT_DISABLE: u16 = 1 << 10;

/// Teknik bağlam dokümanındaki (Bölüm 3) zorunlu aygıt durum makinesi
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeviceLifecycleState {
    NotSeen,
    Discovered,
    DriverMatched,
    ResourcesValidated,
    Initialized,
    SelfTestPassed,
    Operational,
    Failed(&'static str),
}

impl DeviceLifecycleState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotSeen => "Gorulmedi",
            Self::Discovered => "Kesfedildi (Discovered)",
            Self::DriverMatched => "Surucu Eslesti (DriverMatched)",
            Self::ResourcesValidated => "BAR/DMA Dogrulandi",
            Self::Initialized => "Baslatildi (Initialized)",
            Self::SelfTestPassed => "I/O Testi Gecti",
            Self::Operational => "Aktif / Calisiyor (Operational)",
            Self::Failed(_) => "Baslatma Hatasi",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PciBar {
    None,
    IoPort {
        base: u16,
        size: u32,
    },
    Memory32 {
        base: u32,
        size: u32,
        prefetchable: bool,
    },
    Memory64 {
        base: u64,
        size: u64,
        prefetchable: bool,
    },
}

impl PciBar {
    #[inline]
    pub fn mmio_base(&self) -> Option<usize> {
        match *self {
            PciBar::Memory32 { base, .. } if base != 0 => Some(base as usize),
            PciBar::Memory64 { base, .. } if base != 0 => Some(base as usize),
            _ => None,
        }
    }

    #[inline]
    pub fn mmio_size(&self) -> usize {
        match *self {
            PciBar::Memory32 { size, .. } => size as usize,
            PciBar::Memory64 { size, .. } => size as usize,
            _ => 0,
        }
    }

    #[inline]
    pub fn io_base(&self) -> Option<u16> {
        match *self {
            PciBar::IoPort { base, .. } if base != 0 => Some(base),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PciLocation {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct PciEndpoint {
    pub loc: PciLocation,
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsys_vendor_id: u16,
    pub subsys_id: u16,
    pub revision_id: u8,
    pub prog_if: u8,
    pub subclass: u8,
    pub class_code: u8,
    pub header_type: u8,
    pub irq_line: u8,
    pub irq_pin: u8,
    pub bars: [PciBar; 6],
    pub state: DeviceLifecycleState,
}

#[inline]
pub fn pci_read_u32(loc: PciLocation, offset: u8) -> u32 {
    let addr = 0x8000_0000u32
        | ((loc.bus as u32) << 16)
        | ((loc.dev as u32) << 11)
        | ((loc.func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        outl(PCI_CONFIG_ADDRESS, addr);
        inl(PCI_CONFIG_DATA)
    }
}

#[inline]
pub fn pci_write_u32(loc: PciLocation, offset: u8, value: u32) {
    let addr = 0x8000_0000u32
        | ((loc.bus as u32) << 16)
        | ((loc.dev as u32) << 11)
        | ((loc.func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        outl(PCI_CONFIG_ADDRESS, addr);
        outl(PCI_CONFIG_DATA, value);
    }
}

pub fn pci_read_command(loc: PciLocation) -> u16 {
    (pci_read_u32(loc, 0x04) & 0xFFFF) as u16
}

pub fn pci_write_command(loc: PciLocation, cmd: u16) {
    let status_and_cmd = pci_read_u32(loc, 0x04);
    let merged = (status_and_cmd & 0xFFFF_0000) | (cmd as u32);
    pci_write_u32(loc, 0x04, merged);
}

impl PciEndpoint {
    /// Belirtilen indeks için BAR kaydını döndürür
    #[inline]
    pub fn bar(&self, index: usize) -> PciBar {
        if index < 6 {
            self.bars[index]
        } else {
            PciBar::None
        }
    }

    /// Yalnızca MMIO ve/veya I/O erişimini açar; Bus Mastering'i AÇMAZ.
    pub fn enable_resource_decoding(&mut self, mem_space: bool, io_space: bool) {
        let mut cmd = pci_read_command(self.loc);
        if mem_space {
            cmd |= PCI_CMD_MEM_SPACE;
        }
        if io_space {
            cmd |= PCI_CMD_IO_SPACE;
        }
        // Kesici fırtınasını önlemek için sürücü IRQ işleyicisi kurana kadar INTx kapalı tutulabilir
        pci_write_command(self.loc, cmd);
        self.state = DeviceLifecycleState::ResourcesValidated;
    }

    /// Sürücü DMA tamponlarını (`DmaBuffer`) hazırlayıp donanım register'larına yazdıktan sonra çağrılır.
    pub fn enable_bus_mastering(&mut self) {
        let cmd = pci_read_command(self.loc) | PCI_CMD_BUS_MASTER | PCI_CMD_MEM_SPACE;
        pci_write_command(self.loc, cmd);
    }

    /// Hata veya sıfırlama (reset) durumunda aygıtın DMA yetkisini derhal keser.
    pub fn disable_bus_mastering(&mut self) {
        let cmd = pci_read_command(self.loc) & !PCI_CMD_BUS_MASTER;
        pci_write_command(self.loc, cmd);
    }
}

/// Standart Type-0 PCI aygıtının 6 BAR kaydını (32-bit / 64-bit / IO) güvenli biçimde çözümler.
fn decode_endpoint_bars(loc: PciLocation) -> [PciBar; 6] {
    let mut bars = [PciBar::None; 6];
    let orig_cmd = pci_read_command(loc);

    // BAR boyut testi sırasında (0xFFFFFFFF yazılırken) aygıtın yanlış adrese yanıt vermemesi için
    // Memory ve I/O decoding bitlerini geçici olarak kapatıyoruz.
    pci_write_command(loc, orig_cmd & !(PCI_CMD_IO_SPACE | PCI_CMD_MEM_SPACE));

    let mut idx = 0usize;
    while idx < 6 {
        let offset = 0x10 + (idx as u8) * 4;
        let orig_low = pci_read_u32(loc, offset);

        if orig_low == 0 {
            idx += 1;
            continue;
        }

        if (orig_low & 0x01) != 0 {
            // I/O Port BAR
            pci_write_u32(loc, offset, 0xFFFF_FFFF);
            let mask = pci_read_u32(loc, offset) & 0xFFFF_FFFC;
            pci_write_u32(loc, offset, orig_low);

            let base = (orig_low & 0xFFFC) as u16;
            let size = (!mask).wrapping_add(1) & 0xFFFF;
            if base != 0 && size > 0 {
                bars[idx] = PciBar::IoPort { base, size };
            }
            idx += 1;
        } else {
            // Memory (MMIO) BAR
            let mem_type = (orig_low >> 1) & 0x03;
            let prefetchable = (orig_low & 0x08) != 0;

            if mem_type == 0x02 && idx + 1 < 6 {
                // 64-bit MMIO BAR (iki ardışık BAR slotu tüketir)
                let offset_high = offset + 4;
                let orig_high = pci_read_u32(loc, offset_high);

                pci_write_u32(loc, offset, 0xFFFF_FFFF);
                pci_write_u32(loc, offset_high, 0xFFFF_FFFF);
                let mask_low = pci_read_u32(loc, offset) & 0xFFFF_FFF0;
                let mask_high = pci_read_u32(loc, offset_high);

                pci_write_u32(loc, offset, orig_low);
                pci_write_u32(loc, offset_high, orig_high);

                let base = ((orig_high as u64) << 32) | ((orig_low & 0xFFFF_FFF0) as u64);
                let combined_mask = ((mask_high as u64) << 32) | (mask_low as u64);
                let size = (!combined_mask).wrapping_add(1);

                if base != 0 && size > 0 {
                    bars[idx] = PciBar::Memory64 {
                        base,
                        size,
                        prefetchable,
                    };
                }
                idx += 2;
            } else {
                // 32-bit MMIO BAR
                pci_write_u32(loc, offset, 0xFFFF_FFFF);
                let mask = pci_read_u32(loc, offset) & 0xFFFF_FFF0;
                pci_write_u32(loc, offset, orig_low);

                let base = orig_low & 0xFFFF_FFF0;
                let size = (!mask).wrapping_add(1);
                if base != 0 && size > 0 {
                    bars[idx] = PciBar::Memory32 {
                        base,
                        size,
                        prefetchable,
                    };
                }
                idx += 1;
            }
        }
    }

    // Orijinal command yazmacını geri yükle (Bus Master açmadan!)
    pci_write_command(loc, orig_cmd);
    bars
}

/// Tüm PCI/PCIe veriyollarını (0..=255) güvenli biçimde tarar.
/// Bilinmeyen aygıtları başlatmaz ve hiçbir aygıta topluca Bus Mastering vermez.
pub fn enumerate_pci_bus() -> Vec<PciEndpoint> {
    let mut endpoints = Vec::new();

    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            for func in 0..8u8 {
                let loc = PciLocation { bus, dev, func };
                let vd = pci_read_u32(loc, 0x00);
                let vendor_id = (vd & 0xFFFF) as u16;
                let device_id = ((vd >> 16) & 0xFFFF) as u16;

                if vendor_id == 0xFFFF || vendor_id == 0x0000 {
                    if func == 0 {
                        break;
                    }
                    continue;
                }

                let class_reg = pci_read_u32(loc, 0x08);
                let revision_id = (class_reg & 0xFF) as u8;
                let prog_if = ((class_reg >> 8) & 0xFF) as u8;
                let subclass = ((class_reg >> 16) & 0xFF) as u8;
                let class_code = ((class_reg >> 24) & 0xFF) as u8;

                let bhlc = pci_read_u32(loc, 0x0C);
                let header_type = ((bhlc >> 16) & 0xFF) as u8;
                let clean_header = header_type & 0x7F;

                let (subsys_vendor_id, subsys_id) = if clean_header == 0x00 {
                    let sub = pci_read_u32(loc, 0x2C);
                    ((sub & 0xFFFF) as u16, ((sub >> 16) & 0xFFFF) as u16)
                } else {
                    (0, 0)
                };

                let intr = pci_read_u32(loc, 0x3C);
                let irq_line = (intr & 0xFF) as u8;
                let irq_pin = ((intr >> 8) & 0xFF) as u8;

                // Yalnızca standart Type-0 endpoint'ler için 6 BAR çözülür
                let bars = if clean_header == 0x00 {
                    decode_endpoint_bars(loc)
                } else {
                    [PciBar::None; 6]
                };

                endpoints.push(PciEndpoint {
                    loc,
                    vendor_id,
                    device_id,
                    subsys_vendor_id,
                    subsys_id,
                    revision_id,
                    prog_if,
                    subclass,
                    class_code,
                    header_type,
                    irq_line,
                    irq_pin,
                    bars,
                    state: DeviceLifecycleState::Discovered,
                });

                // Çok işlevli (multi-function) değilse diğer func'ları tarama
                if func == 0 && (header_type & 0x80) == 0 {
                    break;
                }
            }
        }
    }

    endpoints
}
