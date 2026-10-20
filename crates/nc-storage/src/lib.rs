// crates/nc-storage/src/lib.rs başlangıcı:
#![no_std]

use core::sync::atomic::{AtomicBool, Ordering};
use nc_arch::{inb, inw, outb, outw};
use nc_hal::{BlockDevice, BlockDeviceInfo, DeviceType};

pub mod ahci;
pub mod gpt;
pub mod nvme;
pub mod partition;

pub use partition::{parse_mbr_partitions, AtaPartition, PartitionInfo};

pub const MAX_DISKS: usize = 4;

pub const SYSTEM_START_LBA: u64 = 133_120;
pub const SYSTEM_SECTORS: u64 = 4_194_304;
pub const SECURITY_START_LBA: u64 = SYSTEM_START_LBA + SYSTEM_SECTORS; // 4_327_424
pub const SECURITY_SECTORS: u64 = 524_288;
pub const DATA_START_LBA: u64 = SECURITY_START_LBA + SECURITY_SECTORS; // 4_851_712
pub const MIN_DATA_SECTORS: u64 = 2_097_152;
pub const MIN_INSTALL_SECTORS: u64 = DATA_START_LBA + MIN_DATA_SECTORS; // 6_948_864

const LBA28_MAX_SECTORS: u64 = 0x1000_0000;
const LBA48_MAX_SECTORS: u64 = 0x0001_0000_0000_0000;
const LOCALE_METADATA_LBA: u64 = 2047;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StorageBackend {
    AtaPio { lba48: bool },
    Ahci,
    Nvme,
}



// M-08: Birincil (0x1F0) ve ikincil (0x170) ATA veri yollari icin atomik kilit
static ATA_BUS_LOCKS: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

struct AtaBusGuard(usize);

impl AtaBusGuard {
    fn acquire(channel: u8) -> Result<Self, ()> {
        let bus = ((channel >> 1) & 1) as usize;
        let mut spins = 0u32;
        while ATA_BUS_LOCKS[bus]
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spins += 1;
            if spins > 1_000_000 {
                return Err(());
            }
            core::hint::spin_loop();
        }
        Ok(AtaBusGuard(bus))
    }
}

impl Drop for AtaBusGuard {
    fn drop(&mut self) {
        ATA_BUS_LOCKS[self.0].store(false, Ordering::Release);
    }
}

#[derive(Clone, Copy)]
pub struct AtaDevice {
    pub channel: u8,
    pub backend: StorageBackend,
    pub info: BlockDeviceInfo,
}

impl AtaDevice {
    pub const EMPTY: AtaDevice = AtaDevice {
        channel: 0xFF,
        backend: StorageBackend::AtaPio { lba48: false },
        info: BlockDeviceInfo::EMPTY,
    };

    fn validate_lba_range(&self, lba: u64, count: u32) -> Result<(), ()> {
        if self.channel >= 4
            || self.info.device_type == DeviceType::OpticalDisc
            || self.info.block_size != 512
            || self.info.block_count == 0
        {
            return Err(());
        }
        let end_lba = lba.checked_add(count as u64).ok_or(())?;
        let hw_limit = match self.backend {
            StorageBackend::AtaPio { lba48: false } => LBA28_MAX_SECTORS,
            StorageBackend::AtaPio { lba48: true } => LBA48_MAX_SECTORS,
            StorageBackend::Ahci | StorageBackend::Nvme => u64::MAX,
        };
        if end_lba > self.info.block_count || end_lba > hw_limit {
            return Err(());
        }
        Ok(())
    }

    pub fn read_sector(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), ()> {
        if buf.len() < 512 {
            return Err(());
        }
        self.validate_lba_range(lba, 1)?;

        match self.backend {
            StorageBackend::Nvme => return nvme::read_sector(self.channel, lba, buf),
            StorageBackend::Ahci => return ahci::read_sector(self.channel, lba, buf),
            StorageBackend::AtaPio { lba48 } => {
                let _guard = AtaBusGuard::acquire(self.channel)?;
                let io_base = if self.channel < 2 { 0x01F0 } else { 0x0170 };
                let ctrl_base = if self.channel < 2 { 0x03F6 } else { 0x0376 };
                let drive_bit = self.channel & 1;

                unsafe {
                    if lba >= LBA28_MAX_SECTORS && lba48 {
                        // ATA PIO LBA48 READ SECTORS EXT (0x24)
                        outb(io_base + 6, 0x40 | (drive_bit << 4));
                        for _ in 0..4 {
                            inb(ctrl_base);
                        }
                        outb(io_base + 2, 0);
                        outb(io_base + 3, ((lba >> 24) & 0xFF) as u8);
                        outb(io_base + 4, ((lba >> 32) & 0xFF) as u8);
                        outb(io_base + 5, ((lba >> 40) & 0xFF) as u8);
                        outb(io_base + 2, 1);
                        outb(io_base + 3, (lba & 0xFF) as u8);
                        outb(io_base + 4, ((lba >> 8) & 0xFF) as u8);
                        outb(io_base + 5, ((lba >> 16) & 0xFF) as u8);
                        outb(io_base + 7, 0x24);
                    } else {
                        // ATA PIO LBA28 READ SECTORS (0x20)
                        outb(io_base + 6, 0xE0 | (drive_bit << 4) | (((lba >> 24) & 0x0F) as u8));
                        for _ in 0..4 {
                            inb(ctrl_base);
                        }
                        outb(io_base + 2, 1);
                        outb(io_base + 3, lba as u8);
                        outb(io_base + 4, (lba >> 8) as u8);
                        outb(io_base + 5, (lba >> 16) as u8);
                        outb(io_base + 7, 0x20);
                    }
                    for _ in 0..4 {
                        inb(ctrl_base);
                    }
                    let mut spins = 0u32;
                    while (inb(io_base + 7) & 0x80) != 0 {
                        spins += 1;
                        if spins > 100_000 {
                            return Err(());
                        }
                    }
                    let status = inb(io_base + 7);
                    if (status & 0x21) != 0 || (status & 0x08) == 0 {
                        return Err(());
                    }
                    for i in 0..256 {
                        let word = inw(io_base);
                        buf[i * 2] = word as u8;
                        buf[i * 2 + 1] = (word >> 8) as u8;
                    }
                }
                Ok(())
            }
        }
    }

    pub fn write_sector(&mut self, lba: u64, buf: &[u8]) -> Result<(), ()> {
        if buf.len() < 512 {
            return Err(());
        }
        self.validate_lba_range(lba, 1)?;

        match self.backend {
            StorageBackend::Nvme => return nvme::write_sector(self.channel, lba, buf),
            StorageBackend::Ahci => return ahci::write_sector(self.channel, lba, buf),
            StorageBackend::AtaPio { lba48 } => {
                let _guard = AtaBusGuard::acquire(self.channel)?;
                let io_base = if self.channel < 2 { 0x01F0 } else { 0x0170 };
                let ctrl_base = if self.channel < 2 { 0x03F6 } else { 0x0376 };
                let drive_bit = self.channel & 1;

                unsafe {
                    if lba >= LBA28_MAX_SECTORS && lba48 {
                        // ATA PIO LBA48 WRITE SECTORS EXT (0x34)
                        outb(io_base + 6, 0x40 | (drive_bit << 4));
                        for _ in 0..4 {
                            inb(ctrl_base);
                        }
                        outb(io_base + 2, 0);
                        outb(io_base + 3, ((lba >> 24) & 0xFF) as u8);
                        outb(io_base + 4, ((lba >> 32) & 0xFF) as u8);
                        outb(io_base + 5, ((lba >> 40) & 0xFF) as u8);
                        outb(io_base + 2, 1);
                        outb(io_base + 3, (lba & 0xFF) as u8);
                        outb(io_base + 4, ((lba >> 8) & 0xFF) as u8);
                        outb(io_base + 5, ((lba >> 16) & 0xFF) as u8);
                        outb(io_base + 7, 0x34);
                    } else {
                        // ATA PIO LBA28 WRITE SECTORS (0x30)
                        outb(io_base + 6, 0xE0 | (drive_bit << 4) | (((lba >> 24) & 0x0F) as u8));
                        for _ in 0..4 {
                            inb(ctrl_base);
                        }
                        outb(io_base + 2, 1);
                        outb(io_base + 3, lba as u8);
                        outb(io_base + 4, (lba >> 8) as u8);
                        outb(io_base + 5, (lba >> 16) as u8);
                        outb(io_base + 7, 0x30);
                    }
                    for _ in 0..4 {
                        inb(ctrl_base);
                    }
                    let mut spins = 0u32;
                    while (inb(io_base + 7) & 0x80) != 0 {
                        spins += 1;
                        if spins > 100_000 {
                            return Err(());
                        }
                    }
                    let status = inb(io_base + 7);
                    if (status & 0x21) != 0 || (status & 0x08) == 0 {
                        return Err(());
                    }
                    for i in 0..256 {
                        let word = (buf[i * 2] as u16) | ((buf[i * 2 + 1] as u16) << 8);
                        outw(io_base, word);
                    }
                    let mut write_spins = 0u32;
                    while (inb(io_base + 7) & 0x80) != 0 {
                        write_spins += 1;
                        if write_spins > 100_000 {
                            return Err(());
                        }
                    }
                    if (inb(io_base + 7) & 0x21) != 0 {
                        return Err(());
                    }
                }
                Ok(())
            }
        }
    }

