use core::arch::asm;

const GIB: u64 = 1 << 30;
const BLOCK: u64 = 1 << 21;
const PAGE: u64 = 4096;
pub const MAPPED_GIB: usize = 16;
pub const MAPPED_BYTES: u64 = (MAPPED_GIB as u64) << 30;
pub const USER_L0_INDEX: usize = 1;
const IMAGE_TABLES: usize = 8;

const ATTR_DEVICE: u64 = 0;
const ATTR_NORMAL: u64 = 1;
const ATTR_NORMAL_NC: u64 = 2;

const MAIR: u64 = 0x0000_0000_0044_FF00;

const DESC_BLOCK: u64 = 0b01;
const DESC_TABLE: u64 = 0b11;
const DESC_PAGE: u64 = 0b11;
const DESC_AF: u64 = 1 << 10;
const DESC_SH_INNER: u64 = 3 << 8;
const DESC_AP_RO: u64 = 1 << 7;
const DESC_PXN: u64 = 1 << 53;
const DESC_UXN: u64 = 1 << 54;

const SCTLR_RES1: u64 = 0x30D0_0800;
const SCTLR_M: u64 = 1 << 0;
const SCTLR_C: u64 = 1 << 2;
const SCTLR_I: u64 = 1 << 12;
const SCTLR_WXN: u64 = 1 << 19;

#[derive(Clone, Copy)]
#[repr(C, align(4096))]
struct Table([u64; 512]);

static mut LEVEL0: Table = Table([0; 512]);
static mut LEVEL1: Table = Table([0; 512]);
static mut LEVEL2: [Table; MAPPED_GIB] = [Table([0; 512]); MAPPED_GIB];
static mut LEVEL3: [Table; IMAGE_TABLES] = [Table([0; 512]); IMAGE_TABLES];

#[derive(Clone, Copy)]
pub struct Range {
    pub start: u64,
    pub end: u64,
}

pub struct Layout<'a> {
    pub ram: &'a [Range],
    pub uncached: &'a [Range],
    pub image: Range,
    pub text: Range,
    pub rodata: Range,
}

fn overlaps(ranges: &[Range], start: u64, end: u64) -> bool {
    for range in ranges {
        if range.start < end && range.end > start {
            return true;
        }
    }
    false
}

fn inside(range: Range, address: u64) -> bool {
    address >= range.start && address < range.end
}

fn block_descriptor(address: u64, attr: u64) -> u64 {
    let mut value = address | DESC_BLOCK | DESC_AF | (attr << 2) | DESC_UXN | DESC_PXN;
    if attr != ATTR_DEVICE {
        value |= DESC_SH_INNER;
    }
    value
}

fn kernel_page(address: u64, read_only: bool, executable: bool) -> u64 {
    let mut value = address | DESC_PAGE | DESC_AF | (ATTR_NORMAL << 2) | DESC_SH_INNER | DESC_UXN;
    if read_only {
        value |= DESC_AP_RO;
    }
    if !executable {
        value |= DESC_PXN;
    }
    value
}

pub fn kernel_root() -> u64 {
    core::ptr::addr_of!(LEVEL0) as u64
}

pub fn kernel_l1_phys() -> u64 {
    core::ptr::addr_of!(LEVEL1) as u64
}

pub unsafe fn switch_root(root: u64) {
    let current: u64;
    asm!("mrs {}, ttbr0_el1", out(reg) current, options(nomem, nostack, preserves_flags));
    if (current & !0xFFFu64) == (root & !0xFFFu64) {
        return;
    }
    asm!(
        "msr ttbr0_el1, {root}",
        "isb",
        "tlbi vmalle1",
        "dsb nsh",
        "isb",
        root = in(reg) root,
        options(nostack)
    );
}

pub unsafe fn enable(layout: &Layout<'_>) -> bool {
    let level0 = &mut *core::ptr::addr_of_mut!(LEVEL0);
    let level1 = &mut *core::ptr::addr_of_mut!(LEVEL1);
    let level2 = &mut *core::ptr::addr_of_mut!(LEVEL2);
    let level3 = &mut *core::ptr::addr_of_mut!(LEVEL3);

    level0.0[0] = (level1.0.as_ptr() as u64) | DESC_TABLE;
    for gib in 0..MAPPED_GIB {
        level1.0[gib] = (level2[gib].0.as_ptr() as u64) | DESC_TABLE;
        for index in 0..512usize {
            let base = gib as u64 * GIB + index as u64 * BLOCK;
            let end = base + BLOCK;
            let attr = if overlaps(layout.uncached, base, end) {
                ATTR_NORMAL_NC
            } else if overlaps(layout.ram, base, end) {
                ATTR_NORMAL
            } else {
                ATTR_DEVICE
            };
            level2[gib].0[index] = block_descriptor(base, attr);
        }
    }

    let mut used = 0usize;
    let mut block = layout.image.start & !(BLOCK - 1);
    while block < layout.image.end {
        if used >= IMAGE_TABLES || block >= MAPPED_BYTES {
            return false;
        }
        let gib = (block >> 30) as usize;
        let index = ((block >> 21) & 511) as usize;
        let table = &mut level3[used];
        used += 1;
        for page in 0..512usize {
            let address = block + page as u64 * PAGE;
            let (read_only, executable) = if inside(layout.text, address) {
                (true, true)
            } else if inside(layout.rodata, address) {
                (true, false)
            } else {
                (false, false)
            };
            table.0[page] = kernel_page(address, read_only, executable);
        }
        level2[gib].0[index] = (table.0.as_ptr() as u64) | DESC_TABLE;
        block += BLOCK;
    }

    let mmfr0: u64;
    asm!("mrs {}, id_aa64mmfr0_el1", out(reg) mmfr0, options(nomem, nostack, preserves_flags));
    let ips = (mmfr0 & 0xF).min(5);
    let tcr: u64 = 16 | (1 << 8) | (1 << 10) | (3 << 12) | (1 << 23) | (ips << 32);
    let ttbr = level0.0.as_ptr() as u64;
    let sctlr = SCTLR_RES1 | SCTLR_M | SCTLR_C | SCTLR_I | SCTLR_WXN;

    asm!(
        "dsb sy",
        "msr mair_el1, {mair}",
        "msr tcr_el1, {tcr}",
        "msr ttbr0_el1, {ttbr}",
        "isb",
        "tlbi vmalle1",
        "ic iallu",
        "dsb nsh",
        "isb",
        "msr sctlr_el1, {sctlr}",
        "isb",
        mair = in(reg) MAIR,
        tcr = in(reg) tcr,
        ttbr = in(reg) ttbr,
        sctlr = in(reg) sctlr,
        options(nostack)
    );
    true
}
