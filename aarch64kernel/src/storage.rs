use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use core::ptr::addr_of_mut;
use nc_abi::vfs as wire;
use nc_fs::{DirEntry, Exfat, Fat32, FileStat, FileSystem, FileType, FsError};
use nc_fs_policy::{Denied, Op};
use nc_hal::BlockDevice;
use nc_pkg::{InstallSource, NtzcPackage, Tier};

use crate::{blk, lp};

const PARTITION_NAMES: [&str; 2] = ["system", "system_a"];
const MAX_TEXT: usize = 512;

struct Serial;

impl Write for Serial {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        nc_arch::serial::write_str(text);
        Ok(())
    }
}

macro_rules! log {
    ($($argument:tt)*) => {{
        let _ = writeln!(Serial, $($argument)*);
    }};
}

pub struct RegisteredApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub tier: Tier,
    pub caps: u32,
    pub payload: Vec<u8>,
}

static mut SYSTEM_FS: Option<Fat32> = None;
static mut SYSTEM_SECTORS: u64 = 0;
static mut DATA_FS: Option<Exfat> = None;
static mut DATA_DISK: usize = usize::MAX;
static mut DATA_SECTORS: u64 = 0;
static mut DATA_READ_ONLY: bool = true;
static mut PACKAGES: Vec<RegisteredApp> = Vec::new();
static mut WRITE_STAGE: Option<(String, Vec<u8>, u64)> = None;
static mut CACHED_MEDIA_COUNT: usize = 0;

pub fn cached_media_count() -> usize {
    unsafe { *addr_of_mut!(CACHED_MEDIA_COUNT) }
}

pub fn system_fs() -> Option<&'static mut Fat32> {
    unsafe { (*addr_of_mut!(SYSTEM_FS)).as_mut() }
}

pub fn system_sectors() -> u64 {
    unsafe { *addr_of_mut!(SYSTEM_SECTORS) }
}

pub fn data_fs() -> Option<&'static mut Exfat> {
    unsafe { (*addr_of_mut!(DATA_FS)).as_mut() }
}

pub fn data_sectors() -> u64 {
    unsafe { *addr_of_mut!(DATA_SECTORS) }
}

pub fn data_writable() -> bool {
    unsafe { (*addr_of_mut!(DATA_FS)).is_some() && !*addr_of_mut!(DATA_READ_ONLY) }
}

pub fn sync_data() -> bool {
    let index = unsafe { *addr_of_mut!(DATA_DISK) };
    match blk::open(index) {
        Some(disk) => disk.flush(),
        None => false,
    }
}

fn builtin_system_caps(app_id: &str) -> u32 {
    match app_id {
        "nc.files" => {
            nc_caps::FS_ALL_READ
                | nc_caps::FS_ALL_WRITE
                | nc_caps::FS_USER_READ
                | nc_caps::FS_USER_WRITE
                | nc_caps::FS_USER_DELETE
                | nc_caps::PACKAGE_INSTALL
                | nc_caps::PROCESS_LAUNCH
        }
        "nc.settings" => {
            nc_caps::SETTINGS_READ
                | nc_caps::SETTINGS_WRITE_DISPLAY
                | nc_caps::SETTINGS_WRITE_LOCALE
                | nc_caps::FS_ALL_READ
                | nc_caps::PROCESS_LAUNCH
        }
        _ => nc_caps::USER_ALLOWED,
    }
}

fn upsert_package(app: RegisteredApp) {
    let list = unsafe { &mut *addr_of_mut!(PACKAGES) };
    if let Some(existing) = list.iter_mut().find(|item| item.id == app.id) {
        *existing = app;
    } else {
        list.push(app);
    }
}

