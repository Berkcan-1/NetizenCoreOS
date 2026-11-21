#![no_std]

extern crate alloc;

#[cfg(all(feature = "kernel", feature = "client"))]
compile_error!("nc-sys-facade: `kernel` ve `client` ozellikleri birlikte acilamaz");

mod mirror;
mod proto;
#[cfg(feature = "kernel")]
pub mod service;

pub use mirror::*;
