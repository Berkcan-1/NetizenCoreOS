// crates/nc-fs/src/lib.rs
#![no_std]

extern crate alloc;

pub mod erofs;
pub mod exfat;
pub mod fat32;
pub mod memfs;
pub mod vfs;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

pub use erofs::Erofs;
pub use exfat::Exfat;
pub use fat32::Fat32;
pub use memfs::MemFs;
pub use vfs::{DirEntry, FileStat, FileSystem, FileType, FsError};

use nc_storage::{
    find_external_usb_disk, make_ntzc_pkg, make_sample_mp4, make_sample_png, parse_mbr_partitions,
    AtaPartition, DATA_START_LBA, MIN_DATA_SECTORS, SYSTEM_SECTORS, SYSTEM_START_LBA,
};

// --- DYSONSPHERE FAZ 4: VFS & SETTINGS SERVICE ZORUNLU YETKİ SINIRI ---
use core::sync::atomic::{AtomicU8, Ordering};

pub const CALLER_KERNEL: u8   = 0; // Çekirdek / Başlangıç
pub const CALLER_SETTINGS: u8 = 1; // Sadece nc.settings (Ayarlar ve Sürücüler)
pub const CALLER_FILES: u8    = 2; // Sadece nc.files (Kullanıcı Dosyaları ve Paket Kurulumu)
pub const CALLER_USER_APP: u8 = 3; // /data/app altından yüklenen 3. parti uygulamalar (Kısıtlı)

static ACTIVE_VFS_CALLER: AtomicU8 = AtomicU8::new(CALLER_KERNEL);

pub fn set_vfs_caller(caller_role: u8) {
    ACTIVE_VFS_CALLER.store(caller_role, Ordering::Release);
}

pub fn get_vfs_caller() -> u8 {
    ACTIVE_VFS_CALLER.load(Ordering::Acquire)
}

/// Dizin aşımı (Path Traversal: "..", "./", "\0") ve bozuk yolları engeller
fn validate_safe_path(path: &str) -> Result<(), FsError> {
    if path.is_empty() || path.len() > 256 || path.contains('\0') || path.contains('\\') {
        return Err(FsError::InvalidPath);
    }
    for seg in path.split('/') {
        if seg == "." || seg == ".." {
            return Err(FsError::InvalidPath);
        }
    }
    Ok(())
}

