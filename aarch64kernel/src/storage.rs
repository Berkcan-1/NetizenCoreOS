use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicU32, Ordering};
use nc_abi::draw::{
    ICON_BROWSER, ICON_CAMERA, ICON_CONTACTS, ICON_FILES, ICON_GALLERY, ICON_GENERIC,
    ICON_KEYBOARD, ICON_MESSAGES, ICON_PHONE, ICON_SETTINGS, ICON_STOPWATCH, ICON_SYSTEM,
};
use nc_abi::vfs as wire;
use nc_fs::{DirEntry, Exfat, Fat32, FileStat, FileSystem, FileType, FsError};
use nc_fs_policy::{Denied, Op};
use nc_hal::{BlockDevice, BlockDeviceInfo};
use nc_pkg::{InstallSource, NtzcPackage, Tier};

use crate::{blk, lp, task};

const PARTITION_NAMES: [&str; 2] = ["system", "system_a"];
const MAX_TEXT: usize = 512;
const MAX_PACKAGE_BYTES: usize = 8 * 1024 * 1024; // 8 MiB kaynak siniri
const SETTINGS_SCHEMA_VERSION: u32 = 2;

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

/// MBR bolumlu harici USB diskler icin LBA ofset sarmalayicisi.
struct OffsetDisk {
    inner: blk::VirtioDisk,
    offset_lba: u64,
    sectors: u64,
    read_only: bool,
}

impl BlockDevice for OffsetDisk {
    fn info(&self) -> BlockDeviceInfo {
        let mut info = self.inner.info();
        info.block_count = self.sectors;
        info
    }

    fn read_blocks(&mut self, lba: u64, count: u32, buffer: &mut [u8]) -> Result<(), ()> {
        let end = lba.checked_add(count as u64).ok_or(())?;
        if end > self.sectors {
            return Err(());
        }
        self.inner.read_blocks(self.offset_lba + lba, count, buffer)
    }

    fn write_blocks(&mut self, lba: u64, count: u32, buffer: &[u8]) -> Result<(), ()> {
        if self.read_only {
            return Err(());
        }
        let end = lba.checked_add(count as u64).ok_or(())?;
        if end > self.sectors {
            return Err(());
        }
        self.inner.write_blocks(self.offset_lba + lba, count, buffer)
    }
}

enum UsbFs {
    Fat32(Fat32),
    Exfat(Exfat),
}

impl UsbFs {
    fn list_dir(&mut self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        match self {
            UsbFs::Fat32(fs) => fs.list_dir(path),
            UsbFs::Exfat(fs) => fs.list_dir(path),
        }
    }

    fn file_stat(&mut self, path: &str) -> Result<FileStat, FsError> {
        match self {
            UsbFs::Fat32(fs) => fs.file_stat(path),
            UsbFs::Exfat(fs) => fs.file_stat(path),
        }
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        match self {
            UsbFs::Fat32(fs) => fs.read_file(path),
            UsbFs::Exfat(fs) => fs.read_file(path),
        }
    }

    fn read_file_range(&mut self, path: &str, offset: u64, buf: &mut [u8]) -> Result<usize, FsError> {
        match self {
            UsbFs::Fat32(fs) => fs.read_file_range(path, offset, buf),
            UsbFs::Exfat(fs) => fs.read_file_range(path, offset, buf),
        }
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), FsError> {
        match self {
            UsbFs::Fat32(fs) => fs.write_file(path, data),
            UsbFs::Exfat(fs) => fs.write_file(path, data),
        }
    }

    fn create_dir(&mut self, path: &str) -> Result<(), FsError> {
        match self {
            UsbFs::Fat32(fs) => fs.create_dir(path),
            UsbFs::Exfat(fs) => fs.create_dir(path),
        }
    }

    fn remove(&mut self, path: &str) -> Result<(), FsError> {
        match self {
            UsbFs::Fat32(fs) => fs.remove(path),
            UsbFs::Exfat(fs) => fs.remove(path),
        }
    }
}

pub struct UsbVolume {
    pub disk_index: usize,
    pub sectors: u64,
    pub read_only: bool,
    pub fs_type: &'static str,
    pub label: String,
    fs: UsbFs,
}

pub struct UsbVolumeInfo {
    pub mounted: bool,
    pub disk_index: usize,
    pub total_kib: u64,
    pub read_only: bool,
    pub fs_type: &'static str,
    pub label: String,
}

pub struct RegisteredApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub icon_name: String,
    pub icon_id: u32,
    pub tier: Tier,
    pub source: InstallSource,
    pub requested_caps: u32,
    pub caps: u32,
    pub declared_perms: u32,
    pub is_verified: bool,
    pub publisher_key_id: String,
    pub pkg_bytes: usize,
    pub payload: Vec<u8>,
}

pub fn resolve_icon_id(icon_name: &str) -> u32 {
    match icon_name.trim() {
        "files" | "folder" => ICON_FILES,
        "settings" | "sliders" => ICON_SETTINGS,
        "browser" | "globe" => ICON_BROWSER,
        "system" | "monitor" => ICON_SYSTEM,
        "stopwatch" => ICON_STOPWATCH,
        "keyboard" | "ime" => ICON_KEYBOARD,
        "gallery" | "photos" => ICON_GALLERY,
        "messages" | "sms" => ICON_MESSAGES,
        "phone" | "dialer" => ICON_PHONE,
        "contacts" => ICON_CONTACTS,
        "camera" => ICON_CAMERA,
        _ => ICON_GENERIC,
    }
}

