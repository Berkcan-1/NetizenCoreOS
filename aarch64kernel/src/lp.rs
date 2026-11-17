use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use nc_hal::{BlockDevice, BlockDeviceInfo, DeviceType};

const SECTOR: usize = 512;
const GEOMETRY_MAGIC: u32 = 0x616c_4467;
const HEADER_MAGIC: u32 = 0x414C_5030;
const RESERVED_BYTES: u64 = 4096;
const GEOMETRY_SIZE: u64 = 4096;
const MAJOR_VERSION: u16 = 10;
const MINOR_VERSION_MAX: u16 = 2;
const HEADER_MIN: usize = 128;
const HEADER_MAX: usize = 4096;
const METADATA_MAX_LIMIT: u32 = 1024 * 1024;
const PARTITION_ENTRY_MIN: usize = 52;
const EXTENT_ENTRY_MIN: usize = 24;
const TARGET_LINEAR: u32 = 0;
const NAME_BYTES: usize = 36;
const MAX_EXTENTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LpError {
    Io,
    BadGeometry,
    BadHeader,
    BadTable,
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

fn read_bytes(device: &mut dyn BlockDevice, offset: u64, length: usize) -> Result<Vec<u8>, LpError> {
    if device.info().block_size as usize != SECTOR {
        return Err(LpError::Unsupported);
    }
    let first = offset / SECTOR as u64;
    let skip = (offset % SECTOR as u64) as usize;
    let sectors = (skip + length + SECTOR - 1) / SECTOR;
    let mut raw = vec![0u8; sectors * SECTOR];
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
    let geometry = read_bytes(device, RESERVED_BYTES, 64)?;
    if le32(&geometry, 0) != Some(GEOMETRY_MAGIC) {
        return Err(LpError::BadGeometry);
    }
    let struct_size = le32(&geometry, 4).ok_or(LpError::BadGeometry)? as usize;
    if struct_size < 52 || struct_size > GEOMETRY_SIZE as usize {
        return Err(LpError::BadGeometry);
    }
    let metadata_max = le32(&geometry, 40).ok_or(LpError::BadGeometry)?;
    let slot_count = le32(&geometry, 44).ok_or(LpError::BadGeometry)?;
    if metadata_max == 0 || metadata_max > METADATA_MAX_LIMIT || metadata_max % SECTOR as u32 != 0 || slot_count == 0 {
        return Err(LpError::BadGeometry);
    }
    let metadata_offset = RESERVED_BYTES + GEOMETRY_SIZE * 2;
    let header = read_bytes(device, metadata_offset, HEADER_MAX.min(metadata_max as usize))?;
    if le32(&header, 0) != Some(HEADER_MAGIC) {
        return Err(LpError::BadHeader);
    }
    let major = le16(&header, 4).ok_or(LpError::BadHeader)?;
    let minor = le16(&header, 6).ok_or(LpError::BadHeader)?;
    if major != MAJOR_VERSION || minor > MINOR_VERSION_MAX {
        return Err(LpError::Unsupported);
    }
    let header_size = le32(&header, 8).ok_or(LpError::BadHeader)? as usize;
    if header_size < HEADER_MIN || header_size > HEADER_MAX {
        return Err(LpError::BadHeader);
    }
    let tables_size = le32(&header, 44).ok_or(LpError::BadHeader)? as usize;
    if tables_size > metadata_max as usize - header_size.min(metadata_max as usize) {
        return Err(LpError::BadHeader);
    }
    let partitions = table(&header, 80, tables_size, PARTITION_ENTRY_MIN)?;
    let extents = table(&header, 92, tables_size, EXTENT_ENTRY_MIN)?;
    let tables = read_bytes(device, metadata_offset + header_size as u64, tables_size)?;

    let mut found = None;
    for index in 0..partitions.entries {
        let at = partitions.offset + index * partitions.entry_size;
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
        let at = extents.offset + index * extents.entry_size;
        let entry = tables.get(at..at + extents.entry_size).ok_or(LpError::BadTable)?;
        let sectors = le64(entry, 0).ok_or(LpError::BadTable)?;
        let target_type = le32(entry, 8).ok_or(LpError::BadTable)?;
        let start = le64(entry, 12).ok_or(LpError::BadTable)?;
        let source = le32(entry, 20).ok_or(LpError::BadTable)?;
        if target_type != TARGET_LINEAR || source != 0 {
            return Err(LpError::Unsupported);
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

#[cfg(test)]
impl PartitionDevice {
    pub fn from_parts(inner: Box<dyn BlockDevice>, extents: Vec<Extent>) -> Self {
        let total = extents.iter().map(|extent| extent.sectors).sum();
        PartitionDevice { inner, extents, total }
    }
}

impl PartitionDevice {
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
        let wanted = count as usize * SECTOR;
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
