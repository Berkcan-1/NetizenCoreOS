// crates/nc-driver/src/pci.rs
extern crate alloc;

use alloc::vec::Vec;
use nc_arch::{inl, outl, outw};

const PCI_CONFIG_ADDRESS: u16 = 0x0CF8;
const PCI_CONFIG_DATA: u16 = 0x0CFC;

pub const PCI_CMD_IO_SPACE: u16 = 1 << 0;
pub const PCI_CMD_MEM_SPACE: u16 = 1 << 1;
pub const PCI_CMD_BUS_MASTER: u16 = 1 << 2;
pub const PCI_CMD_INT_DISABLE: u16 = 1 << 10;

#[derive(Clone, Copy, Debug)]
enum PciConfigAccess {
    LegacyMechanism1,
    Ecam {
        base: usize,
        mapped_len: usize,
        bus_start: u8,
        bus_end: u8,
    },
}

impl PciConfigAccess {
    fn ecam_address(
        base: usize,
        mapped_len: usize,
        bus_start: u8,
        bus_end: u8,
        loc: PciLocation,
        offset: u16,
    ) -> Option<usize> {
        if loc.bus < bus_start || loc.bus > bus_end || loc.dev >= 32 || loc.func >= 8 || offset > 0xFFC {
            return None;
        }
        let bus = (loc.bus - bus_start) as usize;
        let index = (bus << 20)
            | ((loc.dev as usize) << 15)
            | ((loc.func as usize) << 12)
            | offset as usize;
        if index.checked_add(4)? > mapped_len { return None; }
        base.checked_add(index)
    }

    fn read_u16(self, loc: PciLocation, offset: u16) -> Option<u16> {
        if offset & 1 != 0 { return None; }
        match self {
            Self::LegacyMechanism1 => {
                if offset > 0xFC { return None; }
                let value = pci_read_u32(loc, (offset as u8) & !3);
                Some((value >> ((offset & 2) * 8)) as u16)
            }
            Self::Ecam { base, mapped_len, bus_start, bus_end, .. } => {
                let address = Self::ecam_address(base, mapped_len, bus_start, bus_end, loc, offset)?;
                Some(unsafe { core::ptr::read_volatile(address as *const u16) })
            }
        }
    }

