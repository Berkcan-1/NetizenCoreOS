//! Bounded Crash-Consistent Write-Ahead Metadata Journal (Feature 22).
//!
//! Stages block writes into a bounded transaction buffer, seals commits with
//! SPARK Ada SHA-256, and replays only verified committed transactions on
//! recovery (cleanly discarding uncommitted or torn writes after power loss).

use alloc::vec::Vec;
use nc_hal::BlockDevice;

pub const JOURNAL_MAGIC: [u8; 4] = *b"NCJL";
pub const MAX_TX_BLOCKS: usize = 16;
const SECTOR: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalError {
    TxFull,
    EmptyTx,
    CorruptedCommit,
    Io,
}

#[derive(Clone, Copy)]
struct StagedBlock {
    target_lba: u64,
    data: [u8; SECTOR],
}

pub struct JournalTx {
    tx_id: u64,
    blocks: Vec<StagedBlock>,
    committed: bool,
    commit_hash: [u8; 32],
}

impl JournalTx {
    pub fn begin(tx_id: u64) -> Self {
        Self {
            tx_id,
            blocks: Vec::new(),
            committed: false,
            commit_hash: [0u8; 32],
        }
    }

    pub fn stage_write(&mut self, target_lba: u64, sector: &[u8; SECTOR]) -> Result<(), JournalError> {
        if self.blocks.len() >= MAX_TX_BLOCKS {
            return Err(JournalError::TxFull);
        }
        self.blocks.push(StagedBlock {
            target_lba,
            data: *sector,
        });
        Ok(())
    }

    fn compute_digest(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(16 + self.blocks.len() * (8 + SECTOR));
        buf.extend_from_slice(&JOURNAL_MAGIC);
        buf.extend_from_slice(&self.tx_id.to_le_bytes());
        buf.extend_from_slice(&(self.blocks.len() as u32).to_le_bytes());
        for b in &self.blocks {
            buf.extend_from_slice(&b.target_lba.to_le_bytes());
            buf.extend_from_slice(&b.data);
        }
        nc_crypto::sha256(&buf)
    }

    pub fn commit(&mut self) -> Result<[u8; 32], JournalError> {
        if self.blocks.is_empty() {
            return Err(JournalError::EmptyTx);
        }
        self.commit_hash = self.compute_digest();
        self.committed = true;
        Ok(self.commit_hash)
    }

    /// Replay a committed transaction onto `device`. Uncommitted or corrupted
    /// transactions are discarded without modifying `device`.
    pub fn replay(&self, device: &mut dyn BlockDevice) -> Result<usize, JournalError> {
        if !self.committed {
            return Ok(0);
        }
        let actual = self.compute_digest();
        if !nc_crypto::constant_time_eq(&actual, &self.commit_hash) {
            return Err(JournalError::CorruptedCommit);
        }
        for b in &self.blocks {
            device.write_blocks(b.target_lba, 1, &b.data).map_err(|_| JournalError::Io)?;
        }
        Ok(self.blocks.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use nc_hal::{BlockDeviceInfo, DeviceType};

    struct MemDisk(Vec<u8>);
    impl BlockDevice for MemDisk {
        fn info(&self) -> BlockDeviceInfo {
            BlockDeviceInfo {
                channel: 0,
                model: [0; 40],
                model_len: 0,
                block_count: (self.0.len() / SECTOR) as u64,
                block_size: SECTOR as u32,
                device_type: DeviceType::HardDisk,
            }
        }
        fn read_blocks(&mut self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), ()> {
            let s = lba as usize * SECTOR;
            let n = count as usize * SECTOR;
            buf[..n].copy_from_slice(&self.0[s..s + n]);
            Ok(())
        }
        fn write_blocks(&mut self, lba: u64, count: u32, buf: &[u8]) -> Result<(), ()> {
            let s = lba as usize * SECTOR;
            let n = count as usize * SECTOR;
            self.0[s..s + n].copy_from_slice(&buf[..n]);
            Ok(())
        }
    }

    #[test]
    fn journal_discards_uncommitted_and_corrupted_tx_and_replays_committed() {
        let mut disk = MemDisk(vec![0u8; 8 * SECTOR]);
        let payload = [0x5Au8; SECTOR];

        // 1. Simulated power loss before commit -> 0 blocks replayed, disk untouched.
        let mut torn = JournalTx::begin(1);
        torn.stage_write(2, &payload).unwrap();
        assert_eq!(torn.replay(&mut disk), Ok(0));
        assert!(disk.0[2 * SECTOR..3 * SECTOR].iter().all(|&b| b == 0));

        // 2. Committed transaction replays atomically.
        let mut good = JournalTx::begin(2);
        good.stage_write(2, &payload).unwrap();
        good.commit().unwrap();
        assert_eq!(good.replay(&mut disk), Ok(1));
        assert!(disk.0[2 * SECTOR..3 * SECTOR].iter().all(|&b| b == 0x5A));

        // 3. Corrupted commit record is rejected cleanly.
        good.blocks[0].data[0] ^= 0x01;
        assert_eq!(good.replay(&mut disk), Err(JournalError::CorruptedCommit));
    }
}
