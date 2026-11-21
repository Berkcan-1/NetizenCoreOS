#![no_std]

#[cfg(all(feature = "kernel", feature = "client"))]
compile_error!("nc-net-facade: `kernel` ve `client` ozellikleri birlikte acilamaz");

#[cfg(feature = "kernel")]
pub use nc_net::{global, HttpState, NetworkStack, TcpHttpClient};

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "client")]
pub use client::*;