static mut SYSTEM_FS: Option<Fat32> = None;
static mut SYSTEM_DISK: usize = usize::MAX;
static mut SYSTEM_SECTORS: u64 = 0;
static mut DATA_FS: Option<Exfat> = None;
static mut DATA_DISK: usize = usize::MAX;
static mut DATA_SECTORS: u64 = 0;
static mut DATA_READ_ONLY: bool = true;
static mut USB_VOL: Option<UsbVolume> = None;
static mut PACKAGES: Vec<RegisteredApp> = Vec::new();
static mut WRITE_STAGE: Option<(String, Vec<u8>, u64)> = None;
static mut CACHED_MEDIA_COUNT: usize = 0;
static REGISTRY_GEN: AtomicU32 = AtomicU32::new(1);

pub fn registry_generation() -> u32 {
    REGISTRY_GEN.load(Ordering::Acquire)
}

fn bump_registry() {
    REGISTRY_GEN.fetch_add(1, Ordering::AcqRel);
}

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

fn usb_vol() -> Option<&'static mut UsbVolume> {
    unsafe { (*addr_of_mut!(USB_VOL)).as_mut() }
}

pub fn usb_online() -> bool {
    usb_vol().is_some()
}

pub fn usb_info() -> UsbVolumeInfo {
    match usb_vol() {
        Some(v) => UsbVolumeInfo {
            mounted: true,
            disk_index: v.disk_index,
            total_kib: v.sectors / 2,
            read_only: v.read_only,
            fs_type: v.fs_type,
            label: v.label.clone(),
        },
        None => UsbVolumeInfo {
            mounted: false,
            disk_index: usize::MAX,
            total_kib: 0,
            read_only: true,
            fs_type: "none",
            label: String::new(),
        },
    }
}

pub fn usb_eject() -> bool {
    let vol = unsafe { &mut *addr_of_mut!(USB_VOL) };
    if let Some(v) = vol.take() {
        if !v.read_only {
            if let Some(disk) = blk::open(v.disk_index) {
                let _ = disk.flush();
            }
        }
        log!("[usb] /storage/usb0 guvenle ayrildi (disk {}, {})", v.disk_index, v.label);
        true
    } else {
        false
    }
}

fn trim_label(raw: &[u8]) -> String {
    let mut s = String::new();
    for &b in raw {
        if b == 0 {
            break;
        }
        if b.is_ascii_graphic() || b == b' ' {
            s.push(b as char);
        }
    }
    String::from(s.trim())
}

fn probe_partition_offset(disk: &mut blk::VirtioDisk) -> (u64, u64) {
    let total = disk.sectors();
    let mut mbr = [0u8; 512];
    if disk.read_blocks(0, 1, &mut mbr).is_err() {
        return (0, total);
    }
    // Dogrudan superfloppy FAT32 veya exFAT ise LBA 0 kullan
    if &mbr[3..11] == b"EXFAT   " || &mbr[82..90] == b"FAT32   " || &mbr[54..62] == b"FAT16   " {
        return (0, total);
    }
    // MBR bolum tablosu denetimi (0x55AA)
    if mbr[510] == 0x55 && mbr[511] == 0xAA {
        for p in 0..4 {
            let off = 446 + p * 16;
            let ptype = mbr[off + 4];
            let start = u32::from_le_bytes([mbr[off + 8], mbr[off + 9], mbr[off + 10], mbr[off + 11]]) as u64;
            let len = u32::from_le_bytes([mbr[off + 12], mbr[off + 13], mbr[off + 14], mbr[off + 15]]) as u64;
            if ptype != 0 && start > 0 && len > 0 && start.saturating_add(len) <= total {
                // FAT32 (0x0B/0x0C), exFAT/NTFS (0x07), FAT16 (0x06/0x0E)
                if matches!(ptype, 0x0B | 0x0C | 0x07 | 0x06 | 0x0E) {
                    return (start, len);
                }
            }
        }
    }
    (0, total)
}

pub fn mount_usb() -> bool {
    if usb_online() {
        return true;
    }
    let sys_disk = unsafe { *addr_of_mut!(SYSTEM_DISK) };
    let data_disk = unsafe { *addr_of_mut!(DATA_DISK) };
    for index in 0..blk::count() {
        if index == sys_disk || index == data_disk {
            continue;
        }
        let mut probe = match blk::open(index) {
            Some(d) => d,
            None => continue,
        };
        let (part_lba, part_sectors) = probe_partition_offset(&mut probe);
        if part_sectors < 64 {
            continue;
        }
        let mut vbr = [0u8; 512];
        if probe.read_blocks(part_lba, 1, &mut vbr).is_err() {
            continue;
        }
        // Varsayilan guvenlik: host USB salt-okunur; virtio-blk readonly=on ise kesinlikle salt-okunur
        let read_only = probe.is_read_only();
        let is_exfat = &vbr[3..11] == b"EXFAT   ";
        let is_fat32 = &vbr[82..90] == b"FAT32   " || (vbr[510] == 0x55 && vbr[511] == 0xAA && vbr[11] == 0 && vbr[12] == 2);

        if is_exfat {
            let dev = OffsetDisk { inner: probe, offset_lba: part_lba, sectors: part_sectors, read_only };
            match Exfat::new(Box::new(dev)) {
                Ok(fs) => {
                    let label = String::from("USB-EXFAT");
                    log!(
                        "[usb] /storage/usb0 baglandi: disk {}, exFAT, {} MiB, {}",
                        index,
                        part_sectors / 2048,
                        if read_only { "salt-okunur" } else { "yazilabilir" }
                    );
                    unsafe {
                        *addr_of_mut!(USB_VOL) = Some(UsbVolume {
                            disk_index: index,
                            sectors: part_sectors,
                            read_only,
                            fs_type: "exFAT",
                            label,
                            fs: UsbFs::Exfat(fs),
                        });
                    }
                    return true;
                }
                Err(e) => {
                    log!("[usb] disk {} exFAT baglanamadi: {:?}", index, e);
                }
            }
        } else if is_fat32 {
            let label_raw = trim_label(&vbr[71..82]);
            let label = if label_raw.is_empty() || label_raw == "NO NAME" {
                String::from("USB-DRIVE")
            } else {
                label_raw
            };
            let dev = OffsetDisk { inner: probe, offset_lba: part_lba, sectors: part_sectors, read_only };
            match Fat32::new(Box::new(dev)) {
                Ok(fs) => {
                    log!(
                        "[usb] /storage/usb0 baglandi: disk {}, FAT32, etiket='{}', {} MiB, {}",
                        index,
                        label,
                        part_sectors / 2048,
                        if read_only { "salt-okunur" } else { "yazilabilir" }
                    );
                    unsafe {
                        *addr_of_mut!(USB_VOL) = Some(UsbVolume {
                            disk_index: index,
                            sectors: part_sectors,
                            read_only,
                            fs_type: "FAT32",
                            label,
                            fs: UsbFs::Fat32(fs),
                        });
                    }
                    return true;
                }
                Err(e) => {
                    log!("[usb] disk {} FAT32 baglanamadi: {:?}", index, e);
                }
            }
        } else {
            log!("[usb] disk {} desteklenmeyen dosya sistemi (yalnizca FAT32/exFAT)", index);
        }
    }
    false
}