    fn write_u16(self, loc: PciLocation, offset: u16, value: u16) -> bool {
        if offset & 1 != 0 { return false; }
        match self {
            Self::LegacyMechanism1 => {
                if offset > 0xFC { return false; }
                let address = 0x8000_0000u32
                    | ((loc.bus as u32) << 16)
                    | ((loc.dev as u32) << 11)
                    | ((loc.func as u32) << 8)
                    | ((offset as u32) & 0xFC);
                unsafe {
                    outl(PCI_CONFIG_ADDRESS, address);
                    outw(PCI_CONFIG_DATA + (offset & 2), value);
                }
                true
            }
            Self::Ecam { base, mapped_len, bus_start, bus_end, .. } => {
                let Some(address) = Self::ecam_address(base, mapped_len, bus_start, bus_end, loc, offset) else {
                    return false;
                };
                unsafe { core::ptr::write_volatile(address as *mut u16, value); }
                true
            }
        }
    }
}

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
            PciBar::Memory64 { base, .. } if base != 0 => usize::try_from(base).ok(),
            _ => None,
        }
    }

    #[inline]
    pub fn mmio_size(&self) -> usize {
        match *self {
            PciBar::Memory32 { size, .. } => size as usize,
            PciBar::Memory64 { size, .. } => usize::try_from(size).unwrap_or(0),
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
    config_access: PciConfigAccess,
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
    // The 0x04 DWORD contains the W1C Status register; only the 16-bit
    // Command helper is permitted to modify that register pair.
    if (offset & 0xFC) == 0x04 { return; }
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
    // PCI Status is W1C. Writing a 32-bit merged Command/Status value can
    // accidentally clear latched status bits. Use a 16-bit config write.
    let addr = 0x8000_0000u32
        | ((loc.bus as u32) << 16)
        | ((loc.dev as u32) << 11)
        | ((loc.func as u32) << 8)
        | 0x04;
    unsafe {
        outl(PCI_CONFIG_ADDRESS, addr);
        outw(PCI_CONFIG_DATA, cmd);
    }
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
        let has_mem = self.bars.iter().any(|bar| match bar {
            PciBar::Memory32 { size, .. } => *size != 0,
            PciBar::Memory64 { size, .. } => *size != 0,
            _ => false,
        });
        let has_io = self.bars.iter().any(|bar| match bar {
            PciBar::IoPort { size, .. } => *size != 0,
            _ => false,
        });
        if (mem_space && !has_mem) || (io_space && !has_io) || (!mem_space && !io_space) {
            return;
        }
        let Some(mut cmd) = self.config_access.read_u16(self.loc, 0x04) else { return; };
        cmd &= !(PCI_CMD_BUS_MASTER | PCI_CMD_MEM_SPACE | PCI_CMD_IO_SPACE);
        if mem_space { cmd |= PCI_CMD_MEM_SPACE; }
        if io_space { cmd |= PCI_CMD_IO_SPACE; }
        // INTx stays unchanged; IRQ policy belongs to the interrupt subsystem.
        if !self.config_access.write_u16(self.loc, 0x04, cmd) { return; }
        self.state = DeviceLifecycleState::ResourcesValidated;
    }

    /// Call only after resources and every DMA buffer have been validated.
    pub fn enable_bus_mastering(&mut self) {
        if !matches!(self.state, DeviceLifecycleState::ResourcesValidated | DeviceLifecycleState::Initialized | DeviceLifecycleState::SelfTestPassed) {
            return;
        }
        let Some(cmd) = self.config_access.read_u16(self.loc, 0x04) else { return; };
        let _ = self.config_access.write_u16(self.loc, 0x04, cmd | PCI_CMD_BUS_MASTER);
    }

    /// Hata veya sıfırlama (reset) durumunda aygıtın DMA yetkisini derhal keser.
    pub fn disable_bus_mastering(&mut self) {
        if let Some(cmd) = self.config_access.read_u16(self.loc, 0x04) {
            let _ = self.config_access.write_u16(self.loc, 0x04, cmd & !PCI_CMD_BUS_MASTER);
        }
    }
}