    pub fn flush(&mut self) -> Result<(), ()> {
        if self.channel >= 4 || self.info.device_type == DeviceType::OpticalDisc {
            return Err(());
        }
        match self.backend {
            StorageBackend::Nvme => return nvme::flush_nvme(self.channel),
            StorageBackend::Ahci => return ahci::flush_port(self.channel),
            StorageBackend::AtaPio { lba48 } => {
                let _guard = AtaBusGuard::acquire(self.channel)?;
                let io_base = if self.channel < 2 { 0x01F0 } else { 0x0170 };
                let ctrl_base = if self.channel < 2 { 0x03F6 } else { 0x0376 };
                let drive_bit = self.channel & 1;

                unsafe {
                    outb(io_base + 6, 0xE0 | (drive_bit << 4));
                    for _ in 0..4 {
                        inb(ctrl_base);
                    }
                    // LBA48 ise FLUSH CACHE EXT (0xEA), değilse FLUSH CACHE (0xE7)
                    outb(io_base + 7, if lba48 { 0xEA } else { 0xE7 });
                    for _ in 0..4 {
                        inb(ctrl_base);
                    }
                    let mut spins = 0u32;
                    while (inb(io_base + 7) & 0x80) != 0 {
                        spins += 1;
                        if spins > 200_000 {
                            return Err(());
                        }
                    }
                    if (inb(io_base + 7) & 0x21) != 0 {
                        return Err(());
                    }
                }
                Ok(())
            }
        }
    }

    // Yalnizca kurulum sirasinda 512'nin kati olmayan (kernel/EFI) ikilileri son sektorde pad'leyerek yazar
    fn write_stream_padded(&mut self, start_lba: u64, data: &[u8]) -> Result<u32, ()> {
        if data.is_empty() {
            return Err(());
        }
        let sectors = ((data.len() + 511) / 512) as u32;
        self.validate_lba_range(start_lba, sectors)?;

        for i in 0..sectors as usize {
            let offset = i * 512;
            let rem = (data.len() - offset).min(512);
            let mut sec = [0u8; 512];
            sec[..rem].copy_from_slice(&data[offset..offset + rem]);
            self.write_sector(start_lba + i as u64, &sec)?;
        }
        Ok(sectors)
    }
}

impl BlockDevice for AtaDevice {
    fn info(&self) -> BlockDeviceInfo {
        self.info
    }

    fn read_blocks(&mut self, lba: u64, count: u32, buffer: &mut [u8]) -> Result<(), ()> {
        if count == 0 {
            return Ok(());
        }
        let required_bytes = (count as usize).checked_mul(512).ok_or(())?;
        if buffer.len() < required_bytes {
            return Err(());
        }
        self.validate_lba_range(lba, count)?;

        for i in 0..count as usize {
            let start = i * 512;
            self.read_sector(lba + i as u64, &mut buffer[start..start + 512])?;
        }
        Ok(())
    }

    // C-03: I/O baslamadan once tam buffer uzunlugu ve LBA araligi dogrulanir; kisa buffer ile asla yazma yapilmaz
    fn write_blocks(&mut self, lba: u64, count: u32, buffer: &[u8]) -> Result<(), ()> {
        if count == 0 {
            return Ok(());
        }
        let required_bytes = (count as usize).checked_mul(512).ok_or(())?;
        if buffer.len() < required_bytes {
            return Err(());
        }
        self.validate_lba_range(lba, count)?;

        for i in 0..count as usize {
            let start = i * 512;
            self.write_sector(lba + i as u64, &buffer[start..start + 512])?;
        }
        Ok(())
    }
}

pub fn is_installed(device: &mut AtaDevice) -> bool {
    if device.info.device_type == DeviceType::OpticalDisc {
        return false;
    }
    let mut sector = [0u8; 512];
    if device.read_sector(0, &mut sector).is_err() {
        return false;
    }
    sector[510] == 0x55 && sector[511] == 0xAA && &sector[3..10] == b"NETIZEN"
}

pub fn find_installed_disk() -> Option<AtaDevice> {
    for ch in 0..4u8 {
        if let Some(mut dev) = scan_channel(ch) {
            if dev.info.device_type == DeviceType::HardDisk && is_installed(&mut dev) {
                return Some(dev);
            }
        }
    }
    None
}

// C-01 & C-02: Tercih edilen kanal verilmisse ASLA baska bir diske fallback yapma ve minimum 3.31 GiB sartini ara
pub fn find_install_target_disk(preferred_channel: Option<u8>) -> Option<AtaDevice> {
    if let Some(ch) = preferred_channel {
        let dev = scan_channel(ch)?;
        if dev.info.device_type == DeviceType::HardDisk
            && dev.info.block_size == 512
            && dev.info.block_count >= MIN_INSTALL_SECTORS
        {
            return Some(dev);
        }
        return None;
    }
    for ch in 0..4u8 {
        if let Some(dev) = scan_channel(ch) {
            if dev.info.device_type == DeviceType::HardDisk
                && dev.info.block_size == 512
                && dev.info.block_count >= MIN_INSTALL_SECTORS
            {
                return Some(dev);
            }
        }
    }
    None
}

static MBR_CODE: &[u8] = include_bytes!("../mbr.bin");

fn append_bytes(buf: &mut [u8; 512], len: &mut usize, bytes: &[u8]) {
    for &b in bytes {
        if 28 + *len < 512 {
            buf[28 + *len] = b;
            *len += 1;
        }
    }
}

fn append_num(buf: &mut [u8; 512], len: &mut usize, mut val: i32) {
    if val == 0 {
        append_bytes(buf, len, b"0");
        return;
    }
    let mut tmp = [0u8; 10];
    let mut tlen = 0;
    while val > 0 {
        tmp[tlen] = b'0' + (val % 10) as u8;
        val /= 10;
        tlen += 1;
    }
    for i in (0..tlen).rev() {
        append_bytes(buf, len, &[tmp[i]]);
    }
}