#[derive(Clone, Copy)]
pub struct UsageReport {
    pub apps: u64,
    pub media: u64,
    pub appdata: u64,
    pub other: u64,
    pub files: u32,
    pub complete: bool,
}

impl UsageReport {
    pub fn total(&self) -> u64 {
        self.apps
            .saturating_add(self.media)
            .saturating_add(self.appdata)
            .saturating_add(self.other)
    }
}

const USAGE_MAX_ENTRIES: u32 = 20_000;
const USAGE_MAX_DIRS: u32 = 2_048;

struct UsageBudget {
    entries: u32,
    dirs: u32,
    files: u32,
    complete: bool,
}

static mut DATA_USAGE: Option<UsageReport> = None;
static mut SYSTEM_USAGE: Option<UsageReport> = None;

fn invalidate_usage() {
    unsafe {
        *addr_of_mut!(DATA_USAGE) = None;
    }
}

fn walk_bytes<F: FileSystem>(fs: &mut F, root: &str, budget: &mut UsageBudget) -> u64 {
    let mut bytes = 0u64;
    let mut pending: Vec<String> = Vec::new();
    pending.push(String::from(root));
    while let Some(directory) = pending.pop() {
        if budget.dirs >= USAGE_MAX_DIRS {
            budget.complete = false;
            break;
        }
        budget.dirs += 1;
        let entries = match fs.list_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                budget.complete = false;
                continue;
            }
        };
        for entry in entries.iter() {
            budget.entries += 1;
            if budget.entries > USAGE_MAX_ENTRIES {
                budget.complete = false;
                return bytes;
            }
            let name: &str = &entry.name;
            if name == "." || name == ".." {
                continue;
            }
            match entry.file_type {
                FileType::RegularFile => {
                    bytes = bytes.saturating_add(entry.size as u64);
                    budget.files += 1;
                }
                FileType::Directory => {
                    let mut child = String::from(directory.as_str());
                    if !child.ends_with('/') {
                        child.push('/');
                    }
                    child.push_str(name);
                    pending.push(child);
                }
                FileType::Unknown => {}
            }
        }
    }
    bytes
}

fn measure<F: FileSystem>(fs: &mut F) -> UsageReport {
    let mut budget = UsageBudget { entries: 0, dirs: 0, files: 0, complete: true };
    let mut report = UsageReport { apps: 0, media: 0, appdata: 0, other: 0, files: 0, complete: true };
    let top = match fs.list_dir("/") {
        Ok(entries) => entries,
        Err(_) => {
            report.complete = false;
            return report;
        }
    };
    for entry in top.iter() {
        let name: &str = &entry.name;
        if name == "." || name == ".." {
            continue;
        }
        match entry.file_type {
            FileType::RegularFile => {
                report.other = report.other.saturating_add(entry.size as u64);
                budget.files += 1;
            }
            FileType::Directory => {
                let mut path = String::from("/");
                path.push_str(name);
                let bytes = walk_bytes(fs, &path, &mut budget);
                match name {
                    "app" => report.apps = report.apps.saturating_add(bytes),
                    "media" => report.media = report.media.saturating_add(bytes),
                    "appdata" => report.appdata = report.appdata.saturating_add(bytes),
                    _ => report.other = report.other.saturating_add(bytes),
                }
            }
            FileType::Unknown => {}
        }
    }
    report.files = budget.files;
    report.complete = budget.complete;
    report
}

pub fn data_usage() -> Option<UsageReport> {
    unsafe {
        if let Some(cached) = *addr_of_mut!(DATA_USAGE) {
            return Some(cached);
        }
    }
    let report = measure(data_fs()?);
    unsafe {
        *addr_of_mut!(DATA_USAGE) = Some(report);
    }
    Some(report)
}

pub fn system_usage() -> Option<UsageReport> {
    unsafe {
        if let Some(cached) = *addr_of_mut!(SYSTEM_USAGE) {
            return Some(cached);
        }
    }
    let report = measure(system_fs()?);
    unsafe {
        *addr_of_mut!(SYSTEM_USAGE) = Some(report);
    }
    Some(report)
}

