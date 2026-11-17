//! Ring 3 VFS istemcisi: `nc_fs::VfsManager`'ın uygulamaların kullandığı yüzeyini `SYS_VFS_CALL` ile sağlar.
//! Rol/yetki denetimi ÇEKİRDEKTEDİR (süreç kimliğinden); `set_vfs_caller` burada işlevsizdir.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use nc_abi::vfs as wire;

pub const CALLER_KERNEL: u8 = 0;
pub const CALLER_SETTINGS: u8 = 1;
pub const CALLER_FILES: u8 = 2;
pub const CALLER_USER_APP: u8 = 3;

pub fn set_vfs_caller(_role: u8) {}

pub fn get_vfs_caller() -> u8 {
    CALLER_FILES
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug)]
pub struct FileStat {
    pub size: u64,
    pub is_dir: bool,
    pub readonly: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

fn error_from_status(status: i32) -> FsError {
    match status {
        wire::ERR_INVALID_SUPERBLOCK => FsError::InvalidSuperblock,
        wire::ERR_NOT_FOUND => FsError::NotFound,
        wire::ERR_NOT_A_DIRECTORY => FsError::NotADirectory,
        wire::ERR_NOT_A_FILE => FsError::NotAFile,
        wire::ERR_CORRUPTED => FsError::Corrupted,
        wire::ERR_ALREADY_EXISTS => FsError::AlreadyExists,
        wire::ERR_READ_ONLY => FsError::ReadOnly,
        wire::ERR_PERMISSION_DENIED => FsError::PermissionDenied,
        wire::ERR_INVALID_PATH => FsError::InvalidPath,
        wire::ERR_STORAGE_FULL => FsError::StorageFull,
        _ => FsError::DiskError,
    }
}

type Transport = fn(&[u8], &mut [u8]) -> i64;

fn sys_transport(request: &[u8], response: &mut [u8]) -> i64 {
    nc_app_sdk::sys::vfs_call(request, response)
}

static mut TRANSPORT: Transport = sys_transport;

/// Test için: çekirdeği taklit eden bir taşıma kurar.
pub fn set_transport(transport: Transport) {
    unsafe {
        *core::ptr::addr_of_mut!(TRANSPORT) = transport;
    }
}

const LIST_CAP: usize = 128 * 1024;

fn call(op: u8, arg0: u64, arg1: u64, path: &str, data: &[u8], response_cap: usize) -> Result<Vec<u8>, FsError> {
    // vec![0; n]: sayfalar dokunulur (çekirdek, dokunulmamış talep-sayfasına yazamaz).
    let mut request = vec![0u8; wire::REQ_HEADER + path.len() + data.len()];
    let used = wire::encode_request(&mut request, op, arg0, arg1, path, data).ok_or(FsError::InvalidPath)?;
    request.truncate(used);
    let mut response = vec![0u8; response_cap.max(wire::RESP_HEADER)];
    let transport = unsafe { *core::ptr::addr_of!(TRANSPORT) };
    let n = transport(&request, &mut response);
    if n < 0 {
        return Err(if n == nc_abi::EACCES { FsError::PermissionDenied } else { FsError::DiskError });
    }
    let n = n as usize;
    if n > response.len() {
        return Err(FsError::Corrupted);
    }
    let (status, payload) = wire::parse_response(&response[..n]).map_err(|_| FsError::Corrupted)?;
    if status != 0 {
        return Err(error_from_status(status));
    }
    Ok(payload.to_vec())
}

pub struct VfsManager {
    usb_online: bool,
    usb_label: String,
}

impl Default for VfsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl VfsManager {
    pub fn new() -> VfsManager {
        let mut manager = VfsManager { usb_online: false, usb_label: String::new() };
        manager.refresh_usb();
        manager
    }

    fn refresh_usb(&mut self) {
        self.usb_online = matches!(call(wire::OP_USB_ONLINE, 0, 0, "", &[], 64), Ok(p) if p.first() == Some(&1));
        self.usb_label = match call(wire::OP_USB_LABEL, 0, 0, "", &[], 256) {
            Ok(bytes) => String::from_utf8(bytes).unwrap_or_default(),
            Err(_) => String::new(),
        };
    }

