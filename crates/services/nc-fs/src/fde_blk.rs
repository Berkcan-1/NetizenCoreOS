//! Authenticated Full-Data Encryption (FDE) Block Device (Feature 9).
//!
//! Wraps a raw physical `BlockDevice` (e.g., eMMC or VirtIO-blk) with per-block
//! ChaCha20-Poly1305 AEAD encryption using the formally verified SPARK Ada FDE
//! state machine (`nc_security_encryption.ads/.adb` via `nc-spark-sec`).
//!
//! On-disk layout (512-byte physical sectors):
//! - Sector 0: 512-byte SPARK `NCFDE001` metadata sector (PBKDF2 salt, wrapped DEK, SHA-256).
//! - Logical block `L` (512 B plaintext) -> 2 physical sectors (`1 + L * 2`, `1 + L * 2 + 1`):
//!   - `[0..4]`:   Block magic `0x4544_464E` ("NFDE")
//!   - `[4..16]`:  96-bit per-write nonce (4-byte LBA low + 8-byte monotonic write generation)
//!   - `[16..32]`: 128-bit Poly1305 AEAD authentication tag (bound to `Block_Id = L` as AAD)
//!   - `[32..544]`: 512-byte ChaCha20 ciphertext
//!   - `[544..1024]`: Reserved zero padding
//!
//! Security guarantees:
//! - Fails closed (`Err(())`) on any read or write whenever SPARK FDE status != `FDE_UNLOCKED`.
//! - Never falls back to plaintext I/O.
//! - Rejects any bit flip in ciphertext, nonce, tag, or block relocation (`Block_Id` AAD mismatch),
//!   and zeroizes the caller's output buffer on authentication failure.

use alloc::boxed::Box;
use nc_hal::{BlockDevice, BlockDeviceInfo, DeviceType};

const SECTOR: usize = 512;
const PHYS_PER_LOGICAL: u64 = 2;
const META_SECTORS: u64 = 1;
const BLOCK_MAGIC: [u8; 4] = *b"NFDE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdeError {
    Io,
    DeviceTooSmall,
    UnsupportedBlockSize,
    MetadataInvalid,
    ProvisionFailed,
    Locked,
    AuthenticationFailed,
}

pub struct EncryptedBlockDevice {
    inner: Box<dyn BlockDevice>,
    logical_blocks: u64,
    write_gen: u64,
}

impl EncryptedBlockDevice {
    /// Load and validate sector 0 metadata through SPARK Ada.
    /// The volume starts LOCKED until `unlock()` succeeds.
    pub fn open(mut inner: Box<dyn BlockDevice>) -> Result<(Self, u32), FdeError> {
        let info = inner.info();
        if info.block_size as usize != SECTOR {
            return Err(FdeError::UnsupportedBlockSize);
        }
        if info.block_count < META_SECTORS + PHYS_PER_LOGICAL {
            return Err(FdeError::DeviceTooSmall);
        }
        let mut meta = [0u8; SECTOR];
        inner.read_blocks(0, 1, &mut meta).map_err(|_| FdeError::Io)?;
        let status = nc_spark_sec::fde_load_metadata(&meta);
        if status == nc_spark_sec::FDE_CORRUPTED {
            return Err(FdeError::MetadataInvalid);
        }
        let logical_blocks = (info.block_count - META_SECTORS) / PHYS_PER_LOGICAL;
        Ok((
            Self {
                inner,
                logical_blocks,
                write_gen: 1,
            },
            status,
        ))
    }

