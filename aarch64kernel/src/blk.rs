use core::arch::asm;
use core::fmt::Write;
use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicUsize, Ordering};
use nc_device::DeviceProfile;
use nc_hal::{BlockDevice, BlockDeviceInfo, DeviceType};

const MMIO_STRIDE: usize = 0x200;
const MMIO_SLOTS: usize = 32;
const MAGIC: u32 = 0x7472_6976;
const DEVICE_ID_BLOCK: u32 = 2;
const MAX_DISKS: usize = 8;
const SPINS_RESET: u32 = 1_000_000;
const SPINS_IO: u32 = 50_000_000;

const REG_MAGIC: usize = 0x000;
const REG_VERSION: usize = 0x004;
const REG_DEVICE_ID: usize = 0x008;
const REG_DEVICE_FEATURES: usize = 0x010;
const REG_DEVICE_FEATURES_SEL: usize = 0x014;
const REG_DRIVER_FEATURES: usize = 0x020;
const REG_DRIVER_FEATURES_SEL: usize = 0x024;
const REG_GUEST_PAGE_SIZE: usize = 0x028;
const REG_QUEUE_SEL: usize = 0x030;
const REG_QUEUE_NUM_MAX: usize = 0x034;
const REG_QUEUE_NUM: usize = 0x038;
const REG_QUEUE_ALIGN: usize = 0x03C;
const REG_QUEUE_PFN: usize = 0x040;
const REG_QUEUE_READY: usize = 0x044;
const REG_QUEUE_NOTIFY: usize = 0x050;
const REG_INTERRUPT_STATUS: usize = 0x060;
const REG_INTERRUPT_ACK: usize = 0x064;
const REG_STATUS: usize = 0x070;
const REG_QUEUE_DESC_LOW: usize = 0x080;
const REG_QUEUE_DESC_HIGH: usize = 0x084;
const REG_QUEUE_AVAIL_LOW: usize = 0x090;
const REG_QUEUE_AVAIL_HIGH: usize = 0x094;
const REG_QUEUE_USED_LOW: usize = 0x0A0;
const REG_QUEUE_USED_HIGH: usize = 0x0A4;
const REG_CONFIG_CAPACITY: usize = 0x100;

const STATUS_ACK: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_FAILED: u32 = 128;

const RING_BYTES: usize = 8192;
const USED_OFFSET: usize = 4096;
const REQUEST_OFFSET: usize = RING_BYTES;
const STATUS_OFFSET: usize = REQUEST_OFFSET + 16;
const QUEUE_SIZE: usize = 8;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;
const REQ_IN: u32 = 0;
const REQ_OUT: u32 = 1;
const REQ_FLUSH: u32 = 4;
const FEATURE_RO: u32 = 1 << 5;
const FEATURE_FLUSH: u32 = 1 << 9;
const SECTOR: usize = 512;
const CHUNK_BYTES: usize = 64 * 1024;

#[repr(C, align(4096))]
struct Queue {
    ring: [u8; RING_BYTES],
    request: [u8; 32],
}

const _: () = assert!(core::mem::size_of::<Queue>() >= RING_BYTES + 32);

impl Queue {
    const ZERO: Queue = Queue { ring: [0; RING_BYTES], request: [0; 32] };
}

#[derive(Clone, Copy)]
struct Disk {
    base: usize,
    queue: usize,
    size: u16,
    last_used: u16,
    avail: u16,
    sectors: u64,
    read_only: bool,
    flush: bool,
}

impl Disk {
    const EMPTY: Disk = Disk { base: 0, queue: 0, size: 0, last_used: 0, avail: 0, sectors: 0, read_only: true, flush: false };
}

static mut QUEUES: [Queue; MAX_DISKS] = [Queue::ZERO; MAX_DISKS];
static mut DISKS: [Disk; MAX_DISKS] = [Disk::EMPTY; MAX_DISKS];
static mut EXT_DISKS: [Option<*mut dyn BlockDevice>; MAX_DISKS] = [None; MAX_DISKS];
static COUNT: AtomicUsize = AtomicUsize::new(0);
static EXT_COUNT: AtomicUsize = AtomicUsize::new(0);

