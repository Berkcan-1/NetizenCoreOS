// crates/nc-storage/src/gpt.rs
use nc_hal::BlockDevice;

pub const GPT_SIGNATURE: [u8; 8] = *b"EFI PART";
pub const GPT_REVISION_1_0: u32 = 0x0001_0000;
pub const GPT_HEADER_SIZE: u32 = 92;
pub const GPT_ENTRY_SIZE: u32 = 128;

// Standart GUID sabitleri (Mixed-Endian UEFI formatı)
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
        type_guid: [0; 16],
        unique_guid: [0; 16],
        first_lba: 0,
        last_lba: 0,
        attributes: 0,
    };

    #[inline]
    pub fn is_used(&self) -> bool {
        self.type_guid != [0u8; 16] && self.first_lba > 0 && self.last_lba >= self.first_lba
    }

    #[inline]
    pub fn sector_count(&self) -> u64 {
        if self.is_used() {
            self.last_lba.saturating_sub(self.first_lba).saturating_add(1)
        } else {
            0
        }
    }
}

/// Standart IEEE 802.3 CRC32 hesaplaması (Tablosuz, no_std uyumlu)
pub fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Diskten GPT başlığını (LBA 1 veya yedek son LBA) doğrular ve ilk 4 bölümü döndürür.
pub fn parse_gpt_partitions(device: &mut dyn BlockDevice) -> Option<[Option<GptPartitionEntry>; 4]> {
    let total_sectors = device.info().block_count;
    if total_sectors < 68 {
        return None;
    }

    // Önce Primary GPT Header (LBA 1), bozuksa Backup GPT Header (son LBA) dene
    if let Some(entries) = try_read_gpt_at(device, 1, total_sectors) {
        return Some(entries);
    }
    try_read_gpt_at(device, total_sectors - 1, total_sectors)
}

fn try_read_gpt_at(
    device: &mut dyn BlockDevice,
    header_lba: u64,
    total_sectors: u64,
) -> Option<[Option<GptPartitionEntry>; 4]> {
    let mut hdr = [0u8; 512];
    device.read_blocks(header_lba, 1, &mut hdr).ok()?;

    if &hdr[0..8] != &GPT_SIGNATURE {
        return None;
    }
    let header_size = u32::from_le_bytes([hdr[12], hdr[13], hdr[14], hdr[15]]) as usize;
    if !(92..=512).contains(&header_size) {
        return None;
    }

    let stored_hdr_crc = u32::from_le_bytes([hdr[16], hdr[17], hdr[18], hdr[19]]);
    let mut hdr_for_crc = hdr;
    hdr_for_crc[16..20].copy_from_slice(&[0, 0, 0, 0]);
    if crc32_ieee(&hdr_for_crc[..header_size]) != stored_hdr_crc {
        return None;
    }

    let my_lba = u64::from_le_bytes(hdr[24..32].try_into().ok()?);
    if my_lba != header_lba {
        return None;
    }

    let first_usable = u64::from_le_bytes(hdr[40..48].try_into().ok()?);
    let last_usable = u64::from_le_bytes(hdr[48..56].try_into().ok()?);
    let entries_lba = u64::from_le_bytes(hdr[72..80].try_into().ok()?);
    let num_entries = u32::from_le_bytes(hdr[80..84].try_into().ok()?);
    let entry_size = u32::from_le_bytes(hdr[84..88].try_into().ok()?);

    if entry_size != GPT_ENTRY_SIZE || num_entries < 4 || entries_lba >= total_sectors {
        return None;
    }

    // İlk 4 girdi 1 sektör (4 * 128 = 512 bayt) içinde yer alır
    let mut entry_sec = [0u8; 512];
    device.read_blocks(entries_lba, 1, &mut entry_sec).ok()?;

    let mut out = [None; 4];
    for i in 0..4 {
        let off = i * 128;
        let mut type_guid = [0u8; 16];
        let mut unique_guid = [0u8; 16];
        type_guid.copy_from_slice(&entry_sec[off..off + 16]);
        unique_guid.copy_from_slice(&entry_sec[off + 16..off + 32]);
        let first_lba = u64::from_le_bytes(entry_sec[off + 32..off + 40].try_into().ok()?);
        let last_lba = u64::from_le_bytes(entry_sec[off + 40..off + 48].try_into().ok()?);
        let attributes = u64::from_le_bytes(entry_sec[off + 48..off + 56].try_into().ok()?);

        let entry = GptPartitionEntry {
            type_guid,
            unique_guid,
            first_lba,
            last_lba,
            attributes,
        };
        if entry.is_used() {
            if first_lba < first_usable || last_lba > last_usable || last_lba >= total_sectors {
                return None;
            }
            out[i] = Some(entry);
        }
    }
    Some(out)
}