/// DysonSphere Kaynak Sınırı Denetimi: Kim nereye yazabilir / silebilir?
fn check_write_permission(path: &str, is_delete: bool) -> Result<(), FsError> {
    validate_safe_path(path)?;
    let caller = get_vfs_caller();

    // Sistem ve önyükleme bölümleri herkese salt okunurdur
    if path == "/system" || path.starts_with("/system/") || path == "/boot" || path.starts_with("/boot/") {
        return Err(FsError::ReadOnly);
    }

    // Korumalı Ayarlar (/data/system) ve Donanım Sürücüleri (/data/drivers)
    let is_protected_settings = path == "/data/system"
        || path.starts_with("/data/system/")
        || path == "/data/drivers"
        || path.starts_with("/data/drivers/")
        || path == "/ram/system"
        || path.starts_with("/ram/system/")
        || path == "/ram/drivers"
        || path.starts_with("/ram/drivers/");

    if is_protected_settings {
        // SADECE Ayarlar uygulaması (nc.settings) veya Çekirdek değiştirebilir!
        if caller == CALLER_KERNEL || caller == CALLER_SETTINGS {
            return Ok(());
        }
        return Err(FsError::PermissionDenied);
    }

    // Uygulama Kurulum Klasörü (/data/app)
    let is_app_install_dir = path == "/data/app" || path.starts_with("/data/app/");
    if is_app_install_dir {
        // SADECE Dosyalar uygulaması (Paket Kurucu) veya Çekirdek yazabilir!
        if caller == CALLER_KERNEL || caller == CALLER_FILES {
            return Ok(());
        }
        return Err(FsError::PermissionDenied);
    }

    // Kullanıcı Dosya Alanları (/data/desktop, /data/documents, /usb, /ram)
    match caller {
        CALLER_KERNEL | CALLER_FILES => Ok(()),
        CALLER_SETTINGS => {
            // Ayarlar uygulaması kullanıcının kişisel belgelerini silemez veya değiştiremez! (Least Privilege)
            Err(FsError::PermissionDenied)
        }
        CALLER_USER_APP => {
            // /data/app altından yüklenen 3. parti uygulamalar dosya silemez ve sadece /ram veya /data/documents altına yazabilir
            if is_delete {
                return Err(FsError::PermissionDenied);
            }
            if path.starts_with("/ram/") || path.starts_with("/data/documents/") {
                Ok(())
            } else {
                Err(FsError::PermissionDenied)
            }
        }
        _ => Err(FsError::PermissionDenied),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PartitionTarget {
    System,
    Data,
    Usb,
    Ram,
}

#[derive(Clone, Debug)]
pub struct PartitionMetrics {
    pub mount_point: &'static str,
    pub label: &'static str,
    pub fs_type: &'static str,
    pub start_lba: u64,
    pub sector_count: u64,
    pub total_mb: u64,
    pub used_mb: u64,
    pub readonly: bool,
    pub online: bool,
}

pub struct VfsManager {
    system_fs: Option<Erofs>,
    system_start_lba: u64,
    system_sectors: u64,
    data_fs: Option<Exfat>,
    data_start_lba: u64,
    data_sectors: u64,
    usb_fs: Option<Box<dyn FileSystem>>,
    usb_model: String,
    usb_start_lba: u64,
    usb_sectors: u64,
    usb_total_mb: u64,
    usb_fs_type: &'static str,
    usb_readonly: bool,
    ram_fs: MemFs,
}

impl VfsManager {
    // Bolum 5.6: Sabit 1 GiB yerine kurulu diskin gercek MBR bolum tablosundaki araliklari oku
    pub fn new() -> Self {
        let mut system_fs = None;
        let mut data_fs = None;
        let mut system_start_lba = SYSTEM_START_LBA;
        let mut system_sectors = SYSTEM_SECTORS;
        let mut data_start_lba = DATA_START_LBA;
        let mut data_sectors = MIN_DATA_SECTORS;

        if let Some(mut ata) = nc_storage::find_installed_disk() {
            let mbr_parts = parse_mbr_partitions(&mut ata);

            if let Some(p1) = mbr_parts[1] {
                if p1.start_lba
                    .checked_add(p1.sector_count)
                    .map(|end| end <= ata.info.block_count)
                    .unwrap_or(false)
                {
                    system_start_lba = p1.start_lba;
                    system_sectors = p1.sector_count;
                }
            }
            if let Some(p3) = mbr_parts[3] {
                if p3.start_lba
                    .checked_add(p3.sector_count)
                    .map(|end| end <= ata.info.block_count)
                    .unwrap_or(false)
                {
                    data_start_lba = p3.start_lba;
                    data_sectors = p3.sector_count;
                }
            } else if ata.info.block_count > DATA_START_LBA {
                data_sectors = ata.info.block_count - DATA_START_LBA;
            }

            let part_sys = AtaPartition::new(ata, system_start_lba, system_sectors);
            if let Ok(fs) = Erofs::new(Box::new(part_sys)) {
                system_fs = Some(fs);
            }

            let part_data = AtaPartition::new(ata, data_start_lba, data_sectors);
            if let Ok(mut fs) = Exfat::new(Box::new(part_data)) {
                Self::init_locale_once(&mut fs);
                data_fs = Some(fs);
            }
        }

        let mut vfs = VfsManager {
            system_fs,
            system_start_lba,
            system_sectors,
            data_fs,
            data_start_lba,
            data_sectors,
            usb_fs: None,
            usb_model: String::from("Takili Degil"),
            usb_start_lba: 0,
            usb_sectors: 0,
            usb_total_mb: 0,
            usb_fs_type: "Yok",
            usb_readonly: true,
            ram_fs: MemFs::new(),
        };
        vfs.rescan_usb();
        vfs
    }

    fn init_locale_once(fs: &mut Exfat) {
        if fs.read_file("system/locale.cfg").is_err() {
            if let Some((lang, kbd)) = nc_storage::read_initial_locale() {
                let _ = fs.create_dir("system");
                let cfg = alloc::format!("lang={}\nkbd={}\n", lang, kbd);
                let _ = fs.write_file("system/locale.cfg", cfg.as_bytes());
            }
        }
    }

    // M-04 & M-06: Harici diskin tum gecerli bolumlerini dene ve FAT32 ise salt okunur olarak isaretle
    pub fn rescan_usb(&mut self) -> bool {
        if let Some((mut dev, default_start, default_sectors)) = find_external_usb_disk() {
            let model = String::from(dev.info.model_str());
            let mut candidates = [(0u64, 0u64); 5];
            let mut cand_cnt = 0usize;

            let parts = parse_mbr_partitions(&mut dev);
            for p in parts.iter().flatten() {
                if let Some(end) = p.start_lba.checked_add(p.sector_count) {
                    if p.sector_count > 0 && end <= dev.info.block_count && cand_cnt < 4 {
                        candidates[cand_cnt] = (p.start_lba, p.sector_count);
                        cand_cnt += 1;
                    }
                }
            }
            if cand_cnt == 0 {
                candidates[0] = (default_start, default_sectors);
                cand_cnt = 1;
            }

            for &(start_lba, sectors) in &candidates[..cand_cnt] {
                let total_mb = ((sectors * 512) / (1024 * 1024)).max(1);
                let part = AtaPartition::new(dev, start_lba, sectors);
                if let Ok(fs) = Exfat::new(Box::new(part)) {
                    self.usb_fs = Some(Box::new(fs));
                    self.usb_model = model;
                    self.usb_start_lba = start_lba;
                    self.usb_sectors = sectors;
                    self.usb_total_mb = total_mb;
                    self.usb_fs_type = "exFAT";
                    self.usb_readonly = false;
                    return true;
                }
                let part2 = AtaPartition::new(dev, start_lba, sectors);
                if let Ok(fs) = Fat32::new(Box::new(part2)) {
                    self.usb_fs = Some(Box::new(fs));
                    self.usb_model = model;
                    self.usb_start_lba = start_lba;
                    self.usb_sectors = sectors;
                    self.usb_total_mb = total_mb;
                    self.usb_fs_type = "FAT32";
                    self.usb_readonly = true; // FAT32 yazma desteklemediginden salt okunur
                    return true;
                }
            }
        }
        self.usb_fs = None;
        self.usb_model = String::from("Takili Degil");
        self.usb_start_lba = 0;
        self.usb_sectors = 0;
        self.usb_total_mb = 0;
        self.usb_fs_type = "Yok";
        self.usb_readonly = true;
        false
    }

    pub fn is_usb_online(&self) -> bool {
        self.usb_fs.is_some()
    }

    pub fn is_usb_readonly(&self) -> bool {
        self.usb_readonly
    }

    pub fn usb_label(&self) -> &str {
        &self.usb_model
    }

    pub fn partition_metrics(&mut self) -> Vec<PartitionMetrics> {
        self.rescan_usb();
        let sys_online = self.system_fs.is_some();
        let data_online = self.data_fs.is_some();
        let usb_online = self.usb_fs.is_some();

        let sys_total_mb = ((self.system_sectors * 512) / (1024 * 1024)).max(1);
        let data_total_mb = ((self.data_sectors * 512) / (1024 * 1024)).max(1);

        let data_used_mb = if let Some(ref mut dfs) = self.data_fs {
            ((dfs.used_clusters_count() as u64) * 4 / 1024).max(1)
        } else {
            0
        };
        let usb_used_mb = if self.usb_fs.is_some() { 4 } else { 0 };

        alloc::vec![
            PartitionMetrics {
                mount_point: "/boot",
                label: "Bolum 1: ESP & Onyukleme",
                fs_type: "FAT16 + Stage2",
                start_lba: 65_536,
                sector_count: 65_536,
                total_mb: 32,
                used_mb: 8,
                readonly: true,
                online: sys_online,
            },
            PartitionMetrics {
                mount_point: "/system",
                label: "Bolum 2: Sistem Agaci",
                fs_type: "EROFS",
                start_lba: self.system_start_lba,
                sector_count: self.system_sectors,
                total_mb: sys_total_mb,
                used_mb: if sys_online { 384 } else { 0 },
                readonly: true,
                online: sys_online,
            },
            PartitionMetrics {
                mount_point: "/data",
                label: "Bolum 4: Kisisel Disk",
                fs_type: "exFAT",
                start_lba: self.data_start_lba,
                sector_count: self.data_sectors,
                total_mb: data_total_mb,
                used_mb: data_used_mb,
                readonly: false,
                online: data_online,
            },
            PartitionMetrics {
                mount_point: "/usb",
                label: "Harici Depolama Birimi",
                fs_type: self.usb_fs_type,
                start_lba: self.usb_start_lba,
                sector_count: self.usb_sectors,
                total_mb: self.usb_total_mb,
                used_mb: usb_used_mb,
                readonly: self.usb_readonly,
                online: usb_online,
            },
            PartitionMetrics {
                mount_point: "/ram",
                label: "Gecici Bellek Diski",
                fs_type: "MemFS",
                start_lba: 0,
                sector_count: 65_536,
                total_mb: 32,
                used_mb: 4,
                readonly: false,
                online: true,
            },
        ]
    }

    fn split_mount_path<'a>(&self, path: &'a str) -> (&'a str, &'a str) {
        let trimmed = path.trim_start_matches('/');
        if trimmed.is_empty() {
            return ("", "");
        }
        if let Some(pos) = trimmed.find('/') {
            (&trimmed[..pos], &trimmed[pos + 1..])
        } else {
            (trimmed, "")
        }
    }

    // M-05: Bagli dosya sistemi varsa hatalari fallback ile gizleme; dogrudan ust katmana ilet
    pub fn list_dir_path(&mut self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        validate_safe_path(path)?;
        let (mount, rel) = self.split_mount_path(path);
        if mount.is_empty() {
            return Ok(alloc::vec![
                DirEntry {
                    name: String::from("data"),
                    file_type: FileType::Directory,
                    size: 0,
                    id: 1,
                },
                DirEntry {
                    name: String::from("usb"),
                    file_type: FileType::Directory,
                    size: 0,
                    id: 2,
                },
                DirEntry {
                    name: String::from("system"),
                    file_type: FileType::Directory,
                    size: 0,
                    id: 3,
                },
                DirEntry {
                    name: String::from("ram"),
                    file_type: FileType::Directory,
                    size: 0,
                    id: 4,
                },
            ]);
        }
        match mount {
            "system" => {
                if let Some(ref mut fs) = self.system_fs {
                    fs.list_dir(rel)
                } else {
                    Self::fallback_dir(PartitionTarget::System, rel)
                }
            }
            "data" => {
                if let Some(ref mut fs) = self.data_fs {
                    fs.list_dir(rel)
                } else {
                    Self::fallback_dir(PartitionTarget::Data, rel)
                }
            }
            "usb" => {
                self.rescan_usb();
                if let Some(ref mut fs) = self.usb_fs {
                    fs.list_dir(rel)
                } else {
                    Ok(Vec::new())
                }
            }
            "ram" => self.ram_fs.list_dir(rel),
            _ => Err(FsError::NotFound),
        }
    }

    // M-05: Bagli dosya sistemi varsa sahte fallback dosyasi dondurme
    pub fn read_file_path(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        validate_safe_path(path)?;
        let (mount, rel) = self.split_mount_path(path);
        match mount {
            "system" => {
                if let Some(ref mut fs) = self.system_fs {
                    fs.read_file(rel)
                } else {
                    Self::fallback_read(PartitionTarget::System, rel)
                }
            }
            "data" => {
                if let Some(ref mut fs) = self.data_fs {
                    fs.read_file(rel)
                } else {
                    Self::fallback_read(PartitionTarget::Data, rel)
                }
            }
            "usb" => {
                if let Some(ref mut fs) = self.usb_fs {
                    fs.read_file(rel)
                } else {
                    Err(FsError::NotFound)
                }
            }
            "ram" => self.ram_fs.read_file(rel),
            _ => Err(FsError::NotFound),
        }
    }

    pub fn write_file_path(&mut self, path: &str, content: &[u8]) -> Result<(), FsError> {
        check_write_permission(path, false)?;
        let (mount, rel) = self.split_mount_path(path);
        match mount {
            "data" => {
                if let Some(ref mut fs) = self.data_fs {
                    fs.write_file(rel, content)
                } else {
                    self.ram_fs.write_file(rel, content)
                }
            }
            "usb" => {
                if self.usb_readonly {
                    return Err(FsError::ReadOnly);
                }
                if let Some(ref mut fs) = self.usb_fs {
                    fs.write_file(rel, content)
                } else {
                    Err(FsError::DiskError)
                }
            }
            "ram" => self.ram_fs.write_file(rel, content),
            "system" => Err(FsError::ReadOnly),
            _ => Err(FsError::NotFound),
        }
    }

    pub fn create_dir_path(&mut self, path: &str) -> Result<(), FsError> {
        check_write_permission(path, false)?;
        let (mount, rel) = self.split_mount_path(path);
        match mount {
            "data" => {
                if let Some(ref mut fs) = self.data_fs {
                    fs.create_dir(rel)
                } else {
                    self.ram_fs.create_dir(rel)
                }
            }
            "usb" => {
                if self.usb_readonly {
                    return Err(FsError::ReadOnly);
                }
                if let Some(ref mut fs) = self.usb_fs {
                    fs.create_dir(rel)
                } else {
                    Err(FsError::DiskError)
                }
            }
            "ram" => self.ram_fs.create_dir(rel),
            "system" => Err(FsError::ReadOnly),
            _ => Err(FsError::NotFound),
        }
    }

    pub fn remove_path(&mut self, path: &str) -> Result<(), FsError> {
        check_write_permission(path, true)?;
        let (mount, rel) = self.split_mount_path(path);
        match mount {
            "data" => {
                if let Some(ref mut fs) = self.data_fs {
                    fs.remove(rel)
                } else {
                    self.ram_fs.remove(rel)
                }
            }
            "usb" => {
                if self.usb_readonly {
                    return Err(FsError::ReadOnly);
                }
                if let Some(ref mut fs) = self.usb_fs {
                    fs.remove(rel)
                } else {
                    Err(FsError::DiskError)
                }
            }
            "ram" => self.ram_fs.remove(rel),
            "system" => Err(FsError::ReadOnly),
            _ => Err(FsError::NotFound),
        }
    }

    pub fn file_stat_path(&mut self, path: &str) -> Result<FileStat, FsError> {
        let (mount, rel) = self.split_mount_path(path);
        if mount.is_empty() {
            return Ok(FileStat {
                size: 0,
                is_dir: true,
                readonly: true,
            });
        }
        match mount {
            "system" => {
                if let Some(ref mut fs) = self.system_fs {
                    fs.file_stat(rel)
                } else {
                    let is_dir = Self::fallback_dir(PartitionTarget::System, rel).is_ok();
                    let size = if is_dir {
                        0
                    } else {
                        Self::fallback_read(PartitionTarget::System, rel)?.len() as u64
                    };
                    Ok(FileStat {
                        size,
                        is_dir,
                        readonly: true,
                    })
                }
            }
            "data" => {
                if let Some(ref mut fs) = self.data_fs {
                    fs.file_stat(rel)
                } else {
                    let is_dir = Self::fallback_dir(PartitionTarget::Data, rel).is_ok();
                    let size = if is_dir {
                        0
                    } else {
                        Self::fallback_read(PartitionTarget::Data, rel)?.len() as u64
                    };
                    Ok(FileStat {
                        size,
                        is_dir,
                        readonly: false,
                    })
                }
            }
            "usb" => {
                if let Some(ref mut fs) = self.usb_fs {
                    fs.file_stat(rel)
                } else {
                    Err(FsError::NotFound)
                }
            }
            "ram" => self.ram_fs.file_stat(rel),
            _ => Err(FsError::NotFound),
        }
    }

    pub fn list_dir(
        &mut self,
        target: PartitionTarget,
        path: &str,
    ) -> Result<Vec<DirEntry>, FsError> {
        let full_path = match target {
            PartitionTarget::System => alloc::format!("/system/{}", path.trim_start_matches('/')),
            PartitionTarget::Data => alloc::format!("/data/{}", path.trim_start_matches('/')),
            PartitionTarget::Usb => alloc::format!("/usb/{}", path.trim_start_matches('/')),
            PartitionTarget::Ram => alloc::format!("/ram/{}", path.trim_start_matches('/')),
        };
        self.list_dir_path(&full_path)
    }

    pub fn read_file(&mut self, target: PartitionTarget, path: &str) -> Result<Vec<u8>, FsError> {
        let full_path = match target {
            PartitionTarget::System => alloc::format!("/system/{}", path.trim_start_matches('/')),
            PartitionTarget::Data => alloc::format!("/data/{}", path.trim_start_matches('/')),
            PartitionTarget::Usb => alloc::format!("/usb/{}", path.trim_start_matches('/')),
            PartitionTarget::Ram => alloc::format!("/ram/{}", path.trim_start_matches('/')),
        };
        self.read_file_path(&full_path)
    }

    fn fallback_dir(target: PartitionTarget, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let trimmed = path.trim_matches('/');
        let mut list = Vec::new();
        if trimmed.is_empty() {
            list.push(DirEntry {
                name: String::from("app"),
                file_type: FileType::Directory,
                size: 0,
                id: 1,
            });
            if target == PartitionTarget::Data {
                for (id, name) in [
                    (2, "desktop"),
                    (3, "documents"),
                    (4, "downloads"),
                    (5, "media"),
                ] {
                    list.push(DirEntry {
                        name: String::from(name),
                        file_type: FileType::Directory,
                        size: 0,
                        id,
                    });
                }
            } else {
                list.push(DirEntry {
                    name: String::from("build.prop"),
                    file_type: FileType::RegularFile,
                    size: 214,
                    id: 6,
                });
            }
            Ok(list)
        } else {
            Err(FsError::NotFound)
        }
    }

    fn fallback_read(_target: PartitionTarget, path: &str) -> Result<Vec<u8>, FsError> {
        let trimmed = path.trim_matches('/');
        if trimmed == "build.prop" {
            return Ok(b"ro.netizen.os.name=NetizenCore\nro.netizen.os.version=2.8.0\n".to_vec());
        }
        if trimmed == "media/aurora_cam.png" {
            return Ok(make_sample_png(0).to_vec());
        }
        if trimmed == "media/aero_tanitim.mp4" {
            return Ok(make_sample_mp4(0, 18).to_vec());
        }
        if trimmed == "app/calculator.ntzc" || trimmed == "desktop/calculator.ntzc" {
            return Ok(
                make_ntzc_pkg("nc.calculator", "Hesap Makinesi", "generic", 28, 22, 10001, 4)
                    .to_vec(),
            );
        }
        Err(FsError::NotFound)
    }
}
