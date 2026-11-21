use core::arch::asm;
use core::fmt::Write;
use core::ptr::{addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicUsize, Ordering};
use nc_abi::event::{PTR_CLICKED, PTR_DOWN, PTR_HAS, PTR_RIGHT_CLICKED};
use nc_abi::key;
use nc_device::DeviceProfile;
use nc_hal::{InputDevice, KeyCode};

const MMIO_STRIDE: usize = 0x200;
const MMIO_SLOTS: usize = 32;
const MAGIC: u32 = 0x7472_6976;
const DEVICE_ID_INPUT: u32 = 18;
const MAX_DEVICES: usize = 4;
const SPINS: u32 = 1_000_000;

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

const CFG_SELECT: usize = 0x100;
const CFG_SUBSEL: usize = 0x101;
const CFG_SIZE: usize = 0x102;
const CFG_DATA: usize = 0x108;
const CFG_EV_BITS: u8 = 0x11;
const CFG_ABS_INFO: u8 = 0x12;

const STATUS_ACK: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_FAILED: u32 = 128;

const EV_KEY: u16 = 1;
const EV_ABS: u16 = 3;
const ABS_X: u8 = 0;
const ABS_Y: u8 = 1;
const KEY_A: u16 = 30;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_TOUCH: u16 = 0x14A;

const QUEUE_MAX: usize = 64;
const RING_BYTES: usize = 8192;
const USED_OFFSET: usize = 4096;
const EVENT_BYTES: usize = 8;
const EVENTS_OFFSET: usize = RING_BYTES;
const DESC_F_WRITE: u16 = 2;
const DRAIN_BUDGET: usize = 64;
const KEY_QUEUE: usize = 16;

#[repr(C, align(4096))]
struct Queue {
    ring: [u8; RING_BYTES],
    events: [[u8; EVENT_BYTES]; QUEUE_MAX],
}

const _: () = assert!(core::mem::size_of::<Queue>() >= RING_BYTES + QUEUE_MAX * EVENT_BYTES);

impl Queue {
    const ZERO: Queue = Queue { ring: [0; RING_BYTES], events: [[0; EVENT_BYTES]; QUEUE_MAX] };
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Keyboard,
    Tablet,
    Unsupported,
}

#[derive(Clone, Copy)]
struct Device {
    base: usize,
    queue: usize,
    kind: Kind,
    size: u16,
    last_used: u16,
    avail: u16,
}

impl Device {
    const EMPTY: Device = Device { base: 0, queue: 0, kind: Kind::Unsupported, size: 0, last_used: 0, avail: 0 };
}

struct State {
    screen_w: i32,
    screen_h: i32,
    abs_x: (i32, i32),
    abs_y: (i32, i32),
    tablet_bound: bool,
    has_pointer: bool,
    raw_x: i32,
    raw_y: i32,
    down: bool,
    right_down: bool,
    clicked: bool,
    right_clicked: bool,
    click_x: i32,
    click_y: i32,
    wake: bool,
    shift: bool,
    keys: [u32; KEY_QUEUE],
    key_head: usize,
    key_len: usize,
}

impl State {
    const EMPTY: State = State {
        screen_w: 0,
        screen_h: 0,
        abs_x: (0, 0),
        abs_y: (0, 0),
        tablet_bound: false,
        has_pointer: false,
        raw_x: 0,
        raw_y: 0,
        down: false,
        right_down: false,
        clicked: false,
        right_clicked: false,
        click_x: 0,
        click_y: 0,
        wake: false,
        shift: false,
        keys: [0; KEY_QUEUE],
        key_head: 0,
        key_len: 0,
    };
}

#[derive(Clone, Copy)]
pub struct Snapshot {
    pub pointer_flags: u32,
    pub x: i32,
    pub y: i32,
    pub key: u32,
}

static mut QUEUES: [Queue; MAX_DEVICES] = [Queue::ZERO; MAX_DEVICES];
static mut DEVICES: [Device; MAX_DEVICES] = [Device::EMPTY; MAX_DEVICES];
static mut EXT_DEVICES: [Option<*mut dyn InputDevice>; MAX_DEVICES] = [None; MAX_DEVICES];
static mut EXT_DOWN: [bool; MAX_DEVICES] = [false; MAX_DEVICES];
static mut STATE: State = State::EMPTY;
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

