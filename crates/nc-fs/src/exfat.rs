extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use nc_hal::BlockDevice;

use crate::vfs::{DirEntry, FileStat, FileSystem, FileType, FsError};

const EXFAT_BITMAP_ENTRY: u8 = 0x81;
const EXFAT_FILE_ENTRY: u8 = 0x85;
const EXFAT_STREAM_ENTRY: u8 = 0xC0;
const EXFAT_NAME_ENTRY: u8 = 0xC1;
const EXFAT_ATTR_DIRECTORY: u16 = 0x0010;
const MAX_CLUSTERS_LIMIT: u32 = 65_536;
const MAX_FILE_SIZE_BYTES: u64 = 16 * 1024 * 1024; // Kernel heap OOM korumasi: 16 MiB ust sinir

pub struct Exfat {
    device: Box<dyn BlockDevice>,
    #[allow(dead_code)]
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    cluster_size: u32,
    fat_offset_lba: u64,
    cluster_heap_offset_lba: u64,
    root_dir_cluster: u32,
    max_cluster: u32,
    bitmap_first_cluster: Option<u32>,
    bitmap_size: u64,
}

struct RawEntryLoc {
    byte_offset: usize,
    total_bytes: usize,
    first_cluster: u32,
    no_fat_chain: bool,
    size: u64,
    is_dir: bool,
    name: String,
}

impl Exfat {
    // H-05 & C-06: VBR geometrisini, shift sinirlarini, bolum kapasitesini ve Allocation Bitmap'i dogrula
    pub fn new(mut device: Box<dyn BlockDevice>) -> Result<Self, FsError> {
        let dev_info = device.info();
        if dev_info.block_size != 512 || dev_info.block_count < 16 {
            return Err(FsError::InvalidSuperblock);
        }

        let mut vbr = [0u8; 512];
        if device.read_blocks(0, 1, &mut vbr).is_err() {
            return Err(FsError::DiskError);
        }
        if &vbr[3..11] != b"EXFAT   " || vbr[510] != 0x55 || vbr[511] != 0xAA {
            return Err(FsError::InvalidSuperblock);
        }

        let bytes_per_sector_shift = vbr[108];
        let sectors_per_cluster_shift = vbr[109];
        // Sadece 512 bayt sektor (shift=9) ve en fazla 256 KB cluster (shift<=9) kabul et
        if bytes_per_sector_shift != 9 || sectors_per_cluster_shift > 9 {
            return Err(FsError::InvalidSuperblock);
        }

        let bytes_per_sector = 1u32 << bytes_per_sector_shift;
        let sectors_per_cluster = 1u32 << sectors_per_cluster_shift;
        let cluster_size = bytes_per_sector
            .checked_mul(sectors_per_cluster)
            .ok_or(FsError::InvalidSuperblock)?;

        let fat_offset_lba = u32::from_le_bytes([vbr[80], vbr[81], vbr[82], vbr[83]]) as u64;
        let fat_length_sectors = u32::from_le_bytes([vbr[84], vbr[85], vbr[86], vbr[87]]) as u64;
        let cluster_heap_offset_lba = u32::from_le_bytes([vbr[88], vbr[89], vbr[90], vbr[91]]) as u64;
        let vbr_cluster_count = u32::from_le_bytes([vbr[92], vbr[93], vbr[94], vbr[95]]);
        let root_dir_cluster = u32::from_le_bytes([vbr[96], vbr[97], vbr[98], vbr[99]]);

        if fat_offset_lba == 0
            || cluster_heap_offset_lba <= fat_offset_lba
            || cluster_heap_offset_lba >= dev_info.block_count
            || root_dir_cluster < 2
        {
            return Err(FsError::InvalidSuperblock);
        }

        let heap_sectors = dev_info.block_count - cluster_heap_offset_lba;
        let possible_clusters = (heap_sectors / sectors_per_cluster as u64)
            .min(MAX_CLUSTERS_LIMIT as u64) as u32;
        if possible_clusters < 2 {
            return Err(FsError::InvalidSuperblock);
        }

        let fat_avail_sectors = if fat_length_sectors > 0 {
            fat_length_sectors.min(cluster_heap_offset_lba - fat_offset_lba)
        } else {
            cluster_heap_offset_lba - fat_offset_lba
        };
        let fat_max_entries = (fat_avail_sectors * 128).min(MAX_CLUSTERS_LIMIT as u64) as u32;

        let mut max_cluster = (possible_clusters + 2).min(fat_max_entries);
        if vbr_cluster_count > 0 {
            max_cluster = max_cluster.min(vbr_cluster_count.saturating_add(2));
        }
        if root_dir_cluster >= max_cluster {
            return Err(FsError::InvalidSuperblock);
        }

        let mut fs = Exfat {
            device,
            bytes_per_sector,
            sectors_per_cluster,
            cluster_size,
            fat_offset_lba,
            cluster_heap_offset_lba,
            root_dir_cluster,
            max_cluster,
            bitmap_first_cluster: None,
            bitmap_size: 0,
        };

        // C-06: Kok dizinde standart exFAT Allocation Bitmap (0x81) varsa konumunu kaydet
        if let Ok(root_buf) = fs.read_single_cluster(root_dir_cluster) {
            let mut off = 0usize;
            while off + 32 <= root_buf.len() {
                let etype = root_buf[off];
                if etype == 0x00 {
                    break;
                }
                if etype == EXFAT_BITMAP_ENTRY {
                    let b_cluster = u32::from_le_bytes([
                        root_buf[off + 20],
                        root_buf[off + 21],
                        root_buf[off + 22],
                        root_buf[off + 23],
                    ]);
                    let b_size = u64::from_le_bytes([
                        root_buf[off + 24],
                        root_buf[off + 25],
                        root_buf[off + 26],
                        root_buf[off + 27],
                        root_buf[off + 28],
                        root_buf[off + 29],
                        root_buf[off + 30],
                        root_buf[off + 31],
                    ]);
                    if b_cluster >= 2 && b_cluster < fs.max_cluster && b_size > 0 {
                        fs.bitmap_first_cluster = Some(b_cluster);
                        fs.bitmap_size = b_size;
                    }
                    break;
                }
                off += 32;
            }
        }

        Ok(fs)
    }

