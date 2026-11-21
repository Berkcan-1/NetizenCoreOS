use crate::{mmio_read8, mmio_write16, mmio_write32};

pub const QEMU_VIRT_FWCFG_BASE: usize = 0x0902_0000;

const REG_DATA: usize = 0x00;
const REG_SELECTOR: usize = 0x08;
const REG_DMA: usize = 0x10;
const SELECT_FILE_DIR: u16 = 0x0019;
const DMA_ERROR: u32 = 1;
const DMA_SELECT: u32 = 8;
const DMA_WRITE: u32 = 0x10;
const FOURCC_XR24: u32 = 0x3432_5258;
const RAMFB_CONFIG_BYTES: u32 = 28;

#[repr(C, align(16))]
struct DmaAccess {
    control: u32,
    length: u32,
    address: u64,
}

#[repr(C, align(16))]
struct RamfbConfig {
    address: u64,
    fourcc: u32,
    flags: u32,
    width: u32,
    height: u32,
    stride: u32,
}

static mut DMA: DmaAccess = DmaAccess { control: 0, length: 0, address: 0 };
static mut CONFIG: RamfbConfig = RamfbConfig { address: 0, fourcc: 0, flags: 0, width: 0, height: 0, stride: 0 };

unsafe fn select(base: usize, selector: u16) {
    mmio_write16(base + REG_SELECTOR, selector.to_be());
}

unsafe fn read_u32_be(base: usize) -> u32 {
    let mut value = 0u32;
    for _ in 0..4 {
        value = (value << 8) | mmio_read8(base + REG_DATA) as u32;
    }
    value
}

unsafe fn read_u16_be(base: usize) -> u16 {
    let high = mmio_read8(base + REG_DATA) as u16;
    let low = mmio_read8(base + REG_DATA) as u16;
    (high << 8) | low
}

unsafe fn find_file(base: usize, wanted: &[u8]) -> Option<u16> {
    select(base, SELECT_FILE_DIR);
    let count = read_u32_be(base);
    for _ in 0..count.min(256) {
        let _size = read_u32_be(base);
        let selector = read_u16_be(base);
        let _reserved = read_u16_be(base);
        let mut name = [0u8; 56];
        for slot in name.iter_mut() {
            *slot = mmio_read8(base + REG_DATA);
        }
        let length = name.iter().position(|byte| *byte == 0).unwrap_or(name.len());
        if &name[..length] == wanted {
            return Some(selector);
        }
    }
    None
}

pub fn setup_ramfb(base: usize, address: u64, width: u32, height: u32, stride: u32) -> bool {
    unsafe {
        let selector = match find_file(base, b"etc/ramfb") {
            Some(selector) => selector,
            None => return false,
        };
        let config = &mut *core::ptr::addr_of_mut!(CONFIG);
        config.address = address.to_be();
        config.fourcc = FOURCC_XR24.to_be();
        config.flags = 0;
        config.width = width.to_be();
        config.height = height.to_be();
        config.stride = stride.to_be();

        let dma = &mut *core::ptr::addr_of_mut!(DMA);
        dma.control = (((selector as u32) << 16) | DMA_SELECT | DMA_WRITE).to_be();
        dma.length = RAMFB_CONFIG_BYTES.to_be();
        dma.address = (config as *const RamfbConfig as u64).to_be();

        crate::cache_clean_range(config as *const RamfbConfig as usize, core::mem::size_of::<RamfbConfig>());
        crate::cache_clean_range(dma as *const DmaAccess as usize, core::mem::size_of::<DmaAccess>());

        let dma_address = dma as *const DmaAccess as u64;
        mmio_write32(base + REG_DMA, ((dma_address >> 32) as u32).to_be());
        mmio_write32(base + REG_DMA + 4, (dma_address as u32).to_be());

        let mut spins = 0u32;
        loop {
            crate::cache_clean_invalidate_range(dma as *const DmaAccess as usize, core::mem::size_of::<DmaAccess>());
            let control = u32::from_be(core::ptr::read_volatile(&dma.control));
            if control & !DMA_ERROR == 0 {
                return control & DMA_ERROR == 0;
            }
            spins += 1;
            if spins > 1_000_000 {
                return false;
            }
        }
    }
}

pub fn probe(base: usize) -> bool {
    unsafe {
        select(base, 0x0000);
        let a = mmio_read8(base + REG_DATA);
        let b = mmio_read8(base + REG_DATA);
        let c = mmio_read8(base + REG_DATA);
        let d = mmio_read8(base + REG_DATA);
        a == b'Q' && b == b'E' && c == b'M' && d == b'U'
    }
}
