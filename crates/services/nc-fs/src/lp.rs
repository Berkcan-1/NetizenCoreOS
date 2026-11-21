//! Android Logical Partition (`liblp`) metadata parser and verifier (Feature 10).
//!
//! Validates AOSP `metadata_format.h` geometry, header, and table SHA-256 checksums
//! (computed via the authoritative SPARK Ada SHA-256 implementation in `nc-crypto`),
//! bounds, alignment, integer overflows, and extent ranges.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use nc_hal::{BlockDevice, BlockDeviceInfo, DeviceType};

const SECTOR: usize = 512;
const GEOMETRY_MAGIC: u32 = 0x616c_4467;
const HEADER_MAGIC: u32 = 0x414C_5030;
const RESERVED_BYTES: u64 = 4096;
const GEOMETRY_SIZE: u64 = 4096;
const GEOMETRY_MIN_SIZE: usize = 52;
const GEOMETRY_CHECKSUM_OFFSET: usize = 8;
const HEADER_CHECKSUM_OFFSET: usize = 12;
const TABLES_CHECKSUM_OFFSET: usize = 48;
const CHECKSUM_LEN: usize = 32;
const MAJOR_VERSION: u16 = 10;
const MINOR_VERSION_MAX: u16 = 2;
const HEADER_MIN: usize = 128;
const HEADER_MAX: usize = 4096;
const METADATA_MAX_LIMIT: u32 = 1024 * 1024;
const MAX_SLOTS: u32 = 32;
const PARTITION_ENTRY_MIN: usize = 52;
const EXTENT_ENTRY_MIN: usize = 24;
const GROUP_ENTRY_MIN: usize = 48;
const BLOCK_DEVICE_ENTRY_MIN: usize = 64;
const TARGET_LINEAR: u32 = 0;
const NAME_BYTES: usize = 36;
const MAX_EXTENTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LpError {
    Io,
    BadGeometry,
    BadHeader,
    BadTable,
    BadChecksum,
    NotFound,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    pub start: u64,
    pub sectors: u64,
}

pub struct PartitionDevice {
    inner: Box<dyn BlockDevice>,
    extents: Vec<Extent>,
    total: u64,
}

fn le16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(offset..offset + 2)?.try_into().ok()?))
}

fn le32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(offset..offset + 4)?.try_into().ok()?))
}

fn le64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(data.get(offset..offset + 8)?.try_into().ok()?))
}