fn register_ntzc_bytes(bytes: &[u8], source: InstallSource) -> Result<String, &'static str> {
    let pkg = NtzcPackage::parse_checked(bytes).map_err(|e| e.as_str())?;
    if !pkg.runnable_free() {
        return Err("paket bu mimaride calistirilamaz (ELF/ABI)");
    }
    let id = pkg.manifest.id.clone();
    let is_system = source == InstallSource::SystemImage
        && (pkg.tier(source) == Tier::System || NtzcPackage::is_trusted_builtin(&id));
    let tier = if is_system { Tier::System } else { Tier::User };
    let declared = pkg.capability_bits();
    let caps = if is_system {
        if declared != 0 {
            declared | builtin_system_caps(&id)
        } else {
            builtin_system_caps(&id)
        }
    } else {
        nc_caps::clamp_user(declared)
    };
    upsert_package(RegisteredApp {
        id: id.clone(),
        name: pkg.manifest.name.clone(),
        version: pkg.manifest.version.clone(),
        tier,
        caps,
        payload: pkg.payload,
    });
    Ok(id)
}

pub fn find_package(app_id: &str) -> Option<&'static RegisteredApp> {
    let list = unsafe { &*addr_of_mut!(PACKAGES) };
    list.iter().find(|item| item.id == app_id)
}

pub fn packages() -> &'static [RegisteredApp] {
    unsafe { &*addr_of_mut!(PACKAGES) }
}

pub fn mount_system() -> bool {
    if blk::count() == 0 {
        log!("[depo] blok aygiti yok; sistem bolumu baglanamadi");
        return false;
    }
    let mut selected: Option<(usize, &str)> = None;
    let mut last_error = lp::LpError::NotFound;
    'disks: for index in 0..blk::count() {
        let disk = match blk::open(index) {
            Some(disk) => disk,
            None => continue,
        };
        let mut probe: Box<dyn nc_hal::BlockDevice> = Box::new(disk);
        for name in PARTITION_NAMES {
            match lp::find_extents(probe.as_mut(), name) {
                Ok(_) => {
                    selected = Some((index, name));
                    break 'disks;
                }
                Err(error) => last_error = error,
            }
        }
    }
    let (disk_index, name) = match selected {
        Some(found) => found,
        None => {
            log!("[depo] super.img icinde sistem bolumu bulunamadi: {:?}", last_error);
            return false;
        }
    };
    let device: Box<dyn nc_hal::BlockDevice> = match blk::open(disk_index) {
        Some(disk) => Box::new(disk),
        None => return false,
    };
    let partition = match lp::open_partition(device, name) {
        Ok(partition) => partition,
        Err(error) => {
            log!("[depo] bolum '{}' acilamadi: {:?}", name, error);
            return false;
        }
    };
    let sectors = partition.sectors();
    log!("[depo] super.img: bolum '{}', {} uzanti, {} sektor", name, partition.extent_count(), sectors);
    let mut fs = match Fat32::new(Box::new(partition)) {
        Ok(fs) => fs,
        Err(error) => {
            log!("[depo] sistem bolumu FAT32 olarak baglanamadi: {:?}", error);
            return false;
        }
    };
    log!("[depo] sistem bolumu FAT32 baglandi (salt-okunur)");
    match fs.read_file("/etc/netizencore-release") {
        Ok(bytes) => {
            let shown = &bytes[..bytes.len().min(MAX_TEXT)];
            match core::str::from_utf8(shown) {
                Ok(text) => {
                    log!("[depo] /etc/netizencore-release ({} bayt):", bytes.len());
                    for line in text.lines() {
                        log!("[depo]   {}", line);
                    }
                }
                Err(_) => log!("[depo] /etc/netizencore-release metin degil ({} bayt)", bytes.len()),
            }
        }
        Err(error) => log!("[depo] /etc/netizencore-release okunamadi: {:?}", error),
    }
    match fs.list_dir("/packages") {
        Ok(entries) => {
            let mut count = 0usize;
            for entry in entries.iter() {
                let lower = entry.name.to_ascii_lowercase();
                if lower.ends_with(".ntzc") {
                    count += 1;
                    let mut full = String::from("/packages/");
                    full.push_str(&entry.name);
                    match fs.read_file(&full) {
                        Ok(bytes) => match register_ntzc_bytes(&bytes, InstallSource::SystemImage) {
                            Ok(id) => log!("[depo]   paket: {} ({} bayt, kimlik={})", entry.name, entry.size, id),
                            Err(reason) => log!("[depo]   paket: {} ({} bayt, reddedildi: {})", entry.name, entry.size, reason),
                        },
                        Err(_) => log!("[depo]   paket: {} ({} bayt)", entry.name, entry.size),
                    }
                }
            }
            log!("[depo] /packages: {} paket", count);
        }
        Err(error) => log!("[depo] /packages listelenemedi: {:?}", error),
    }
    unsafe {
        *addr_of_mut!(SYSTEM_SECTORS) = sectors;
        *addr_of_mut!(SYSTEM_FS) = Some(fs);
    }
    true
}