    fn cluster_to_lba(&self, cluster: u32) -> Result<u64, FsError> {
        if cluster < 2 || cluster >= self.max_cluster {
            return Err(FsError::Corrupted);
        }
        let cluster_idx = (cluster - 2) as u64;
        let sector_off = cluster_idx
            .checked_mul(self.sectors_per_cluster as u64)
            .ok_or(FsError::Corrupted)?;
        self.cluster_heap_offset_lba
            .checked_add(sector_off)
            .ok_or(FsError::Corrupted)
    }

    fn read_fat_entry(&mut self, cluster: u32) -> Result<u32, FsError> {
        if cluster >= self.max_cluster {
            return Err(FsError::Corrupted);
        }
        let fat_entry_offset = (cluster as u64) * 4;
        let sector_offset = fat_entry_offset / 512;
        let byte_in_sector = (fat_entry_offset % 512) as usize;
        let lba = self
            .fat_offset_lba
            .checked_add(sector_offset)
            .ok_or(FsError::Corrupted)?;
        if lba >= self.cluster_heap_offset_lba {
            return Err(FsError::Corrupted);
        }

        let mut buf = [0u8; 512];
        if self.device.read_blocks(lba, 1, &mut buf).is_err() {
            return Err(FsError::DiskError);
        }
        Ok(u32::from_le_bytes([
            buf[byte_in_sector],
            buf[byte_in_sector + 1],
            buf[byte_in_sector + 2],
            buf[byte_in_sector + 3],
        ]))
    }

    fn write_fat_entry(&mut self, cluster: u32, value: u32) -> Result<(), FsError> {
        if cluster < 2 || cluster >= self.max_cluster {
            return Err(FsError::Corrupted);
        }
        let fat_entry_offset = (cluster as u64) * 4;
        let sector_offset = fat_entry_offset / 512;
        let byte_in_sector = (fat_entry_offset % 512) as usize;
        let lba = self
            .fat_offset_lba
            .checked_add(sector_offset)
            .ok_or(FsError::Corrupted)?;
        if lba >= self.cluster_heap_offset_lba {
            return Err(FsError::Corrupted);
        }

        let mut buf = [0u8; 512];
        if self.device.read_blocks(lba, 1, &mut buf).is_err() {
            return Err(FsError::DiskError);
        }
        buf[byte_in_sector..byte_in_sector + 4].copy_from_slice(&value.to_le_bytes());
        if self.device.write_blocks(lba, 1, &buf).is_err() {
            return Err(FsError::DiskError);
        }
        Ok(())
    }

