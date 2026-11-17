// crates/services/nc-fs/src/lib.rs
#![no_std]

extern crate alloc;

pub mod erofs;
pub mod exfat;
pub mod fat32;
pub mod memfs;
pub mod vfs;

pub use erofs::Erofs;
pub use exfat::Exfat;
pub use fat32::Fat32;
pub use memfs::{MemFs, SharedMemFs};
pub use vfs::{DirEntry, FileStat, FileSystem, FileType, FsError};

#[cfg(feature = "storage")]
mod manager;
#[cfg(feature = "storage")]
pub use manager::*;