pub fn sync_data() -> bool {
    invalidate_usage();
    let index = unsafe { *addr_of_mut!(DATA_DISK) };
    match blk::open(index) {
        Some(disk) => disk.flush(),
        None => false,
    }
}

/// En az ayricalik (least privilege) sistem uygulamasi yetkileri:
/// - `nc.files`: artik `PACKAGE_INSTALL` almaz; paket kurulumunu `nc.package-installer`'a devreder.
/// - `nc.package-installer`: yalnizca kurulum icin gereken asgari yetkiler.
/// - `nc.settings`: ayarlar, guvenlik ve uygulama yonetimi yetkileri.
fn builtin_system_caps(app_id: &str) -> u32 {
    match app_id {
        "nc.files" => {
            nc_caps::FS_ALL_READ
                | nc_caps::FS_ALL_WRITE
                | nc_caps::FS_USER_READ
                | nc_caps::FS_USER_WRITE
                | nc_caps::FS_USER_DELETE
                | nc_caps::SETTINGS_READ
                | nc_caps::PROCESS_LAUNCH
        }
        "nc.package-installer" => {
            nc_caps::PACKAGE_INSTALL
                | nc_caps::FS_ALL_READ
                | nc_caps::SETTINGS_READ
                | nc_caps::PROCESS_LAUNCH
        }
        "nc.settings" => {
            nc_caps::SETTINGS_READ
                | nc_caps::SETTINGS_WRITE_DISPLAY
                | nc_caps::SETTINGS_WRITE_LOCALE
                | nc_caps::SETTINGS_WRITE_NETWORK
                | nc_caps::SECURITY_MANAGE
                | nc_caps::PACKAGE_REMOVE
                | nc_caps::FS_ALL_READ
                | nc_caps::FS_ALL_WRITE
                | nc_caps::PROCESS_LAUNCH
        }
        "nc.keyboard"
        | "nc.gallery"
        | "nc.messages"
        | "nc.phone"
        | "nc.contacts"
        | "nc.camera" => nc_caps::SETTINGS_READ,
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
    bump_registry();
}

fn register_ntzc_bytes(bytes: &[u8], source: InstallSource) -> Result<String, &'static str> {
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err("paket boyut sinirini asiyor (maks 8 MiB)");
    }
    let pkg = NtzcPackage::parse_checked(bytes).map_err(|e| e.as_str())?;
    if !pkg.runnable_free() {
        return Err("paket bu mimaride calistirilamaz (ELF/ABI)");
    }
    let id = pkg.manifest.id.clone();
    // Kullanici kurulumu, yerlesik sistem paketinin uzerine yazamaz!
    if source == InstallSource::UserData {
        if let Some(existing) = find_package(&id) {
            if existing.tier == Tier::System {
                return Err("sistem uygulamasi kullanici paketiyle degistirilemez");
            }
        }
    }
    let is_system = source == InstallSource::SystemImage
        && (pkg.tier(source) == Tier::System || NtzcPackage::is_trusted_builtin(&id));
    let tier = if is_system { Tier::System } else { Tier::User };
    let requested_caps = pkg.capability_bits();
    let caps = if is_system {
        if requested_caps != 0 {
            requested_caps | builtin_system_caps(&id)
        } else {
            builtin_system_caps(&id)
        }
    } else {
        nc_caps::clamp_user(requested_caps)
    };
    let icon_name = if pkg.manifest.icon_name.is_empty() {
        String::from("generic")
    } else {
        pkg.manifest.icon_name.clone()
    };
    let icon_id = resolve_icon_id(&icon_name);
    let declared_perms = pkg.manifest.declared_permission_mask();
    upsert_package(RegisteredApp {
        id: id.clone(),
        name: pkg.manifest.name.clone(),
        version: pkg.manifest.version.clone(),
        icon_name,
        icon_id,
        tier,
        source,
        requested_caps,
        caps,
        declared_perms,
        is_verified: pkg.is_verified,
        publisher_key_id: pkg.manifest.publisher_key_id.clone(),
        pkg_bytes: bytes.len(),
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
            if let Ok(text) = core::str::from_utf8(shown) {
                log!("[depo] /etc/netizencore-release ({} bayt):", bytes.len());
                for line in text.lines() {
                    log!("[depo]   {}", line);
                }
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
        *addr_of_mut!(SYSTEM_DISK) = disk_index;
        *addr_of_mut!(SYSTEM_SECTORS) = sectors;
        *addr_of_mut!(SYSTEM_FS) = Some(fs);
    }
    true
}

const DATA_DIRECTORIES: [&str; 7] = [
    "/app",
    "/appdata",
    "/appdata/nc.settings",
    "/appdata/nc.security",
    "/appdata/nc.contacts.db",
    "/appdata/nc.home",
    "/media",
];
const PROBE_PATH: &str = "/.nc-rw-probe";
const PROBE_BYTES: usize = 9000;
const SETTINGS_CFG_PATH: &str = "/appdata/nc.settings/settings.cfg";
const SETTINGS_TMP_PATH: &str = "/appdata/nc.settings/settings.tmp";
const SETTINGS_BAK_PATH: &str = "/appdata/nc.settings/settings.bak";

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
    if fs.write_file(PROBE_PATH, &pattern).is_err() {
        return false;
    }
    let Ok(read_back) = fs.read_file(PROBE_PATH) else {
        return false;
    };
    let same = read_back == pattern;
    if fs.remove(PROBE_PATH).is_err() {
        return false;
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

#[derive(Clone)]
pub struct SavedSettings {
    pub schema_version: u32,
    pub accent: u32,
    pub wallpaper: u8,
    pub tz_hours: i8,
    pub lang: u32,
    pub wifi: bool,
    pub bt: bool,
    pub brightness: u8,
    pub dark_theme: bool,
    pub screen_timeout_s: u32,
    pub ui_scale: u8,
    pub dev_options: bool,
    pub anim_scale: u8,
    pub show_touch: bool,
    pub verbose_log: bool,
    pub device_name: String,
    pub lock_timeout_s: u32,
    pub lock_on_boot: bool,
    pub lock_show_info: bool,
    pub dnd: bool,
    pub airplane: bool,
}

impl SavedSettings {
    pub fn defaults() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            accent: 0x000A_59F7,
            wallpaper: 0,
            tz_hours: 3,
            lang: 0,
            wifi: true,
            bt: false,
            brightness: 100,
            dark_theme: false,
            screen_timeout_s: 60,
            ui_scale: 100,
            dev_options: false,
            anim_scale: 100,
            show_touch: false,
            verbose_log: false,
            device_name: String::from(nc_device_select::PROFILE.display_name),
            lock_timeout_s: 60,
            lock_on_boot: true,
            lock_show_info: true,
            dnd: false,
            airplane: false,
        }
    }
}