    // C-06: Standart exFAT Allocation Bitmap uzerinde kumenin dolu olup olmadigini kontrol et
    fn is_bitmap_cluster_allocated(&mut self, cluster: u32) -> Result<bool, FsError> {
        let bmp_cluster = match self.bitmap_first_cluster {
            Some(c) => c,
            None => return Ok(false),
        };
        if cluster < 2 || cluster >= self.max_cluster {
            return Ok(true);
        }
        let bit_index = (cluster - 2) as u64;
        let byte_index = bit_index / 8;
        if byte_index >= self.bitmap_size {
            return Ok(true);
        }
        let bit_offset = (bit_index % 8) as u8;
        let bmp_base_lba = self.cluster_to_lba(bmp_cluster)?;
        let sector_lba = bmp_base_lba
            .checked_add(byte_index / 512)
            .ok_or(FsError::Corrupted)?;
        let off_in_sec = (byte_index % 512) as usize;

        let mut sec = [0u8; 512];
        if self.device.read_blocks(sector_lba, 1, &mut sec).is_err() {
            return Err(FsError::DiskError);
        }
        Ok((sec[off_in_sec] & (1 << bit_offset)) != 0)
    }

    // C-06: Standart exFAT Allocation Bitmap bitini guncelle
    fn set_bitmap_cluster(&mut self, cluster: u32, allocated: bool) -> Result<(), FsError> {
        let bmp_cluster = match self.bitmap_first_cluster {
            Some(c) => c,
            None => return Ok(()),
        };
        if cluster < 2 || cluster >= self.max_cluster {
            return Err(FsError::Corrupted);
        }
        let bit_index = (cluster - 2) as u64;
        let byte_index = bit_index / 8;
        if byte_index >= self.bitmap_size {
            return Err(FsError::StorageFull);
        }
        let bit_offset = (bit_index % 8) as u8;
        let bmp_base_lba = self.cluster_to_lba(bmp_cluster)?;
        let sector_lba = bmp_base_lba
            .checked_add(byte_index / 512)
            .ok_or(FsError::Corrupted)?;
        let off_in_sec = (byte_index % 512) as usize;

        let mut sec = [0u8; 512];
        if self.device.read_blocks(sector_lba, 1, &mut sec).is_err() {
            return Err(FsError::DiskError);
        }
        if allocated {
            sec[off_in_sec] |= 1 << bit_offset;
        } else {
            sec[off_in_sec] &= !(1 << bit_offset);
        }
        if self.device.write_blocks(sector_lba, 1, &sec).is_err() {
            return Err(FsError::DiskError);
        }
        Ok(())
    }

    // H-03: NoFatChain dosyalarin tum kumelerini boyutuna gore serbest birak
    fn free_cluster_chain(&mut self, start_cluster: u32, no_fat_chain: bool, size_bytes: u64) -> Result<(), FsError> {
        if start_cluster < 2 || start_cluster >= self.max_cluster {
            return Ok(());
        }
        if no_fat_chain {
            let csize = self.cluster_size as u64;
            let count = ((size_bytes.saturating_add(csize - 1)) / csize).max(1) as u32;
            for i in 0..count {
                let cl = start_cluster.saturating_add(i);
                if cl >= self.max_cluster {
                    break;
                }
                let _ = self.write_fat_entry(cl, 0);
                let _ = self.set_bitmap_cluster(cl, false);
            }
            return Ok(());
        }

        let mut curr = start_cluster;
        let mut steps = 0u32;
        while curr >= 2 && curr < self.max_cluster && steps < self.max_cluster {
            let next = self.read_fat_entry(curr)?;
            self.write_fat_entry(curr, 0)?;
            let _ = self.set_bitmap_cluster(curr, false);
            if next < 2 || next >= 0xFFFF_FFF7 {
                break;
            }
            curr = next;
            steps += 1;
        }
        Ok(())
    }