const DATA_DIRECTORIES: [&str; 5] = ["/app", "/appdata", "/appdata/nc.settings", "/appdata/nc.home", "/media"];
const PROBE_PATH: &str = "/.nc-rw-probe";
const PROBE_BYTES: usize = 9000;
const SETTINGS_CFG_PATH: &str = "/appdata/nc.settings/settings.cfg";

fn ensure_layout(fs: &mut Exfat) {
    for directory in DATA_DIRECTORIES {
        match fs.list_dir(directory) {
            Ok(_) => {}
            Err(FsError::NotFound) => match fs.create_dir(directory) {
                Ok(()) => log!("[depo] /data{} olusturuldu", directory),
                Err(error) => log!("[depo] /data{} olusturulamadi: {:?}", directory, error),
            },
            Err(error) => log!("[depo] /data{} okunamadi: {:?}", directory, error),
        }
    }
}

fn write_probe(fs: &mut Exfat) -> bool {
    let mut pattern: Vec<u8> = Vec::with_capacity(PROBE_BYTES);
    for index in 0..PROBE_BYTES as u32 {
        pattern.push(index.wrapping_mul(31) as u8 ^ (index >> 8) as u8);
    }
    if let Err(error) = fs.write_file(PROBE_PATH, &pattern) {
        log!("[depo] /data yazma sinamasi: yazilamadi: {:?}", error);
        return false;
    }
    let read_back = match fs.read_file(PROBE_PATH) {
        Ok(bytes) => bytes,
        Err(error) => {
            log!("[depo] /data yazma sinamasi: geri okunamadi: {:?}", error);
            return false;
        }
    };
    let same = read_back == pattern;
    if let Err(error) = fs.remove(PROBE_PATH) {
        log!("[depo] /data yazma sinamasi: silinemedi: {:?}", error);
        return false;
    }
    if !same {
        log!("[depo] /data yazma sinamasi: geri okunan veri farkli");
    }
    same
}

fn scan_user_packages(fs: &mut Exfat) {
    if let Ok(entries) = fs.list_dir("/app") {
        for entry in entries.iter() {
            if entry.name.to_ascii_lowercase().ends_with(".ntzc") {
                let mut full = String::from("/app/");
                full.push_str(&entry.name);
                if let Ok(bytes) = fs.read_file(&full) {
                    if let Ok(id) = register_ntzc_bytes(&bytes, InstallSource::UserData) {
                        log!("[depo] /data/app paketi yuklendi: {} (kimlik={})", entry.name, id);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct SavedSettings {
    pub accent: u32,
    pub wallpaper: u8,
    pub tz_hours: i8,
    pub lang: u32,
    pub wifi: bool,
    pub bt: bool,
    pub brightness: u8,
}

pub fn load_settings_cfg() -> Option<SavedSettings> {
    let fs = data_fs()?;
    let bytes = fs.read_file(SETTINGS_CFG_PATH).ok()?;
    let text = core::str::from_utf8(&bytes).ok()?;
    let mut cfg = SavedSettings {
        accent: 0x000A_59F7,
        wallpaper: 0,
        tz_hours: 3,
        lang: 0,
        wifi: true,
        bt: false,
        brightness: 100,
    };
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim();
        let v = v.trim();
        match k {
            "accent" => {
                if let Ok(val) = u32::from_str_radix(v.trim_start_matches("0x"), 16) {
                    cfg.accent = val & 0x00FF_FFFF;
                }
            }
            "wallpaper" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.wallpaper = val.min(3);
                }
            }
            "tz_hours" => {
                if let Ok(val) = v.parse::<i8>() {
                    cfg.tz_hours = val.clamp(-12, 14);
                }
            }
            "lang" => {
                if let Ok(val) = v.parse::<u32>() {
                    cfg.lang = val.min(1);
                }
            }
            "wifi" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.wifi = val != 0;
                }
            }
            "bt" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.bt = val != 0;
                }
            }
            "brightness" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.brightness = val.clamp(20, 100);
                }
            }
            _ => {}
        }
    }
    Some(cfg)
}

