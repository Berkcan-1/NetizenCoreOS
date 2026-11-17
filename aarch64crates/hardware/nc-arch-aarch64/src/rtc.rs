use crate::mmio_read32;
use core::sync::atomic::{AtomicUsize, Ordering};

static PL031_BASE: AtomicUsize = AtomicUsize::new(0);

pub fn configure_pl031(base: usize) {
    PL031_BASE.store(base, Ordering::Relaxed);
}

pub fn unix_time_now() -> Option<u64> {
    let base = PL031_BASE.load(Ordering::Relaxed);
    if base == 0 {
        return None;
    }
    let seconds = unsafe { mmio_read32(base) } as u64;
    if seconds == 0 {
        None
    } else {
        Some(seconds)
    }
}