fn ct_eq_32(a: &[u8], b: &[u8; 32]) -> bool {
    if a.len() != 32 {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

fn verify_embedded_sha256(buf: &[u8], checksum_offset: usize) -> bool {
    if buf.len() < checksum_offset + CHECKSUM_LEN {
        return false;
    }
    let expected = &buf[checksum_offset..checksum_offset + CHECKSUM_LEN];
    let mut copy = buf.to_vec();
    for b in &mut copy[checksum_offset..checksum_offset + CHECKSUM_LEN] {
        *b = 0;
    }
    let actual = nc_crypto::sha256(&copy);
    ct_eq_32(expected, &actual)
}

fn read_bytes(device: &mut dyn BlockDevice, offset: u64, length: usize) -> Result<Vec<u8>, LpError> {
    if device.info().block_size as usize != SECTOR {
        return Err(LpError::Unsupported);
    }
    let first = offset / SECTOR as u64;
    let skip = (offset % SECTOR as u64) as usize;
    let total_span = skip.checked_add(length).ok_or(LpError::BadHeader)?;
    let sectors = (total_span + SECTOR - 1) / SECTOR;
    let total_bytes = sectors.checked_mul(SECTOR).ok_or(LpError::BadHeader)?;
    let mut raw = vec![0u8; total_bytes];
    device.read_blocks(first, sectors as u32, &mut raw).map_err(|_| LpError::Io)?;
    Ok(raw[skip..skip + length].to_vec())
}

struct Table {
    offset: usize,
    entries: usize,
    entry_size: usize,
}

fn table(header: &[u8], at: usize, tables_size: usize, minimum: usize) -> Result<Table, LpError> {
    let offset = le32(header, at).ok_or(LpError::BadHeader)? as usize;
    let entries = le32(header, at + 4).ok_or(LpError::BadHeader)? as usize;
    let entry_size = le32(header, at + 8).ok_or(LpError::BadHeader)? as usize;
    if entries > 0 && entry_size < minimum {
        return Err(LpError::BadTable);
    }
    let bytes = entries.checked_mul(entry_size).ok_or(LpError::BadTable)?;
    let end = offset.checked_add(bytes).ok_or(LpError::BadTable)?;
    if end > tables_size {
        return Err(LpError::BadTable);
    }
    Ok(Table { offset, entries, entry_size })
}

fn name_matches(entry: &[u8], wanted: &str) -> bool {
    let raw = match entry.get(..NAME_BYTES) {
        Some(raw) => raw,
        None => return false,
    };
    let length = raw.iter().position(|byte| *byte == 0).unwrap_or(NAME_BYTES);
    &raw[..length] == wanted.as_bytes()
}

pub fn find_extents(device: &mut dyn BlockDevice, name: &str) -> Result<Vec<Extent>, LpError> {
    let geom_prefix = read_bytes(device, RESERVED_BYTES, GEOMETRY_MIN_SIZE)?;
    if le32(&geom_prefix, 0) != Some(GEOMETRY_MAGIC) {
        return Err(LpError::BadGeometry);
    }
    let struct_size = le32(&geom_prefix, 4).ok_or(LpError::BadGeometry)? as usize;
    if struct_size < GEOMETRY_MIN_SIZE || struct_size > GEOMETRY_SIZE as usize {
        return Err(LpError::BadGeometry);
    }
    let geometry = if struct_size == GEOMETRY_MIN_SIZE {
        geom_prefix
    } else {
        read_bytes(device, RESERVED_BYTES, struct_size)?
    };
    if !verify_embedded_sha256(&geometry, GEOMETRY_CHECKSUM_OFFSET) {
        return Err(LpError::BadChecksum);
    }

    let metadata_max = le32(&geometry, 40).ok_or(LpError::BadGeometry)?;
    let slot_count = le32(&geometry, 44).ok_or(LpError::BadGeometry)?;
    let logical_block_size = le32(&geometry, 48).ok_or(LpError::BadGeometry)?;
    if metadata_max == 0
        || metadata_max > METADATA_MAX_LIMIT
        || metadata_max % SECTOR as u32 != 0
        || slot_count == 0
        || slot_count > MAX_SLOTS
        || logical_block_size == 0
        || logical_block_size % SECTOR as u32 != 0
    {
        return Err(LpError::BadGeometry);
    }

    let metadata_offset = RESERVED_BYTES + GEOMETRY_SIZE * 2;
    let header_probe = read_bytes(device, metadata_offset, HEADER_MIN)?;
    if le32(&header_probe, 0) != Some(HEADER_MAGIC) {
        return Err(LpError::BadHeader);
    }
    let major = le16(&header_probe, 4).ok_or(LpError::BadHeader)?;
    let minor = le16(&header_probe, 6).ok_or(LpError::BadHeader)?;
    if major != MAJOR_VERSION || minor > MINOR_VERSION_MAX {
        return Err(LpError::Unsupported);
    }
    let header_size = le32(&header_probe, 8).ok_or(LpError::BadHeader)? as usize;
    if header_size < HEADER_MIN || header_size > HEADER_MAX || header_size > metadata_max as usize {
        return Err(LpError::BadHeader);
    }
    let header = if header_size == HEADER_MIN {
        header_probe
    } else {
        read_bytes(device, metadata_offset, header_size)?
    };
    if !verify_embedded_sha256(&header, HEADER_CHECKSUM_OFFSET) {
        return Err(LpError::BadChecksum);
    }

    let tables_size = le32(&header, 44).ok_or(LpError::BadHeader)? as usize;
    if tables_size > metadata_max as usize - header_size {
        return Err(LpError::BadHeader);
    }
    let partitions = table(&header, 80, tables_size, PARTITION_ENTRY_MIN)?;
    let extents = table(&header, 92, tables_size, EXTENT_ENTRY_MIN)?;
    let _groups = table(&header, 104, tables_size, GROUP_ENTRY_MIN)?;
    let _block_devices = table(&header, 116, tables_size, BLOCK_DEVICE_ENTRY_MIN)?;

    let tables_offset = metadata_offset
        .checked_add(header_size as u64)
        .ok_or(LpError::BadHeader)?;
    let tables = read_bytes(device, tables_offset, tables_size)?;
    let expected_tables_sum = header
        .get(TABLES_CHECKSUM_OFFSET..TABLES_CHECKSUM_OFFSET + CHECKSUM_LEN)
        .ok_or(LpError::BadHeader)?;
    let actual_tables_sum = nc_crypto::sha256(&tables);
    if !ct_eq_32(expected_tables_sum, &actual_tables_sum) {
        return Err(LpError::BadChecksum);
    }

    let mut found = None;
    for index in 0..partitions.entries {
        let at = partitions
            .offset
            .checked_add(index.checked_mul(partitions.entry_size).ok_or(LpError::BadTable)?)
            .ok_or(LpError::BadTable)?;
        let entry = tables.get(at..at + partitions.entry_size).ok_or(LpError::BadTable)?;
        if name_matches(entry, name) {
            let first = le32(entry, 40).ok_or(LpError::BadTable)? as usize;
            let count = le32(entry, 44).ok_or(LpError::BadTable)? as usize;
            found = Some((first, count));
            break;
        }
    }
    let (first, count) = found.ok_or(LpError::NotFound)?;
    if count == 0 || count > MAX_EXTENTS || first.checked_add(count).map_or(true, |end| end > extents.entries) {
        return Err(LpError::BadTable);
    }
    let mut result = Vec::with_capacity(count);
    for index in first..first + count {
        let at = extents
            .offset
            .checked_add(index.checked_mul(extents.entry_size).ok_or(LpError::BadTable)?)
            .ok_or(LpError::BadTable)?;
        let entry = tables.get(at..at + extents.entry_size).ok_or(LpError::BadTable)?;
        let sectors = le64(entry, 0).ok_or(LpError::BadTable)?;
        let target_type = le32(entry, 8).ok_or(LpError::BadTable)?;
        let start = le64(entry, 12).ok_or(LpError::BadTable)?;
        let source = le32(entry, 20).ok_or(LpError::BadTable)?;
        if target_type != TARGET_LINEAR || source != 0 {
            return Err(LpError::Unsupported);
        }
        if sectors == 0 {
            return Err(LpError::BadTable);
        }
        result.push(Extent { start, sectors });
    }
    Ok(result)
}

pub fn open_partition(mut device: Box<dyn BlockDevice>, name: &str) -> Result<PartitionDevice, LpError> {
    let extents = find_extents(device.as_mut(), name)?;
    let capacity = device.info().block_count;
    let mut total = 0u64;
    for extent in extents.iter() {
        let end = extent.start.checked_add(extent.sectors).ok_or(LpError::BadTable)?;
        if end > capacity {
            return Err(LpError::BadTable);
        }
        total = total.checked_add(extent.sectors).ok_or(LpError::BadTable)?;
    }
    Ok(PartitionDevice { inner: device, extents, total })
}

impl PartitionDevice {
    pub fn from_parts(inner: Box<dyn BlockDevice>, extents: Vec<Extent>) -> Self {
        let total = extents.iter().map(|extent| extent.sectors).sum();
        PartitionDevice { inner, extents, total }
    }

    pub fn extent_count(&self) -> usize {
        self.extents.len()
    }

    pub fn sectors(&self) -> u64 {
        self.total
    }
}

impl BlockDevice for PartitionDevice {
    fn info(&self) -> BlockDeviceInfo {
        let mut info = self.inner.info();
        info.block_count = self.total;
        info.block_size = SECTOR as u32;
        info.device_type = DeviceType::HardDisk;
        info
    }

    fn read_blocks(&mut self, lba: u64, count: u32, buffer: &mut [u8]) -> Result<(), ()> {
        let wanted = (count as usize).checked_mul(SECTOR).ok_or(())?;
        if buffer.len() < wanted || lba.checked_add(count as u64).map_or(true, |end| end > self.total) {
            return Err(());
        }
        let mut logical = lba;
        let mut remaining = count as u64;
        let mut done = 0usize;
        while remaining > 0 {
            let mut skipped = 0u64;
            let mut mapped = None;
            for extent in self.extents.iter() {
                if logical < skipped + extent.sectors {
                    let within = logical - skipped;
                    mapped = Some((extent.start + within, extent.sectors - within));
                    break;
                }
                skipped += extent.sectors;
            }
            let (physical, available) = mapped.ok_or(())?;
            let run = remaining.min(available);
            let bytes = run as usize * SECTOR;
            self.inner.read_blocks(physical, run as u32, &mut buffer[done..done + bytes])?;
            logical += run;
            remaining -= run;
            done += bytes;
        }
        Ok(())
    }

    fn write_blocks(&mut self, _lba: u64, _count: u32, _buffer: &[u8]) -> Result<(), ()> {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MemBlock {
        data: Vec<u8>,
    }

    impl BlockDevice for MemBlock {
        fn info(&self) -> BlockDeviceInfo {
            BlockDeviceInfo {
                channel: 0,
                model: [0; 40],
                model_len: 0,
                block_count: (self.data.len() / SECTOR) as u64,
                block_size: SECTOR as u32,
                device_type: DeviceType::HardDisk,
            }
        }

        fn read_blocks(&mut self, lba: u64, count: u32, buffer: &mut [u8]) -> Result<(), ()> {
            let start = (lba as usize).checked_mul(SECTOR).ok_or(())?;
            let len = (count as usize).checked_mul(SECTOR).ok_or(())?;
            let end = start.checked_add(len).ok_or(())?;
            if end > self.data.len() || buffer.len() < len {
                return Err(());
            }
            buffer[..len].copy_from_slice(&self.data[start..end]);
            Ok(())
        }

        fn write_blocks(&mut self, _lba: u64, _count: u32, _buffer: &[u8]) -> Result<(), ()> {
            Err(())
        }
    }

    fn build_valid_super(part_name: &str, extent_start: u64, extent_sectors: u64, total_sectors: usize) -> Vec<u8> {
        let mut img = vec![0u8; total_sectors * SECTOR];

        let mut partition = [0u8; 52];
        let name_b = part_name.as_bytes();
        partition[..name_b.len()].copy_from_slice(name_b);
        partition[36..40].copy_from_slice(&1u32.to_le_bytes()); // readonly
        partition[40..44].copy_from_slice(&0u32.to_le_bytes()); // first_extent
        partition[44..48].copy_from_slice(&1u32.to_le_bytes()); // num_extents
        partition[48..52].copy_from_slice(&0u32.to_le_bytes()); // group_index

        let mut extent = [0u8; 24];
        extent[0..8].copy_from_slice(&extent_sectors.to_le_bytes());
        extent[8..12].copy_from_slice(&TARGET_LINEAR.to_le_bytes());
        extent[12..20].copy_from_slice(&extent_start.to_le_bytes());
        extent[20..24].copy_from_slice(&0u32.to_le_bytes());

        let group = [0u8; 48];
        let bdev = [0u8; 64];

        let mut tables = Vec::new();
        tables.extend_from_slice(&partition);
        tables.extend_from_slice(&extent);
        tables.extend_from_slice(&group);
        tables.extend_from_slice(&bdev);
        let tables_sum = nc_crypto::sha256(&tables);

        let mut header = vec![0u8; 256];
        header[0..4].copy_from_slice(&HEADER_MAGIC.to_le_bytes());
        header[4..6].copy_from_slice(&MAJOR_VERSION.to_le_bytes());
        header[6..8].copy_from_slice(&2u16.to_le_bytes());
        header[8..12].copy_from_slice(&256u32.to_le_bytes());
        header[44..48].copy_from_slice(&(tables.len() as u32).to_le_bytes());
        header[48..80].copy_from_slice(&tables_sum);
        // partitions desc
        header[80..84].copy_from_slice(&0u32.to_le_bytes());
        header[84..88].copy_from_slice(&1u32.to_le_bytes());
        header[88..92].copy_from_slice(&52u32.to_le_bytes());
        // extents desc
        header[92..96].copy_from_slice(&52u32.to_le_bytes());
        header[96..100].copy_from_slice(&1u32.to_le_bytes());
        header[100..104].copy_from_slice(&24u32.to_le_bytes());
        // groups desc
        header[104..108].copy_from_slice(&76u32.to_le_bytes());
        header[108..112].copy_from_slice(&1u32.to_le_bytes());
        header[112..116].copy_from_slice(&48u32.to_le_bytes());
        // block_devices desc
        header[116..120].copy_from_slice(&124u32.to_le_bytes());
        header[120..124].copy_from_slice(&1u32.to_le_bytes());
        header[124..128].copy_from_slice(&64u32.to_le_bytes());

        let header_sum = nc_crypto::sha256(&header);
        header[12..44].copy_from_slice(&header_sum);

        let mut geom = [0u8; 52];
        geom[0..4].copy_from_slice(&GEOMETRY_MAGIC.to_le_bytes());
        geom[4..8].copy_from_slice(&52u32.to_le_bytes());
        geom[40..44].copy_from_slice(&65536u32.to_le_bytes());
        geom[44..48].copy_from_slice(&2u32.to_le_bytes());
        geom[48..52].copy_from_slice(&4096u32.to_le_bytes());
        let geom_sum = nc_crypto::sha256(&geom);
        geom[8..40].copy_from_slice(&geom_sum);

        img[4096..4096 + 52].copy_from_slice(&geom);
        img[8192..8192 + 52].copy_from_slice(&geom);
        img[12288..12288 + 256].copy_from_slice(&header);
        img[12288 + 256..12288 + 256 + tables.len()].copy_from_slice(&tables);
        img
    }

    #[test]
    fn valid_lp_metadata_passes_all_checksums() {
        let data = build_valid_super("system", 2048, 128, 4096);
        let dev = Box::new(MemBlock { data });
        let part = open_partition(dev, "system").expect("valid super.img");
        assert_eq!(part.extent_count(), 1);
        assert_eq!(part.sectors(), 128);
    }

    #[test]
    fn tampered_geometry_is_rejected() {
        let mut data = build_valid_super("system", 2048, 128, 4096);
        data[4096 + 44] ^= 0x01; // flip slot_count without updating checksum
        let dev = Box::new(MemBlock { data });
        assert_eq!(open_partition(dev, "system").err(), Some(LpError::BadChecksum));
    }

    #[test]
    fn tampered_header_is_rejected() {
        let mut data = build_valid_super("system", 2048, 128, 4096);
        data[12288 + 130] ^= 0x80; // flip reserved byte in header
        let dev = Box::new(MemBlock { data });
        assert_eq!(open_partition(dev, "system").err(), Some(LpError::BadChecksum));
    }

    #[test]
    fn tampered_tables_is_rejected() {
        let mut data = build_valid_super("system", 2048, 128, 4096);
        data[12288 + 256 + 52] ^= 0x01; // flip extent sector count without updating tables_checksum
        let dev = Box::new(MemBlock { data });
        assert_eq!(open_partition(dev, "system").err(), Some(LpError::BadChecksum));
    }

    #[test]
    fn out_of_bounds_extent_is_rejected() {
        let data = build_valid_super("system", 4000, 200, 4096);
        let dev = Box::new(MemBlock { data });
        assert_eq!(open_partition(dev, "system").err(), Some(LpError::BadTable));
    }

    #[test]
    fn missing_partition_returns_not_found() {
        let data = build_valid_super("system", 2048, 128, 4096);
        let dev = Box::new(MemBlock { data });
        assert_eq!(open_partition(dev, "vendor").err(), Some(LpError::NotFound));
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let mut data = build_valid_super("system", 2048, 128, 4096);
        data[12288 + 4..12288 + 6].copy_from_slice(&11u16.to_le_bytes());
        let dev = Box::new(MemBlock { data });
        assert_eq!(open_partition(dev, "system").err(), Some(LpError::Unsupported));
    }

    #[test]
    fn lp_parser_mutation_fuzz_never_panics() {
        let base = build_valid_super("system", 2048, 128, 4096);
        let mut x = 0xD1B5_4A32_D192_ED03u64;
        for iter in 0..256usize {
            let mut mutated = base.clone();
            // Mutate 1..8 bytes across geometry, header, and tables regions
            let flips = (iter % 8) + 1;
            for _ in 0..flips {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                let region = (x as usize) % 3;
                let off = match region {
                    0 => 4096 + ((x >> 8) as usize % 52),
                    1 => 12288 + ((x >> 8) as usize % 256),
                    _ => 12288 + 256 + ((x >> 8) as usize % 188),
                };
                mutated[off] ^= ((x >> 24) & 0xFF) as u8 | 1;
            }
            let dev = Box::new(MemBlock { data: mutated });
            // Any mutation must be rejected cleanly without panicking!
            assert!(open_partition(dev, "system").is_err());
        }
    }
}