/// Diske hem Primary GPT (LBA 1..3) hem de Backup GPT (Son 3 LBA) tablolarını CRC32 mühürlü yazar.
/// Not: LBA 2048'deki Stage2/Kernel alanına asla dokunmaz (LBA 1..33 arasını kullanır).
pub fn write_gpt_tables(
    device: &mut dyn BlockDevice,
    entries: &[GptPartitionEntry; 4],
) -> Result<(), ()> {
    let total_sectors = device.info().block_count;
    if total_sectors < 4096 {
        return Err(());
    }

    // 4 bölüm girdisi (4 * 128 = 512 bayt = 1 sektör) kullanarak hem yalın hem hızlı GPT tablosu kuruyoruz
    let num_entries: u32 = 4;
    let mut entry_sec = [0u8; 512];

    for (i, e) in entries.iter().enumerate() {
        let off = i * 128;
        entry_sec[off..off + 16].copy_from_slice(&e.type_guid);
        entry_sec[off + 16..off + 32].copy_from_slice(&e.unique_guid);
        entry_sec[off + 32..off + 40].copy_from_slice(&e.first_lba.to_le_bytes());
        entry_sec[off + 40..off + 48].copy_from_slice(&e.last_lba.to_le_bytes());
        entry_sec[off + 48..off + 56].copy_from_slice(&e.attributes.to_le_bytes());

        let name_ascii: &[u8] = match i {
            0 => b"Netizen_ESP",
            1 => b"Netizen_System",
            2 => b"Netizen_Security",
            _ => b"Netizen_Data",
        };
        for (c_idx, &ch) in name_ascii.iter().take(36).enumerate() {
            let pos = off + 56 + c_idx * 2;
            entry_sec[pos] = ch;
            entry_sec[pos + 1] = 0;
        }
    }

    let entries_crc = crc32_ieee(&entry_sec);
    let primary_hdr_lba = 1u64;
    let primary_entries_lba = 2u64;
    let backup_hdr_lba = total_sectors - 1;
    let backup_entries_lba = total_sectors - 2;
    let first_usable_lba = 34u64;
    let last_usable_lba = total_sectors - 34;

    let disk_guid: [u8; 16] = [
        0x4E, 0x45, 0x54, 0x49, 0x5A, 0x45, 0x4E, 0x5F, 0x47, 0x50, 0x54, 0x5F, 0x44, 0x49, 0x53,
        0x4B,
    ];

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
        h[80..84].copy_from_slice(&num_entries.to_le_bytes());
        h[84..88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
        h[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        let crc = crc32_ieee(&h[..92]);
        h[16..20].copy_from_slice(&crc.to_le_bytes());
        h
    };

    let pri_hdr = build_header(primary_hdr_lba, backup_hdr_lba, primary_entries_lba);
    let bak_hdr = build_header(backup_hdr_lba, primary_hdr_lba, backup_entries_lba);

    device.write_blocks(primary_entries_lba, 1, &entry_sec)?;
    device.write_blocks(primary_hdr_lba, 1, &pri_hdr)?;
    device.write_blocks(backup_entries_lba, 1, &entry_sec)?;
    device.write_blocks(backup_hdr_lba, 1, &bak_hdr)?;

    Ok(())
}
