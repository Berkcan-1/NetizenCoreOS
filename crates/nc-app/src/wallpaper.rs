use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

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