fn parse_settings_text(text: &str) -> Option<SavedSettings> {
    let mut cfg = SavedSettings::defaults();
    let mut seen_any = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v)) = trimmed.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim();
        seen_any = true;
        match k {
            "schema_version" => {
                if let Ok(val) = v.parse::<u32>() {
                    cfg.schema_version = val;
                }
            }
            "accent" => {
                if let Ok(val) = u32::from_str_radix(v.trim_start_matches("0x"), 16) {
                    cfg.accent = val & 0x00FF_FFFF;
                }
            }
            "wallpaper" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.wallpaper = val.min(5);
                }
            }
            "wallpaper_image" => crate::wallpaper::restore_pending(v),
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
            "dark_theme" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.dark_theme = val != 0;
                }
            }
            "screen_timeout_s" => {
                if let Ok(val) = v.parse::<u32>() {
                    cfg.screen_timeout_s = val.min(3600);
                }
            }
            "ui_scale" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.ui_scale = val.clamp(85, 125);
                }
            }
            "dev_options" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.dev_options = val != 0;
                }
            }
            "anim_scale" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.anim_scale = val.min(200);
                }
            }
            "show_touch" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.show_touch = val != 0;
                }
            }
            "verbose_log" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.verbose_log = val != 0;
                }
            }
            "device_name" => {
                if !v.is_empty() && v.len() <= 32 {
                    cfg.device_name = String::from(v);
                }
            }
            "lock_timeout_s" => {
                if let Ok(val) = v.parse::<u32>() {
                    cfg.lock_timeout_s = val.min(3600);
                }
            }
            "lock_on_boot" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.lock_on_boot = val != 0;
                }
            }
            "lock_show_info" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.lock_show_info = val != 0;
                }
            }
            "dnd" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.dnd = val != 0;
                }
            }
            "airplane" => {
                if let Ok(val) = v.parse::<u8>() {
                    cfg.airplane = val != 0;
                }
            }
            _ => {
                // Gelecekteki/bilinmeyen alanlar guvenle yok sayilir
            }
        }
    }
    if !seen_any {
        return None;
    }
    // Sema gecisi (v1 -> v2)
    if cfg.schema_version < SETTINGS_SCHEMA_VERSION {
        cfg.schema_version = SETTINGS_SCHEMA_VERSION;
    }
    Some(cfg)
}

pub fn load_settings_cfg() -> Option<SavedSettings> {
    let fs = data_fs()?;
    for path in [SETTINGS_CFG_PATH, SETTINGS_BAK_PATH] {
        if let Ok(bytes) = fs.read_file(path) {
            if let Ok(text) = core::str::from_utf8(&bytes) {
                if let Some(cfg) = parse_settings_text(text) {
                    if path == SETTINGS_BAK_PATH {
                        log!("[depo] settings.cfg bozuk; settings.bak yedeginden kurtarildi");
                    }
                    return Some(cfg);
                }
            }
        }
    }
    None
}