struct Serial;

impl Write for Serial {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        nc_arch::serial::write_str(text);
        Ok(())
    }
}

macro_rules! log {
    ($($argument:tt)*) => {{
        let _ = writeln!(Serial, $($argument)*);
    }};
}

#[inline(always)]
fn barrier() {
    unsafe {
        asm!("dmb sy", options(nostack, preserves_flags));
    }
}

unsafe fn rd(base: usize, register: usize) -> u32 {
    nc_arch::mmio_read32(base + register)
}

unsafe fn wr(base: usize, register: usize, value: u32) {
    nc_arch::mmio_write32(base + register, value);
}

unsafe fn r16(address: usize) -> u16 {
    read_volatile(address as *const u16)
}

unsafe fn r8(address: usize) -> u8 {
    read_volatile(address as *const u8)
}

unsafe fn w8(address: usize, value: u8) {
    write_volatile(address as *mut u8, value);
}

unsafe fn w16(address: usize, value: u16) {
    write_volatile(address as *mut u16, value);
}

unsafe fn w32(address: usize, value: u32) {
    write_volatile(address as *mut u32, value);
}

unsafe fn w64(address: usize, value: u64) {
    write_volatile(address as *mut u64, value);
}

unsafe fn negotiate(base: usize, version: u32) -> Option<(bool, bool)> {
    wr(base, REG_DEVICE_FEATURES_SEL, 0);
    let low = rd(base, REG_DEVICE_FEATURES);
    let read_only = low & FEATURE_RO != 0;
    let flush = low & FEATURE_FLUSH != 0;
    let accepted = if flush { FEATURE_FLUSH } else { 0 };
    if version == 2 {
        wr(base, REG_DEVICE_FEATURES_SEL, 1);
        if rd(base, REG_DEVICE_FEATURES) & 1 == 0 {
            return None;
        }
        wr(base, REG_DRIVER_FEATURES_SEL, 0);
        wr(base, REG_DRIVER_FEATURES, accepted);
        wr(base, REG_DRIVER_FEATURES_SEL, 1);
        wr(base, REG_DRIVER_FEATURES, 1);
        wr(base, REG_STATUS, STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK);
        if rd(base, REG_STATUS) & STATUS_FEATURES_OK == 0 {
            return None;
        }
    } else {
        wr(base, REG_DRIVER_FEATURES_SEL, 0);
        wr(base, REG_DRIVER_FEATURES, accepted);
        wr(base, REG_GUEST_PAGE_SIZE, 4096);
    }
    Some((read_only, flush))
}

unsafe fn setup_queue(base: usize, version: u32, queue: usize) -> Option<usize> {
    wr(base, REG_QUEUE_SEL, 0);
    barrier();
    let maximum = rd(base, REG_QUEUE_NUM_MAX) as usize;
    if maximum < 4 {
        return None;
    }
    let mut size = QUEUE_SIZE;
    while size > maximum {
        size >>= 1;
    }
    let available_at = queue + size * 16;
    w16(available_at, 0);
    w16(available_at + 2, 0);
    w16(queue + USED_OFFSET, 0);
    w16(queue + USED_OFFSET + 2, 0);
    barrier();
    wr(base, REG_QUEUE_NUM, size as u32);
    if version == 1 {
        if queue >> 44 != 0 {
            return None;
        }
        wr(base, REG_QUEUE_ALIGN, 4096);
        wr(base, REG_QUEUE_PFN, (queue >> 12) as u32);
        if rd(base, REG_QUEUE_PFN) == 0 {
            return None;
        }
    } else {
        let descriptors = queue as u64;
        let available = available_at as u64;
        let used = (queue + USED_OFFSET) as u64;
        wr(base, REG_QUEUE_DESC_LOW, descriptors as u32);
        wr(base, REG_QUEUE_DESC_HIGH, (descriptors >> 32) as u32);
        wr(base, REG_QUEUE_AVAIL_LOW, available as u32);
        wr(base, REG_QUEUE_AVAIL_HIGH, (available >> 32) as u32);
        wr(base, REG_QUEUE_USED_LOW, used as u32);
        wr(base, REG_QUEUE_USED_HIGH, (used >> 32) as u32);
        wr(base, REG_QUEUE_READY, 1);
        if rd(base, REG_QUEUE_READY) != 1 {
            return None;
        }
    }
    Some(size)
}