    // C-06 & H-02: Hem FAT hem Allocation Bitmap'i kontrol et; hata aninda ayrilan kumeleri rollback yap
    fn allocate_clusters(&mut self, count: usize) -> Result<Vec<u32>, FsError> {
        let needed = count.max(1);
        let mut allocated = Vec::with_capacity(needed);
        let bmp_cluster = self.bitmap_first_cluster.unwrap_or(0);

        for candidate in 3..self.max_cluster {
            if candidate == self.root_dir_cluster || candidate == bmp_cluster {
                continue;
            }
            let fat_val = self.read_fat_entry(candidate)?;
            if fat_val == 0 && !self.is_bitmap_cluster_allocated(candidate)? {
                allocated.push(candidate);
                if allocated.len() == needed {
                    break;
                }
            }
        }
        if allocated.len() < needed {
            return Err(FsError::StorageFull);
        }

        for i in 0..allocated.len() {
            let next_val = if i + 1 < allocated.len() {
                allocated[i + 1]
            } else {
                0xFFFF_FFFF
            };
            if let Err(e) = self
                .write_fat_entry(allocated[i], next_val)
                .and_then(|_| self.set_bitmap_cluster(allocated[i], true))
            {
                // Hata olusursa o ana kadar ayrilan kumeleri geri al
                for &cl in &allocated[..=i] {
                    let _ = self.write_fat_entry(cl, 0);
                    let _ = self.set_bitmap_cluster(cl, false);
                }
                return Err(e);
            }
        }
        Ok(allocated)
    }

    fn write_data_to_clusters(&mut self, clusters: &[u32], data: &[u8]) -> Result<(), FsError> {
        let csize = self.cluster_size as usize;
        let mut cluster_buf = Vec::with_capacity(csize);
        cluster_buf.resize(csize, 0);
        for (idx, &cl) in clusters.iter().enumerate() {
            for b in cluster_buf.iter_mut() {
                *b = 0;
            }
            let start = idx * csize;
            if start < data.len() {
                let copy_len = (data.len() - start).min(csize);
                cluster_buf[..copy_len].copy_from_slice(&data[start..start + copy_len]);
            }
            let lba = self.cluster_to_lba(cl)?;
            if self
                .device
                .write_blocks(lba, self.sectors_per_cluster, &cluster_buf)
                .is_err()
            {
                return Err(FsError::DiskError);
            }
        }
        Ok(())
    }

    fn read_single_cluster(&mut self, cluster: u32) -> Result<Vec<u8>, FsError> {
        let csize = self.cluster_size as usize;
        let mut buf = Vec::with_capacity(csize);
        buf.resize(csize, 0);
        let lba = self.cluster_to_lba(cluster)?;
        if self
            .device
            .read_blocks(lba, self.sectors_per_cluster, &mut buf)
            .is_err()
        {
            return Err(FsError::DiskError);
        }
        Ok(buf)
    }

    fn write_single_cluster(&mut self, cluster: u32, buf: &[u8]) -> Result<(), FsError> {
        let lba = self.cluster_to_lba(cluster)?;
        if self
            .device
            .write_blocks(lba, self.sectors_per_cluster, buf)
            .is_err()
        {
            return Err(FsError::DiskError);
        }
        Ok(())
    }

    // H-07: Zincir beklenenden erken biterse veya bozuksa kismi veriyi Ok donme, Corrupted dondur
    fn read_cluster_chain(
        &mut self,
        start_cluster: u32,
        no_fat_chain: bool,
        size: u64,
    ) -> Result<Vec<u8>, FsError> {
        if size == 0 {
            return Ok(Vec::new());
        }
        if size > MAX_FILE_SIZE_BYTES {
            return Err(FsError::Corrupted);
        }

        let mut buffer = Vec::with_capacity(size as usize);
        let mut current_cluster = start_cluster;
        let mut bytes_left = size;
        let mut guard = 0u32;

        while bytes_left > 0 {
            if current_cluster < 2
                || current_cluster >= self.max_cluster
                || guard >= self.max_cluster
            {
                return Err(FsError::Corrupted);
            }
            guard += 1;
            let cluster_buf = self.read_single_cluster(current_cluster)?;
            let to_copy = (bytes_left as usize).min(self.cluster_size as usize);
            buffer.extend_from_slice(&cluster_buf[..to_copy]);
            bytes_left -= to_copy as u64;
            if bytes_left == 0 {
                break;
            }
            if no_fat_chain {
                current_cluster = current_cluster.checked_add(1).ok_or(FsError::Corrupted)?;
            } else {
                current_cluster = self.read_fat_entry(current_cluster)?;
            }
        }
        Ok(buffer)
    }

