// crates/nc-fs/src/lib.rs
#![no_std]

extern crate alloc;

pub mod erofs;
pub mod exfat;
pub mod memfs;
pub mod vfs;
pub mod fat32;


use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

pub use erofs::Erofs;
pub use exfat::Exfat;
pub use fat32::Fat32;
pub use memfs::MemFs;
pub use vfs::{DirEntry, FileStat, FileSystem, FileType, FsError};

use nc_storage::{
    find_external_usb_disk, make_ntzc_pkg, make_sample_mp4, make_sample_png, scan_channel,
    AtaPartition,
};

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
    data_fs: Option<Exfat>,
    usb_fs: Option<Box<dyn FileSystem>>,
    usb_model: String,
    usb_total_mb: u64,
    ram_fs: MemFs,
}

impl VfsManager {
    pub fn new() -> Self {
        let mut system_fs = None;
        let mut data_fs = None;

        if let Some(ata) = nc_storage::find_installed_disk().or_else(|| scan_channel(0)) {
            let part1 = AtaPartition::new(ata, 133_120, 4_194_304);
            if let Ok(fs) = Erofs::new(Box::new(part1)) {
                system_fs = Some(fs);
            }

            let part3 = AtaPartition::new(ata, 4_851_712, 2_097_152);
            if let Ok(mut fs) = Exfat::new(Box::new(part3)) {
                Self::init_locale_once(&mut fs);
                data_fs = Some(fs);
            }
        }

        let mut vfs = VfsManager {
            system_fs,
            data_fs,
            usb_fs: None,
            usb_model: String::from("Takili Degil"),
            usb_total_mb: 0,
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

   pub fn rescan_usb(&mut self) -> bool {
        if let Some((dev, start_lba, sectors)) = find_external_usb_disk() {
            let model = String::from(dev.info.model_str());
            let total_mb = (sectors * 512 / (1024 * 1024)).max(16);
            let part = AtaPartition::new(dev, start_lba, sectors);
            if let Ok(fs) = Exfat::new(Box::new(part)) {
                self.usb_fs = Some(Box::new(fs));
                self.usb_model = model;
                self.usb_total_mb = total_mb;
                return true;
            }
            let part2 = AtaPartition::new(dev, start_lba, sectors);
            if let Ok(fs) = Fat32::new(Box::new(part2)) {
                self.usb_fs = Some(Box::new(fs));
                self.usb_model = model;
                self.usb_total_mb = total_mb;
                return true;
            }
        }
        self.usb_fs = None;
        self.usb_model = String::from("Takili Degil");
        self.usb_total_mb = 0;
        false
    }

    pub fn is_usb_online(&self) -> bool {
        self.usb_fs.is_some()
    }

    pub fn usb_label(&self) -> &str {
        &self.usb_model
    }

        pub fn partition_metrics(&mut self) -> Vec<PartitionMetrics> {
             self.rescan_usb();
        let sys_online = self.system_fs.is_some();
        let data_online = self.data_fs.is_some();
        let usb_online = self.usb_fs.is_some();
        let data_used_mb = if let Some(ref mut dfs) = self.data_fs {
            ((dfs.used_clusters_count() as u64) * 4 / 1024).max(16)
        } else {
            24
        };
        let usb_used_mb = if self.usb_fs.is_some() { 4 } else { 0 };


        alloc::vec![
            PartitionMetrics {
                mount_point: "/boot",
                label: "Bolum 1: Stage2 + Cekirdek",
                fs_type: "RAW Binary",
                start_lba: 2048,
                sector_count: 131_072,
                total_mb: 64,
                used_mb: 8,
                readonly: true,
                online: true,
            },
            PartitionMetrics {
                mount_point: "/system",
                label: "Bolum 2: Sistem Agaci",
                fs_type: "EROFS",
                start_lba: 133_120,
                sector_count: 4_194_304,
                total_mb: 2048,
                used_mb: 384,
                readonly: true,
                online: sys_online,
            },
            PartitionMetrics {
                mount_point: "/data",
                label: "Bolum 4: Kisisel Disk",
                fs_type: "exFAT",
                start_lba: 4_851_712,
                sector_count: 2_097_152,
                total_mb: 1024,
                used_mb: data_used_mb,
                readonly: false,
                online: data_online,
            },
            PartitionMetrics {
                mount_point: "/usb",
                label: "Harici USB / SSD Surucu",
                fs_type: "exFAT",
                start_lba: 0,
                sector_count: self.usb_total_mb * 2048,
                total_mb: self.usb_total_mb.max(64),
                used_mb: usb_used_mb,
                readonly: false,
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

    pub fn list_dir_path(&mut self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let (mount, rel) = self.split_mount_path(path);
        if mount.is_empty() {
            let mut root_list = alloc::vec![
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
            ];
            let _ = &mut root_list;
            return Ok(root_list);
        }
        match mount {
            "system" => {
                if let Some(ref mut fs) = self.system_fs {
                    if let Ok(res) = fs.list_dir(rel) {
                        return Ok(res);
                    }
                }
                Self::fallback_dir(PartitionTarget::System, rel)
            }
            "data" => {
                if let Some(ref mut fs) = self.data_fs {
                    if let Ok(res) = fs.list_dir(rel) {
                        return Ok(res);
                    }
                }
                Self::fallback_dir(PartitionTarget::Data, rel)
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

    pub fn read_file_path(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        let (mount, rel) = self.split_mount_path(path);
        match mount {
            "system" => {
                if let Some(ref mut fs) = self.system_fs {
                    if let Ok(res) = fs.read_file(rel) {
                        return Ok(res);
                    }
                }
                Self::fallback_read(PartitionTarget::System, rel)
            }
            "data" => {
                if let Some(ref mut fs) = self.data_fs {
                    if let Ok(res) = fs.read_file(rel) {
                        return Ok(res);
                    }
                }
                Self::fallback_read(PartitionTarget::Data, rel)
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
            "system" => Ok(FileStat {
                size: self.read_file_path(path).map(|v| v.len() as u64).unwrap_or(0),
                is_dir: self.list_dir_path(path).is_ok(),
                readonly: true,
            }),
            "data" => {
                if let Some(ref mut fs) = self.data_fs {
                    fs.file_stat(rel)
                } else {
                    Ok(FileStat {
                        size: self.read_file_path(path).map(|v| v.len() as u64).unwrap_or(0),
                        is_dir: self.list_dir_path(path).is_ok(),
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

    pub fn list_dir(&mut self, target: PartitionTarget, path: &str) -> Result<Vec<DirEntry>, FsError> {
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
                for (id, name) in [(2, "desktop"), (3, "documents"), (4, "downloads"), (5, "media")] {
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
        } else {
            return Err(FsError::NotFound);
        }
        Ok(list)
    }

    fn fallback_read(_target: PartitionTarget, path: &str) -> Result<Vec<u8>, FsError> {
        let trimmed = path.trim_matches('/');
        if trimmed == "build.prop" {
            return Ok(b"ro.netizen.os.name=NetizenCore\nro.netizen.os.version=2.8.0\n".to_vec());
        }
        if trimmed.ends_with(".png") {
            return Ok(make_sample_png(0).to_vec());
        }
        if trimmed.ends_with(".mp4") {
            return Ok(make_sample_mp4(0, 18).to_vec());
        }
        if trimmed.ends_with(".ntzc") {
            return Ok(make_ntzc_pkg("nc.calculator", "Hesap Makinesi", "generic", 28, 22, 10001, 4).to_vec());
        }
        Err(FsError::NotFound)
    }
}