    pub fn rescan_usb(&mut self) -> bool {
        let found = matches!(call(wire::OP_USB_RESCAN, 0, 0, "", &[], 64), Ok(p) if p.first() == Some(&1));
        self.refresh_usb();
        found
    }

    pub fn is_usb_online(&self) -> bool {
        self.usb_online
    }

    pub fn usb_label(&self) -> &str {
        &self.usb_label
    }

    pub fn list_dir_path(&mut self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let payload = call(wire::OP_LIST, 0, 0, path, &[], LIST_CAP)?;
        let mut entries = Vec::new();
        for item in wire::DirEntries::new(&payload).map_err(|_| FsError::Corrupted)? {
            let item = item.map_err(|_| FsError::Corrupted)?;
            entries.push(DirEntry {
                name: String::from(item.name),
                file_type: match item.kind {
                    wire::KIND_FILE => FileType::RegularFile,
                    wire::KIND_DIR => FileType::Directory,
                    _ => FileType::Unknown,
                },
                size: item.size,
                id: item.id,
            });
        }
        Ok(entries)
    }

    pub fn file_stat_path(&mut self, path: &str) -> Result<FileStat, FsError> {
        let p = call(wire::OP_STAT, 0, 0, path, &[], 64)?;
        if p.len() < 10 {
            return Err(FsError::Corrupted);
        }
        Ok(FileStat {
            size: u64::from_le_bytes([p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7]]),
            is_dir: p[8] != 0,
            readonly: p[9] != 0,
        })
    }

    pub fn read_file_range_path(&mut self, path: &str, offset: u64, output: &mut [u8]) -> Result<usize, FsError> {
        let mut done = 0usize;
        while done < output.len() {
            let want = (output.len() - done).min(wire::MAX_CHUNK);
            let chunk = call(wire::OP_READ_RANGE, offset + done as u64, want as u64, path, &[], wire::RESP_HEADER + want)?;
            if chunk.len() > want {
                return Err(FsError::Corrupted);
            }
            output[done..done + chunk.len()].copy_from_slice(&chunk);
            done += chunk.len();
            if chunk.len() < want {
                break;
            }
        }
        Ok(done)
    }

    pub fn read_file_path(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        let stat = self.file_stat_path(path)?;
        if stat.is_dir {
            return Err(FsError::NotAFile);
        }
        let size = usize::try_from(stat.size).map_err(|_| FsError::StorageFull)?;
        if size as u64 > wire::MAX_WRITE_TOTAL * 4 {
            return Err(FsError::StorageFull);
        }
        let mut bytes = vec![0u8; size];
        let read = self.read_file_range_path(path, 0, &mut bytes)?;
        bytes.truncate(read);
        Ok(bytes)
    }

    pub fn write_file_path(&mut self, path: &str, content: &[u8]) -> Result<(), FsError> {
        if content.len() as u64 > wire::MAX_WRITE_TOTAL {
            return Err(FsError::StorageFull);
        }
        let total = content.len() as u64;
        if content.is_empty() {
            call(wire::OP_WRITE, 0, 0, path, &[], 64)?;
            return Ok(());
        }
        let mut offset = 0usize;
        while offset < content.len() {
            let end = (offset + wire::MAX_CHUNK).min(content.len());
            call(wire::OP_WRITE, offset as u64, total, path, &content[offset..end], 64)?;
            offset = end;
        }
        Ok(())
    }

    pub fn create_dir_path(&mut self, path: &str) -> Result<(), FsError> {
        call(wire::OP_MKDIR, 0, 0, path, &[], 64).map(|_| ())
    }

