#![cfg_attr(target_arch = "aarch64", no_std)]
#![cfg_attr(target_arch = "aarch64", no_main)]

#[cfg(target_arch = "aarch64")]
extern crate alloc;

#[cfg(target_arch = "aarch64")]
mod dtb;
#[cfg(target_arch = "aarch64")]
mod entry;
#[cfg(target_arch = "aarch64")]
mod home;
#[cfg(target_arch = "aarch64")]
mod blk;
#[cfg(target_arch = "aarch64")]
mod input;
#[cfg(target_arch = "aarch64")]
mod lp;
#[cfg(target_arch = "aarch64")]
mod storage;
#[cfg(target_arch = "aarch64")]
mod host;
#[cfg(target_arch = "aarch64")]
mod boot;
#[cfg(target_arch = "aarch64")]
mod selftest;
#[cfg(target_arch = "aarch64")]
mod syscall;
#[cfg(target_arch = "aarch64")]
mod task;
#[cfg(target_arch = "aarch64")]
mod vm;

#[cfg(not(target_arch = "aarch64"))]
fn main() {}