    /// Provision a new FDE volume using caller-provided cryptographic randomness
    /// (salt, wrap nonce, and 256-bit DEK), writing the SPARK-sealed metadata sector to LBA 0.
    pub fn provision(
        mut inner: Box<dyn BlockDevice>,
        passphrase: &[u8],
        salt: &[u8; 16],
        wrap_nonce: &[u8; 12],
        raw_dek: &[u8; 32],
        iterations: u32,
    ) -> Result<Self, FdeError> {
        let info = inner.info();
        if info.block_size as usize != SECTOR {
            return Err(FdeError::UnsupportedBlockSize);
        }
        if info.block_count < META_SECTORS + PHYS_PER_LOGICAL {
            return Err(FdeError::DeviceTooSmall);
        }
        let mut meta = [0u8; SECTOR];
        if !nc_spark_sec::fde_provision(passphrase, salt, wrap_nonce, raw_dek, iterations, &mut meta)
        {
            return Err(FdeError::ProvisionFailed);
        }
        inner.write_blocks(0, 1, &meta).map_err(|_| FdeError::Io)?;
        let logical_blocks = (info.block_count - META_SECTORS) / PHYS_PER_LOGICAL;
        Ok(Self {
            inner,
            logical_blocks,
            write_gen: 1,
        })
    }

    pub fn unlock(&mut self, passphrase: &[u8]) -> Result<(), FdeError> {
        if nc_spark_sec::fde_unlock(passphrase) {
            Ok(())
        } else {
            Err(FdeError::AuthenticationFailed)
        }
    }

    pub fn lock(&mut self) {
        nc_spark_sec::fde_lock();
    }

    pub fn status(&self) -> u32 {
        nc_spark_sec::fde_status()
    }

    fn next_nonce(&mut self, lba: u64) -> [u8; 12] {
        let g = self.write_gen;
        self.write_gen = self.write_gen.wrapping_add(1).max(1);
        let mut nonce = [0u8; 12];
        nonce[0..4].copy_from_slice(&(lba as u32).to_le_bytes());
        nonce[4..12].copy_from_slice(&g.to_le_bytes());
        nonce
    }
}

impl BlockDevice for EncryptedBlockDevice {
    fn info(&self) -> BlockDeviceInfo {
        let mut info = self.inner.info();
        info.block_count = self.logical_blocks;
        info.block_size = SECTOR as u32;
        info.device_type = DeviceType::HardDisk;
        info
    }

    fn read_blocks(&mut self, lba: u64, count: u32, buffer: &mut [u8]) -> Result<(), ()> {
        if nc_spark_sec::fde_status() != nc_spark_sec::FDE_UNLOCKED {
            return Err(());
        }
        let wanted = (count as usize).checked_mul(SECTOR).ok_or(())?;
        if buffer.len() < wanted
            || lba
                .checked_add(count as u64)
                .map_or(true, |end| end > self.logical_blocks)
        {
            return Err(());
        }
        let mut raw = [0u8; SECTOR * 2];
        for i in 0..(count as u64) {
            let block_id = lba + i;
            let phys = META_SECTORS + block_id * PHYS_PER_LOGICAL;
            self.inner.read_blocks(phys, PHYS_PER_LOGICAL as u32, &mut raw)?;

            let out = &mut buffer[(i as usize) * SECTOR..(i as usize + 1) * SECTOR];
            if raw.iter().all(|&b| b == 0) {
                // Unwritten sparse physical slot -> zero plaintext block.
                out.fill(0);
                continue;
            }
            if raw[0..4] != BLOCK_MAGIC {
                out.fill(0);
                return Err(());
            }
            let mut nonce = [0u8; 12];
            nonce.copy_from_slice(&raw[4..16]);
            let mut tag = [0u8; 16];
            tag.copy_from_slice(&raw[16..32]);
            let ct = &raw[32..32 + SECTOR];

            if !nc_spark_sec::fde_decrypt_block(block_id, &nonce, ct, &tag, out) {
                out.fill(0);
                return Err(());
            }
        }
        Ok(())
    }