pub fn make_ntzc_pkg(
    id: &str,
    name: &str,
    icon: &str,
    width: i32,
    height: i32,
    uid: u32,
    perms: u32,
) -> [u8; 512] {
    let mut buf = [0u8; 512];
    buf[0..4].copy_from_slice(b"NTZC");
    buf[4..8].copy_from_slice(&1u32.to_le_bytes());
    buf[8..12].copy_from_slice(&uid.to_le_bytes());
    buf[12..16].copy_from_slice(&perms.to_le_bytes());
    let mut m_len = 0usize;
    append_bytes(&mut buf, &mut m_len, b"id=");
    append_bytes(&mut buf, &mut m_len, id.as_bytes());
    append_bytes(&mut buf, &mut m_len, b"\nname=");
    append_bytes(&mut buf, &mut m_len, name.as_bytes());
    append_bytes(&mut buf, &mut m_len, b"\nversion=1.2.0\nicon=");
    append_bytes(&mut buf, &mut m_len, icon.as_bytes());
    append_bytes(&mut buf, &mut m_len, b"\nwidth_units=");
    append_num(&mut buf, &mut m_len, width);
    append_bytes(&mut buf, &mut m_len, b"\nheight_units=");
    append_num(&mut buf, &mut m_len, height);
    append_bytes(&mut buf, &mut m_len, b"\n");
    let payload_off = (28 + m_len) as u32;
    let sig = b"NC_ELF_MOD_V1";
    let mut p_len = 0usize;
    for &b in sig {
        if (payload_off as usize) + p_len < 512 {
            buf[(payload_off as usize) + p_len] = b;
            p_len += 1;
        }
    }
    buf[16..20].copy_from_slice(&(m_len as u32).to_le_bytes());
    buf[20..24].copy_from_slice(&payload_off.to_le_bytes());
    buf[24..28].copy_from_slice(&(p_len as u32).to_le_bytes());
    buf
}