pub fn save_settings_cfg(cfg: &SavedSettings) -> bool {
    let Some(fs) = data_fs() else { return false };
    if !data_writable() {
        return false;
    }
    let _ = fs.create_dir("/appdata");
    let _ = fs.create_dir("/appdata/nc.settings");
    let mut text = String::new();
    let _ = writeln!(text, "schema_version={}", SETTINGS_SCHEMA_VERSION);
    let _ = writeln!(text, "accent=0x{:06x}", cfg.accent & 0x00FF_FFFF);
    let _ = writeln!(text, "wallpaper={}", cfg.wallpaper.min(5));
    if cfg.wallpaper == 5 {
        if let Some(name) = crate::wallpaper::current_name() {
            let _ = writeln!(text, "wallpaper_image={}", name);
        }
    }
    let _ = writeln!(text, "tz_hours={}", cfg.tz_hours.clamp(-12, 14));
    let _ = writeln!(text, "lang={}", cfg.lang.min(1));
    let _ = writeln!(text, "wifi={}", cfg.wifi as u8);
    let _ = writeln!(text, "bt={}", cfg.bt as u8);
    let _ = writeln!(text, "dnd={}", cfg.dnd as u8);
    let _ = writeln!(text, "airplane={}", cfg.airplane as u8);
    let _ = writeln!(text, "brightness={}", cfg.brightness.clamp(20, 100));
    let _ = writeln!(text, "dark_theme={}", cfg.dark_theme as u8);
    let _ = writeln!(text, "screen_timeout_s={}", cfg.screen_timeout_s.min(3600));
    let _ = writeln!(text, "ui_scale={}", cfg.ui_scale.clamp(85, 125));
    let _ = writeln!(text, "dev_options={}", cfg.dev_options as u8);
    let _ = writeln!(text, "anim_scale={}", cfg.anim_scale.min(200));
    let _ = writeln!(text, "show_touch={}", cfg.show_touch as u8);
    let _ = writeln!(text, "verbose_log={}", cfg.verbose_log as u8);
    let _ = writeln!(text, "device_name={}", cfg.device_name);
    let _ = writeln!(text, "lock_timeout_s={}", cfg.lock_timeout_s.min(3600));
    let _ = writeln!(text, "lock_on_boot={}", cfg.lock_on_boot as u8);
    let _ = writeln!(text, "lock_show_info={}", cfg.lock_show_info as u8);

    // Guvenli yazma: once gecici dosyaya yaz, eskiyi yedekle, sonra asil dosyayi guncelle
    if fs.write_file(SETTINGS_TMP_PATH, text.as_bytes()).is_err() {
        return false;
    }
    if let Ok(prev) = fs.read_file(SETTINGS_CFG_PATH) {
        if !prev.is_empty() {
            let _ = fs.write_file(SETTINGS_BAK_PATH, &prev);
        }
    }
    if fs.write_file(SETTINGS_CFG_PATH, text.as_bytes()).is_err() {
        return false;
    }
    let _ = fs.remove(SETTINGS_TMP_PATH);
    sync_data()
}