unsafe fn bring_up(base: usize, version: u32, index: usize) -> bool {
    if version != 1 && version != 2 {
        log!("[depo] {:#x}: virtio-mmio surum {} desteklenmiyor", base, version);
        return false;
    }
    wr(base, REG_STATUS, 0);
    let mut spins = 0u32;
    while rd(base, REG_STATUS) != 0 {
        spins += 1;
        if spins > SPINS_RESET {
            log!("[depo] {:#x}: sifirlama zaman asimi", base);
            return false;
        }
    }
    wr(base, REG_STATUS, STATUS_ACK);
    wr(base, REG_STATUS, STATUS_ACK | STATUS_DRIVER);
    let sectors = (rd(base, REG_CONFIG_CAPACITY) as u64) | ((rd(base, REG_CONFIG_CAPACITY + 4) as u64) << 32);
    if sectors == 0 {
        wr(base, REG_STATUS, STATUS_FAILED);
        log!("[depo] {:#x}: disk kapasitesi 0", base);
        return false;
    }
    let (read_only, flush) = match negotiate(base, version) {
        Some(flags) => flags,
        None => {
            wr(base, REG_STATUS, STATUS_FAILED);
            log!("[depo] {:#x}: ozellik anlasmasi basarisiz", base);
            return false;
        }
    };
    let queue = (addr_of_mut!(QUEUES) as *mut Queue).add(index) as usize;
    let size = match setup_queue(base, version, queue) {
        Some(size) => size,
        None => {
            wr(base, REG_STATUS, STATUS_FAILED);
            log!("[depo] {:#x}: kuyruk kurulamadi", base);
            return false;
        }
    };
    *(addr_of_mut!(DISKS) as *mut Disk).add(index) =
        Disk { base, queue, size: size as u16, last_used: 0, avail: 0, sectors, read_only, flush };
    let ok = if version == 2 {
        STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK
    } else {
        STATUS_ACK | STATUS_DRIVER | STATUS_DRIVER_OK
    };
    barrier();
    wr(base, REG_STATUS, ok);
    log!(
        "[depo] {:#x}: virtio-blk {} sektor ({} MiB, surum {}, kuyruk {}, {}{})",
        base,
        sectors,
        sectors * SECTOR as u64 / (1024 * 1024),
        version,
        size,
        if read_only { "salt-okunur" } else { "yazilabilir" },
        if flush { ", flush" } else { "" }
    );
    true
}

pub fn register_device(device: &'static mut dyn BlockDevice) {
    let slot = EXT_COUNT.load(Ordering::Acquire);
    if slot >= MAX_DISKS {
        return;
    }
    unsafe {
        let base = addr_of_mut!(EXT_DISKS) as *mut Option<*mut dyn BlockDevice>;
        *base.add(slot) = Some(device as *mut dyn BlockDevice);
    }
    EXT_COUNT.store(slot + 1, Ordering::Release);
}