pub fn save_settings_cfg(cfg: SavedSettings) -> bool {
    let Some(fs) = data_fs() else { return false };
    let _ = fs.create_dir("/appdata");
    let _ = fs.create_dir("/appdata/nc.settings");
    let mut text = String::new();
    let _ = writeln!(text, "accent=0x{:06x}", cfg.accent & 0x00FF_FFFF);
    let _ = writeln!(text, "wallpaper={}", cfg.wallpaper.min(3));
    let _ = writeln!(text, "tz_hours={}", cfg.tz_hours.clamp(-12, 14));
    let _ = writeln!(text, "lang={}", cfg.lang.min(1));
    let _ = writeln!(text, "wifi={}", cfg.wifi as u8);
    let _ = writeln!(text, "bt={}", cfg.bt as u8);
    let _ = writeln!(text, "brightness={}", cfg.brightness.clamp(20, 100));
    if fs.write_file(SETTINGS_CFG_PATH, text.as_bytes()).is_err() {
        return false;
    }
    sync_data()
}

pub fn mount_data() -> bool {
    for index in 0..blk::count() {
        let mut probe = match blk::open(index) {
            Some(disk) => disk,
            None => continue,
        };
        let mut boot = [0u8; 512];
        if probe.read_blocks(0, 1, &mut boot).is_err() || &boot[3..11] != b"EXFAT   " {
            continue;
        }
        let read_only = probe.is_read_only();
        let sectors = probe.sectors();
        let mut fs = match Exfat::new(Box::new(probe)) {
            Ok(fs) => fs,
            Err(error) => {
                log!("[depo] aygit {} exFAT olarak baglanamadi: {:?}", index, error);
                return false;
            }
        };
        if read_only {
            log!("[depo] /data baglandi (SALT-OKUNUR aygit: paket kurma ve yazma calismaz)");
        } else {
            ensure_layout(&mut fs);
            let ok = write_probe(&mut fs);
            let flushed = match blk::open(index) {
                Some(disk) => disk.flush(),
                None => false,
            };
            log!(
                "[depo] /data baglandi (exFAT, yazilabilir): yazma sinamasi {}, flush {}",
                if ok { "tamam" } else { "BASARISIZ" },
                if flushed { "tamam" } else { "BASARISIZ" }
            );
            if !ok {
                return false;
            }
        }
        scan_user_packages(&mut fs);
        unsafe {
            *addr_of_mut!(DATA_DISK) = index;
            *addr_of_mut!(DATA_SECTORS) = sectors;
            *addr_of_mut!(DATA_READ_ONLY) = read_only;
            *addr_of_mut!(DATA_FS) = Some(fs);
            *addr_of_mut!(CACHED_MEDIA_COUNT) = dir_entry_count("/data/media");
        }
        if let Some(cfg) = load_settings_cfg() {
            crate::host::apply_saved_settings(cfg);
            log!(
                "[depo] kalici ayarlar yuklendi (/data{}): accent={:#08x}, tema={}, saat=UTC{:+}, dil={}, wifi={}, bt={}, parlaklik={}%",
                SETTINGS_CFG_PATH,
                cfg.accent,
                cfg.wallpaper,
                cfg.tz_hours,
                cfg.lang,
                cfg.wifi,
                cfg.bt,
                cfg.brightness
            );
        }
        return true;
    }
    log!("[depo] /data icin exFAT aygiti bulunamadi (userdata.img yok)");
    false
}

