extern crate alloc;

use alloc::vec::Vec;
use nc_hal::BlockDevice;

pub const GPT_SIGNATURE: [u8; 8] = *b"EFI PART";
pub const GPT_REVISION_1_0: u32 = 0x0001_0000;
pub const GPT_HEADER_SIZE: u32 = 92;
pub const GPT_ENTRY_SIZE: u32 = 128;
pub const GPT_ENTRY_COUNT: usize = 128;
pub const GPT_ARRAY_BYTES: usize = GPT_ENTRY_COUNT * GPT_ENTRY_SIZE as usize;
pub const GPT_ARRAY_SECTORS: u32 = (GPT_ARRAY_BYTES / 512) as u32;
const GPT_MIN_ARRAY_BYTES: usize = 16 * 1024;
const GPT_MAX_ARRAY_BYTES: usize = 4 * 1024 * 1024;

pub const GUID_EFI_SYSTEM: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];
pub const GUID_LINUX_FS: [u8; 16] = [
    0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4,
];
pub const GUID_BASIC_DATA: [u8; 16] = [
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GptPartitionEntry {
    pub type_guid: [u8; 16],
    pub unique_guid: [u8; 16],
    pub first_lba: u64,
    pub last_lba: u64,
    pub attributes: u64,
}

impl GptPartitionEntry {
    pub const EMPTY: Self = Self {
        type_guid: [0; 16], unique_guid: [0; 16], first_lba: 0, last_lba: 0, attributes: 0,
    };

    #[inline]
    pub fn is_used(&self) -> bool {
        self.type_guid != [0; 16] && self.unique_guid != [0; 16]
            && self.first_lba > 0 && self.last_lba >= self.first_lba
    }

    #[inline]
    pub fn sector_count(&self) -> u64 {
        if self.is_used() { self.last_lba - self.first_lba + 1 } else { 0 }
    }
}

pub fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if (crc & 1) != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// UUID v4. Secure hardware entropy is mandatory; installation fails closed if unavailable.
pub fn random_guid() -> Result<[u8; 16], ()> {
    let mut guid = [0u8; 16];
    nc_arch::rand::HardwareRng::fill_secure_bytes(&mut guid)?;
    guid[6] = (guid[6] & 0x0F) | 0x40;
    guid[8] = (guid[8] & 0x3F) | 0x80;
    Ok(guid)
}

/// Reads and validates all non-empty GPT entries. The legacy partition API may still
/// expose four MBR-compatible slots, but GPT parsing itself is no longer four-entry limited.
pub fn parse_gpt_partitions(device: &mut dyn BlockDevice) -> Option<Vec<GptPartitionEntry>> {
    let info = device.info();
    if info.block_size != 512 || info.block_count < 68 { return None; }
    try_read_gpt_at(device, 1, info.block_count)
        .or_else(|| try_read_gpt_at(device, info.block_count - 1, info.block_count))
}

fn try_read_gpt_at(
    device: &mut dyn BlockDevice,
    header_lba: u64,
    total_sectors: u64,
) -> Option<Vec<GptPartitionEntry>> {
    let mut hdr = [0u8; 512];
    device.read_blocks(header_lba, 1, &mut hdr).ok()?;
    if hdr[0..8] != GPT_SIGNATURE { return None; }

    let header_size = u32::from_le_bytes(hdr[12..16].try_into().ok()?) as usize;
    if !(GPT_HEADER_SIZE as usize..=512).contains(&header_size) { return None; }
    let stored_hdr_crc = u32::from_le_bytes(hdr[16..20].try_into().ok()?);
    let mut header_for_crc = hdr;
    header_for_crc[16..20].fill(0);
    if crc32_ieee(&header_for_crc[..header_size]) != stored_hdr_crc { return None; }

    let my_lba = u64::from_le_bytes(hdr[24..32].try_into().ok()?);
    let alt_lba = u64::from_le_bytes(hdr[32..40].try_into().ok()?);
    let first_usable = u64::from_le_bytes(hdr[40..48].try_into().ok()?);
    let last_usable = u64::from_le_bytes(hdr[48..56].try_into().ok()?);
    let entries_lba = u64::from_le_bytes(hdr[72..80].try_into().ok()?);
    let num_entries = u32::from_le_bytes(hdr[80..84].try_into().ok()?) as usize;
    let entry_size = u32::from_le_bytes(hdr[84..88].try_into().ok()?) as usize;
    let stored_array_crc = u32::from_le_bytes(hdr[88..92].try_into().ok()?);

    if my_lba != header_lba || alt_lba >= total_sectors || alt_lba == my_lba
        || (header_lba == 1 && alt_lba != total_sectors - 1)
        || (header_lba == total_sectors - 1 && alt_lba != 1)
        || entry_size < GPT_ENTRY_SIZE as usize || entry_size % 8 != 0
        || num_entries == 0 || num_entries > GPT_MAX_ARRAY_BYTES / entry_size
        || first_usable < 2 || last_usable >= total_sectors || first_usable > last_usable
    { return None; }

    let array_bytes = num_entries.checked_mul(entry_size)?;
    if array_bytes < GPT_MIN_ARRAY_BYTES || array_bytes > GPT_MAX_ARRAY_BYTES { return None; }
    let sectors = array_bytes.checked_add(511)?.checked_div(512)?;
    let sectors_u64 = sectors as u64;
    let array_end = entries_lba.checked_add(sectors_u64)?;
    if entries_lba == 0 || array_end > total_sectors { return None; }
    // Partition-entry arrays and headers must remain outside the usable partition range.
    if !(array_end - 1 < first_usable || entries_lba > last_usable)
        || (header_lba >= first_usable && header_lba <= last_usable)
        || (header_lba >= entries_lba && header_lba < array_end)
        || (alt_lba >= entries_lba && alt_lba < array_end)
    { return None; }

    let read_bytes = sectors.checked_mul(512)?;
    let mut entry_array = Vec::new();
    entry_array.try_reserve_exact(read_bytes).ok()?;
    entry_array.resize(read_bytes, 0);
    device.read_blocks(entries_lba, u32::try_from(sectors).ok()?, &mut entry_array).ok()?;
    if crc32_ieee(&entry_array[..array_bytes]) != stored_array_crc { return None; }

    let mut out: Vec<GptPartitionEntry> = Vec::new();
    out.try_reserve(num_entries).ok()?;
    for i in 0..num_entries {
        let off = i.checked_mul(entry_size)?;
        let type_guid = &entry_array[off..off + 16];
        if type_guid.iter().all(|b| *b == 0) { continue; }

        let mut type_id = [0u8; 16];
        let mut unique_guid = [0u8; 16];
        type_id.copy_from_slice(type_guid);
        unique_guid.copy_from_slice(&entry_array[off + 16..off + 32]);
        let entry = GptPartitionEntry {
            type_guid: type_id,
            unique_guid,
            first_lba: u64::from_le_bytes(entry_array[off + 32..off + 40].try_into().ok()?),
            last_lba: u64::from_le_bytes(entry_array[off + 40..off + 48].try_into().ok()?),
            attributes: u64::from_le_bytes(entry_array[off + 48..off + 56].try_into().ok()?),
        };
        if !entry.is_used() || entry.first_lba < first_usable || entry.last_lba > last_usable {
            return None;
        }
        for previous in &out {
            if entry.first_lba <= previous.last_lba && previous.first_lba <= entry.last_lba {
                return None;
            }
            if entry.unique_guid == previous.unique_guid { return None; }
        }
        out.push(entry);
    }
    Some(out)
}

pub fn write_gpt_tables(
    device: &mut dyn BlockDevice,
    entries: &[GptPartitionEntry; 4],
    disk_guid: [u8; 16],
) -> Result<(), ()> {
    let info = device.info();
    let total_sectors = info.block_count;
    if info.block_size != 512 || total_sectors < 4096 { return Err(()); }

    let first_usable_lba = 34u64;
    let last_usable_lba = total_sectors.checked_sub(34).ok_or(())?;
    for (i, entry) in entries.iter().enumerate() {
        if !entry.is_used() || entry.first_lba < first_usable_lba || entry.last_lba > last_usable_lba {
            return Err(());
        }
        for other in entries.iter().skip(i + 1) {
            if entry.first_lba <= other.last_lba && other.first_lba <= entry.last_lba
                || entry.unique_guid == other.unique_guid
            { return Err(()); }
        }
    }

    let mut entry_array = [0u8; GPT_ARRAY_BYTES];
    for (i, e) in entries.iter().enumerate() {
        let off = i * GPT_ENTRY_SIZE as usize;
        entry_array[off..off + 16].copy_from_slice(&e.type_guid);
        entry_array[off + 16..off + 32].copy_from_slice(&e.unique_guid);
        entry_array[off + 32..off + 40].copy_from_slice(&e.first_lba.to_le_bytes());
        entry_array[off + 40..off + 48].copy_from_slice(&e.last_lba.to_le_bytes());
        entry_array[off + 48..off + 56].copy_from_slice(&e.attributes.to_le_bytes());
        let name: &[u8] = match i { 0 => b"Netizen_ESP", 1 => b"Netizen_System", 2 => b"Netizen_Security", _ => b"Netizen_Data" };
        for (idx, &ch) in name.iter().enumerate() {
            let p = off + 56 + idx * 2;
            entry_array[p] = ch;
        }
    }

    let entries_crc = crc32_ieee(&entry_array);
    if disk_guid == [0; 16] { return Err(()); }
    let primary_header_lba = 1u64;
    let primary_entries_lba = 2u64;
    let backup_header_lba = total_sectors - 1;
    let backup_entries_lba = total_sectors - 1 - GPT_ARRAY_SECTORS as u64;

    let build_header = |my_lba: u64, alt_lba: u64, part_lba: u64| -> [u8; 512] {
        let mut h = [0u8; 512];
        h[0..8].copy_from_slice(&GPT_SIGNATURE);
        h[8..12].copy_from_slice(&GPT_REVISION_1_0.to_le_bytes());
        h[12..16].copy_from_slice(&GPT_HEADER_SIZE.to_le_bytes());
        h[24..32].copy_from_slice(&my_lba.to_le_bytes());
        h[32..40].copy_from_slice(&alt_lba.to_le_bytes());
        h[40..48].copy_from_slice(&first_usable_lba.to_le_bytes());
        h[48..56].copy_from_slice(&last_usable_lba.to_le_bytes());
        h[56..72].copy_from_slice(&disk_guid);
        h[72..80].copy_from_slice(&part_lba.to_le_bytes());
        h[80..84].copy_from_slice(&(GPT_ENTRY_COUNT as u32).to_le_bytes());
        h[84..88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
        h[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        let crc = crc32_ieee(&h[..GPT_HEADER_SIZE as usize]);
        h[16..20].copy_from_slice(&crc.to_le_bytes());
        h
    };

    let primary_header = build_header(primary_header_lba, backup_header_lba, primary_entries_lba);
    let backup_header = build_header(backup_header_lba, primary_header_lba, backup_entries_lba);
    device.write_blocks(primary_entries_lba, GPT_ARRAY_SECTORS, &entry_array)?;
    device.write_blocks(primary_header_lba, 1, &primary_header)?;
    device.write_blocks(backup_entries_lba, GPT_ARRAY_SECTORS, &entry_array)?;
    device.write_blocks(backup_header_lba, 1, &backup_header)?;
    Ok(())
}

pub fn verify_gpt_tables(
    device: &mut dyn BlockDevice,
    expected: &[GptPartitionEntry; 4],
) -> Result<(), ()> {
    let total = device.info().block_count;
    let primary = try_read_gpt_at(device, 1, total).ok_or(())?;
    let backup = try_read_gpt_at(device, total.checked_sub(1).ok_or(())?, total).ok_or(())?;
    if primary.len() != expected.len() || backup.len() != expected.len() { return Err(()); }
    for expected_entry in expected {
        if !primary.iter().any(|entry| entry == expected_entry)
            || !backup.iter().any(|entry| entry == expected_entry)
        { return Err(()); }
    }
    Ok(())
}
