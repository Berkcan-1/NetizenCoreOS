extern crate alloc;

use alloc::vec::Vec;
use crate::gpt::{parse_gpt_partitions, GUID_BASIC_DATA, GUID_EFI_SYSTEM, GUID_LINUX_FS};
use crate::AtaDevice;
use nc_hal::{BlockDevice, BlockDeviceInfo};

#[derive(Clone, Copy)]
pub struct PartitionInfo {
    pub start_lba: u64,
    pub sector_count: u64,
    pub partition_type: u8,
    pub bootable: bool,
}

#[derive(Clone, Copy)]
pub struct AtaPartition {
    pub ata: AtaDevice,
    pub start_lba: u64,
    pub sector_count: u64,
}

impl AtaPartition {
    pub fn new(ata: AtaDevice, start_lba: u64, sector_count: u64) -> Self {
        AtaPartition { ata, start_lba, sector_count }
    }
}

impl BlockDevice for AtaPartition {
    fn info(&self) -> BlockDeviceInfo {
        let mut info = self.ata.info();
        info.block_count = self.sector_count;
        info
    }

    fn read_blocks(&mut self, lba: u64, count: u32, buffer: &mut [u8]) -> Result<(), ()> {
        let end_lba = lba.checked_add(count as u64).ok_or(())?;
        if end_lba > self.sector_count { return Err(()); }
        let phys_lba = self.start_lba.checked_add(lba).ok_or(())?;
        self.ata.read_blocks(phys_lba, count, buffer)
    }

    fn write_blocks(&mut self, lba: u64, count: u32, buffer: &[u8]) -> Result<(), ()> {
        let end_lba = lba.checked_add(count as u64).ok_or(())?;
        if end_lba > self.sector_count { return Err(()); }
        let phys_lba = self.start_lba.checked_add(lba).ok_or(())?;
        self.ata.write_blocks(phys_lba, count, buffer)
    }
}

/// Parses the complete GPT table or the four legacy MBR entries.
pub fn parse_partitions(device: &mut dyn BlockDevice) -> Option<Vec<PartitionInfo>> {
    if let Some(entries) = parse_gpt_partitions(device) {
        let mut out = Vec::new();
        out.try_reserve(entries.len()).ok()?;
        for entry in entries.into_iter() {
            let partition_type = if entry.type_guid == GUID_EFI_SYSTEM { 0xEF }
                else if entry.type_guid == GUID_LINUX_FS { 0x83 }
                else if entry.type_guid == GUID_BASIC_DATA { 0x07 }
                else { 0xFF };
            out.push(PartitionInfo {
                start_lba: entry.first_lba,
                sector_count: entry.sector_count(),
                partition_type,
                bootable: partition_type == 0xEF,
            });
        }
        return Some(out);
    }

    let mut sector = [0u8; 512];
    device.read_blocks(0, 1, &mut sector).ok()?;
    if sector[510] != 0x55 || sector[511] != 0xAA { return None; }

    let mut partitions = Vec::new();
    partitions.try_reserve(4).ok()?;
    for i in 0..4 {
        let offset = 446 + i * 16;
        let p_type = sector[offset + 4];
        if p_type == 0 { continue; }
        // A Protective MBR is not a usable fallback partition table. If GPT parsing
        // failed, report failure rather than mounting the protective 0xEE extent.
        if p_type == 0xEE { return None; }
        let bootable = sector[offset] == 0x80;
        let start_lba = u32::from_le_bytes([
            sector[offset + 8], sector[offset + 9], sector[offset + 10], sector[offset + 11],
        ]) as u64;
        let sector_count = u32::from_le_bytes([
            sector[offset + 12], sector[offset + 13], sector[offset + 14], sector[offset + 15],
        ]) as u64;
        if sector_count > 0 {
            partitions.push(PartitionInfo { start_lba, sector_count, partition_type: p_type, bootable });
        }
    }
    Some(partitions)
}

/// Compatibility adapter for callers that still consume an MBR-sized four-slot array.
pub fn parse_mbr_partitions(device: &mut dyn BlockDevice) -> [Option<PartitionInfo>; 4] {
    let mut out = [None; 4];
    if let Some(entries) = parse_partitions(device) {
        for (slot, entry) in entries.into_iter().take(4).enumerate() {
            out[slot] = Some(entry);
        }
    }
    out
}
