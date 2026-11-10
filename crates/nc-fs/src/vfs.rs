extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileType {
    RegularFile,
    Directory,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub file_type: FileType,
    pub size: u64,
    pub id: u64,
}

#[derive(Clone, Debug)]
pub struct FileStat {
    pub size: u64,
    pub is_dir: bool,
    pub readonly: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FsError {
    DiskError,
    InvalidSuperblock,
    NotFound,
    NotADirectory,
    NotAFile,
    Corrupted,
    AlreadyExists,
    ReadOnly,
    PermissionDenied,
    InvalidPath,
    StorageFull,
}

pub trait FileSystem {
    fn list_dir(&mut self, path: &str) -> Result<Vec<DirEntry>, FsError>;
    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FsError>;

    /// Read a bounded range without requiring callers to allocate the whole file.
    /// Filesystems should override this method with direct block/extent reads.
    /// The default is intentionally compatible with older filesystem drivers.
    fn read_file_range(
        &mut self,
        path: &str,
        offset: u64,
        output: &mut [u8],
    ) -> Result<usize, FsError> {
        if output.is_empty() {
            return Ok(0);
        }
        let data = self.read_file(path)?;
        let start = usize::try_from(offset).map_err(|_| FsError::Corrupted)?;
        if start >= data.len() {
            return Ok(0);
        }
        let count = output.len().min(data.len() - start);
        output[..count].copy_from_slice(&data[start..start + count]);
        Ok(count)
    }

    fn file_size(&mut self, path: &str) -> Result<u64, FsError> {
        self.read_file(path).map(|v| v.len() as u64)
    }
    fn write_file(&mut self, _path: &str, _content: &[u8]) -> Result<(), FsError> {
        Err(FsError::ReadOnly)
    }
    fn create_dir(&mut self, _path: &str) -> Result<(), FsError> {
        Err(FsError::ReadOnly)
    }
    fn remove(&mut self, _path: &str) -> Result<(), FsError> {
        Err(FsError::ReadOnly)
    }
    fn file_stat(&mut self, path: &str) -> Result<FileStat, FsError> {
        let is_dir = self.list_dir(path).is_ok();
        if is_dir {
            Ok(FileStat { size: 0, is_dir: true, readonly: false })
        } else {
            let bytes = self.read_file(path)?;
            Ok(FileStat { size: bytes.len() as u64, is_dir: false, readonly: false })
        }
    }
}

