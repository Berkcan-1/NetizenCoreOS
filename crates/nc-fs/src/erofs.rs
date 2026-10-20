// crates/nc-fs/src/erofs.rs
extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use nc_hal::BlockDevice;

use crate::vfs::{DirEntry, FileStat, FileSystem, FileType, FsError};

const EROFS_MAGIC: u32 = 0xE0F5_E1E2;
const EROFS_SUPERBLOCK_OFFSET: u64 = 1024;
const EROFS_INODE_FLAT_PLAIN: u16 = 0;
const EROFS_INODE_FLAT_INLINE: u16 = 1;
const MAX_EROFS_FILE_SIZE: u64 = 16 * 1024 * 1024; // H-06: 16 MiB OOM ust siniri
const MAX_EROFS_DIR_SIZE: u64 = 1024 * 1024; // 1 MiB dizin boyutu ust siniri

pub struct Erofs {
    device: Box<dyn BlockDevice>,
    block_size: usize,
    root_nid: u64,
    meta_blkaddr: u32,
    volume_bytes: u64,
}

impl Erofs {
    // H-05 & M-02: Superblock geometrisini, blkszbits sinirini ve meta_blkaddr ofsetini dogrula
    pub fn new(mut device: Box<dyn BlockDevice>) -> Result<Self, FsError> {
        let info = device.info();
        if info.block_size != 512 || info.block_count < 4 {
            return Err(FsError::InvalidSuperblock);
        }
        let volume_bytes = info
            .block_count
            .checked_mul(512)
            .ok_or(FsError::InvalidSuperblock)?;

        let mut buf = [0u8; 1024];
        if device
            .read_blocks(EROFS_SUPERBLOCK_OFFSET / 512, 2, &mut buf)
            .is_err()
        {
            return Err(FsError::DiskError);
        }

        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != EROFS_MAGIC {
            return Err(FsError::InvalidSuperblock);
        }

        let blkszbits = buf[12];
        if !(9..=16).contains(&blkszbits) {
            return Err(FsError::InvalidSuperblock);
        }
        let block_size = 1usize << blkszbits;
        let root_nid = u16::from_le_bytes([buf[14], buf[15]]) as u64;

        // M-02: Standart EROFS ofseti 0x28 (40..44), geriye donuk NetizenCore ofseti (32..36)
        let std_meta = u32::from_le_bytes([buf[40], buf[41], buf[42], buf[43]]);
        let legacy_meta = u32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]);
        let meta_blkaddr = if std_meta != 0 { std_meta } else { legacy_meta };
        if meta_blkaddr == 0 {
            return Err(FsError::InvalidSuperblock);
        }

        let meta_start = (meta_blkaddr as u64)
            .checked_mul(block_size as u64)
            .ok_or(FsError::InvalidSuperblock)?;
        if meta_start >= volume_bytes {
            return Err(FsError::InvalidSuperblock);
        }

        let mut fs = Erofs {
            device,
            block_size,
            root_nid,
            meta_blkaddr,
            volume_bytes,
        };

        // Kok dizin inode'unun gecerli ve dizin tipinde oldugunu mount sirasinda dogrula
        let root = fs.read_inode(root_nid)?;
        if !root.is_dir {
            return Err(FsError::InvalidSuperblock);
        }

        Ok(fs)
    }

    fn read_bytes(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), FsError> {
        if buffer.is_empty() {
            return Ok(());
        }
        let end_offset = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::Corrupted)?;
        if end_offset > self.volume_bytes {
            return Err(FsError::Corrupted);
        }

        let start_sector = offset / 512;
        let start_offset = (offset % 512) as usize;
        let total_bytes = start_offset
            .checked_add(buffer.len())
            .and_then(|v| v.checked_add(511))
            .ok_or(FsError::Corrupted)?;
        let sector_count = (total_bytes / 512) as u32;
        let alloc_bytes = (sector_count as usize)
            .checked_mul(512)
            .ok_or(FsError::Corrupted)?;

        let mut temp = Vec::with_capacity(alloc_bytes);
        temp.resize(alloc_bytes, 0);
        if self
            .device
            .read_blocks(start_sector, sector_count, &mut temp)
            .is_err()
        {
            return Err(FsError::DiskError);
        }
        buffer.copy_from_slice(&temp[start_offset..start_offset + buffer.len()]);
        Ok(())
    }

    fn read_inode(&mut self, nid: u64) -> Result<InternalInode, FsError> {
        let meta_base = (self.meta_blkaddr as u64)
            .checked_mul(self.block_size as u64)
            .ok_or(FsError::Corrupted)?;
        let nid_off = nid.checked_mul(32).ok_or(FsError::Corrupted)?;
        let inode_offset = meta_base.checked_add(nid_off).ok_or(FsError::Corrupted)?;

        let mut buf = [0u8; 32];
        self.read_bytes(inode_offset, &mut buf)?;

        let format = u16::from_le_bytes([buf[0], buf[1]]);
        let mode = u16::from_le_bytes([buf[4], buf[5]]);
        let size = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]) as u64;
        let raw_blkaddr = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
        let is_dir = (mode & 0o170000) == 0o040000;
        let layout = (format >> 1) & 0x07;

        let max_allowed = if is_dir {
            MAX_EROFS_DIR_SIZE
        } else {
            MAX_EROFS_FILE_SIZE
        };
        if size > max_allowed {
            return Err(FsError::Corrupted);
        }

        Ok(InternalInode {
            nid,
            size,
            is_dir,
            layout,
            raw_blkaddr,
            inode_offset,
        })
    }

    // H-06: Dosya boyutu ust sinirini ve blok carpimini dogrulayarak bellek tahsis et
    fn read_inode_data(&mut self, inode: &InternalInode) -> Result<Vec<u8>, FsError> {
        let max_allowed = if inode.is_dir {
            MAX_EROFS_DIR_SIZE
        } else {
            MAX_EROFS_FILE_SIZE
        };
        if inode.size > max_allowed {
            return Err(FsError::Corrupted);
        }
        let sz = inode.size as usize;
        let mut data = Vec::with_capacity(sz);
        data.resize(sz, 0);
        if sz == 0 {
            return Ok(data);
        }

        if inode.layout == EROFS_INODE_FLAT_PLAIN {
            let offset = (inode.raw_blkaddr as u64)
                .checked_mul(self.block_size as u64)
                .ok_or(FsError::Corrupted)?;
            self.read_bytes(offset, &mut data)?;
        } else if inode.layout == EROFS_INODE_FLAT_INLINE {
            let inline_offset = inode
                .inode_offset
                .checked_add(32)
                .ok_or(FsError::Corrupted)?;
            self.read_bytes(inline_offset, &mut data)?;
        } else {
            return Err(FsError::Corrupted);
        }
        Ok(data)
    }

    // H-06: Bozuk nameoff ve dirent tablosu tasmasi (OOB) ihtimaline karsi tam sinir kontrolu
    fn parse_directory(&self, data: &[u8]) -> Result<Vec<DirEntry>, FsError> {
        let mut entries = Vec::new();
        let mut cursor = 0usize;
        while cursor + 12 <= data.len() {
            let block_limit = cursor
                .checked_add(self.block_size)
                .unwrap_or(data.len())
                .min(data.len());
            let block_data = &data[cursor..block_limit];
            if block_data.len() < 12 {
                break;
            }

            let first_nameoff = u16::from_le_bytes([block_data[8], block_data[9]]) as usize;
            if first_nameoff < 12
                || first_nameoff > block_data.len()
                || (first_nameoff % 12) != 0
            {
                return Err(FsError::Corrupted);
            }
            let dirent_count = first_nameoff / 12;

            for i in 0..dirent_count {
                let ent_offset = i * 12;
                if ent_offset + 12 > block_data.len() {
                    return Err(FsError::Corrupted);
                }
                let nid = u64::from_le_bytes([
                    block_data[ent_offset],
                    block_data[ent_offset + 1],
                    block_data[ent_offset + 2],
                    block_data[ent_offset + 3],
                    block_data[ent_offset + 4],
                    block_data[ent_offset + 5],
                    block_data[ent_offset + 6],
                    block_data[ent_offset + 7],
                ]);
                let nameoff = u16::from_le_bytes([
                    block_data[ent_offset + 8],
                    block_data[ent_offset + 9],
                ]) as usize;
                let file_type_byte = block_data[ent_offset + 10];
                let file_type = match file_type_byte {
                    1 => FileType::RegularFile,
                    2 => FileType::Directory,
                    _ => FileType::Unknown,
                };

                let name_end = if i + 1 < dirent_count {
                    let next_off = (i + 1) * 12;
                    if next_off + 10 > block_data.len() {
                        return Err(FsError::Corrupted);
                    }
                    u16::from_le_bytes([
                        block_data[next_off + 8],
                        block_data[next_off + 9],
                    ]) as usize
                } else {
                    let mut end = nameoff.min(block_data.len());
                    while end < block_data.len() && block_data[end] != 0 {
                        end += 1;
                    }
                    end
                };

                if nameoff < first_nameoff
                    || nameoff > block_data.len()
                    || name_end > block_data.len()
                    || name_end < nameoff
                {
                    return Err(FsError::Corrupted);
                }

                let name_str =
                    core::str::from_utf8(&block_data[nameoff..name_end]).map_err(|_| FsError::Corrupted)?;
                if !name_str.is_empty() && name_str != "." && name_str != ".." {
                    entries.push(DirEntry {
                        name: String::from(name_str),
                        file_type,
                        size: 0,
                        id: nid,
                    });
                }
            }
            cursor = block_limit;
        }
        Ok(entries)
    }

    fn resolve_path(&mut self, path: &str) -> Result<InternalInode, FsError> {
        let mut current_inode = self.read_inode(self.root_nid)?;
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok(current_inode);
        }
        for segment in trimmed.split('/') {
            if segment.is_empty() {
                continue;
            }
            if !current_inode.is_dir {
                return Err(FsError::NotADirectory);
            }
            let data = self.read_inode_data(&current_inode)?;
            let entries = self.parse_directory(&data)?;
            let found = entries
                .into_iter()
                .find(|e| e.name == segment)
                .ok_or(FsError::NotFound)?;
            current_inode = self.read_inode(found.id)?;
        }
        Ok(current_inode)
    }
}