pub fn init(profile: &DeviceProfile) -> usize {
    let ext = EXT_COUNT.load(Ordering::Acquire);
    let mut base = 0usize;
    for entry in profile.drivers.iter() {
        if entry.compatible == "virtio,mmio" {
            base = entry.regs.base as usize;
        }
    }
    if base == 0 {
        return ext;
    }
    let mut count = 0usize;
    for slot in 0..MMIO_SLOTS {
        let address = base + slot * MMIO_STRIDE;
        unsafe {
            if rd(address, REG_MAGIC) != MAGIC || rd(address, REG_DEVICE_ID) != DEVICE_ID_BLOCK {
                continue;
            }
            if count >= MAX_DISKS {
                log!("[depo] en fazla {} blok aygiti destekleniyor", MAX_DISKS);
                break;
            }
            let version = rd(address, REG_VERSION);
            if bring_up(address, version, count) {
                count += 1;
            }
        }
    }
    COUNT.store(count, Ordering::Release);
    count + ext
}

unsafe fn transfer(disk: &mut Disk, kind: u32, lba: u64, buffer: *mut u8, bytes: usize) -> bool {
    let queue = disk.queue;
    let request = queue + REQUEST_OFFSET;
    let status = queue + STATUS_OFFSET;
    let size = disk.size as usize;
    let available_at = queue + size * 16;
    let used = queue + USED_OFFSET;
    w32(request, kind);
    w32(request + 4, 0);
    w64(request + 8, lba);
    w8(status, 0xFF);
    w64(queue, request as u64);
    w32(queue + 8, 16);
    w16(queue + 12, DESC_F_NEXT);
    w16(queue + 14, 1);
    if bytes > 0 {
        let data_flags = if kind == REQ_IN { DESC_F_NEXT | DESC_F_WRITE } else { DESC_F_NEXT };
        w64(queue + 16, buffer as u64);
        w32(queue + 24, bytes as u32);
        w16(queue + 28, data_flags);
        w16(queue + 30, 2);
        w64(queue + 32, status as u64);
        w32(queue + 40, 1);
        w16(queue + 44, DESC_F_WRITE);
        w16(queue + 46, 0);
    } else {
        w64(queue + 16, status as u64);
        w32(queue + 24, 1);
        w16(queue + 28, DESC_F_WRITE);
        w16(queue + 30, 0);
    }
    let slot = disk.avail as usize & (size - 1);
    w16(available_at + 4 + slot * 2, 0);
    disk.avail = disk.avail.wrapping_add(1);
    barrier();
    w16(available_at + 2, disk.avail);
    barrier();
    wr(disk.base, REG_QUEUE_NOTIFY, 0);
    let mut spins = 0u32;
    loop {
        barrier();
        if r16(used + 2) != disk.last_used {
            break;
        }
        spins += 1;
        if spins > SPINS_IO {
            return false;
        }
    }
    disk.last_used = disk.last_used.wrapping_add(1);
    barrier();
    let pending = rd(disk.base, REG_INTERRUPT_STATUS);
    if pending != 0 {
        wr(disk.base, REG_INTERRUPT_ACK, pending);
    }
    r8(status) == 0
}

pub struct VirtioDisk {
    index: usize,
}

pub fn open(index: usize) -> Option<VirtioDisk> {
    if index < count() {
        Some(VirtioDisk { index })
    } else {
        None
    }
}

impl BlockDevice for VirtioDisk {
    fn info(&self) -> BlockDeviceInfo {
        let virtio = COUNT.load(Ordering::Acquire);
        if self.index >= virtio {
            let ext_idx = self.index - virtio;
            let base = addr_of_mut!(EXT_DISKS) as *const Option<*mut dyn BlockDevice>;
            if let Some(ptr) = unsafe { *base.add(ext_idx) } {
                return unsafe { (&*ptr).info() };
            }
            return BlockDeviceInfo::EMPTY;
        }
        let disk = unsafe { *(addr_of_mut!(DISKS) as *mut Disk).add(self.index) };
        let name = b"virtio-blk";
        let mut model = [0u8; 40];
        model[..name.len()].copy_from_slice(name);
        BlockDeviceInfo {
            channel: self.index as u8,
            model,
            model_len: name.len(),
            block_count: disk.sectors,
            block_size: SECTOR as u32,
            device_type: DeviceType::HardDisk,
        }
    }