fn map_fs_error(error: FsError) -> i32 {
    match error {
        FsError::DiskError => wire::ERR_DISK,
        FsError::InvalidSuperblock => wire::ERR_INVALID_SUPERBLOCK,
        FsError::NotFound => wire::ERR_NOT_FOUND,
        FsError::NotADirectory => wire::ERR_NOT_A_DIRECTORY,
        FsError::NotAFile => wire::ERR_NOT_A_FILE,
        FsError::Corrupted => wire::ERR_CORRUPTED,
        FsError::AlreadyExists => wire::ERR_ALREADY_EXISTS,
        FsError::ReadOnly => wire::ERR_READ_ONLY,
        FsError::PermissionDenied => wire::ERR_PERMISSION_DENIED,
        FsError::InvalidPath => wire::ERR_INVALID_PATH,
        FsError::StorageFull => wire::ERR_STORAGE_FULL,
    }
}

fn map_denied(denied: Denied) -> i32 {
    match denied {
        Denied::BadPath => wire::ERR_INVALID_PATH,
        Denied::ReadOnly => wire::ERR_READ_ONLY,
        Denied::Permission => wire::ERR_PERMISSION_DENIED,
    }
}

enum TargetFs<'a> {
    Root,
    System(&'a str),
    Data(&'a str),
}

fn resolve_target(path: &str) -> Result<TargetFs<'_>, i32> {
    if path == "/" {
        return Ok(TargetFs::Root);
    }
    if path == "/system" {
        return Ok(TargetFs::System("/"));
    }
    if let Some(rest) = path.strip_prefix("/system/") {
        if rest.is_empty() {
            return Ok(TargetFs::System("/"));
        }
        return Ok(TargetFs::System(&path["/system".len()..]));
    }
    if path == "/data" {
        return Ok(TargetFs::Data("/"));
    }
    if let Some(rest) = path.strip_prefix("/data/") {
        if rest.is_empty() {
            return Ok(TargetFs::Data("/"));
        }
        return Ok(TargetFs::Data(&path["/data".len()..]));
    }
    Err(wire::ERR_NOT_FOUND)
}

fn encode_entries(entries: &[DirEntry], payload: &mut Vec<u8>) {
    payload.extend_from_slice(&[0u8; 8]);
    let mut count = 0u32;
    let mut scratch = [0u8; 320];
    for entry in entries.iter().take(wire::MAX_LIST_ENTRIES) {
        let kind = match entry.file_type {
            FileType::RegularFile => wire::KIND_FILE,
            FileType::Directory => wire::KIND_DIR,
            FileType::Unknown => wire::KIND_UNKNOWN,
        };
        if let Some(written) = wire::encode_dir_entry(&mut scratch, kind, entry.size, entry.id, &entry.name) {
            payload.extend_from_slice(&scratch[..written]);
            count += 1;
        }
    }
    payload[0..4].copy_from_slice(&count.to_le_bytes());
}

fn encode_stat(stat: FileStat, force_ro: bool, payload: &mut Vec<u8>) {
    payload.extend_from_slice(&stat.size.to_le_bytes());
    payload.push(stat.is_dir as u8);
    payload.push((stat.readonly || force_ro) as u8);
    payload.extend_from_slice(&[0u8; 6]);
}

