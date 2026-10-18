// crates/nc-fs/src/fat32.rs
extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use nc_hal::BlockDevice;

use crate::vfs::{DirEntry, FileStat, FileSystem, FileType, FsError};

pub struct Fat32 {
    device: Box<dyn BlockDevice>,
    #[allow(dead_code)]
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    cluster_size: u32,
    fat_start_lba: u64,
    data_start_lba: u64,
    root_cluster: u32,
}

impl Fat32 {
    pub fn new(mut device: Box<dyn BlockDevice>) -> Result<Self, FsError> {
        let mut bpb = [0u8; 512];
        if device.read_blocks(0, 1, &mut bpb).is_err() {
            return Err(FsError::DiskError);
        }
        if bpb[510] != 0x55 || bpb[511] != 0xAA {
            return Err(FsError::InvalidSuperblock);
        }
        let bytes_per_sector = u16::from_le_bytes([bpb[11], bpb[12]]) as u32;
        let sectors_per_cluster = bpb[13] as u32;
        if bytes_per_sector != 512 || sectors_per_cluster == 0 {
            return Err(FsError::InvalidSuperblock);
        }
        let reserved_sectors = u16::from_le_bytes([bpb[14], bpb[15]]) as u64;
        let num_fats = bpb[16] as u64;
        let fat_sz32 = u32::from_le_bytes([bpb[36], bpb[37], bpb[38], bpb[39]]) as u64;
        let root_cluster = u32::from_le_bytes([bpb[44], bpb[45], bpb[46], bpb[47]]);

        if fat_sz32 == 0 || root_cluster < 2 {
            return Err(FsError::InvalidSuperblock);
        }

        let fat_start_lba = reserved_sectors;
        let data_start_lba = reserved_sectors + (num_fats * fat_sz32);

        Ok(Fat32 {
            device,
            bytes_per_sector,
            sectors_per_cluster,
            cluster_size: bytes_per_sector * sectors_per_cluster,
            fat_start_lba,
            data_start_lba,
            root_cluster,
        })
    }

    fn cluster_to_lba(&self, cluster: u32) -> u64 {
        self.data_start_lba + ((cluster as u64 - 2) * self.sectors_per_cluster as u64)
    }

    fn next_cluster(&mut self, cluster: u32) -> Result<u32, FsError> {
        let fat_offset = cluster as u64 * 4;
        let sec = self.fat_start_lba + (fat_offset / 512);
        let off = (fat_offset % 512) as usize;
        let mut buf = [0u8; 512];
        if self.device.read_blocks(sec, 1, &mut buf).is_err() {
            return Err(FsError::DiskError);
        }
        let val = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) & 0x0FFF_FFFF;
        Ok(val)
    }

    fn read_chain(&mut self, start_cluster: u32, max_bytes: usize) -> Result<Vec<u8>, FsError> {
        let mut out = Vec::new();
        let mut cur = start_cluster;
        let mut steps = 0usize;
        let csize = self.cluster_size as usize;
        let mut cbuf = alloc::vec![0u8; csize];

        while cur >= 2 && cur < 0x0FFF_FFF8 && steps < 512 {
            steps += 1;
            let lba = self.cluster_to_lba(cur);
            if self.device.read_blocks(lba, self.sectors_per_cluster, &mut cbuf).is_err() {
                break;
            }
            let remain = max_bytes.saturating_sub(out.len());
            let take = remain.min(csize);
            out.extend_from_slice(&cbuf[..take]);
            if out.len() >= max_bytes {
                break;
            }
            cur = match self.next_cluster(cur) {
                Ok(n) => n,
                Err(_) => break,
            };
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
            let cluster = (hi << 16) | lo;
            let size = u32::from_le_bytes([ent[28], ent[29], ent[30], ent[31]]) as u64;
            let is_dir = (attr & 0x10) != 0;

            list.push((
                DirEntry {
                    name,
                    file_type: if is_dir { FileType::Directory } else { FileType::RegularFile },
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
            return Ok((self.root_cluster, 65536, true));
        }
        let mut cur_cluster = self.root_cluster;
        let mut cur_size = 65536u64;
        let mut is_dir = true;

        for seg in trimmed.split('/') {
            if seg.is_empty() {
                continue;
            }
            if !is_dir {
                return Err(FsError::NotADirectory);
            }
            let raw = self.read_chain(cur_cluster, 65536)?;
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
        let raw = self.read_chain(cl, 65536)?;
        Ok(self.parse_dir(&raw).into_iter().map(|(e, _)| e).collect())
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        let (cl, size, is_dir) = self.resolve(path)?;
        if is_dir {
            return Err(FsError::NotAFile);
        }
        self.read_chain(cl, (size as usize).min(512 * 1024))
    }

    fn file_stat(&mut self, path: &str) -> Result<FileStat, FsError> {
        let (_, size, is_dir) = self.resolve(path)?;
        Ok(FileStat {
            size,
            is_dir,
            readonly: false,
        })
    }
}