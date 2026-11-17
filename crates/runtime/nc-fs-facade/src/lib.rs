//! `nc_fs` cephesi: uygulamalar `nc_fs::VfsManager` ve eşlerini bu crate üzerinden kullanır.
//! `kernel` özelliği gerçek `nc-fs`'i, `client` özelliği Ring 3 VFS hizmeti istemcisini dışa aktarır.
#![no_std]

#[cfg(all(feature = "kernel", feature = "client"))]
compile_error!("nc-fs-facade: `kernel` ve `client` özellikleri birlikte açılamaz");

#[cfg(feature = "kernel")]
pub use nc_fs::*;

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "client")]
pub use client::*;