    // H-04: Secondary entry sayisini ve buffer sinirlarini kesinkes dogrula (OOB panic onleme)
    fn scan_raw_entries(&self, data: &[u8]) -> Result<Vec<RawEntryLoc>, FsError> {
        let mut list = Vec::new();
        let mut i = 0usize;
        while i + 32 <= data.len() {
            let entry_type = data[i];
            if entry_type == 0x00 {
                break;
            }
            if entry_type == EXFAT_FILE_ENTRY {
                let secondary_count = data[i + 1] as usize;
                if secondary_count < 2 || secondary_count > 18 {
                    return Err(FsError::Corrupted);
                }
                let total_bytes = (1usize)
                    .checked_add(secondary_count)
                    .and_then(|c| c.checked_mul(32))
                    .ok_or(FsError::Corrupted)?;
                let entry_end = i.checked_add(total_bytes).ok_or(FsError::Corrupted)?;
                if entry_end > data.len() {
                    return Err(FsError::Corrupted);
                }

                let attr = u16::from_le_bytes([data[i + 4], data[i + 5]]);
                let is_dir = (attr & EXFAT_ATTR_DIRECTORY) != 0;
                let stream_idx = i + 32;
                if data[stream_idx] != EXFAT_STREAM_ENTRY {
                    return Err(FsError::Corrupted);
                }

                let flags = data[stream_idx + 1];
                let no_fat_chain = (flags & 0x02) != 0;
                let first_cluster = u32::from_le_bytes([
                    data[stream_idx + 20],
                    data[stream_idx + 21],
                    data[stream_idx + 22],
                    data[stream_idx + 23],
                ]);
                let data_len = u64::from_le_bytes([
                    data[stream_idx + 24],
                    data[stream_idx + 25],
                    data[stream_idx + 26],
                    data[stream_idx + 27],
                    data[stream_idx + 28],
                    data[stream_idx + 29],
                    data[stream_idx + 30],
                    data[stream_idx + 31],
                ]);

                let mut name = String::new();
                let name_entries_count = secondary_count - 1;
                for n in 0..name_entries_count {
                    let name_offset = stream_idx + 32 + (n * 32);
                    if data[name_offset] != EXFAT_NAME_ENTRY {
                        return Err(FsError::Corrupted);
                    }
                    for c in 0..15 {
                        let ch_idx = name_offset + 2 + (c * 2);
                        let code = u16::from_le_bytes([data[ch_idx], data[ch_idx + 1]]);
                        if code == 0 {
                            break;
                        }
                        if let Some(ch) = char::from_u32(code as u32) {
                            name.push(ch);
                        }
                    }
                }
                if name.is_empty() {
                    return Err(FsError::Corrupted);
                }
                list.push(RawEntryLoc {
                    byte_offset: i,
                    total_bytes,
                    first_cluster,
                    no_fat_chain,
                    size: data_len,
                    is_dir,
                    name,
                });
                i = entry_end;
            } else {
                i += 32;
            }
        }
        Ok(list)
    }

    fn parse_directory_data(&self, data: &[u8]) -> Result<Vec<DirEntry>, FsError> {
        let raw = self.scan_raw_entries(data)?;
        let mut entries = Vec::with_capacity(raw.len());
        for item in raw {
            let file_type = if item.is_dir {
                FileType::Directory
            } else {
                FileType::RegularFile
            };
            let flags_encoded = if item.no_fat_chain { 1u64 << 32 } else { 0 };
            entries.push(DirEntry {
                name: item.name,
                file_type,
                size: item.size,
                id: (item.first_cluster as u64) | flags_encoded,
            });
        }
        Ok(entries)
    }