    fn write_blocks(&mut self, lba: u64, count: u32, buffer: &[u8]) -> Result<(), ()> {
        if nc_spark_sec::fde_status() != nc_spark_sec::FDE_UNLOCKED {
            return Err(());
        }
        let wanted = (count as usize).checked_mul(SECTOR).ok_or(())?;
        if buffer.len() < wanted
            || lba
                .checked_add(count as u64)
                .map_or(true, |end| end > self.logical_blocks)
        {
            return Err(());
        }
        let mut raw = [0u8; SECTOR * 2];
        for i in 0..(count as u64) {
            let block_id = lba + i;
            let pt = &buffer[(i as usize) * SECTOR..(i as usize + 1) * SECTOR];
            let nonce = self.next_nonce(block_id);
            let mut tag = [0u8; 16];
            raw.fill(0);
            raw[0..4].copy_from_slice(&BLOCK_MAGIC);
            raw[4..16].copy_from_slice(&nonce);

            if !nc_spark_sec::fde_encrypt_block(
                block_id,
                &nonce,
                pt,
                &mut raw[32..32 + SECTOR],
                &mut tag,
            ) {
                return Err(());
            }
            raw[16..32].copy_from_slice(&tag);

            let phys = META_SECTORS + block_id * PHYS_PER_LOGICAL;
            self.inner.write_blocks(phys, PHYS_PER_LOGICAL as u32, &raw)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    struct RamDisk {
        data: Vec<u8>,
    }

    impl RamDisk {
        fn new(sectors: usize) -> Self {
            Self {
                data: vec![0u8; sectors * SECTOR],
            }
        }
    }

    impl BlockDevice for RamDisk {
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
            let start = (lba as usize) * SECTOR;
            let len = (count as usize) * SECTOR;
            buffer[..len].copy_from_slice(&self.data[start..start + len]);
            Ok(())
        }

        fn write_blocks(&mut self, lba: u64, count: u32, buffer: &[u8]) -> Result<(), ()> {
            let start = (lba as usize) * SECTOR;
            let len = (count as usize) * SECTOR;
            self.data[start..start + len].copy_from_slice(&buffer[..len]);
            Ok(())
        }
    }

    #[test]
    fn fde_provision_lock_unlock_encrypt_decrypt_and_tamper_detection() {
        let salt = [0x11u8; 16];
        let nonce = [0x22u8; 12];
        let dek = [0x33u8; 32];
        let pass = b"NetizenCore-FDE-Test-2026";

        // Weak iteration count (< 10_000) must be rejected by SPARK policy!
        let weak_disk = Box::new(RamDisk::new(32));
        assert_eq!(
            EncryptedBlockDevice::provision(weak_disk, pass, &salt, &nonce, &dek, 16).err(),
            Some(FdeError::ProvisionFailed)
        );

        let disk = Box::new(RamDisk::new(32));
        let mut fde = EncryptedBlockDevice::provision(disk, pass, &salt, &nonce, &dek, 10_000)
            .expect("provision succeeds");
        assert_eq!(fde.status(), nc_spark_sec::FDE_UNLOCKED);

        let mut pt = [0u8; SECTOR];
        pt[0..16].copy_from_slice(b"SECRET_USER_DATA");
        fde.write_blocks(0, 1, &pt).expect("write encrypted block");

        let mut out = [0u8; SECTOR];
        fde.read_blocks(0, 1, &mut out).expect("read encrypted block");
        assert_eq!(out, pt);

        // Lock must fail closed on both reads and writes.
        fde.lock();
        assert_eq!(fde.status(), nc_spark_sec::FDE_LOCKED);
        assert!(fde.read_blocks(0, 1, &mut out).is_err());
        assert!(fde.write_blocks(0, 1, &pt).is_err());

        // Wrong passphrase must stay locked.
        assert_eq!(fde.unlock(b"wrong-pass"), Err(FdeError::AuthenticationFailed));
        assert_eq!(fde.status(), nc_spark_sec::FDE_LOCKED);

        // Correct passphrase unlocks and decrypts cleanly.
        fde.unlock(pass).expect("unlock succeeds");
        out.fill(0);
        fde.read_blocks(0, 1, &mut out).expect("decrypt after unlock");
        assert_eq!(out, pt);

        // Tamper ciphertext on physical sector 1 -> must reject and zeroize output!
        let mut raw_phys = [0u8; SECTOR];
        fde.inner.read_blocks(1, 1, &mut raw_phys).unwrap();
        raw_phys[40] ^= 0x01;
        fde.inner.write_blocks(1, 1, &raw_phys).unwrap();

        out.fill(0xAA);
        assert!(fde.read_blocks(0, 1, &mut out).is_err());
        assert!(out.iter().all(|&b| b == 0), "output buffer must be zeroized on tamper");
    }
}