pub fn mount_data() -> bool {
    let sys_disk = unsafe { *addr_of_mut!(SYSTEM_DISK) };
    for index in 0..blk::count() {
        if index == sys_disk {
            continue;
        }
        let mut probe = match blk::open(index) {
            Some(disk) => disk,
            None => continue,
        };
        let mut boot = [0u8; 512];
        if probe.read_blocks(0, 1, &mut boot).is_err() || &boot[3..11] != b"EXFAT   " {
            continue;
        }
        // Harici salt-okunur bir USB exFAT diski yanlislikla /data olarak secilmesin:
        // eger baska yazilabilir bir exFAT varsa onu tercih et.
        let read_only = probe.is_read_only();
        if read_only && blk::count() > 2 {
            // Baska diskler varsa salt-okunur exFAT'i USB icin birak
            continue;
        }
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
        let cfg = load_settings_cfg().unwrap_or_else(SavedSettings::defaults);
        crate::host::apply_saved_settings(&cfg);
        crate::lock::load_from_storage(cfg.lock_on_boot, cfg.lock_timeout_s, cfg.lock_show_info);
        crate::perm::load_from_storage();
        log!(
            "[depo] kalici ayarlar yuklendi (sema v{}, accent={:#08x}, tema={}, karanlik={}, dil={})",
            cfg.schema_version,
            cfg.accent,
            cfg.wallpaper,
            cfg.dark_theme,
            cfg.lang
        );
        // Harici USB depolama varsa bagla
        let _ = mount_usb();
        return true;
    }
    log!("[depo] /data icin exFAT aygiti bulunamadi (userdata.img yok)");
    let _ = mount_usb();
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
    StorageRoot,
    System(&'a str),
    Data(&'a str),
    Usb(&'a str),
}

fn resolve_target(path: &str) -> Result<TargetFs<'_>, i32> {
    if path == "/" {
        return Ok(TargetFs::Root);
    }
    if path == "/storage" || path == "/storage/" {
        return Ok(TargetFs::StorageRoot);
    }
    if path == "/storage/usb0" {
        return Ok(TargetFs::Usb("/"));
    }
    if let Some(rest) = path.strip_prefix("/storage/usb0/") {
        if rest.is_empty() {
            return Ok(TargetFs::Usb("/"));
        }
        return Ok(TargetFs::Usb(&path["/storage/usb0".len()..]));
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
        wire::OP_USB_ONLINE => {
            payload.push(if usb_online() { 1 } else { 0 });
        }
        wire::OP_USB_RESCAN => {
            let ok = if usb_online() { true } else { mount_usb() };
            payload.push(if ok { 1 } else { 0 });
        }
        wire::OP_USB_LABEL => {
            if let Some(v) = usb_vol() {
                payload.extend_from_slice(v.label.as_bytes());
            }
        }
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
                            if usb_online() {
                                root_entries.push(DirEntry {
                                    name: String::from("storage"),
                                    file_type: FileType::Directory,
                                    size: 0,
                                    id: 3,
                                });
                            }
                            encode_entries(&root_entries, &mut payload);
                        }
                        wire::OP_STAT => encode_stat(FileStat { size: 0, is_dir: true, readonly: true }, true, &mut payload),
                        _ => status = wire::ERR_READ_ONLY,
                    },
                    Ok(TargetFs::StorageRoot) => match req.op {
                        wire::OP_LIST => {
                            let mut entries = Vec::new();
                            if let Some(v) = usb_vol() {
                                entries.push(DirEntry {
                                    name: String::from("usb0"),
                                    file_type: FileType::Directory,
                                    size: v.sectors * 512,
                                    id: 10,
                                });
                            }
                            encode_entries(&entries, &mut payload);
                        }
                        wire::OP_STAT => encode_stat(FileStat { size: 0, is_dir: true, readonly: true }, true, &mut payload),
                        _ => status = wire::ERR_READ_ONLY,
                    },
                    Ok(TargetFs::Usb(subpath)) => match usb_vol() {
                        None => status = wire::ERR_NOT_FOUND,
                        Some(vol) => match req.op {
                            wire::OP_LIST => match vol.fs.list_dir(subpath) {
                                Ok(entries) => encode_entries(&entries, &mut payload),
                                Err(err) => status = map_fs_error(err),
                            },
                            wire::OP_STAT => match vol.fs.file_stat(subpath) {
                                Ok(stat) => encode_stat(stat, vol.read_only, &mut payload),
                                Err(err) => status = map_fs_error(err),
                            },
                            wire::OP_READ_RANGE => {
                                let want = (req.arg1 as usize).min(wire::MAX_CHUNK);
                                payload.resize(want, 0);
                                match vol.fs.read_file_range(subpath, req.arg0, &mut payload) {
                                    Ok(read) => payload.truncate(read),
                                    Err(err) => {
                                        payload.clear();
                                        status = map_fs_error(err);
                                    }
                                }
                            }
                            wire::OP_WRITE => {
                                if vol.read_only {
                                    status = wire::ERR_READ_ONLY;
                                } else if req.arg0 == 0 && req.data.len() as u64 == req.arg1 {
                                    if let Err(err) = vol.fs.write_file(subpath, req.data) {
                                        status = map_fs_error(err);
                                    }
                                } else {
                                    status = wire::ERR_READ_ONLY;
                                }
                            }
                            wire::OP_MKDIR => {
                                if vol.read_only {
                                    status = wire::ERR_READ_ONLY;
                                } else if let Err(err) = vol.fs.create_dir(subpath) {
                                    status = map_fs_error(err);
                                }
                            }
                            wire::OP_REMOVE => {
                                if vol.read_only {
                                    status = wire::ERR_READ_ONLY;
                                } else if let Err(err) = vol.fs.remove(subpath) {
                                    status = map_fs_error(err);
                                }
                            }
                            _ => status = wire::ERR_INVALID_PATH,
                        },
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

fn read_bytes_from_any_fs(path: &str) -> Result<(Vec<u8>, bool), i64> {
    match resolve_target(path) {
        Ok(TargetFs::System(sub)) => match system_fs().and_then(|fs| fs.read_file(sub).ok()) {
            Some(bytes) => Ok((bytes, true)),
            None => Err(nc_abi::ENOENT),
        },
        Ok(TargetFs::Data(sub)) => match data_fs().and_then(|fs| fs.read_file(sub).ok()) {
            Some(bytes) => Ok((bytes, false)),
            None => Err(nc_abi::ENOENT),
        },
        Ok(TargetFs::Usb(sub)) => match usb_vol().and_then(|v| v.fs.read_file(sub).ok()) {
            Some(bytes) => Ok((bytes, false)),
            None => Err(nc_abi::ENOENT),
        },
        _ => Err(nc_abi::EINVAL),
    }
}

/// Cekirdek yetkili paket denetimi: `nc.package-installer` tarafindan kurulum oncesi cagirilir.
pub fn inspect_package_from_path(caps: u32, path: &str) -> Result<String, i32> {
    if caps & (nc_caps::PACKAGE_INSTALL | nc_caps::FS_ALL_READ) == 0 {
        return Err(nc_abi::sys::ERR_DENIED);
    }
    let (bytes, from_system) = match read_bytes_from_any_fs(path) {
        Ok(v) => v,
        Err(_) => return Err(nc_abi::sys::ERR_FAILED),
    };
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(nc_abi::sys::ERR_BAD_REQUEST);
    }
    let pkg = match NtzcPackage::parse_checked(&bytes) {
        Ok(p) => p,
        Err(e) => {
            let mut out = String::new();
            let _ = writeln!(out, "valid=0");
            let _ = writeln!(out, "error={}", e.as_str());
            return Ok(out);
        }
    };
    let source = if from_system { InstallSource::SystemImage } else { InstallSource::UserData };
    let runnable = pkg.runnable_free();
    let requested = pkg.capability_bits();
    let granted = pkg.granted_caps(source);
    let denied = requested & !granted;
    let existing = find_package(&pkg.manifest.id);
    let system_conflict = !from_system && existing.map(|e| e.tier == Tier::System).unwrap_or(false);
    let icon_name = if pkg.manifest.icon_name.is_empty() { "generic" } else { &pkg.manifest.icon_name };

    let mut out = String::new();
    let _ = writeln!(out, "valid=1");
    let _ = writeln!(out, "runnable={}", runnable as u8);
    let _ = writeln!(out, "id={}", pkg.manifest.id);
    let _ = writeln!(out, "name={}", pkg.manifest.name);
    let _ = writeln!(out, "version={}", pkg.manifest.version);
    let _ = writeln!(out, "icon={}", icon_name);
    let _ = writeln!(out, "icon_id={}", resolve_icon_id(icon_name));
    let _ = writeln!(out, "arch={}", pkg.manifest.target_arch);
    let _ = writeln!(
        out,
        "abi={}",
        if pkg.manifest.abi == nc_pkg::PackageAbi::Ring3V1 { "ring3-v1" } else { "legacy" }
    );
    let _ = writeln!(out, "verified={}", pkg.is_verified as u8);
    let _ = writeln!(out, "free_install={}", pkg.free_install as u8);
    let _ = writeln!(
        out,
        "publisher={}",
        if pkg.manifest.publisher_key_id.is_empty() { "unsigned" } else { &pkg.manifest.publisher_key_id }
    );
    let _ = writeln!(out, "requested_caps=0x{:08x}", requested);
    let _ = writeln!(out, "granted_caps=0x{:08x}", granted);
    let _ = writeln!(out, "denied_caps=0x{:08x}", denied);
    let declared_perms = pkg.manifest.declared_permission_mask();
    let _ = writeln!(out, "declared_perms=0x{:08x}", declared_perms);
    let _ = writeln!(
        out,
        "declared_perm_names={}",
        nc_pkg::permissions::PermissionStore::format_perm_list(declared_perms)
    );
    let unknowns = pkg.manifest.unknown_permissions();
    if !unknowns.is_empty() {
        let mut ustr = String::new();
        for (idx, name) in unknowns.iter().enumerate() {
            if idx > 0 {
                ustr.push(',');
            }
            ustr.push_str(name);
        }
        let _ = writeln!(out, "unknown_perms={}", ustr);
    }
    let _ = writeln!(out, "size_bytes={}", bytes.len());
    let _ = writeln!(out, "already_installed={}", existing.is_some() as u8);
    if let Some(ex) = existing {
        let _ = writeln!(out, "installed_version={}", ex.version);
    }
    let _ = writeln!(out, "system_conflict={}", system_conflict as u8);
    Ok(out)
}

pub fn install_package_from_path(caps: u32, path: &str) -> i64 {
    if caps & nc_caps::PACKAGE_INSTALL == 0 {
        return nc_abi::EACCES;
    }
    let (bytes, from_system) = match read_bytes_from_any_fs(path) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let source = if from_system { InstallSource::SystemImage } else { InstallSource::UserData };
    let id = match register_ntzc_bytes(&bytes, source) {
        Ok(id) => id,
        Err(reason) => {
            log!("[depo] paket kurulumu reddedildi ({}): {}", path, reason);
            return nc_abi::EINVAL;
        }
    };
    if !from_system && data_writable() {
        if let Some(fs) = data_fs() {
            let _ = fs.create_dir("/app");
            let tmp_path = alloc::format!("/app/{}.tmp", id);
            let dst_path = alloc::format!("/app/{}.ntzc", id);
            let full_dst = alloc::format!("/data{}", dst_path);
            if path != full_dst {
                if fs.write_file(&tmp_path, &bytes).is_ok() {
                    let _ = fs.write_file(&dst_path, &bytes);
                    let _ = fs.remove(&tmp_path);
                    sync_data();
                }
            }
        }
    }
    log!("[depo] paket kuruldu/kaydedildi: {} ({})", id, path);
    0
}

fn remove_tree_data(fs: &mut Exfat, root: &str) {
    let Ok(entries) = fs.list_dir(root) else { return };
    for entry in entries.iter() {
        if entry.name == "." || entry.name == ".." {
            continue;
        }
        let child = alloc::format!("{}/{}", root, entry.name);
        if entry.file_type == FileType::Directory {
            remove_tree_data(fs, &child);
        }
        let _ = fs.remove(&child);
    }
}

pub fn clear_app_data(caps: u32, app_id: &str) -> i32 {
    if caps & (nc_caps::PACKAGE_REMOVE | nc_caps::FS_ALL_WRITE) == 0 {
        return nc_abi::sys::ERR_DENIED;
    }
    if app_id.is_empty() || app_id == "nc.security" || app_id == "nc.contacts.db" || !nc_pkg::manifest::valid_app_id(app_id) {
        return nc_abi::sys::ERR_BAD_REQUEST;
    }
    if !data_writable() {
        return nc_abi::sys::ERR_FAILED;
    }
    let Some(fs) = data_fs() else {
        return nc_abi::sys::ERR_FAILED;
    };
    let dir = alloc::format!("/appdata/{}", app_id);
    remove_tree_data(fs, &dir);
    sync_data();
    log!("[depo] uygulama verisi temizlendi: /data{}", dir);
    0
}

pub fn uninstall_package(caps: u32, app_id: &str, remove_data: bool) -> i32 {
    if caps & nc_caps::PACKAGE_REMOVE == 0 {
        return nc_abi::sys::ERR_DENIED;
    }
    if app_id.is_empty() || !nc_pkg::manifest::valid_app_id(app_id) {
        return nc_abi::sys::ERR_BAD_REQUEST;
    }
    let Some(pkg) = find_package(app_id) else {
        return nc_abi::sys::ERR_FAILED;
    };
    // Sistem uygulamalari yanlislikla kaldirilmaya karsi korunur!
    if pkg.tier == Tier::System {
        log!("[depo] sistem uygulamasi kaldirilamaz: {}", app_id);
        return nc_abi::sys::ERR_DENIED;
    }
    if let Some(slot) = task::find_app(app_id) {
        let _ = task::kill_slot(slot);
    }
    if data_writable() {
        if let Some(fs) = data_fs() {
            let pkg_file = alloc::format!("/app/{}.ntzc", app_id);
            let _ = fs.remove(&pkg_file);
            if remove_data {
                let dir = alloc::format!("/appdata/{}", app_id);
                remove_tree_data(fs, &dir);
                let _ = fs.remove(&dir);
            }
            sync_data();
        }
    }
    let list = unsafe { &mut *addr_of_mut!(PACKAGES) };
    list.retain(|item| item.id != app_id);
    crate::perm::on_user_app_uninstalled(app_id);
    bump_registry();
    log!("[depo] kullanici uygulamasi kaldirildi: {} (veri_sil={})", app_id, remove_data);
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
        Ok(TargetFs::Usb(sub)) => usb_vol()
            .and_then(|v| v.fs.list_dir(sub).ok())
            .map(|v| v.len())
            .unwrap_or(0),
        _ => 0,
    }
}
