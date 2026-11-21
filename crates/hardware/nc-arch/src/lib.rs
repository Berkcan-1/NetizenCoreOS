#![no_std]

#[cfg(target_arch = "x86_64")]
pub use nc_arch_x86_64::*;

#[cfg(target_arch = "aarch64")]
pub use nc_arch_aarch64::*;
