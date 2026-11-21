#![no_std]

#[cfg(target_arch = "aarch64")]
pub mod exceptions;
#[cfg(target_arch = "aarch64")]
pub mod fwcfg;
#[cfg(target_arch = "aarch64")]
pub mod gic;
#[cfg(target_arch = "aarch64")]
pub mod mmu;
#[cfg(target_arch = "aarch64")]
pub mod psci;
#[cfg(target_arch = "aarch64")]
pub mod rand;
#[cfg(target_arch = "aarch64")]
pub mod rtc;
#[cfg(target_arch = "aarch64")]
pub mod serial;
#[cfg(target_arch = "aarch64")]
pub mod time;
#[cfg(target_arch = "aarch64")]
pub mod timer;

#[cfg(target_arch = "aarch64")]
mod cpu;

#[cfg(target_arch = "aarch64")]
pub use cpu::{
    cache_clean_range, cache_clean_invalidate_range, current_el, enable_cpu_hardening,
    halt_forever, irq_disable, irq_enable, mmio_read32, mmio_read8, mmio_write16, mmio_write32,
    mmio_write8, probe_security_features, reboot, sync_icache_range, wait_for_interrupt,
    CpuSecurityFeatures,
};
