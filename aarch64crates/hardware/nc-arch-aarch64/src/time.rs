use core::arch::asm;

pub fn read_tsc() -> u64 {
    let value: u64;
    unsafe {
        asm!("isb", "mrs {}, cntvct_el0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

static FREQUENCY_HINT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

pub fn set_frequency_hint(hz: u64) {
    FREQUENCY_HINT.store(hz, core::sync::atomic::Ordering::Relaxed);
}

pub fn frequency_hz() -> u64 {
    let value: u64;
    unsafe {
        asm!("mrs {}, cntfrq_el0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    if value != 0 {
        value
    } else {
        FREQUENCY_HINT.load(core::sync::atomic::Ordering::Relaxed)
    }
}

pub struct Clock {
    ticks_per_ms: u64,
    start: u64,
    precise: bool,
}

impl Clock {
    pub fn calibrate() -> Clock {
        let frequency = frequency_hz();
        let precise = frequency >= 1_000_000;
        let ticks_per_ms = if precise { frequency / 1000 } else { 19_200 };
        Clock {
            ticks_per_ms,
            start: read_tsc(),
            precise,
        }
    }

    pub fn ticks_per_ms(&self) -> u64 {
        self.ticks_per_ms
    }

    pub fn is_precise(&self) -> bool {
        self.precise
    }

    pub fn now_ms(&mut self) -> u32 {
        (read_tsc().wrapping_sub(self.start) / self.ticks_per_ms) as u32
    }

    pub fn sleep_until(&mut self, target_ms: u32) {
        while self.now_ms().wrapping_sub(target_ms) > 0x8000_0000 {
            core::hint::spin_loop();
        }
    }
}
