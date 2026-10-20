// crates/nc-fs/src/fat32.rs
extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use nc_hal::BlockDevice;

use crate::vfs::{DirEntry, FileStat, FileSystem, FileType, FsError};

const MAX_FAT32_FILE_SIZE: usize = 16 * 1024 * 1024; // 16 MiB ust sinir
const MAX_FAT32_DIR_SIZE: usize = 256 * 1024; // 256 KiB dizin ust siniri

pub struct Fat32 {
    device: Box<dyn BlockDevice>,
    #[allow(dead_code)]
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    cluster_size: u32,
    fat_start_lba: u64,
    fat_sectors: u64,
    data_start_lba: u64,
    root_cluster: u32,
    max_cluster: u32,
}

impl Fat32 {
    // H-05 & M-06: BPB toplam sektorlerini, FAT adedini ve cluster ust sinirini dogrula
    pub fn new(mut device: Box<dyn BlockDevice>) -> Result<Self, FsError> {
        let dev_info = device.info();
        if dev_info.block_size != 512 || dev_info.block_count < 32 {
            return Err(FsError::InvalidSuperblock);
        }

        let mut bpb = [0u8; 512];
        if device.read_blocks(0, 1, &mut bpb).is_err() {
            return Err(FsError::DiskError);
        }
        if bpb[510] != 0x55 || bpb[511] != 0xAA {
            return Err(FsError::InvalidSuperblock);
        }

        let bytes_per_sector = u16::from_le_bytes([bpb[11], bpb[12]]) as u32;
        let sectors_per_cluster = bpb[13] as u32;
        if bytes_per_sector != 512
            || sectors_per_cluster == 0
            || !sectors_per_cluster.is_power_of_two()
            || sectors_per_cluster > 128
        {
            return Err(FsError::InvalidSuperblock);
        }

        let reserved_sectors = u16::from_le_bytes([bpb[14], bpb[15]]) as u64;
        let num_fats = bpb[16] as u64;
        let root_ent_cnt = u16::from_le_bytes([bpb[17], bpb[18]]);
        let tot_sec16 = u16::from_le_bytes([bpb[19], bpb[20]]) as u64;
        let tot_sec32 = u32::from_le_bytes([bpb[32], bpb[33], bpb[34], bpb[35]]) as u64;
        let fat_sz32 = u32::from_le_bytes([bpb[36], bpb[37], bpb[38], bpb[39]]) as u64;
        let root_cluster = u32::from_le_bytes([bpb[44], bpb[45], bpb[46], bpb[47]]) & 0x0FFF_FFFF;

        let total_sectors = if tot_sec32 != 0 { tot_sec32 } else { tot_sec16 };

        if reserved_sectors == 0
            || num_fats == 0
            || num_fats > 4
            || root_ent_cnt != 0
            || fat_sz32 == 0
            || total_sectors == 0
            || total_sectors > dev_info.block_count
        {
            return Err(FsError::InvalidSuperblock);
        }

        let fat_total_sectors = num_fats
            .checked_mul(fat_sz32)
            .ok_or(FsError::InvalidSuperblock)?;
        let data_start_lba = reserved_sectors
            .checked_add(fat_total_sectors)
            .ok_or(FsError::InvalidSuperblock)?;
        if data_start_lba >= total_sectors {
            return Err(FsError::InvalidSuperblock);
        }

        let data_sectors = total_sectors - data_start_lba;
        let total_clusters = (data_sectors / sectors_per_cluster as u64)
            .min(0x0FFF_FFF0) as u32;
        if total_clusters == 0 {
            return Err(FsError::InvalidSuperblock);
        }

        let max_cluster = total_clusters.saturating_add(2);
        if root_cluster < 2 || root_cluster >= max_cluster {
            return Err(FsError::InvalidSuperblock);
        }

        Ok(Fat32 {
            device,
            bytes_per_sector,
            sectors_per_cluster,
            cluster_size: bytes_per_sector * sectors_per_cluster,
            fat_start_lba: reserved_sectors,
            fat_sectors: fat_sz32,
            data_start_lba,
            root_cluster,
            max_cluster,
        })
    }

