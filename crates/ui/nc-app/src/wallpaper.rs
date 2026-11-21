use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const SWATCH_SLOTS: usize = 6;
const SWATCH_UNSET: u32 = u32::MAX;
static SWATCH_TOP: [AtomicU32; SWATCH_SLOTS] = [const { AtomicU32::new(SWATCH_UNSET) }; SWATCH_SLOTS];
static SWATCH_BOTTOM: [AtomicU32; SWATCH_SLOTS] = [const { AtomicU32::new(SWATCH_UNSET) }; SWATCH_SLOTS];

pub fn set_swatch(index: usize, top: u32, bottom: u32) {
    if index < SWATCH_SLOTS {
        SWATCH_TOP[index].store(top & 0x00FF_FFFF, Ordering::Relaxed);
        SWATCH_BOTTOM[index].store(bottom & 0x00FF_FFFF, Ordering::Relaxed);
    }
}

pub fn swatch(index: usize) -> (u32, u32) {
    let i = index % SWATCH_SLOTS;
    let top = SWATCH_TOP[i].load(Ordering::Relaxed);
    let bottom = SWATCH_BOTTOM[i].load(Ordering::Relaxed);
    if top == SWATCH_UNSET || bottom == SWATCH_UNSET {
        let fallback = &crate::theme::WALLPAPERS[i];
        (fallback.top, fallback.bottom)
    } else {
        (top, bottom)
    }
}

pub fn pack565(color: u32) -> u16 {
    let r = (color >> 16) & 0xFF;
    let g = (color >> 8) & 0xFF;
    let b = color & 0xFF;
    (((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)) as u16
}

pub fn unpack565(value: u16) -> u32 {
    let v = value as u32;
    let r = ((v >> 11) & 0x1F) * 255 / 31;
    let g = ((v >> 5) & 0x3F) * 255 / 63;
    let b = (v & 0x1F) * 255 / 31;
    (r << 16) | (g << 8) | b
}

pub struct CustomWallpaper {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
    pub source: String,
}

struct Slot {
    lock: AtomicBool,
    changed: AtomicBool,
    value: UnsafeCell<Option<CustomWallpaper>>,
}

unsafe impl Sync for Slot {}

static SLOT: Slot = Slot {
    lock: AtomicBool::new(false),
    changed: AtomicBool::new(false),
    value: UnsafeCell::new(None),
};

fn locked<R>(run: impl FnOnce(&mut Option<CustomWallpaper>) -> R) -> R {
    while SLOT
        .lock
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let result = run(unsafe { &mut *SLOT.value.get() });
    SLOT.lock.store(false, Ordering::Release);
    result
}

pub fn set(wallpaper: CustomWallpaper) {
    locked(|slot| *slot = Some(wallpaper));
    SLOT.changed.store(true, Ordering::Release);
}

pub fn clear() {
    let had = locked(|slot| slot.take().is_some());
    if had {
        SLOT.changed.store(true, Ordering::Release);
    }
}

pub fn is_active() -> bool {
    locked(|slot| slot.is_some())
}

pub fn with_active<R>(run: impl FnOnce(&CustomWallpaper) -> R) -> Option<R> {
    locked(|slot| slot.as_ref().map(run))
}

pub fn take_changed() -> bool {
    SLOT.changed.swap(false, Ordering::AcqRel)
}