impl FileSystem for Erofs {
    fn list_dir(&mut self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let inode = self.resolve_path(path)?;
        if !inode.is_dir {
            return Err(FsError::NotADirectory);
        }
        let data = self.read_inode_data(&inode)?;
        let mut entries = self.parse_directory(&data)?;
        for entry in entries.iter_mut() {
            let target_inode = self.read_inode(entry.id)?;
            entry.size = target_inode.size;
        }
        Ok(entries)
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        let inode = self.resolve_path(path)?;
        if inode.is_dir {
            return Err(FsError::NotAFile);
        }
        self.read_inode_data(&inode)
    }

    fn file_size(&mut self, path: &str) -> Result<u64, FsError> {
        let inode = self.resolve_path(path)?;
        if inode.is_dir {
            return Err(FsError::NotAFile);
        }
        Ok(inode.size)
    }

    fn file_stat(&mut self, path: &str) -> Result<FileStat, FsError> {
        let inode = self.resolve_path(path)?;
        Ok(FileStat {
            size: if inode.is_dir { 0 } else { inode.size },
            is_dir: inode.is_dir,
            readonly: true,
        })
    }
}

struct InternalInode {
    #[allow(dead_code)]
    nid: u64,
    size: u64,
    is_dir: bool,
    layout: u16,
    raw_blkaddr: u32,
    inode_offset: u64,
}