    fn encode_entry_set(
        buf: &mut [u8],
        offset: usize,
        name: &str,
        is_dir: bool,
        cluster: u32,
        size: u64,
    ) -> Result<usize, FsError> {
        let chars_cnt = name.chars().count().min(30);
        let name_entries = ((chars_cnt + 14) / 15).max(1);
        let secondary_count = 1 + name_entries;
        let total_bytes = (1 + secondary_count) * 32;
        let end = offset.checked_add(total_bytes).ok_or(FsError::StorageFull)?;
        if end > buf.len() {
            return Err(FsError::StorageFull);
        }
        for b in &mut buf[offset..end] {
            *b = 0;
        }
        buf[offset] = EXFAT_FILE_ENTRY;
        buf[offset + 1] = secondary_count as u8;
        buf[offset + 4] = if is_dir { 0x10 } else { 0x20 };

        let s_off = offset + 32;
        buf[s_off] = EXFAT_STREAM_ENTRY;
        buf[s_off + 1] = 0x01; // AllocationPossible = 1, NoFatChain = 0
        buf[s_off + 3] = chars_cnt as u8;
        buf[s_off + 8..s_off + 16].copy_from_slice(&size.to_le_bytes());
        buf[s_off + 20..s_off + 24].copy_from_slice(&cluster.to_le_bytes());
        buf[s_off + 24..s_off + 32].copy_from_slice(&size.to_le_bytes());

        let mut ch_iter = name.chars();
        for n in 0..name_entries {
            let n_off = offset + 64 + n * 32;
            buf[n_off] = EXFAT_NAME_ENTRY;
            buf[n_off + 1] = 0x00;
            for c_idx in 0..15 {
                if let Some(ch) = ch_iter.next() {
                    let code = ch as u16;
                    let pos = n_off + 2 + c_idx * 2;
                    buf[pos] = (code & 0xFF) as u8;
                    buf[pos + 1] = (code >> 8) as u8;
                }
            }
        }
        Ok(total_bytes)
    }

    fn split_parent_and_name<'a>(&self, path: &'a str) -> Result<(&'a str, &'a str), FsError> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Err(FsError::InvalidPath);
        }
        if let Some(pos) = trimmed.rfind('/') {
            let parent = &trimmed[..pos];
            let name = &trimmed[pos + 1..];
            if name.is_empty() {
                Err(FsError::InvalidPath)
            } else {
                Ok((parent, name))
            }
        } else {
            Ok(("", trimmed))
        }
    }

    fn resolve_path(&mut self, path: &str) -> Result<(u32, bool, u64, bool), FsError> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok((self.root_dir_cluster, false, self.cluster_size as u64, true));
        }
        let mut current_cluster = self.root_dir_cluster;
        let mut no_fat_chain = false;
        let mut current_size = self.cluster_size as u64;
        let mut is_dir = true;
        for segment in trimmed.split('/') {
            if segment.is_empty() {
                continue;
            }
            if !is_dir {
                return Err(FsError::NotADirectory);
            }
            let dir_data =
                self.read_cluster_chain(current_cluster, no_fat_chain, self.cluster_size as u64)?;
            let entries = self.parse_directory_data(&dir_data)?;
            let found = match entries.into_iter().find(|e| e.name == segment) {
                Some(entry) => entry,
                None => return Err(FsError::NotFound),
            };
            current_cluster = (found.id & 0xFFFF_FFFF) as u32;
            no_fat_chain = (found.id & (1u64 << 32)) != 0;
            current_size = if found.file_type == FileType::Directory {
                self.cluster_size as u64
            } else {
                found.size
            };
            is_dir = found.file_type == FileType::Directory;
        }
        Ok((current_cluster, no_fat_chain, current_size, is_dir))
    }

    pub fn used_clusters_count(&mut self) -> u32 {
        let mut used = 0u32;
        let scan_end = self.max_cluster.min(512);
        for c in 2..scan_end {
            let fat_used = self.read_fat_entry(c).map(|v| v != 0).unwrap_or(false);
            let bmp_used = self.is_bitmap_cluster_allocated(c).unwrap_or(false);
            if fat_used || bmp_used {
                used += 1;
            }
        }
        used
    }
}