    pub fn remove_path(&mut self, path: &str) -> Result<(), FsError> {
        call(wire::OP_REMOVE, 0, 0, path, &[], 64).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::collections::BTreeMap;
    use std::string::ToString;
    use std::sync::Mutex;

    // Çekirdeğin VFS hizmetini taklit eder (aynı tel biçimi, ayrı bellekte).
    static FAKE: Mutex<Option<BTreeMap<String, Vec<u8>>>> = Mutex::new(None);
    static STAGE: Mutex<Option<(String, Vec<u8>, u64)>> = Mutex::new(None);
    static GUARD: Mutex<()> = Mutex::new(());

    fn kernel(request: &[u8], response: &mut [u8]) -> i64 {
        let req = match wire::parse_request(request) {
            Ok(r) => r,
            Err(_) => return nc_abi::EINVAL,
        };
        let mut files = FAKE.lock().unwrap();
        let fs = files.get_or_insert_with(BTreeMap::new);
        let mut payload: Vec<u8> = Vec::new();
        let mut status = 0;
        match req.op {
            wire::OP_STAT => match fs.get(req.path) {
                Some(data) => {
                    payload.extend_from_slice(&(data.len() as u64).to_le_bytes());
                    payload.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
                }
                None if req.path == "/dir" => {
                    payload.extend_from_slice(&0u64.to_le_bytes());
                    payload.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
                }
                None => status = wire::ERR_NOT_FOUND,
            },
            wire::OP_READ_RANGE => match fs.get(req.path) {
                Some(data) => {
                    let start = (req.arg0 as usize).min(data.len());
                    let end = (start + req.arg1 as usize).min(data.len());
                    payload.extend_from_slice(&data[start..end]);
                }
                None => status = wire::ERR_NOT_FOUND,
            },
            wire::OP_WRITE => {
                let mut stage = STAGE.lock().unwrap();
                if req.arg0 == 0 {
                    *stage = Some((req.path.to_string(), Vec::new(), req.arg1));
                }
                let (path, buf, total) = stage.as_mut().unwrap();
                assert_eq!(buf.len() as u64, req.arg0, "parcalar sirayla gelmeli");
                assert_eq!(*total, req.arg1);
                assert!(req.data.len() <= wire::MAX_CHUNK);
                buf.extend_from_slice(req.data);
                if buf.len() as u64 == *total {
                    let done = (path.clone(), buf.clone());
                    *stage = None;
                    fs.insert(done.0, done.1);
                }
            }
            wire::OP_LIST => {
                payload.extend_from_slice(&[0; 8]);
                let mut count = 0u32;
                let mut scratch = [0u8; 300];
                for (name, data) in fs.iter() {
                    if let Some(short) = name.strip_prefix(req.path) {
                        let n = wire::encode_dir_entry(&mut scratch, wire::KIND_FILE, data.len() as u64, count as u64, short.trim_start_matches('/')).unwrap();
                        payload.extend_from_slice(&scratch[..n]);
                        count += 1;
                    }
                }
                payload[0..4].copy_from_slice(&count.to_le_bytes());
            }
            wire::OP_MKDIR => {
                if req.path.contains("denied") {
                    status = wire::ERR_PERMISSION_DENIED;
                }
            }
            wire::OP_REMOVE => {
                if fs.remove(req.path).is_none() {
                    status = wire::ERR_NOT_FOUND;
                }
            }
            wire::OP_USB_ONLINE => payload.push(1),
            wire::OP_USB_RESCAN => payload.push(1),
            wire::OP_USB_LABEL => payload.extend_from_slice(b"Test USB"),
            _ => status = wire::ERR_INVALID_PATH,
        }
        if wire::RESP_HEADER + payload.len() > response.len() {
            return nc_abi::EFBIG;
        }
        wire::encode_response_header(response, status, payload.len()).unwrap();
        response[wire::RESP_HEADER..wire::RESP_HEADER + payload.len()].copy_from_slice(&payload);
        (wire::RESP_HEADER + payload.len()) as i64
    }

    fn fresh() -> (std::sync::MutexGuard<'static, ()>, VfsManager) {
        let guard = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        *FAKE.lock().unwrap() = Some(BTreeMap::new());
        *STAGE.lock().unwrap() = None;
        set_transport(kernel);
        (guard, VfsManager::new())
    }

    #[test]
    fn small_and_empty_files_roundtrip() {
        let (_g, mut vfs) = fresh();
        vfs.write_file_path("/data/documents/a.txt", b"merhaba").unwrap();
        assert_eq!(vfs.read_file_path("/data/documents/a.txt").unwrap(), b"merhaba");
        vfs.write_file_path("/data/documents/bos.txt", b"").unwrap();
        assert_eq!(vfs.read_file_path("/data/documents/bos.txt").unwrap(), b"");
        assert_eq!(vfs.file_stat_path("/data/documents/a.txt").unwrap().size, 7);
    }

    #[test]
    fn large_files_are_chunked_both_ways_without_corruption() {
        let (_g, mut vfs) = fresh();
        let data: Vec<u8> = (0..(wire::MAX_CHUNK * 2 + 12345)).map(|i| (i * 31 % 251) as u8).collect();
        vfs.write_file_path("/data/documents/buyuk.bin", &data).unwrap();
        assert_eq!(vfs.read_file_path("/data/documents/buyuk.bin").unwrap(), data);
        let mut part = vec![0u8; 100_000];
        let n = vfs.read_file_range_path("/data/documents/buyuk.bin", 70_000, &mut part).unwrap();
        assert_eq!(&part[..n], &data[70_000..70_000 + n]);
        let mut tail = vec![0u8; 1000];
        let n = vfs.read_file_range_path("/data/documents/buyuk.bin", data.len() as u64 - 10, &mut tail).unwrap();
        assert_eq!(n, 10, "dosya sonunda kisa okuma");
    }

    #[test]
    fn errors_map_to_fs_error_variants() {
        let (_g, mut vfs) = fresh();
        assert_eq!(vfs.read_file_path("/yok").err(), Some(FsError::NotFound));
        assert_eq!(vfs.read_file_path("/dir").err(), Some(FsError::NotAFile));
        assert_eq!(vfs.create_dir_path("/data/denied").err(), Some(FsError::PermissionDenied));
        assert_eq!(vfs.remove_path("/yok").err(), Some(FsError::NotFound));
    }

    #[test]
    fn listing_usb_and_removal() {
        let (_g, mut vfs) = fresh();
        vfs.write_file_path("/data/documents/a.txt", b"1").unwrap();
        vfs.write_file_path("/data/documents/b.txt", b"22").unwrap();
        let entries = vfs.list_dir_path("/data/documents").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].name, "b.txt");
        assert_eq!(entries[1].size, 2);
        assert_eq!(entries[0].file_type, FileType::RegularFile);
        assert!(vfs.is_usb_online() && vfs.usb_label() == "Test USB");
        assert!(vfs.rescan_usb());
        vfs.remove_path("/data/documents/a.txt").unwrap();
        assert_eq!(vfs.list_dir_path("/data/documents").unwrap().len(), 1);
    }

    #[test]
    fn transport_failures_are_errors_not_panics() {
        let _g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        fn denied(_: &[u8], _: &mut [u8]) -> i64 {
            nc_abi::EACCES
        }
        fn garbage(_: &[u8], r: &mut [u8]) -> i64 {
            r[..3].copy_from_slice(&[1, 2, 3]);
            3
        }
        fn oversize(_: &[u8], r: &mut [u8]) -> i64 {
            (r.len() + 1) as i64
        }
        set_transport(denied);
        let mut vfs = VfsManager { usb_online: false, usb_label: String::new() };
        assert_eq!(vfs.file_stat_path("/x").err(), Some(FsError::PermissionDenied));
        set_transport(garbage);
        assert_eq!(vfs.file_stat_path("/x").err(), Some(FsError::Corrupted));
        set_transport(oversize);
        assert_eq!(vfs.file_stat_path("/x").err(), Some(FsError::Corrupted));
        set_transport(kernel);
    }
}