/// Standart Type-0 PCI aygıtının 6 BAR kaydını (32-bit / 64-bit / IO) güvenli biçimde çözümler.
fn decode_endpoint_bars(loc: PciLocation) -> [PciBar; 6] {
    let mut bars = [PciBar::None; 6];
    let orig_cmd = pci_read_command(loc);

    // BAR probing must not let the device decode addresses or initiate DMA.
    pci_write_command(loc, orig_cmd & !(PCI_CMD_IO_SPACE | PCI_CMD_MEM_SPACE | PCI_CMD_BUS_MASTER));

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
            let size = (!mask).wrapping_add(1);
            if base != 0 && size.is_power_of_two() && (orig_low & 0xFFFF_0000) == 0 {
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

                if base != 0 && combined_mask != 0 && size.is_power_of_two() {
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
                if base != 0 && mask != 0 && size.is_power_of_two() {
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
                    config_access: PciConfigAccess::LegacyMechanism1,
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


/// Standard PCI capability header discovered through the legacy capability list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciCapability {
    pub id: u8,
    pub offset: u8,
}

#[inline]
fn pci_read_u8(loc: PciLocation, offset: u8) -> u8 {
    let value = pci_read_u32(loc, offset & !3);
    ((value >> ((offset & 3) * 8)) & 0xFF) as u8
}

/// Walks the conventional PCI capability list with alignment, range and cycle checks.
pub fn pci_capabilities(loc: PciLocation) -> Vec<PciCapability> {
    let status = (pci_read_u32(loc, 0x04) >> 16) as u16;
    if (status & (1 << 4)) == 0 {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut pointer = pci_read_u8(loc, 0x34) & !0x03;
    let mut visited = [0u64; 4];
    let mut hops = 0usize;
    while pointer >= 0x40 && pointer <= 0xFC && hops < 48 {
        let slot = pointer as usize;
        let word = slot / 64;
        let bit = slot % 64;
        let mask = 1u64 << bit;
        if (visited[word] & mask) != 0 {
            break;
        }
        visited[word] |= mask;
        let id = pci_read_u8(loc, pointer);
        let next = pci_read_u8(loc, pointer.saturating_add(1)) & !0x03;
        result.push(PciCapability { id, offset: pointer });
        if next == 0 || next == pointer {
            break;
        }
        pointer = next;
        hops += 1;
    }
    result
}

/// A PCIe ECAM window that has already been mapped into the kernel's virtual address
/// space by the memory manager. This helper does not discover ACPI MCFG or map memory.
/// The caller must ensure the supplied mapping covers the selected buses and is UC.
pub struct PciEcam {
    base: *mut u8,
    mapped_len: usize,
    segment: u16,
    bus_start: u8,
    bus_end: u8,
}

/// One allocation entry from the ACPI MCFG table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McfgRegion {
    pub base_address: u64,
    pub segment_group: u16,
    pub start_bus: u8,
    pub end_bus: u8,
}

/// Parse and validate a complete ACPI MCFG table. This does not locate the RSDP,
/// map the physical ECAM range, or change memory cache attributes.
pub fn parse_mcfg_table(table: &[u8]) -> Option<Vec<McfgRegion>> {
    if table.len() < 44 || &table[..4] != b"MCFG" { return None; }
    let declared_len = u32::from_le_bytes([table[4], table[5], table[6], table[7]]) as usize;
    if declared_len < 44 || declared_len > table.len() || (declared_len - 44) % 16 != 0 {
        return None;
    }
    if table[..declared_len].iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) != 0 {
        return None;
    }
    if table[36..44].iter().any(|byte| *byte != 0) { return None; }

    let mut regions = Vec::new();
    let mut offset = 44usize;
    while offset < declared_len {
        let base_address = u64::from_le_bytes([
            table[offset], table[offset + 1], table[offset + 2], table[offset + 3],
            table[offset + 4], table[offset + 5], table[offset + 6], table[offset + 7],
        ]);
        let segment_group = u16::from_le_bytes([table[offset + 8], table[offset + 9]]);
        let start_bus = table[offset + 10];
        let end_bus = table[offset + 11];
        let reserved = u32::from_le_bytes([
            table[offset + 12], table[offset + 13], table[offset + 14], table[offset + 15],
        ]);
        let bus_count = (end_bus as u64).checked_sub(start_bus as u64)?.checked_add(1)?;
        let window_size = bus_count.checked_mul(1 << 20)?;
        let window_end = base_address.checked_add(window_size)?;
        if base_address == 0 || (base_address & 0xFFFFF) != 0 || reserved != 0 || start_bus > end_bus || window_end <= base_address {
            return None;
        }
        regions.push(McfgRegion { base_address, segment_group, start_bus, end_bus });
        offset += 16;
    }
    Some(regions)
}

unsafe impl Send for PciEcam {}

impl PciEcam {
    /// # Safety
    /// `base` must be a valid, uncached mapping of an ECAM window for the whole bus range.
    pub unsafe fn from_mapped_window(
        base: *mut u8,
        mapped_len: usize,
        segment: u16,
        bus_start: u8,
        bus_end: u8,
    ) -> Option<Self> {
        if base.is_null() || bus_start > bus_end || (base as usize) & 0xFFF != 0 {
            return None;
        }
        let bus_count = (bus_end as usize).checked_sub(bus_start as usize)?.checked_add(1)?;
        let required_len = bus_count.checked_mul(1 << 20)?;
        if mapped_len < required_len { return None; }
        Some(Self { base, mapped_len, segment, bus_start, bus_end })
    }

    fn address(&self, loc: PciLocation, offset: u16) -> Option<*mut u8> {
        if loc.bus < self.bus_start || loc.bus > self.bus_end || loc.dev >= 32 || loc.func >= 8 || offset > 0xFFC {
            return None;
        }
        let bus = (loc.bus - self.bus_start) as usize;
        let index = (bus << 20)
            | ((loc.dev as usize) << 15)
            | ((loc.func as usize) << 12)
            | (offset as usize);
        if index.checked_add(4)? > self.mapped_len { return None; }
        (self.base as usize)
            .checked_add(index)
            .map(|address| address as *mut u8)
    }

    pub fn segment(&self) -> u16 { self.segment }

    fn config_access(&self) -> PciConfigAccess {
        PciConfigAccess::Ecam {
            base: self.base as usize,
            mapped_len: self.mapped_len,
            bus_start: self.bus_start,
            bus_end: self.bus_end,
        }
    }

    pub fn read_u8(&self, loc: PciLocation, offset: u16) -> Option<u8> {
        let ptr = self.address(loc, offset)? as *const u8;
        Some(unsafe { core::ptr::read_volatile(ptr) })
    }

    pub fn read_u16(&self, loc: PciLocation, offset: u16) -> Option<u16> {
        if offset & 1 != 0 { return None; }
        let ptr = self.address(loc, offset)? as *const u16;
        Some(unsafe { core::ptr::read_volatile(ptr) })
    }

    pub fn write_u16(&self, loc: PciLocation, offset: u16, value: u16) -> bool {
        if offset & 1 != 0 { return false; }
        let Some(ptr) = self.address(loc, offset) else { return false; };
        unsafe { core::ptr::write_volatile(ptr as *mut u16, value); }
        true
    }

    pub fn read_u32(&self, loc: PciLocation, offset: u16) -> Option<u32> {
        if offset & 3 != 0 { return None; }
        let ptr = self.address(loc, offset)? as *const u32;
        Some(unsafe { core::ptr::read_volatile(ptr) })
    }

    pub fn write_u32(&self, loc: PciLocation, offset: u16, value: u32) -> bool {
        if offset & 3 != 0 || offset == 0x04 { return false; }
        let Some(ptr) = self.address(loc, offset) else { return false; };
        unsafe { core::ptr::write_volatile(ptr as *mut u32, value); }
        true
    }

    /// Enumerate functions on the mapped MCFG bus range. MCFG discovery and the
    /// uncached mapping itself remain the platform memory-manager's responsibility.
    pub fn enumerate(&self) -> Vec<PciEndpoint> {
        let mut endpoints = Vec::new();
        for bus in self.bus_start..=self.bus_end {
            for dev in 0..32u8 {
                let loc0 = PciLocation { bus, dev, func: 0 };
                let Some(id0) = self.read_u32(loc0, 0x00) else { continue; };
                let vendor0 = id0 as u16;
                if vendor0 == 0xFFFF || vendor0 == 0 { continue; }
                let header0 = self.read_u32(loc0, 0x0C).unwrap_or(0);
                let header_type0 = ((header0 >> 16) & 0xFF) as u8;
                let function_count = if (header_type0 & 0x80) != 0 { 8 } else { 1 };
                for func in 0..function_count {
                    let loc = PciLocation { bus, dev, func };
                    let Some(id) = self.read_u32(loc, 0x00) else { continue; };
                    let vendor_id = id as u16;
                    if vendor_id == 0xFFFF || vendor_id == 0 { continue; }
                    let device_id = (id >> 16) as u16;
                    let class_reg = self.read_u32(loc, 0x08).unwrap_or(0);
                    let h = self.read_u32(loc, 0x0C).unwrap_or(0);
                    let header_type = ((h >> 16) & 0xFF) as u8;
                    let clean_header = header_type & 0x7F;
                    let (subsys_vendor_id, subsys_id) = if clean_header == 0 {
                        let sub = self.read_u32(loc, 0x2C).unwrap_or(0);
                        (sub as u16, (sub >> 16) as u16)
                    } else { (0, 0) };
                    let interrupt = self.read_u32(loc, 0x3C).unwrap_or(0);
                    let bars = if clean_header == 0 { decode_ecam_bars(self, loc) } else { [PciBar::None; 6] };
                    endpoints.push(PciEndpoint {
                        loc,
                        vendor_id,
                        device_id,
                        subsys_vendor_id,
                        subsys_id,
                        revision_id: class_reg as u8,
                        prog_if: (class_reg >> 8) as u8,
                        subclass: (class_reg >> 16) as u8,
                        class_code: (class_reg >> 24) as u8,
                        header_type,
                        irq_line: interrupt as u8,
                        irq_pin: (interrupt >> 8) as u8,
                        bars,
                        state: DeviceLifecycleState::Discovered,
                        config_access: self.config_access(),
                    });
                }
            }
        }
        endpoints
    }
}

fn decode_ecam_bars(ecam: &PciEcam, loc: PciLocation) -> [PciBar; 6] {
    let mut bars = [PciBar::None; 6];
    let original_command = ecam.read_u16(loc, 0x04).unwrap_or(0);
    let disabled = original_command & !(PCI_CMD_IO_SPACE | PCI_CMD_MEM_SPACE | PCI_CMD_BUS_MASTER);
    if !ecam.write_u16(loc, 0x04, disabled) { return bars; }

    let mut index = 0usize;
    while index < 6 {
        let offset = 0x10 + (index as u16) * 4;
        let Some(original_low) = ecam.read_u32(loc, offset) else { break; };
        if original_low == 0 { index += 1; continue; }
        if (original_low & 1) != 0 {
            if !ecam.write_u32(loc, offset, u32::MAX) { break; }
            let mask = ecam.read_u32(loc, offset).unwrap_or(0) & 0xFFFF_FFFC;
            let _ = ecam.write_u32(loc, offset, original_low);
            let base32 = original_low & 0xFFFF_FFFC;
            let size = (!mask).wrapping_add(1);
            if base32 != 0 && (base32 >> 16) == 0 && size.is_power_of_two() {
                bars[index] = PciBar::IoPort { base: base32 as u16, size };
            }
            index += 1;
            continue;
        }

        let mem_type = (original_low >> 1) & 3;
        let prefetchable = (original_low & 8) != 0;
        if mem_type == 2 && index + 1 < 6 {
            let Some(original_high) = ecam.read_u32(loc, offset + 4) else { break; };
            if !ecam.write_u32(loc, offset, u32::MAX) { break; }
            if !ecam.write_u32(loc, offset + 4, u32::MAX) {
                // Restore the low half even if the second write unexpectedly fails.
                let _ = ecam.write_u32(loc, offset, original_low);
                break;
            }
            let mask_low = ecam.read_u32(loc, offset).unwrap_or(0) & 0xFFFF_FFF0;
            let mask_high = ecam.read_u32(loc, offset + 4).unwrap_or(0);
            let _ = ecam.write_u32(loc, offset, original_low);
            let _ = ecam.write_u32(loc, offset + 4, original_high);
            let base = ((original_high as u64) << 32) | ((original_low & 0xFFFF_FFF0) as u64);
            let mask = ((mask_high as u64) << 32) | mask_low as u64;
            let size = (!mask).wrapping_add(1);
            if base != 0 && mask != 0 && size.is_power_of_two() {
                bars[index] = PciBar::Memory64 { base, size, prefetchable };
            }
            index += 2;
        } else {
            if !ecam.write_u32(loc, offset, u32::MAX) { break; }
            let mask = ecam.read_u32(loc, offset).unwrap_or(0) & 0xFFFF_FFF0;
            let _ = ecam.write_u32(loc, offset, original_low);
            let base = original_low & 0xFFFF_FFF0;
            let size = (!mask).wrapping_add(1);
            if base != 0 && mask != 0 && size.is_power_of_two() {
                bars[index] = PciBar::Memory32 { base, size, prefetchable };
            }
            index += 1;
        }
    }
    let _ = ecam.write_u16(loc, 0x04, original_command);
    bars
}
