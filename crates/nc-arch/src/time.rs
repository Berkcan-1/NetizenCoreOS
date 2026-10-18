use crate::idt::IRQ_TIMER_TICKS;
use crate::{inb, outb};
use core::arch::asm;
use core::sync::atomic::Ordering;

const PIT_COMMAND: u16 = 0x43;
const PIT_CHANNEL0: u16 = 0x40;
const PIT_HZ: u64 = 1_193_182;
const CALIBRATION_DIVISOR: u64 = 50;
const MIN_TICKS_PER_MS: u64 = 1_000;

fn read_pit() -> u16 {
    unsafe {
        outb(PIT_COMMAND, 0x00);
        let low = inb(PIT_CHANNEL0) as u16;
        let high = inb(PIT_CHANNEL0) as u16;
        (high << 8) | low
    }
}

pub fn read_tsc() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!("rdtsc", out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags));
    }
    ((high as u64) << 32) | low as u64
}

pub struct Clock {
    ticks_per_ms: u64,
    start: u64,
    last_pit: u16,
    pit_accum_ticks: u64,
}

impl Clock {
    pub fn calibrate() -> Clock {
        unsafe {
            outb(PIT_COMMAND, 0x34);
            outb(PIT_CHANNEL0, 0x00);
            outb(PIT_CHANNEL0, 0x00);
        }
        let target = PIT_HZ / CALIBRATION_DIVISOR;
        let mut elapsed: u64 = 0;
        let mut last = read_pit();
        let tsc_start = read_tsc();
        while elapsed < target {
            let now = read_pit();
            elapsed += last.wrapping_sub(now) as u64;
            last = now;
            core::hint::spin_loop();
        }
        let tsc_end = read_tsc();
        let microseconds = elapsed * 1_000_000 / PIT_HZ;
        let mut ticks_per_ms = 0;
        if microseconds > 0 {
            ticks_per_ms = tsc_end.wrapping_sub(tsc_start) * 1000 / microseconds;
        }
        if ticks_per_ms < MIN_TICKS_PER_MS {
            ticks_per_ms = 0;
        }
        Clock {
            ticks_per_ms,
            start: read_tsc(),
            last_pit: read_pit(),
            pit_accum_ticks: 0,
        }
    }

    pub fn is_precise(&self) -> bool {
        self.ticks_per_ms != 0
    }

    pub fn now_ms(&mut self) -> u32 {
        if self.ticks_per_ms != 0 {
            return (read_tsc().wrapping_sub(self.start) / self.ticks_per_ms) as u32;
        }
        // PAIR-025: TSC yoksa once 1000 Hz IRQ sayacina, kesmeler kapaliysa gercek PIT donanimina bak
        let irq_ms = IRQ_TIMER_TICKS.load(Ordering::Relaxed);
        if irq_ms > 0 {
            return irq_ms as u32;
        }
        let now_pit = read_pit();
        let delta = self.last_pit.wrapping_sub(now_pit) as u64;
        self.last_pit = now_pit;
        self.pit_accum_ticks = self.pit_accum_ticks.wrapping_add(delta);
        ((self.pit_accum_ticks * 1000) / PIT_HZ) as u32
    }

    pub fn sleep_until(&mut self, target_ms: u32) {
        while self.now_ms().wrapping_sub(target_ms) > u32::MAX / 2 {
            crate::wait_for_interrupt();
        }
    }
}