    fn cluster_to_lba(&self, cluster: u32) -> Result<u64, FsError> {
        if cluster < 2 || cluster >= self.max_cluster {
            return Err(FsError::Corrupted);
        }
        let offset_sectors = ((cluster - 2) as u64)
            .checked_mul(self.sectors_per_cluster as u64)
            .ok_or(FsError::Corrupted)?;
        self.data_start_lba
            .checked_add(offset_sectors)
            .ok_or(FsError::Corrupted)
    }

    fn next_cluster(&mut self, cluster: u32) -> Result<u32, FsError> {
        if cluster < 2 || cluster >= self.max_cluster {
            return Err(FsError::Corrupted);
        }
        let fat_offset = (cluster as u64) * 4;
        let sec_offset = fat_offset / 512;
        if sec_offset >= self.fat_sectors {
            return Err(FsError::Corrupted);
        }
        let sec = self
            .fat_start_lba
            .checked_add(sec_offset)
            .ok_or(FsError::Corrupted)?;
        let off = (fat_offset % 512) as usize;
        let mut buf = [0u8; 512];
        if self.device.read_blocks(sec, 1, &mut buf).is_err() {
            return Err(FsError::DiskError);
        }
        let val =
            u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) & 0x0FFF_FFFF;
        Ok(val)
    }

    // H-07: Okuma veya FAT zinciri hatalarini asla kismi basari (Ok) olarak dondurme
    fn read_chain(
        &mut self,
        start_cluster: u32,
        target_bytes: usize,
        exact_size: bool,
    ) -> Result<Vec<u8>, FsError> {
        if target_bytes == 0 {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity(if exact_size {
            target_bytes
        } else {
            target_bytes.min(self.cluster_size as usize)
        });
        let mut cur = start_cluster;
        let mut steps = 0u32;
        let csize = self.cluster_size as usize;
        let mut cbuf = alloc::vec![0u8; csize];

        while out.len() < target_bytes {
            if cur < 2 || cur >= self.max_cluster || steps >= self.max_cluster {
                return Err(FsError::Corrupted);
            }
            steps += 1;
            let lba = self.cluster_to_lba(cur)?;
            if self
                .device
                .read_blocks(lba, self.sectors_per_cluster, &mut cbuf)
                .is_err()
            {
                return Err(FsError::DiskError);
            }
            let remain = target_bytes - out.len();
            let take = remain.min(csize);
            out.extend_from_slice(&cbuf[..take]);
            if out.len() >= target_bytes {
                break;
            }
            let next = self.next_cluster(cur)?;
            if next >= 0x0FFF_FFF8 {
                if exact_size {
                    return Err(FsError::Corrupted);
                }
                break;
            }
            if next == 0x0FFF_FFF7 || next < 2 {
                return Err(FsError::Corrupted);
            }
            cur = next;
        }
        Ok(out)
    }

    fn parse_dir(&self, data: &[u8]) -> Vec<(DirEntry, u32)> {
        let mut list = Vec::new();
        let mut lfn_parts: Vec<(u8, String)> = Vec::new();
        let mut i = 0usize;
        while i + 32 <= data.len() {
            let ent = &data[i..i + 32];
            i += 32;
            if ent[0] == 0x00 {
                break;
            }
            if ent[0] == 0xE5 {
                lfn_parts.clear();
                continue;
            }
            let attr = ent[11];
            if attr == 0x0F {
                let ord = ent[0] & 0x1F;
                let mut part = String::new();
                for &pos in &[1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30] {
                    let ch_u16 = u16::from_le_bytes([ent[pos], ent[pos + 1]]);
                    if ch_u16 == 0 || ch_u16 == 0xFFFF {
                        break;
                    }
                    if let Some(c) = char::from_u32(ch_u16 as u32) {
                        part.push(c);
                    }
                }
                lfn_parts.push((ord, part));
                continue;
            }
            if (attr & 0x08) != 0 {
                lfn_parts.clear();
                continue;
            }
            let name = if !lfn_parts.is_empty() {
                lfn_parts.sort_by_key(|k| k.0);
                let mut full = String::new();
                for (_, p) in &lfn_parts {
                    full.push_str(p);
                }
                lfn_parts.clear();
                full
            } else {
                let base = core::str::from_utf8(&ent[0..8]).unwrap_or("").trim();
                let ext = core::str::from_utf8(&ent[8..11]).unwrap_or("").trim();
                if ext.is_empty() {
                    base.to_ascii_lowercase()
                } else {
                    alloc::format!("{}.{}", base.to_ascii_lowercase(), ext.to_ascii_lowercase())
                }
            };
            if name == "." || name == ".." || name.is_empty() {
                continue;
            }
            let hi = u16::from_le_bytes([ent[20], ent[21]]) as u32;
            let lo = u16::from_le_bytes([ent[26], ent[27]]) as u32;
            let cluster = ((hi << 16) | lo) & 0x0FFF_FFFF;
            let size = u32::from_le_bytes([ent[28], ent[29], ent[30], ent[31]]) as u64;
            let is_dir = (attr & 0x10) != 0;
            list.push((
                DirEntry {
                    name,
                    file_type: if is_dir {
                        FileType::Directory
                    } else {
                        FileType::RegularFile
                    },
                    size,
                    id: cluster as u64,
                },
                cluster,
            ));
        }
        list
    }

    fn resolve(&mut self, path: &str) -> Result<(u32, u64, bool), FsError> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok((self.root_cluster, MAX_FAT32_DIR_SIZE as u64, true));
        }
        let mut cur_cluster = self.root_cluster;
        let mut cur_size = MAX_FAT32_DIR_SIZE as u64;
        let mut is_dir = true;
        for seg in trimmed.split('/') {
            if seg.is_empty() {
                continue;
            }
            if !is_dir {
                return Err(FsError::NotADirectory);
            }
            let raw = self.read_chain(cur_cluster, MAX_FAT32_DIR_SIZE, false)?;
            let entries = self.parse_dir(&raw);
            let found = entries
                .into_iter()
                .find(|(e, _)| e.name.eq_ignore_ascii_case(seg))
                .ok_or(FsError::NotFound)?;
            cur_cluster = found.1;
            cur_size = found.0.size;
            is_dir = found.0.file_type == FileType::Directory;
        }
        Ok((cur_cluster, cur_size, is_dir))
    }
}

impl FileSystem for Fat32 {
    fn list_dir(&mut self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let (cl, _, is_dir) = self.resolve(path)?;
        if !is_dir {
            return Err(FsError::NotADirectory);
        }
        let raw = self.read_chain(cl, MAX_FAT32_DIR_SIZE, false)?;
        Ok(self.parse_dir(&raw).into_iter().map(|(e, _)| e).collect())
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        let (cl, size, is_dir) = self.resolve(path)?;
        if is_dir {
            return Err(FsError::NotAFile);
        }
        if size == 0 {
            return Ok(Vec::new());
        }
        if size as usize > MAX_FAT32_FILE_SIZE {
            return Err(FsError::StorageFull);
        }
        self.read_chain(cl, size as usize, true)
    }

    // M-06: FAT32 surucusu salt okunur oldugundan readonly: true bildir
    fn file_stat(&mut self, path: &str) -> Result<FileStat, FsError> {
        let (_, size, is_dir) = self.resolve(path)?;
        Ok(FileStat {
            size: if is_dir { 0 } else { size },
            is_dir,
            readonly: true,
        })
    }
}