pub fn vfs_call(app_id: &str, caps: u32, req_bytes: &[u8], resp_buf: &mut [u8]) -> i64 {
    let req = match wire::parse_request(req_bytes) {
        Ok(req) => req,
        Err(_) => return nc_abi::EINVAL,
    };
    let mut payload: Vec<u8> = Vec::new();
    let mut status: i32 = 0;

    match req.op {
        wire::OP_USB_ONLINE | wire::OP_USB_RESCAN => {
            payload.push(0);
        }
        wire::OP_USB_LABEL => {}
        wire::OP_LIST | wire::OP_STAT | wire::OP_READ_RANGE | wire::OP_WRITE | wire::OP_MKDIR | wire::OP_REMOVE => {
            let policy_op = match req.op {
                wire::OP_LIST | wire::OP_STAT | wire::OP_READ_RANGE => Op::Read,
                wire::OP_WRITE | wire::OP_MKDIR => Op::Write,
                wire::OP_REMOVE => Op::Delete,
                _ => Op::Read,
            };
            if let Err(denied) = nc_fs_policy::check(app_id, caps, req.path, policy_op) {
                status = map_denied(denied);
            } else {
                match resolve_target(req.path) {
                    Err(code) => status = code,
                    Ok(TargetFs::Root) => match req.op {
                        wire::OP_LIST => {
                            let mut root_entries = Vec::new();
                            if data_fs().is_some() {
                                root_entries.push(DirEntry {
                                    name: String::from("data"),
                                    file_type: FileType::Directory,
                                    size: data_sectors() * 512,
                                    id: 1,
                                });
                            }
                            if system_fs().is_some() {
                                root_entries.push(DirEntry {
                                    name: String::from("system"),
                                    file_type: FileType::Directory,
                                    size: system_sectors() * 512,
                                    id: 2,
                                });
                            }
                            encode_entries(&root_entries, &mut payload);
                        }
                        wire::OP_STAT => encode_stat(FileStat { size: 0, is_dir: true, readonly: true }, true, &mut payload),
                        _ => status = wire::ERR_READ_ONLY,
                    },
                    Ok(TargetFs::System(subpath)) => match system_fs() {
                        None => status = wire::ERR_NOT_FOUND,
                        Some(fs) => match req.op {
                            wire::OP_LIST => match fs.list_dir(subpath) {
                                Ok(entries) => encode_entries(&entries, &mut payload),
                                Err(err) => status = map_fs_error(err),
                            },
                            wire::OP_STAT => match fs.file_stat(subpath) {
                                Ok(stat) => encode_stat(stat, true, &mut payload),
                                Err(err) => status = map_fs_error(err),
                            },
                            wire::OP_READ_RANGE => {
                                let want = (req.arg1 as usize).min(wire::MAX_CHUNK);
                                payload.resize(want, 0);
                                match fs.read_file_range(subpath, req.arg0, &mut payload) {
                                    Ok(read) => payload.truncate(read),
                                    Err(err) => {
                                        payload.clear();
                                        status = map_fs_error(err);
                                    }
                                }
                            }
                            _ => status = wire::ERR_READ_ONLY,
                        },
                    },
                    Ok(TargetFs::Data(subpath)) => match data_fs() {
                        None => status = wire::ERR_NOT_FOUND,
                        Some(fs) => match req.op {
                            wire::OP_LIST => match fs.list_dir(subpath) {
                                Ok(entries) => encode_entries(&entries, &mut payload),
                                Err(err) => status = map_fs_error(err),
                            },
                            wire::OP_STAT => match fs.file_stat(subpath) {
                                Ok(stat) => encode_stat(stat, !data_writable(), &mut payload),
                                Err(err) => status = map_fs_error(err),
                            },
                            wire::OP_READ_RANGE => {
                                let want = (req.arg1 as usize).min(wire::MAX_CHUNK);
                                payload.resize(want, 0);
                                match fs.read_file_range(subpath, req.arg0, &mut payload) {
                                    Ok(read) => payload.truncate(read),
                                    Err(err) => {
                                        payload.clear();
                                        status = map_fs_error(err);
                                    }
                                }
                            }
                            wire::OP_WRITE => {
                                if !data_writable() {
                                    status = wire::ERR_READ_ONLY;
                                } else if req.arg1 > wire::MAX_WRITE_TOTAL {
                                    status = wire::ERR_STORAGE_FULL;
                                } else if req.arg0 == 0 && req.data.len() as u64 == req.arg1 {
                                    match fs.write_file(subpath, req.data) {
                                        Ok(()) => {
                                            sync_data();
                                        }
                                        Err(err) => status = map_fs_error(err),
                                    }
                                } else {
                                    let stage = unsafe { &mut *addr_of_mut!(WRITE_STAGE) };
                                    if req.arg0 == 0 {
                                        *stage = Some((String::from(subpath), Vec::new(), req.arg1));
                                    }
                                    match stage {
                                        Some((staged_path, buffer, total))
                                            if staged_path == subpath
                                                && buffer.len() as u64 == req.arg0
                                                && *total == req.arg1
                                                && buffer.len() + req.data.len() <= *total as usize =>
                                        {
                                            buffer.extend_from_slice(req.data);
                                            if buffer.len() as u64 == *total {
                                                let result = fs.write_file(subpath, buffer);
                                                *stage = None;
                                                match result {
                                                    Ok(()) => {
                                                        sync_data();
                                                    }
                                                    Err(err) => status = map_fs_error(err),
                                                }
                                            }
                                        }
                                        _ => {
                                            *stage = None;
                                            status = wire::ERR_INVALID_PATH;
                                        }
                                    }
                                }
                            }
                            wire::OP_MKDIR => {
                                if !data_writable() {
                                    status = wire::ERR_READ_ONLY;
                                } else {
                                    match fs.create_dir(subpath) {
                                        Ok(()) => {
                                            sync_data();
                                        }
                                        Err(err) => status = map_fs_error(err),
                                    }
                                }
                            }
                            wire::OP_REMOVE => {
                                if !data_writable() {
                                    status = wire::ERR_READ_ONLY;
                                } else {
                                    match fs.remove(subpath) {
                                        Ok(()) => {
                                            sync_data();
                                        }
                                        Err(err) => status = map_fs_error(err),
                                    }
                                }
                            }
                            _ => status = wire::ERR_INVALID_PATH,
                        },
                    },
                }
            }
        }
        _ => status = wire::ERR_INVALID_PATH,
    }

    let total = wire::RESP_HEADER + payload.len();
    if total > resp_buf.len() {
        return nc_abi::EFBIG;
    }
    if wire::encode_response_header(resp_buf, status, payload.len()).is_none() {
        return nc_abi::EINVAL;
    }
    resp_buf[wire::RESP_HEADER..total].copy_from_slice(&payload);
    total as i64
}