impl FileSystem for Exfat {
    fn list_dir(&mut self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let (cluster, no_fat, _, is_dir) = self.resolve_path(path)?;
        if !is_dir {
            return Err(FsError::NotADirectory);
        }
        let dir_data = self.read_cluster_chain(cluster, no_fat, self.cluster_size as u64)?;
        self.parse_directory_data(&dir_data)
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        let (cluster, no_fat, size, is_dir) = self.resolve_path(path)?;
        if is_dir {
            return Err(FsError::NotAFile);
        }
        self.read_cluster_chain(cluster, no_fat, size)
    }

    fn file_size(&mut self, path: &str) -> Result<u64, FsError> {
        let (_, _, size, is_dir) = self.resolve_path(path)?;
        if is_dir {
            return Err(FsError::NotAFile);
        }
        Ok(size)
    }

    // H-02: Once yeni kumelere yaz ve dizin girdisini commit et; eski zinciri YALNIZCA basarili commit sonrasinda birak
    fn write_file(&mut self, path: &str, content: &[u8]) -> Result<(), FsError> {
        if content.len() as u64 > MAX_FILE_SIZE_BYTES {
            return Err(FsError::StorageFull);
        }
        let (parent_path, file_name) = self.split_parent_and_name(path)?;
        let (parent_cluster, _, _, parent_is_dir) = self.resolve_path(parent_path)?;
        if !parent_is_dir {
            return Err(FsError::NotADirectory);
        }

        let mut dir_buf = self.read_single_cluster(parent_cluster)?;
        let raw_entries = self.scan_raw_entries(&dir_buf)?;
        let csize = self.cluster_size as usize;
        let needed_clusters = ((content.len() + csize - 1) / csize).max(1);

        if let Some(existing) = raw_entries.iter().find(|e| e.name == file_name) {
            if existing.is_dir {
                return Err(FsError::NotAFile);
            }
            let new_clusters = self.allocate_clusters(needed_clusters)?;
            if let Err(e) = self.write_data_to_clusters(&new_clusters, content) {
                let _ = self.free_cluster_chain(new_clusters[0], false, content.len() as u64);
                return Err(e);
            }

            let s_off = existing.byte_offset + 32;
            dir_buf[s_off + 1] = 0x01; // FAT zinciri kullaniliyor
            let sz = content.len() as u64;
            dir_buf[s_off + 8..s_off + 16].copy_from_slice(&sz.to_le_bytes());
            dir_buf[s_off + 20..s_off + 24].copy_from_slice(&new_clusters[0].to_le_bytes());
            dir_buf[s_off + 24..s_off + 32].copy_from_slice(&sz.to_le_bytes());

            if let Err(e) = self.write_single_cluster(parent_cluster, &dir_buf) {
                let _ = self.free_cluster_chain(new_clusters[0], false, content.len() as u64);
                return Err(e);
            }

            // Eski dosya kumelerini yalnizca yeni dosya ve metadata diske yazildiktan sonra serbest birak
            if existing.first_cluster != new_clusters[0] {
                let _ = self.free_cluster_chain(
                    existing.first_cluster,
                    existing.no_fat_chain,
                    existing.size,
                );
            }
            return Ok(());
        }

        let end_offset = raw_entries
            .last()
            .map(|e| e.byte_offset + e.total_bytes)
            .unwrap_or(0);
        let new_clusters = self.allocate_clusters(needed_clusters)?;
        if let Err(e) = self.write_data_to_clusters(&new_clusters, content) {
            let _ = self.free_cluster_chain(new_clusters[0], false, content.len() as u64);
            return Err(e);
        }
        if let Err(e) = Self::encode_entry_set(
            &mut dir_buf,
            end_offset,
            file_name,
            false,
            new_clusters[0],
            content.len() as u64,
        )
        .and_then(|_| self.write_single_cluster(parent_cluster, &dir_buf))
        {
            let _ = self.free_cluster_chain(new_clusters[0], false, content.len() as u64);
            return Err(e);
        }
        Ok(())
    }

