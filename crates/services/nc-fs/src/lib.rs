// crates/services/nc-fs/src/lib.rs
#![no_std]

extern crate alloc;

pub mod ab_slot;
pub mod erofs;
pub mod exfat;
pub mod fat32;
pub mod fde_blk;
pub mod journal;
pub mod lp;
pub mod memfs;
pub mod quota;
pub mod vfs;

pub use erofs::Erofs;
pub use exfat::Exfat;
pub use fat32::Fat32;
pub use memfs::{MemFs, SharedMemFs};
pub use vfs::{DirEntry, FileStat, FileSystem, FileType, FsError};

mod manager;
pub use manager::*;