pub fn install_package_from_path(caps: u32, path: &str) -> i64 {
    if caps & nc_caps::PACKAGE_INSTALL == 0 {
        return nc_abi::EACCES;
    }
    let (bytes, from_system) = match resolve_target(path) {
        Ok(TargetFs::System(sub)) => match system_fs().and_then(|fs| fs.read_file(sub).ok()) {
            Some(bytes) => (bytes, true),
            None => return nc_abi::ENOENT,
        },
        Ok(TargetFs::Data(sub)) => match data_fs().and_then(|fs| fs.read_file(sub).ok()) {
            Some(bytes) => (bytes, false),
            None => return nc_abi::ENOENT,
        },
        _ => return nc_abi::EINVAL,
    };
    let source = if from_system { InstallSource::SystemImage } else { InstallSource::UserData };
    let Ok(id) = register_ntzc_bytes(&bytes, source) else {
        return nc_abi::EINVAL;
    };
    if !from_system && data_writable() {
        if let Some(fs) = data_fs() {
            let mut dst = String::from("/app/");
            dst.push_str(&id);
            dst.push_str(".ntzc");
            if path != alloc::format!("/data{}", dst) {
                let _ = fs.write_file(&dst, &bytes);
                sync_data();
            }
        }
    }
    log!("[depo] paket kuruldu/kaydedildi: {} ({})", id, path);
    0
}

pub fn dir_entry_count(path: &str) -> usize {
    match resolve_target(path) {
        Ok(TargetFs::System(sub)) => system_fs()
            .and_then(|fs| fs.list_dir(sub).ok())
            .map(|v| v.len())
            .unwrap_or(0),
        Ok(TargetFs::Data(sub)) => data_fs()
            .and_then(|fs| fs.list_dir(sub).ok())
            .map(|v| v.len())
            .unwrap_or(0),
        _ => 0,
    }
}