    fn create_dir(&mut self, path: &str) -> Result<(), FsError> {
        let (parent_path, dir_name) = self.split_parent_and_name(path)?;
        let (parent_cluster, _, _, parent_is_dir) = self.resolve_path(parent_path)?;
        if !parent_is_dir {
            return Err(FsError::NotADirectory);
        }
        let mut dir_buf = self.read_single_cluster(parent_cluster)?;
        let raw_entries = self.scan_raw_entries(&dir_buf)?;
        if raw_entries.iter().any(|e| e.name == dir_name) {
            return Err(FsError::AlreadyExists);
        }
        let end_offset = raw_entries
            .last()
            .map(|e| e.byte_offset + e.total_bytes)
            .unwrap_or(0);
        let new_clusters = self.allocate_clusters(1)?;
        let empty_dir = alloc::vec![0u8; self.cluster_size as usize];
        if let Err(e) = self.write_data_to_clusters(&new_clusters, &empty_dir) {
            let _ = self.free_cluster_chain(new_clusters[0], false, self.cluster_size as u64);
            return Err(e);
        }
        if let Err(e) = Self::encode_entry_set(
            &mut dir_buf,
            end_offset,
            dir_name,
            true,
            new_clusters[0],
            self.cluster_size as u64,
        )
        .and_then(|_| self.write_single_cluster(parent_cluster, &dir_buf))
        {
            let _ = self.free_cluster_chain(new_clusters[0], false, self.cluster_size as u64);
            return Err(e);
        }
        Ok(())
    }

    // H-03 & H-04: Bos olmayan klasoru silmeyi reddet, once dizin girdisini guncelle, sonra zinciri bosalt
    fn remove(&mut self, path: &str) -> Result<(), FsError> {
        let (parent_path, target_name) = self.split_parent_and_name(path)?;
        let (parent_cluster, _, _, parent_is_dir) = self.resolve_path(parent_path)?;
        if !parent_is_dir {
            return Err(FsError::NotADirectory);
        }
        let mut dir_buf = self.read_single_cluster(parent_cluster)?;
        let raw_entries = self.scan_raw_entries(&dir_buf)?;
        let found = match raw_entries.iter().find(|e| e.name == target_name) {
            Some(e) => e,
            None => return Err(FsError::NotFound),
        };

        // Klasorse icinin bos oldugunu dogrula
        if found.is_dir {
            let child_buf = self.read_single_cluster(found.first_cluster)?;
            let child_entries = self.scan_raw_entries(&child_buf)?;
            if !child_entries.is_empty() {
                return Err(FsError::PermissionDenied);
            }
        }

        let mut rebuilt = alloc::vec![0u8; dir_buf.len()];
        let mut write_cursor = 0usize;

        // Kok dizindeki 0x81/0x82 gibi ozel sistem girdilerini koru
        if parent_cluster == self.root_dir_cluster {
            let first_file_off = raw_entries.first().map(|e| e.byte_offset).unwrap_or(0);
            if first_file_off > 0 && first_file_off <= dir_buf.len() {
                rebuilt[..first_file_off].copy_from_slice(&dir_buf[..first_file_off]);
                write_cursor = first_file_off;
            }
        }

        for entry in &raw_entries {
            if entry.name == target_name {
                continue;
            }
            let src_end = entry
                .byte_offset
                .checked_add(entry.total_bytes)
                .ok_or(FsError::Corrupted)?;
            let dst_end = write_cursor
                .checked_add(entry.total_bytes)
                .ok_or(FsError::Corrupted)?;
            if src_end > dir_buf.len() || dst_end > rebuilt.len() {
                return Err(FsError::Corrupted);
            }
            rebuilt[write_cursor..dst_end].copy_from_slice(&dir_buf[entry.byte_offset..src_end]);
            write_cursor = dst_end;
        }

        // H-03: Once dizin tablosunu diske commit et, ardindan eski kume zincirini serbest birak
        dir_buf.copy_from_slice(&rebuilt);
        self.write_single_cluster(parent_cluster, &dir_buf)?;
        self.free_cluster_chain(found.first_cluster, found.no_fat_chain, found.size)?;
        Ok(())
    }

    fn file_stat(&mut self, path: &str) -> Result<FileStat, FsError> {
        let (_, _, size, is_dir) = self.resolve_path(path)?;
        Ok(FileStat {
            size,
            is_dir,
            readonly: false,
        })
    }
}