fn state() -> &'static mut State {
    unsafe { &mut *addr_of_mut!(STATE) }
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

unsafe fn r32(address: usize) -> u32 {
    read_volatile(address as *const u32)
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

unsafe fn config_select(base: usize, select: u8, subsel: u8) -> usize {
    nc_arch::mmio_write8(base + CFG_SELECT, select);
    nc_arch::mmio_write8(base + CFG_SUBSEL, subsel);
    barrier();
    nc_arch::mmio_read8(base + CFG_SIZE) as usize
}

unsafe fn config_byte(base: usize, index: usize) -> u8 {
    nc_arch::mmio_read8(base + CFG_DATA + index)
}

unsafe fn config_u32(base: usize, index: usize) -> u32 {
    (config_byte(base, index) as u32)
        | ((config_byte(base, index + 1) as u32) << 8)
        | ((config_byte(base, index + 2) as u32) << 16)
        | ((config_byte(base, index + 3) as u32) << 24)
}

unsafe fn has_bit(base: usize, select: u8, subsel: u8, bit: usize) -> bool {
    let size = config_select(base, select, subsel);
    if bit / 8 >= size {
        return false;
    }
    config_byte(base, bit / 8) & (1 << (bit % 8)) != 0
}

unsafe fn detect(base: usize) -> (Kind, (i32, i32), (i32, i32)) {
    let absolute = has_bit(base, CFG_EV_BITS, EV_ABS as u8, ABS_X as usize) && has_bit(base, CFG_EV_BITS, EV_ABS as u8, ABS_Y as usize);
    if absolute {
        let mut ranges = [(0i32, 0i32); 2];
        for (index, axis) in [ABS_X, ABS_Y].iter().enumerate() {
            if config_select(base, CFG_ABS_INFO, *axis) < 8 {
                return (Kind::Unsupported, (0, 0), (0, 0));
            }
            let minimum = config_u32(base, 0) as i32;
            let maximum = config_u32(base, 4) as i32;
            if maximum <= minimum {
                return (Kind::Unsupported, (0, 0), (0, 0));
            }
            ranges[index] = (minimum, maximum);
        }
        return (Kind::Tablet, ranges[0], ranges[1]);
    }
    if has_bit(base, CFG_EV_BITS, EV_KEY as u8, KEY_A as usize) {
        return (Kind::Keyboard, (0, 0), (0, 0));
    }
    (Kind::Unsupported, (0, 0), (0, 0))
}

unsafe fn negotiate(base: usize, version: u32) -> bool {
    if version == 2 {
        wr(base, REG_DEVICE_FEATURES_SEL, 1);
        if rd(base, REG_DEVICE_FEATURES) & 1 == 0 {
            return false;
        }
        wr(base, REG_DRIVER_FEATURES_SEL, 0);
        wr(base, REG_DRIVER_FEATURES, 0);
        wr(base, REG_DRIVER_FEATURES_SEL, 1);
        wr(base, REG_DRIVER_FEATURES, 1);
        wr(base, REG_STATUS, STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK);
        rd(base, REG_STATUS) & STATUS_FEATURES_OK != 0
    } else {
        wr(base, REG_DRIVER_FEATURES_SEL, 0);
        wr(base, REG_DRIVER_FEATURES, 0);
        wr(base, REG_GUEST_PAGE_SIZE, 4096);
        true
    }
}

unsafe fn setup_queue(base: usize, version: u32, queue: usize) -> Option<usize> {
    wr(base, REG_QUEUE_SEL, 0);
    barrier();
    let maximum = rd(base, REG_QUEUE_NUM_MAX) as usize;
    if maximum < 8 {
        return None;
    }
    let mut size = QUEUE_MAX;
    while size > maximum {
        size >>= 1;
    }
    let events = queue + EVENTS_OFFSET;
    let available_at = queue + size * 16;
    for index in 0..size {
        let descriptor = queue + index * 16;
        w64(descriptor, (events + index * EVENT_BYTES) as u64);
        w32(descriptor + 8, EVENT_BYTES as u32);
        w16(descriptor + 12, DESC_F_WRITE);
        w16(descriptor + 14, 0);
        w16(available_at + 4 + index * 2, index as u16);
    }
    w16(available_at, 0);
    w16(available_at + 2, size as u16);
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

unsafe fn fail(base: usize) {
    wr(base, REG_STATUS, STATUS_FAILED);
}

unsafe fn bring_up(base: usize, version: u32, index: usize) -> bool {
    if version != 1 && version != 2 {
        log!("[girdi] {:#x}: virtio-mmio surum {} desteklenmiyor", base, version);
        return false;
    }
    wr(base, REG_STATUS, 0);
    let mut spins = 0u32;
    while rd(base, REG_STATUS) != 0 {
        spins += 1;
        if spins > SPINS {
            log!("[girdi] {:#x}: sifirlama zaman asimi", base);
            return false;
        }
    }
    wr(base, REG_STATUS, STATUS_ACK);
    wr(base, REG_STATUS, STATUS_ACK | STATUS_DRIVER);

    let (kind, range_x, range_y) = detect(base);
    if kind == Kind::Unsupported {
        wr(base, REG_STATUS, 0);
        log!("[girdi] {:#x}: desteklenmeyen girdi aygiti (goreli fare vb.), atlandi", base);
        return false;
    }
    let state = state();
    if kind == Kind::Tablet && state.tablet_bound {
        wr(base, REG_STATUS, 0);
        log!("[girdi] {:#x}: ikinci mutlak isaretci aygiti atlandi", base);
        return false;
    }
    if !negotiate(base, version) {
        fail(base);
        log!("[girdi] {:#x}: ozellik anlasmasi basarisiz", base);
        return false;
    }
    let queue = (addr_of_mut!(QUEUES) as *mut Queue).add(index) as usize;
    let size = match setup_queue(base, version, queue) {
        Some(size) => size,
        None => {
            fail(base);
            log!("[girdi] {:#x}: kuyruk kurulamadi", base);
            return false;
        }
    };
    if kind == Kind::Tablet {
        state.abs_x = range_x;
        state.abs_y = range_y;
        state.tablet_bound = true;
    }
    *(addr_of_mut!(DEVICES) as *mut Device).add(index) =
        Device { base, queue, kind, size: size as u16, last_used: 0, avail: size as u16 };
    let ok = if version == 2 {
        STATUS_ACK | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK
    } else {
        STATUS_ACK | STATUS_DRIVER | STATUS_DRIVER_OK
    };
    barrier();
    wr(base, REG_STATUS, ok);
    wr(base, REG_QUEUE_NOTIFY, 0);
    match kind {
        Kind::Tablet => log!(
            "[girdi] {:#x}: virtio-input mutlak isaretci (surum {}, x {}..{}, y {}..{}, kuyruk {})",
            base, version, range_x.0, range_x.1, range_y.0, range_y.1, size
        ),
        _ => log!("[girdi] {:#x}: virtio-input klavye (surum {}, kuyruk {})", base, version, size),
    }
    true
}

pub fn register_device(device: &'static mut dyn InputDevice) {
    let slot = EXT_COUNT.load(Ordering::Acquire);
    if slot >= MAX_DEVICES {
        return;
    }
    unsafe {
        let base = addr_of_mut!(EXT_DEVICES) as *mut Option<*mut dyn InputDevice>;
        *base.add(slot) = Some(device as *mut dyn InputDevice);
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
        if ext == 0 {
            log!("[girdi] profilde virtio-mmio yok; girdi surucusu kapali");
        }
        return ext;
    }
    let mut count = 0usize;
    for slot in 0..MMIO_SLOTS {
        let address = base + slot * MMIO_STRIDE;
        unsafe {
            if rd(address, REG_MAGIC) != MAGIC || rd(address, REG_DEVICE_ID) != DEVICE_ID_INPUT {
                continue;
            }
            if count >= MAX_DEVICES {
                log!("[girdi] en fazla {} girdi aygiti destekleniyor", MAX_DEVICES);
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

pub fn set_screen(width: i32, height: i32) {
    let state = state();
    state.screen_w = width;
    state.screen_h = height;
}

fn scale(raw: i32, range: (i32, i32), size: i32) -> i32 {
    let (minimum, maximum) = range;
    if size <= 0 || maximum <= minimum {
        return 0;
    }
    let clamped = raw.clamp(minimum, maximum) as i64;
    (((clamped - minimum as i64) * (size as i64 - 1)) / (maximum as i64 - minimum as i64)) as i32
}

fn char_for(code: u16, shift: bool) -> Option<char> {
    let (start, plain, shifted): (u16, &[u8], &[u8]) = match code {
        2..=13 => (2, &b"1234567890-="[..], &b"!@#$%^&*()_+"[..]),
        16..=27 => (16, &b"qwertyuiop[]"[..], &b"QWERTYUIOP{}"[..]),
        30..=40 => (30, &b"asdfghjkl;'"[..], &b"ASDFGHJKL:\""[..]),
        44..=53 => (44, &b"zxcvbnm,./"[..], &b"ZXCVBNM<>?"[..]),
        41 => return Some(if shift { '~' } else { '`' }),
        43 => return Some(if shift { '|' } else { '\\' }),
        _ => return None,
    };
    let table = if shift { shifted } else { plain };
    table.get((code - start) as usize).map(|byte| *byte as char)
}

fn translate(code: u16, shift: bool) -> u32 {
    match code {
        1 => key::ESCAPE,
        14 => key::BACKSPACE,
        15 => key::TAB,
        28 | 96 => key::ENTER,
        57 => key::SPACE,
        103 => key::UP,
        108 => key::DOWN,
        105 => key::LEFT,
        106 => key::RIGHT,
        _ => match char_for(code, shift) {
            Some(character) => key::from_char(character),
            None => key::UNKNOWN,
        },
    }
}

fn push_key(state: &mut State, value: u32) {
    if state.key_len >= KEY_QUEUE {
        return;
    }
    let tail = (state.key_head + state.key_len) % KEY_QUEUE;
    state.keys[tail] = value;
    state.key_len += 1;
    state.wake = true;
}

fn handle_tablet(state: &mut State, kind: u16, code: u16, value: u32) {
    if kind == EV_ABS {
        if code == ABS_X as u16 {
            state.raw_x = value as i32;
            state.has_pointer = true;
        } else if code == ABS_Y as u16 {
            state.raw_y = value as i32;
            state.has_pointer = true;
        } else {
            return;
        }
        if state.down {
            state.wake = true;
        }
        return;
    }
    if kind != EV_KEY {
        return;
    }
    let pressed = value != 0;
    if code == BTN_LEFT || code == BTN_TOUCH {
        if pressed && !state.down {
            state.down = true;
            state.clicked = true;
            state.click_x = state.raw_x;
            state.click_y = state.raw_y;
            state.wake = true;
        } else if !pressed && state.down {
            state.down = false;
            state.wake = true;
        }
    } else if code == BTN_RIGHT {
        if pressed && !state.right_down {
            state.right_down = true;
            state.right_clicked = true;
            state.wake = true;
        } else if !pressed {
            state.right_down = false;
        }
    }
}

fn handle_keyboard(state: &mut State, kind: u16, code: u16, value: u32) {
    if kind != EV_KEY {
        return;
    }
    match code {
        42 | 54 => state.shift = value != 0,
        29 | 97 | 56 | 100 | 125 | 126 => {}
        _ => {
            if value != 0 {
                let translated = translate(code, state.shift);
                push_key(state, translated);
            }
        }
    }
}

unsafe fn drain(device: &mut Device) {
    let ring = device.queue;
    let used = ring + USED_OFFSET;
    let size = device.size as usize;
    let available = ring + size * 16;
    barrier();
    let produced = r16(used + 2);
    let mut handled = false;
    let mut budget = DRAIN_BUDGET;
    while device.last_used != produced && budget > 0 {
        let slot = device.last_used as usize & (size - 1);
        let id = r32(used + 4 + slot * 8) as usize;
        if id < size {
            let event = ring + EVENTS_OFFSET + id * EVENT_BYTES;
            let kind = r16(event);
            let code = r16(event + 2);
            let value = r32(event + 4);
            let state = state();
            match device.kind {
                Kind::Tablet => handle_tablet(state, kind, code, value),
                Kind::Keyboard => handle_keyboard(state, kind, code, value),
                Kind::Unsupported => {}
            }
            let position = device.avail as usize & (size - 1);
            w16(available + 4 + position * 2, id as u16);
            device.avail = device.avail.wrapping_add(1);
        }
        device.last_used = device.last_used.wrapping_add(1);
        budget -= 1;
        handled = true;
    }
    if handled {
        barrier();
        w16(available + 2, device.avail);
        barrier();
        wr(device.base, REG_QUEUE_NOTIFY, 0);
        let pending = rd(device.base, REG_INTERRUPT_STATUS);
        if pending != 0 {
            wr(device.base, REG_INTERRUPT_ACK, pending);
        }
    }
}

fn translate_keycode(kc: KeyCode) -> u32 {
    match kc {
        KeyCode::Enter => key::ENTER,
        KeyCode::Escape => key::ESCAPE,
        KeyCode::Space => key::SPACE,
        KeyCode::Tab => key::TAB,
        KeyCode::Backspace => key::BACKSPACE,
        KeyCode::Up => key::UP,
        KeyCode::Down => key::DOWN,
        KeyCode::Left => key::LEFT,
        KeyCode::Right => key::RIGHT,
        KeyCode::Char(c) => key::from_char(c),
        KeyCode::Unknown => key::UNKNOWN,
    }
}

pub fn poll() {
    let count = COUNT.load(Ordering::Acquire);
    for index in 0..count {
        unsafe {
            let device = &mut *(addr_of_mut!(DEVICES) as *mut Device).add(index);
            drain(device);
        }
    }
    let ext_count = EXT_COUNT.load(Ordering::Acquire);
    if ext_count > 0 {
        let state = state();
        let w = state.screen_w.max(1);
        let h = state.screen_h.max(1);
        for index in 0..ext_count {
            let ext_base = addr_of_mut!(EXT_DEVICES) as *const Option<*mut dyn InputDevice>;
            let down_base = addr_of_mut!(EXT_DOWN) as *mut bool;
            let Some(ptr) = (unsafe { *ext_base.add(index) }) else {
                continue;
            };
            let (mouse, key_opt) = unsafe { (&mut *ptr).poll(w, h) };
            let was_down = unsafe { *down_base.add(index) };
            if mouse.left_down || mouse.left_clicked || was_down {
                if !state.tablet_bound {
                    state.abs_x = (0, (w - 1).max(1));
                    state.abs_y = (0, (h - 1).max(1));
                }
                state.has_pointer = true;
                let moved = state.raw_x != mouse.x || state.raw_y != mouse.y;
                state.raw_x = mouse.x;
                state.raw_y = mouse.y;
                if mouse.left_clicked || (mouse.left_down && !was_down) {
                    state.down = true;
                    state.clicked = true;
                    state.click_x = mouse.x;
                    state.click_y = mouse.y;
                    state.wake = true;
                } else if !mouse.left_down && was_down {
                    state.down = false;
                    state.wake = true;
                } else if mouse.left_down && moved {
                    state.wake = true;
                }
                unsafe {
                    *down_base.add(index) = mouse.left_down;
                }
            }
            if mouse.right_clicked {
                state.right_down = mouse.right_down;
                state.right_clicked = true;
                state.wake = true;
            }
            if let Some(kc) = key_opt {
                let code = translate_keycode(kc);
                if code != key::NONE {
                    push_key(state, code);
                }
            }
        }
    }
}

pub fn pending() -> bool {
    let state = state();
    state.wake || state.key_len > 0
}

pub fn is_down() -> bool {
    state().down
}

pub fn snapshot() -> Snapshot {
    let state = state();
    let mut flags = 0u32;
    let (mut x, mut y) = (0, 0);
    if state.has_pointer {
        flags |= PTR_HAS;
        if state.down {
            flags |= PTR_DOWN;
        }
        if state.clicked {
            flags |= PTR_CLICKED;
        }
        if state.right_clicked {
            flags |= PTR_RIGHT_CLICKED;
        }
        let (raw_x, raw_y) = if state.clicked { (state.click_x, state.click_y) } else { (state.raw_x, state.raw_y) };
        x = scale(raw_x, state.abs_x, state.screen_w);
        y = scale(raw_y, state.abs_y, state.screen_h);
    }
    let key = if state.key_len > 0 { state.keys[state.key_head] } else { key::NONE };
    Snapshot { pointer_flags: flags, x, y, key }
}

pub fn consume() {
    let state = state();
    if state.clicked {
        log!(
            "[girdi] dokunma: x={} y={}",
            scale(state.click_x, state.abs_x, state.screen_w),
            scale(state.click_y, state.abs_y, state.screen_h)
        );
    }
    state.clicked = false;
    state.right_clicked = false;
    if state.key_len > 0 {
        log!("[girdi] tus: kod={:#x}", state.keys[state.key_head]);
        state.key_head = (state.key_head + 1) % KEY_QUEUE;
        state.key_len -= 1;
    }
    state.wake = state.key_len > 0;
}