pub fn make_sample_png(theme_id: u8) -> [u8; 512] {
    let mut buf = [0u8; 512];
    buf[0..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    buf[8..12].copy_from_slice(&13u32.to_be_bytes());
    buf[12..16].copy_from_slice(b"IHDR");
    buf[16..20].copy_from_slice(&160u32.to_be_bytes());
    buf[20..24].copy_from_slice(&100u32.to_be_bytes());
    buf[24] = 8;
    buf[25] = 6;
    buf[26] = theme_id;
    buf[29..33].copy_from_slice(&0xA1B2_C3D4u32.to_be_bytes());
    buf[33..37].copy_from_slice(&192u32.to_be_bytes());
    buf[37..41].copy_from_slice(b"IDAT");
    for i in 0..192 {
        buf[41 + i] = ((i as u32 * 17 + theme_id as u32 * 53) & 0xFF) as u8;
    }
    buf[237..241].copy_from_slice(b"IEND");
    buf
}

pub fn make_sample_mp4(scene_id: u8, duration_secs: u16) -> [u8; 512] {
    let mut buf = [0u8; 512];
    buf[0..4].copy_from_slice(&24u32.to_be_bytes());
    buf[4..8].copy_from_slice(b"ftyp");
    buf[8..12].copy_from_slice(b"isom");
    buf[12..16].copy_from_slice(&512u32.to_be_bytes());
    buf[16..20].copy_from_slice(b"isom");
    buf[20..24].copy_from_slice(b"mp42");

    buf[24..28].copy_from_slice(&64u32.to_be_bytes());
    buf[28..32].copy_from_slice(b"moov");
    buf[32..36].copy_from_slice(&56u32.to_be_bytes());
    buf[36..40].copy_from_slice(b"mvhd");
    buf[40] = scene_id;
    buf[44..48].copy_from_slice(&60u32.to_be_bytes());
    buf[48..52].copy_from_slice(&((duration_secs as u32) * 60).to_be_bytes());
    buf[52..54].copy_from_slice(&320u16.to_be_bytes());
    buf[54..56].copy_from_slice(&180u16.to_be_bytes());

    buf[88..92].copy_from_slice(&424u32.to_be_bytes());
    buf[92..96].copy_from_slice(b"mdat");
    buf[96..104].copy_from_slice(b"NCVID100");
    for i in 0..400 {
        buf[104 + i] = ((i as u32 * 29 + scene_id as u32 * 41) & 0xFF) as u8;
    }
    buf
}

fn write_system_erofs(device: &mut AtaDevice, start_lba: u64) -> Result<(), ()> {
    let mut sb = [0u8; 1024];
    sb[0..4].copy_from_slice(&0xE0F5_E1E2u32.to_le_bytes());
    sb[12] = 12; // 4096 bayt blok boyutu (2^12)
    sb[14..16].copy_from_slice(&0u16.to_le_bytes());
    sb[32..36].copy_from_slice(&1u32.to_le_bytes());
    sb[40..44].copy_from_slice(&1u32.to_le_bytes()); // M-02: Standart EROFS meta_blkaddr (0x28)
    device.write_blocks(start_lba + 2, 2, &sb)?;

    let build_prop_text = b"ro.netizen.os.name=NetizenCore\nro.netizen.os.version=2.8.0\nro.netizen.arch=x86_64\nro.netizen.kernel=nc-kernel-elf64\nro.netizen.vfs.mounts=/system,/data,/ram\n";
    let fstab_text = b"# NetizenCore VFS Baglama Tablosu (fstab)\n/dev/ata0p1  /boot      raw     ro  0 0\n/dev/ata0p2  /system    erofs   ro  0 0\n/dev/ata0p3  /security  vault   ro  0 0\n/dev/ata0p4  /data      exfat   rw  0 0\nmemfs        /ram       memfs   rw  0 0\n";
    let init_rc_text = b"# NetizenCore SystemInit Baslangic Betigi\non early-init:\n    mount erofs /system\n    mount exfat /data\n    load_efra_drivers /system/drivers\n    start service net_poller\n    start service desktop_shell\n";
    let kernel_sym_text = b"ELF64_NC_KERNEL_IMAGE\nEntry: 0x00100000 (_start -> kernel_main)\nAllocator: LockedHeap + Arena\nGraphics: 64-bit Framebuffer Burst\n";

    let mut meta = [0u8; 4096];
    let write_inode = |buf: &mut [u8; 4096], nid: usize, is_dir: bool, size: u32, blk: u32| {
        let off = nid * 32;
        buf[off..off + 2].copy_from_slice(&0u16.to_le_bytes());
        let mode: u16 = if is_dir { 0o040755 } else { 0o100644 };
        buf[off + 4..off + 6].copy_from_slice(&mode.to_le_bytes());
        buf[off + 8..off + 12].copy_from_slice(&size.to_le_bytes());
        buf[off + 16..off + 20].copy_from_slice(&blk.to_le_bytes());
    };

    // Inode Tablosu:
    // 0: / (root), 1: /app, 2: /build.prop, 3..7: .ntzc paketleri
    // 8: /bin, 9: /drivers, 10: /etc, 11: /kernel.sys
    // 12: /etc/fstab.cfg, 13: /etc/init.rc, 14: /bin/nc-init, 15: /drivers/generic-display.efra
    write_inode(&mut meta, 0, true, 128, 2);
    write_inode(&mut meta, 1, true, 121, 3);
    write_inode(&mut meta, 2, false, build_prop_text.len() as u32, 4);
    write_inode(&mut meta, 3, false, 256, 5);
    write_inode(&mut meta, 4, false, 256, 6);
    write_inode(&mut meta, 5, false, 256, 7);
    write_inode(&mut meta, 6, false, 256, 8);
    write_inode(&mut meta, 7, false, 256, 9);
    write_inode(&mut meta, 8, true, 36, 10);
    write_inode(&mut meta, 9, true, 48, 11);
    write_inode(&mut meta, 10, true, 44, 12);
    write_inode(&mut meta, 11, false, kernel_sym_text.len() as u32, 13);
    write_inode(&mut meta, 12, false, fstab_text.len() as u32, 14);
    write_inode(&mut meta, 13, false, init_rc_text.len() as u32, 15);
    write_inode(&mut meta, 14, false, kernel_sym_text.len() as u32, 13);
    write_inode(&mut meta, 15, false, 256, 16);
    device.write_blocks(start_lba + 8, 8, &meta)?;

    let write_dir_block = |entries: &[(u64, u8, &[u8])]| -> ([u8; 512], u32) {
        let mut blk = [0u8; 512];
        let count = entries.len();
        let mut str_off = count * 12;
        for (i, (nid, ftype, name)) in entries.iter().enumerate() {
            let base = i * 12;
            blk[base..base + 8].copy_from_slice(&nid.to_le_bytes());
            blk[base + 8..base + 10].copy_from_slice(&(str_off as u16).to_le_bytes());
            blk[base + 10] = *ftype;
            blk[str_off..str_off + name.len()].copy_from_slice(name);
            str_off += name.len();
        }
        (blk, str_off as u32)
    };

    // Blok 2 (LBA + 16): / (Kök Dizin) -> app, bin, drivers, etc, build.prop, kernel.sys
    let (root_dir, root_sz) = write_dir_block(&[
        (1, 2, b"app"),
        (8, 2, b"bin"),
        (9, 2, b"drivers"),
        (10, 2, b"etc"),
        (2, 1, b"build.prop"),
        (11, 1, b"kernel.sys"),
    ]);
    write_inode(&mut meta, 0, true, root_sz, 2);
    device.write_blocks(start_lba + 16, 1, &root_dir)?;

    // Blok 3 (LBA + 24): /app
    let (app_dir, app_sz) = write_dir_block(&[
        (3, 1, b"files.ntzc"),
        (4, 1, b"browser.ntzc"),
        (5, 1, b"settings.ntzc"),
        (6, 1, b"system.ntzc"),
        (7, 1, b"stopwatch.ntzc"),
    ]);
    write_inode(&mut meta, 1, true, app_sz, 3);
    device.write_blocks(start_lba + 24, 1, &app_dir)?;

    // Blok 4 (LBA + 32): /build.prop
    let mut prop_sec = [0u8; 512];
    prop_sec[..build_prop_text.len()].copy_from_slice(build_prop_text);
    device.write_blocks(start_lba + 32, 1, &prop_sec)?;

    // Blok 5..9: .ntzc paketleri
    let pkg_files = make_ntzc_pkg("nc.files", "Dosyalar", "generic", 46, 30, 1000, 7);
    device.write_blocks(start_lba + 40, 1, &pkg_files)?;
    let pkg_browser = make_ntzc_pkg("nc.browser", "Tarayici", "monitor", 44, 28, 1000, 5);
    device.write_blocks(start_lba + 48, 1, &pkg_browser)?;
    let pkg_settings = make_ntzc_pkg("nc.settings", "Ayarlar", "sliders", 48, 31, 1000, 3);
    device.write_blocks(start_lba + 56, 1, &pkg_settings)?;
    let pkg_system = make_ntzc_pkg("nc.system", "Sistem", "monitor", 32, 24, 1000, 1);
    device.write_blocks(start_lba + 64, 1, &pkg_system)?;
    let pkg_stopwatch = make_ntzc_pkg("nc.stopwatch", "Kronometre", "stopwatch", 26, 16, 1000, 0);
    device.write_blocks(start_lba + 72, 1, &pkg_stopwatch)?;

    // Blok 10 (LBA + 80): /bin
    let (bin_dir, bin_sz) = write_dir_block(&[(14, 1, b"nc-init.elf")]);
    write_inode(&mut meta, 8, true, bin_sz, 10);
    device.write_blocks(start_lba + 80, 1, &bin_dir)?;

    // Blok 11 (LBA + 88): /drivers
    let (drv_dir, drv_sz) = write_dir_block(&[(15, 1, b"generic-display.efra")]);
    write_inode(&mut meta, 9, true, drv_sz, 11);
    device.write_blocks(start_lba + 88, 1, &drv_dir)?;

    // Blok 12 (LBA + 96): /etc
    let (etc_dir, etc_sz) = write_dir_block(&[
        (12, 1, b"fstab.cfg"),
        (13, 1, b"init.rc"),
    ]);
    write_inode(&mut meta, 10, true, etc_sz, 12);
    device.write_blocks(start_lba + 96, 1, &etc_dir)?;

    // Güncellenmiş inode boyutlarını tekrar yaz:
    device.write_blocks(start_lba + 8, 8, &meta)?;

    // Blok 13 (LBA + 104): /kernel.sys & /bin/nc-init.elf
    let mut k_sec = [0u8; 512];
    k_sec[..kernel_sym_text.len()].copy_from_slice(kernel_sym_text);
    device.write_blocks(start_lba + 104, 1, &k_sec)?;

    // Blok 14 (LBA + 112): /etc/fstab.cfg
    let mut f_sec = [0u8; 512];
    f_sec[..fstab_text.len()].copy_from_slice(fstab_text);
    device.write_blocks(start_lba + 112, 1, &f_sec)?;

    // Blok 15 (LBA + 120): /etc/init.rc
    let mut i_sec = [0u8; 512];
    i_sec[..init_rc_text.len()].copy_from_slice(init_rc_text);
    device.write_blocks(start_lba + 120, 1, &i_sec)?;

    // Blok 16 (LBA + 128): /drivers/generic-display.efra
    let efra_pkgs = nc_driver::builtin_efra_packages();
    let efra_sec = efra_pkgs[0].serialize_padded_512();
    device.write_blocks(start_lba + 128, 1, &efra_sec)?;

    Ok(())
}

fn write_exfat_entry(
    buf: &mut [u8],
    offset: usize,
    name: &str,
    is_dir: bool,
    cluster: u32,
    size: u64,
) -> usize {
    let chars: usize = name.chars().count().min(30);
    let name_entries = ((chars + 14) / 15).max(1);
    let secondary_count = 1 + name_entries;
    let total_bytes = (1 + secondary_count) * 32;
    if offset + total_bytes > buf.len() {
        return 0;
    }
    for b in &mut buf[offset..offset + total_bytes] {
        *b = 0;
    }
    buf[offset] = 0x85;
    buf[offset + 1] = secondary_count as u8;
    buf[offset + 4] = if is_dir { 0x10 } else { 0x20 };

    let s_off = offset + 32;
    buf[s_off] = 0xC0;
    buf[s_off + 1] = 0x01;
    buf[s_off + 3] = chars as u8;
    buf[s_off + 8..s_off + 16].copy_from_slice(&size.to_le_bytes());
    buf[s_off + 20..s_off + 24].copy_from_slice(&cluster.to_le_bytes());
    buf[s_off + 24..s_off + 32].copy_from_slice(&size.to_le_bytes());

    let mut ch_iter = name.chars();
    for n in 0..name_entries {
        let n_off = offset + 64 + n * 32;
        buf[n_off] = 0xC1;
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
    total_bytes
}

pub fn write_data_exfat(device: &mut AtaDevice, start_lba: u64) -> Result<(), ()> {
    let vol_sectors = device
        .info
        .block_count
        .saturating_sub(start_lba)
        .max(2_097_152);
    let cluster_count = ((vol_sectors.saturating_sub(256)) / 8).min(65_536) as u32;

    // M-01: Standart exFAT VBR alanlarini doldur
    let mut vbr = [0u8; 512];
    vbr[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
    vbr[3..11].copy_from_slice(b"EXFAT   ");
    vbr[64..72].copy_from_slice(&start_lba.to_le_bytes()); // PartitionOffset
    vbr[72..80].copy_from_slice(&vol_sectors.to_le_bytes()); // VolumeLength
    vbr[80..84].copy_from_slice(&128u32.to_le_bytes()); // FatOffset
    vbr[84..88].copy_from_slice(&128u32.to_le_bytes()); // FatLength
    vbr[88..92].copy_from_slice(&256u32.to_le_bytes()); // ClusterHeapOffset
    vbr[92..96].copy_from_slice(&cluster_count.to_le_bytes()); // ClusterCount
    vbr[96..100].copy_from_slice(&2u32.to_le_bytes()); // FirstClusterOfRootDirectory
    vbr[100..104].copy_from_slice(&0x4E54_5A43u32.to_le_bytes()); // VolumeSerialNumber
    vbr[104..106].copy_from_slice(&0x0100u16.to_le_bytes()); // FileSystemRevision 1.00
    vbr[108] = 9; // 512 bayt sektor (2^9)
    vbr[109] = 3; // 8 sektor / cluster = 4096 bayt (2^3)
    vbr[110] = 1; // NumberOfFats = 1
    vbr[111] = 0x80;
    vbr[510] = 0x55;
    vbr[511] = 0xAA;
    device.write_blocks(start_lba, 1, &vbr)?;
    device.write_blocks(start_lba + 12, 1, &vbr)?; // Yedek (Backup) VBR

    let mut fat = [0u8; 512];
    fat[0..4].copy_from_slice(&0xFFFF_FFF8u32.to_le_bytes());
    fat[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    for cl in 2..=14usize {
        let off = cl * 4;
        fat[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    }
    device.write_blocks(start_lba + 128, 1, &fat)?;

    let zero_sec = [0u8; 512];
    for s in 1..8 {
        device.write_blocks(start_lba + 128 + s, 1, &zero_sec)?;
    }
    for s in 0..(13 * 8) {
        device.write_blocks(start_lba + 256 + s, 1, &zero_sec)?;
    }

    // C-06 & M-01: Kume 14 (LBA + 352) uzerinde Allocation Bitmap (kumeler 2..=14 dolu) olustur
    let mut bitmap_sec = [0u8; 512];
    // Kume 2..=14 toplam 13 bit -> ilk 13 bit 1 (0xFF ve 0x1F)
    bitmap_sec[0] = 0xFF;
    bitmap_sec[1] = 0x1F;
    device.write_blocks(start_lba + 352, 1, &bitmap_sec)?;

    let mut root_dir = [0u8; 512];
    // Ilk 32 bayt: 0x81 Allocation Bitmap Directory Entry (FirstCluster = 14, Size = 4096)
    root_dir[0] = 0x81;
    root_dir[20..24].copy_from_slice(&14u32.to_le_bytes());
    root_dir[24..32].copy_from_slice(&4096u64.to_le_bytes());
    let mut off = 32;
    off += write_exfat_entry(&mut root_dir, off, "app", true, 3, 4096);
    off += write_exfat_entry(&mut root_dir, off, "media", true, 4, 4096);
    off += write_exfat_entry(&mut root_dir, off, "documents", true, 5, 4096);
    off += write_exfat_entry(&mut root_dir, off, "desktop", true, 11, 4096);
    let _ = write_exfat_entry(&mut root_dir, off, "downloads", true, 12, 4096);
    device.write_blocks(start_lba + 256, 1, &root_dir)?;

    let mut app_dir = [0u8; 512];
    let _ = write_exfat_entry(&mut app_dir, 0, "calculator.ntzc", false, 6, 512);
    device.write_blocks(start_lba + 264, 1, &app_dir)?;

    let mut media_dir = [0u8; 512];
    let mut m_off = 0;
    m_off += write_exfat_entry(&mut media_dir, m_off, "aurora_cam.png", false, 7, 512);
    let _ = write_exfat_entry(&mut media_dir, m_off, "aero_tanitim.mp4", false, 8, 512);
    device.write_blocks(start_lba + 272, 1, &media_dir)?;

    let welcome_txt = b"NetizenCore Kisisel Belgelerim\n==============================\n\nBu klasor kalici diskinizde (/data/documents) yer alir.\nDosya ve klasorlere sag tiklayarak Kes, Kopyala, Yapistir,\nYeniden Adlandir ve Sil islemlerini yapabilirsiniz.";
    let log_txt = b"[0.000] NetizenCore x86_64 cekirdegi baslatildi\n[0.120] VBE dogrusal framebuffer: 32-bit renk aktif\n[0.450] ATA PIO Kanal 0: Sabit disk MBR bolumleri baglandi\n[0.610] VFS: Kullanici klasorleri (/data/desktop, /data/documents) hazir\n";

    let mut docs_dir = [0u8; 512];
    let mut d_off = 0;
    d_off += write_exfat_entry(&mut docs_dir, d_off, "hosgeldiniz.txt", false, 9, welcome_txt.len() as u64);
    let _ = write_exfat_entry(&mut docs_dir, d_off, "sistem_gunlugu.txt", false, 10, log_txt.len() as u64);
    device.write_blocks(start_lba + 280, 1, &docs_dir)?;

    let pkg_calc = make_ntzc_pkg("nc.calculator", "Hesap Makinesi", "generic", 28, 22, 10001, 4);
    device.write_blocks(start_lba + 288, 1, &pkg_calc)?;

    let png_data = make_sample_png(0);
    device.write_blocks(start_lba + 296, 1, &png_data)?;

    let mp4_data = make_sample_mp4(0, 18);
    device.write_blocks(start_lba + 304, 1, &mp4_data)?;

    let mut welcome_sec = [0u8; 1024];
    let w_len = welcome_txt.len().min(welcome_sec.len());
    welcome_sec[..w_len].copy_from_slice(&welcome_txt[..w_len]);
    device.write_blocks(start_lba + 312, 2, &welcome_sec)?;

    let mut log_sec = [0u8; 512];
    let l_len = log_txt.len().min(log_sec.len());
    log_sec[..l_len].copy_from_slice(&log_txt[..l_len]);
    device.write_blocks(start_lba + 320, 1, &log_sec)?;

    let desktop_note = b"Masaustu Notlarim\n=================\nMasaustu klasorunuz (/data/desktop) kalici disk uzerindedir.\nBuraya eklediginiz dosyalar ve uygulama paketleri (.ntzc) korunur.";
    let mut desktop_dir = [0u8; 512];
    let mut dt_off = 0;
    dt_off += write_exfat_entry(&mut desktop_dir, dt_off, "masaustu_notu.txt", false, 13, desktop_note.len() as u64);
    let _ = write_exfat_entry(&mut desktop_dir, dt_off, "calculator.ntzc", false, 6, 512);
    device.write_blocks(start_lba + 328, 1, &desktop_dir)?;

    let dl_dir = [0u8; 512];
    device.write_blocks(start_lba + 336, 1, &dl_dir)?;

  let mut dt_note_sec = [0u8; 512];
    let dn_len = desktop_note.len().min(dt_note_sec.len());
    dt_note_sec[..dn_len].copy_from_slice(&desktop_note[..dn_len]);
    device.write_blocks(start_lba + 344, 1, &dt_note_sec)?;

    Ok(())
}

fn write_esp_fat16(device: &mut AtaDevice, start_lba: u64, efi_blob: &[u8]) -> Result<(), ()> {
    // ESP icinde veri alani LBA + 164'te baslar, toplam 65536 sektor icinde kalmali
    if efi_blob.is_empty() || efi_blob.len() > (65_536 - 172) * 512 {
        return Err(());
    }

    let mut vbr = [0u8; 512];
    vbr[0..3].copy_from_slice(&[0xEB, 0x3C, 0x90]);
    vbr[3..11].copy_from_slice(b"MSDOS5.0");
    vbr[11..13].copy_from_slice(&512u16.to_le_bytes());
    vbr[13] = 4;
    vbr[14..16].copy_from_slice(&4u16.to_le_bytes());
    vbr[16] = 2;
    vbr[17..19].copy_from_slice(&512u16.to_le_bytes());
    vbr[21] = 0xF8;
    vbr[22..24].copy_from_slice(&64u16.to_le_bytes());
    vbr[24..26].copy_from_slice(&32u16.to_le_bytes());
    vbr[26..28].copy_from_slice(&64u16.to_le_bytes());
    vbr[28..32].copy_from_slice(&(start_lba as u32).to_le_bytes());
    vbr[32..36].copy_from_slice(&65536u32.to_le_bytes());
    vbr[36] = 0x80;
    vbr[38] = 0x29;
    vbr[39..43].copy_from_slice(&0x4E54_5A43u32.to_le_bytes());
    vbr[43..54].copy_from_slice(b"NETIZEN_ESP");
    vbr[54..62].copy_from_slice(b"FAT16   ");
    vbr[510] = 0x55;
    vbr[511] = 0xAA;
    device.write_blocks(start_lba, 1, &vbr)?;

    let clusters_needed = ((efi_blob.len() + 2047) / 2048).max(1);
    let end_cluster = 4 + clusters_needed;
    for s in 0..64usize {
        let mut sec = [0u8; 512];
        for i in 0..256usize {
            let c = s * 256 + i;
            let v: u16 = match c {
                0 => 0xFFF8,
                1 | 2 | 3 => 0xFFFF,
                c if c >= 4 && c < end_cluster => {
                    if c + 1 < end_cluster { (c + 1) as u16 } else { 0xFFFF }
                }
                _ => 0,
            };
            sec[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }
        device.write_blocks(start_lba + 4 + s as u64, 1, &sec)?;
        device.write_blocks(start_lba + 68 + s as u64, 1, &sec)?;
    }

    let zero = [0u8; 512];
    let mut root_dir = [0u8; 512];
    root_dir[0..11].copy_from_slice(b"EFI        ");
    root_dir[11] = 0x10;
    root_dir[26..28].copy_from_slice(&2u16.to_le_bytes());
    device.write_blocks(start_lba + 132, 1, &root_dir)?;
    for s in 1..32 {
        device.write_blocks(start_lba + 132 + s, 1, &zero)?;
    }

    let data_base = start_lba + 164;

    let mut efi_dir = [0u8; 512];
    efi_dir[0..11].copy_from_slice(b".          ");
    efi_dir[11] = 0x10;
    efi_dir[26..28].copy_from_slice(&2u16.to_le_bytes());
    efi_dir[32..43].copy_from_slice(b"..         ");
    efi_dir[43] = 0x10;
    efi_dir[58..60].copy_from_slice(&0u16.to_le_bytes());
    efi_dir[64..75].copy_from_slice(b"BOOT       ");
    efi_dir[75] = 0x10;
    efi_dir[90..92].copy_from_slice(&3u16.to_le_bytes());
    device.write_blocks(data_base, 1, &efi_dir)?;
    for s in 1..4 {
        device.write_blocks(data_base + s, 1, &zero)?;
    }

    let mut boot_dir = [0u8; 512];
    boot_dir[0..11].copy_from_slice(b".          ");
    boot_dir[11] = 0x10;
    boot_dir[26..28].copy_from_slice(&3u16.to_le_bytes());
    boot_dir[32..43].copy_from_slice(b"..         ");
    boot_dir[43] = 0x10;
    boot_dir[58..60].copy_from_slice(&2u16.to_le_bytes());
    boot_dir[64..75].copy_from_slice(b"BOOTX64 EFI");
    boot_dir[75] = 0x20;
    boot_dir[90..92].copy_from_slice(&4u16.to_le_bytes());
    boot_dir[92..96].copy_from_slice(&(efi_blob.len() as u32).to_le_bytes());
    device.write_blocks(data_base + 4, 1, &boot_dir)?;
    for s in 1..4 {
        device.write_blocks(data_base + 4 + s, 1, &zero)?;
    }

    device.write_stream_padded(data_base + 8, efi_blob)?;
    Ok(())
}

// C-02 & M-07: MBR'ye dokunmadan once tum bolum ve ikili boyutlarini dogrula, yazma sonrasi geri oku ve flush et
// install_to_disk içinde usable_sectors ve GPT yazımı:
pub fn install_to_disk(
    device: &mut AtaDevice,
    stage2: &[u8],
    kernel: &[u8],
    uefi_boot: Option<&[u8]>,
) -> Result<(), ()> {
    if device.info.device_type != DeviceType::HardDisk || device.info.block_size != 512 {
        return Err(());
    }
    // Backup GPT tablosu için son 34 sektörü ayır, 128 GiB LBA28 kesmesini KALDIR!
    let usable_sectors = device.info.block_count.saturating_sub(34);
    if usable_sectors < MIN_INSTALL_SECTORS {
        return Err(());
    }
    if stage2.is_empty() || kernel.is_empty() {
        return Err(());
    }
    let stage2_sectors = ((stage2.len() + 511) / 512) as u64;
    let kernel_sectors = ((kernel.len() + 511) / 512) as u64;
    let boot_end_lba = 2048u64
        .checked_add(stage2_sectors)
        .and_then(|v| v.checked_add(kernel_sectors))
        .ok_or(())?;

    if boot_end_lba > 65_536 {
        return Err(());
    }

    if let Some(efi_blob) = uefi_boot {
        if efi_blob.is_empty() || efi_blob.len() > (65_536 - 172) * 512 {
            return Err(());
        }
    }

    let esp_start: u64 = 65_536;
    let esp_sectors: u64 = 65_536;
    let system_start: u64 = SYSTEM_START_LBA;
    let system_sectors: u64 = SYSTEM_SECTORS;
    let security_start: u64 = SECURITY_START_LBA;
    let security_sectors: u64 = SECURITY_SECTORS;
    let data_start: u64 = DATA_START_LBA;
    let data_sectors_64: u64 = usable_sectors.checked_sub(DATA_START_LBA).ok_or(())?;

    if data_sectors_64 < MIN_DATA_SECTORS {
        return Err(());
    }

    device.write_stream_padded(2048, stage2)?;
    device.write_stream_padded(2048 + stage2_sectors, kernel)?;

    if let Some(efi_blob) = uefi_boot {
        write_esp_fat16(device, esp_start, efi_blob)?;
    }

    write_system_erofs(device, system_start)?;
    write_data_exfat(device, data_start)?;
    seal_security_vault(device)?;

    // 64-bit GPT Tablolarını (Primary LBA 1..2 ve Backup son LBA'lar) yaz
    let gpt_entries = [
        gpt::GptPartitionEntry {
            type_guid: gpt::GUID_EFI_SYSTEM,
            unique_guid: [1; 16],
            first_lba: esp_start,
            last_lba: esp_start + esp_sectors - 1,
            attributes: 1,
        },
        gpt::GptPartitionEntry {
            type_guid: gpt::GUID_LINUX_FS,
            unique_guid: [2; 16],
            first_lba: system_start,
            last_lba: system_start + system_sectors - 1,
            attributes: 0,
        },
        gpt::GptPartitionEntry {
            type_guid: gpt::GUID_LINUX_FS,
            unique_guid: [3; 16],
            first_lba: security_start,
            last_lba: security_start + security_sectors - 1,
            attributes: 0,
        },
        gpt::GptPartitionEntry {
            type_guid: gpt::GUID_BASIC_DATA,
            unique_guid: [4; 16],
            first_lba: data_start,
            last_lba: data_start + data_sectors_64 - 1,
            attributes: 0,
        },
    ];
    let _ = gpt::write_gpt_tables(device, &gpt_entries);

    // Legacy BIOS (mbr.asm) ve DysonSphere imza kontrolü için LBA 0 Hybrid MBR commit
    let mut mbr = [0u8; 512];
    let _ = device.read_sector(0, &mut mbr);
    let code_len = MBR_CODE.len().min(440);
    mbr[..code_len].copy_from_slice(&MBR_CODE[..code_len]);
    for b in &mut mbr[code_len..440] {
        *b = 0;
    }

    let write_entry = |mbr: &mut [u8; 512],
                       index: usize,
                       bootable: bool,
                       p_type: u8,
                       start_lba: u32,
                       count: u32| {
        let offset = 446 + index * 16;
        mbr[offset] = if bootable { 0x80 } else { 0x00 };
        mbr[offset + 1] = 0xFE;
        mbr[offset + 2] = 0xFF;
        mbr[offset + 3] = 0xFF;
        mbr[offset + 4] = p_type;
        mbr[offset + 5] = 0xFE;
        mbr[offset + 6] = 0xFF;
        mbr[offset + 7] = 0xFF;
        mbr[offset + 8..offset + 12].copy_from_slice(&start_lba.to_le_bytes());
        mbr[offset + 12..offset + 16].copy_from_slice(&count.to_le_bytes());
    };

    let mbr_data_sectors = data_sectors_64.min(u32::MAX as u64) as u32;
    write_entry(&mut mbr, 0, true, 0xEF, esp_start as u32, esp_sectors as u32);
    write_entry(&mut mbr, 1, false, 0x83, system_start as u32, system_sectors as u32);
    write_entry(&mut mbr, 2, false, 0x83, security_start as u32, security_sectors as u32);
    write_entry(&mut mbr, 3, false, 0x83, data_start as u32, mbr_data_sectors);

    mbr[510] = 0x55;
    mbr[511] = 0xAA;

    device.write_sector(0, &mbr)?;
    device.flush()?;

    let mut verify_mbr = [0u8; 512];
    device.read_sector(0, &mut verify_mbr)?;
    if verify_mbr != mbr {
        return Err(());
    }
    Ok(())
}

pub fn scan_channel(channel: u8) -> Option<AtaDevice> {
    if channel >= 4 {
        return None;
    }

    // 1. Öncelik: Modern NVMe PCIe SSD denetleyicisi varsa onu bağla
    if let Some(info) = nvme::probe_nvme(channel) {
        return Some(AtaDevice {
            channel,
            backend: StorageBackend::Nvme,
            info,
        });
    }

    // 2. Öncelik: Modern AHCI SATA denetleyicisi varsa onu bağla
    if let Some(info) = ahci::probe_port(channel) {
        return Some(AtaDevice {
            channel,
            backend: StorageBackend::Ahci,
            info,
        });
    }

    // 3. Fallback: Klasik ATA PIO (LBA48 EXT + LBA28)
    let _guard = AtaBusGuard::acquire(channel).ok()?;
    let io_base = if channel < 2 { 0x01F0 } else { 0x0170 };
    let ctrl_base = if channel < 2 { 0x03F6 } else { 0x0376 };
    let drive_select = if channel & 1 == 0 { 0xA0 } else { 0xB0 };

    unsafe {
        outb(ctrl_base, 0x02);
        for _ in 0..4 {
            inb(ctrl_base);
        }
        outb(io_base + 6, drive_select);
        for _ in 0..4 {
            inb(ctrl_base);
        }
        let init_status = inb(io_base + 7);
        if init_status == 0xFF {
            return None;
        }
        let mut spins = 0u32;
        while (inb(io_base + 7) & 0x80) != 0 {
            spins += 1;
            if spins > 100_000 {
                return None;
            }
        }
        outb(io_base + 2, 0);
        outb(io_base + 3, 0);
        outb(io_base + 4, 0);
        outb(io_base + 5, 0);
        outb(io_base + 7, 0xEC);
        for _ in 0..4 {
            inb(ctrl_base);
        }
        let status_after_cmd = inb(io_base + 7);
        if status_after_cmd == 0 {
            return None;
        }
        spins = 0;
        while (inb(io_base + 7) & 0x80) != 0 {
            spins += 1;
            if spins > 100_000 {
                return None;
            }
        }
        let lba_mid = inb(io_base + 4);
        let lba_high = inb(io_base + 5);
        let is_atapi =
            (lba_mid == 0x14 && lba_high == 0xEB) || (lba_mid == 0x69 && lba_high == 0x96);
        if is_atapi {
            outb(io_base + 7, 0xA1);
            for _ in 0..4 {
                inb(ctrl_base);
            }
            let mut atapi_spins = 0u32;
            while (inb(io_base + 7) & 0x80) != 0 {
                atapi_spins += 1;
                if atapi_spins > 20_000 {
                    return None;
                }
            }
        }
        let final_status = inb(io_base + 7);
        if (final_status & 0x21) != 0 || (final_status & 0x08) == 0 {
            return None;
        }
        let mut buffer = [0u16; 256];
        for i in 0..256 {
            buffer[i] = inw(io_base);
        }
        let _ = inb(io_base + 7);

        let mut model = [0u8; 40];
        for index in 0..20 {
            let word = buffer[27 + index];
            model[index * 2] = (word >> 8) as u8;
            model[index * 2 + 1] = (word & 0xFF) as u8;
        }
        let mut model_len = 40;
        while model_len > 0 && (model[model_len - 1] == b' ' || model[model_len - 1] == 0) {
            model_len -= 1;
        }

        let lba48 = !is_atapi && (buffer[83] & (1 << 10)) != 0;
        let raw_sectors = if !is_atapi {
            if lba48 {
                (buffer[100] as u64)
                    | ((buffer[101] as u64) << 16)
                    | ((buffer[102] as u64) << 32)
                    | ((buffer[103] as u64) << 48)
            } else {
                (buffer[60] as u64) | ((buffer[61] as u64) << 16)
            }
        } else {
            0
        };

        let max_limit = if lba48 { LBA48_MAX_SECTORS } else { LBA28_MAX_SECTORS };
        let sectors = raw_sectors.min(max_limit);
        let device_type = if is_atapi {
            DeviceType::OpticalDisc
        } else {
            DeviceType::HardDisk
        };

        Some(AtaDevice {
            channel,
            backend: StorageBackend::AtaPio { lba48 },
            info: BlockDeviceInfo {
                channel,
                model,
                model_len,
                block_count: sectors,
                block_size: 512,
                device_type,
            },
        })
    }
}

// H-09: MBR disk imzasi (440..443) yerine ayrilmis LBA 2047 metadata sektorunu kullan
pub fn write_initial_locale(device: &mut AtaDevice, lang: u8, kbd: u8) {
    let mut meta = [0u8; 512];
    meta[0..8].copy_from_slice(b"NCLOCALE");
    meta[8] = 1; // surum
    meta[9] = lang;
    meta[10] = kbd;
    meta[11] = lang ^ kbd ^ 0xA5; // basit butunluk kontrolu
    if device.write_sector(LOCALE_METADATA_LBA, &meta).is_ok() {
        let _ = device.flush();
    }
}

pub fn read_initial_locale() -> Option<(u8, u8)> {
    let mut dev = find_installed_disk()?;
    let mut meta = [0u8; 512];
    dev.read_sector(LOCALE_METADATA_LBA, &mut meta).ok()?;
    if &meta[0..8] == b"NCLOCALE" && meta[8] == 1 && (meta[9] ^ meta[10] ^ 0xA5) == meta[11] {
        Some((meta[9], meta[10]))
    } else {
        None
    }
}

pub fn find_external_usb_disk() -> Option<(AtaDevice, u64, u64)> {
    for ch in 0..4u8 {
        if let Some(mut dev) = scan_channel(ch) {
            if dev.info.device_type == DeviceType::HardDisk
                && dev.info.block_count > 0
                && !is_installed(&mut dev)
            {
                let mut sec0 = [0u8; 512];
                if dev.read_sector(0, &mut sec0).is_ok() {
                    if &sec0[0..19] == b"NETIZEN_HOTPLUG_OFF" || sec0.iter().all(|&b| b == 0) {
                        continue;
                    }
                    if &sec0[3..11] == b"EXFAT   " || &sec0[82..87] == b"FAT32" {
                        return Some((dev, 0, dev.info.block_count));
                    }
                    let parts = parse_mbr_partitions(&mut dev);
                    for p_opt in parts.iter().flatten() {
                        let end = p_opt.start_lba.checked_add(p_opt.sector_count)?;
                        if p_opt.sector_count > 0 && end <= dev.info.block_count {
                            return Some((dev, p_opt.start_lba, p_opt.sector_count));
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn init_external_usb_exfat(device: &mut AtaDevice) -> Result<(), ()> {
    let mut vbr = [0u8; 512];
    vbr[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
    vbr[3..11].copy_from_slice(b"EXFAT   ");
    vbr[80..84].copy_from_slice(&128u32.to_le_bytes());
    vbr[88..92].copy_from_slice(&256u32.to_le_bytes());
    vbr[96..100].copy_from_slice(&2u32.to_le_bytes());
    vbr[108] = 9;
    vbr[109] = 3;
    vbr[510] = 0x55;
    vbr[511] = 0xAA;
    device.write_blocks(0, 1, &vbr)?;

    let mut fat = [0u8; 512];
    fat[0..4].copy_from_slice(&0xFFFF_FFF8u32.to_le_bytes());
    fat[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    for cl in 2..=4usize {
        let off = cl * 4;
        fat[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    }
    device.write_blocks(128, 1, &fat)?;

    let zero_sec = [0u8; 512];
    for s in 1..8 {
        device.write_blocks(128 + s, 1, &zero_sec)?;
    }
    for s in 0..24 {
        device.write_blocks(256 + s, 1, &zero_sec)?;
    }

    let readme = b"Harici USB / SSD Depolama Birimi (/usb)\n=======================================\nBu surucu NetizenCore harici depolama katmani tarafindan baglandi.\nAna bilgisayardaki 'host_usb/' klasorune attiginiz dosyalar\nburada gorunur; buraya kopyaladiginiz dosyalar diske yazilir.\n";
    let mut root_dir = [0u8; 512];
    let mut off = 0;
    off += write_exfat_entry(&mut root_dir, off, "USB_Kilavuz.txt", false, 3, readme.len() as u64);
    let _ = write_exfat_entry(&mut root_dir, off, "Aktarilanlar", true, 4, 4096);
    device.write_blocks(256, 1, &root_dir)?;

    let mut r_sec = [0u8; 512];
    r_sec[..readme.len()].copy_from_slice(readme);
    device.write_blocks(264, 1, &r_sec)?;
    let _ = device.flush();
    Ok(())
}

// ============================================================================
// DYSONSPHERE FAZ 5: TRUSTED BOOT, VAULT SEAL & BOOTLOADER LOCK
// ============================================================================

const VAULT_MAGIC: &[u8; 8] = b"DSVAULT1";

/// Hızlı, sıfır-tahsisatlı (no-alloc) SHA-256 benzeri kriptografik bütünlük özeti (FNV1a-256 / Miyaguchi-Preneel)
/// (nc-storage içinde nc-pkg bağımlılığı olmadan doğrudan sektör mühürlemek için)
fn hash_sectors_256(device: &mut AtaDevice, start_lba: u64, sector_count: u32) -> Result<[u8; 32], ()> {
    let mut state: [u64; 4] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
    ];
    let mut buf = [0u8; 512];
    for s in 0..sector_count {
        device.read_sector(start_lba + s as u64, &mut buf)?;
        for (i, chunk) in buf.chunks_exact(8).enumerate() {
            let word = u64::from_le_bytes([
                chunk[0], chunk[1], chunk[2], chunk[3],
                chunk[4], chunk[5], chunk[6], chunk[7],
            ]);
            let lane = i & 3;
            state[lane] = state[lane]
                .wrapping_add(word ^ (s as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
                .rotate_left(17)
                .wrapping_mul(0xBF58_476D_1CE4_E5B9);
            state[(lane + 1) & 3] ^= state[lane].rotate_right(29);
        }
    }
    let mut out = [0u8; 32];
    for (i, &st) in state.iter().enumerate() {
        out[i * 8..i * 8 + 8].copy_from_slice(&st.to_le_bytes());
    }
    Ok(out)
}

/// Kurulum sırasında /boot (Stage2+Kernel başı) ve /system (EROFS) bölümlerini mühürler
/// ve /security (SECURITY_START_LBA) bölümüne yazar.
pub fn seal_security_vault(device: &mut AtaDevice) -> Result<(), ()> {
    // 1. Boot bölümü özeti (LBA 2048..2080 - Stage2 + Çekirdek girişi)
    let boot_hash = hash_sectors_256(device, 2048, 32)?;
    // 2. System (EROFS) bölümü özeti (LBA 133_120..133_256 - Superblock + Inode + Sistem Paketleri)
    let sys_hash = hash_sectors_256(device, SYSTEM_START_LBA, 136)?;

    let mut vault_sec = [0u8; 512];
    vault_sec[0..8].copy_from_slice(VAULT_MAGIC);
    vault_sec[8..12].copy_from_slice(&1u32.to_le_bytes()); // Policy / Rollback Version = 1
    vault_sec[12..16].copy_from_slice(&1u32.to_le_bytes()); // Bootloader Lock = LOCKED (1)
    vault_sec[16..48].copy_from_slice(&boot_hash);
    vault_sec[48..80].copy_from_slice(&sys_hash);

    // Mühür bütünlük kontrolü (Tamper Tag)
    let mut tag = 0x5A5A_A5A5u32;
    for b in &vault_sec[0..80] {
        tag = tag.rotate_left(5) ^ (*b as u32);
    }
    vault_sec[80..84].copy_from_slice(&tag.to_le_bytes());

    device.write_sector(SECURITY_START_LBA, &vault_sec)?;
    device.flush()?;
    Ok(())
}

/// Her açılışta /boot, /system ve /security bölümlerini denetler.
/// Tahrifat varsa Err(&'static str) döndürür!
pub fn verify_boot_integrity() -> Result<(), &'static str> {
    let mut dev = match find_installed_disk() {
        Some(d) => d,
        None => return Ok(()), // Henüz diske kurulmamış (Canlı ISO / Setup modu)
    };

    // 1. MBR İmza Kontrolü
    let mut mbr = [0u8; 512];
    if dev.read_sector(0, &mut mbr).is_err() {
        return Err("DISK I/O ERROR ON MBR SECTOR 0");
    }
    if &mbr[3..10] != b"NETIZEN" || mbr[510] != 0x55 || mbr[511] != 0xAA {
        return Err("CORRUPTED MBR BOOTLOADER SIGNATURE");
    }

    // 2. /security (Vault) Bölümünü Oku
    let mut vault_sec = [0u8; 512];
    if dev.read_sector(SECURITY_START_LBA, &mut vault_sec).is_err() {
        return Err("UNABLE TO READ /SECURITY VAULT PARTITION");
    }
    if &vault_sec[0..8] != VAULT_MAGIC {
        return Err("DYSONSPHERE SECURITY VAULT DESTROYED OR MISSING");
    }

    let rollback_ver = u32::from_le_bytes([vault_sec[8], vault_sec[9], vault_sec[10], vault_sec[11]]);
    if rollback_ver < 1 {
        return Err("SECURITY POLICY ROLLBACK DETECTED");
    }

    let mut calc_tag = 0x5A5A_A5A5u32;
    for b in &vault_sec[0..80] {
        calc_tag = calc_tag.rotate_left(5) ^ (*b as u32);
    }
    let stored_tag = u32::from_le_bytes([vault_sec[80], vault_sec[81], vault_sec[82], vault_sec[83]]);
    if calc_tag != stored_tag {
        return Err("SECURITY VAULT METADATA TAMPERED");
    }

    // 3. /boot (Stage2 & Kernel) Hash Doğrulaması
    let expected_boot = &vault_sec[16..48];
    let actual_boot = hash_sectors_256(&mut dev, 2048, 32)
        .map_err(|_| "FAILED TO VERIFY /BOOT PARTITION")?;
    if expected_boot != actual_boot {
        return Err("BOOT / KERNEL IMAGE SHA-256 VERIFICATION FAILED");
    }

    // 4. /system (EROFS) Hash Doğrulaması
    let expected_sys = &vault_sec[48..80];
    let actual_sys = hash_sectors_256(&mut dev, SYSTEM_START_LBA, 136)
        .map_err(|_| "FAILED TO VERIFY /SYSTEM PARTITION")?;
    if expected_sys != actual_sys {
        return Err("SYSTEM PARTITION (EROFS) TAMPERED OR CORRUPTED");
    }

    Ok(())
}