    fn read_blocks(&mut self, lba: u64, count: u32, buffer: &mut [u8]) -> Result<(), ()> {
        let virtio = COUNT.load(Ordering::Acquire);
        if self.index >= virtio {
            let ext_idx = self.index - virtio;
            let base = addr_of_mut!(EXT_DISKS) as *const Option<*mut dyn BlockDevice>;
            let ptr = unsafe { *base.add(ext_idx) }.ok_or(())?;
            return unsafe { (&mut *ptr).read_blocks(lba, count, buffer) };
        }
        let disk = unsafe { &mut *(addr_of_mut!(DISKS) as *mut Disk).add(self.index) };
        let total = count as usize * SECTOR;
        if buffer.len() < total || lba.checked_add(count as u64).map_or(true, |end| end > disk.sectors) {
            return Err(());
        }
        let mut done = 0usize;
        while done < total {
            let bytes = (total - done).min(CHUNK_BYTES);
            let ok = unsafe { transfer(disk, REQ_IN, lba + (done / SECTOR) as u64, buffer.as_mut_ptr().add(done), bytes) };
            if !ok {
                return Err(());
            }
            done += bytes;
        }
        Ok(())
    }

    fn write_blocks(&mut self, lba: u64, count: u32, buffer: &[u8]) -> Result<(), ()> {
        let virtio = COUNT.load(Ordering::Acquire);
        if self.index >= virtio {
            let ext_idx = self.index - virtio;
            let base = addr_of_mut!(EXT_DISKS) as *const Option<*mut dyn BlockDevice>;
            let ptr = unsafe { *base.add(ext_idx) }.ok_or(())?;
            return unsafe { (&mut *ptr).write_blocks(lba, count, buffer) };
        }
        let disk = unsafe { &mut *(addr_of_mut!(DISKS) as *mut Disk).add(self.index) };
        let total = count as usize * SECTOR;
        if disk.read_only || buffer.len() < total || lba.checked_add(count as u64).map_or(true, |end| end > disk.sectors) {
            return Err(());
        }
        let mut done = 0usize;
        while done < total {
            let bytes = (total - done).min(CHUNK_BYTES);
            let ok = unsafe { transfer(disk, REQ_OUT, lba + (done / SECTOR) as u64, buffer.as_ptr().add(done) as *mut u8, bytes) };
            if !ok {
                return Err(());
            }
            done += bytes;
        }
        Ok(())
    }
}

impl VirtioDisk {
    pub fn is_read_only(&self) -> bool {
        let virtio = COUNT.load(Ordering::Acquire);
        if self.index >= virtio {
            return true;
        }
        unsafe { (*(addr_of_mut!(DISKS) as *mut Disk).add(self.index)).read_only }
    }

    pub fn flush(&self) -> bool {
        let virtio = COUNT.load(Ordering::Acquire);
        if self.index >= virtio {
            return true;
        }
        let disk = unsafe { &mut *(addr_of_mut!(DISKS) as *mut Disk).add(self.index) };
        if disk.read_only {
            return true;
        }
        if !disk.flush {
            return true;
        }
        unsafe { transfer(disk, REQ_FLUSH, 0, core::ptr::null_mut(), 0) }
    }

    pub fn sectors(&self) -> u64 {
        let virtio = COUNT.load(Ordering::Acquire);
        if self.index >= virtio {
            let ext_idx = self.index - virtio;
            let base = addr_of_mut!(EXT_DISKS) as *const Option<*mut dyn BlockDevice>;
            if let Some(ptr) = unsafe { *base.add(ext_idx) } {
                return unsafe { (&*ptr).info().block_count };
            }
            return 0;
        }
        unsafe { (*(addr_of_mut!(DISKS) as *mut Disk).add(self.index)).sectors }
    }
}

pub fn count() -> usize {
    COUNT.load(Ordering::Acquire) + EXT_COUNT.load(Ordering::Acquire)
}
