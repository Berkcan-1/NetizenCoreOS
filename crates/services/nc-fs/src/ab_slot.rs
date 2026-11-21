//! A/B System Slot Boot Control & Automatic Rollback (Feature 23).
//!
//! Uses redundant 512-byte SHA-256-protected metadata sectors with monotonic
//! generation counters so power loss during metadata updates never bricks the
//! active boot slot.

pub const AB_MAGIC: [u8; 4] = *b"NCAB";
pub const AB_VERSION: u16 = 1;
pub const DEFAULT_BOOT_ATTEMPTS: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    A = 0,
    B = 1,
}

impl Slot {
    pub const fn other(self) -> Self {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbError {
    CorruptedMetadata,
    RollbackRejected,
    NoBootableSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotMeta {
    pub bootable: bool,
    pub successful: bool,
    pub attempts_left: u8,
    pub version_code: u32,
    pub rollback_index: u32,
}

impl SlotMeta {
    pub const EMPTY: Self = Self {
        bootable: false,
        successful: false,
        attempts_left: 0,
        version_code: 0,
        rollback_index: 0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootControl {
    pub generation: u64,
    pub active: Slot,
    pub slots: [SlotMeta; 2],
}

impl BootControl {
    pub const fn new_initial(version_code: u32, rollback_index: u32) -> Self {
        Self {
            generation: 1,
            active: Slot::A,
            slots: [
                SlotMeta {
                    bootable: true,
                    successful: true,
                    attempts_left: DEFAULT_BOOT_ATTEMPTS,
                    version_code,
                    rollback_index,
                },
                SlotMeta::EMPTY,
            ],
        }
    }

    pub fn inactive_slot(&self) -> Slot {
        self.active.other()
    }

    /// Stage a verified update ONLY into the inactive slot, enforcing rollback
    /// protection against the active slot's rollback index.
    pub fn stage_update(&mut self, version_code: u32, rollback_index: u32) -> Result<Slot, AbError> {
        let cur = self.slots[self.active as usize];
        if rollback_index < cur.rollback_index {
            return Err(AbError::RollbackRejected);
        }
        let target = self.inactive_slot();
        self.slots[target as usize] = SlotMeta {
            bootable: true,
            successful: false,
            attempts_left: DEFAULT_BOOT_ATTEMPTS,
            version_code,
            rollback_index,
        };
        self.active = target;
        self.generation = self.generation.saturating_add(1);
        Ok(target)
    }

    /// Called by bootloader on each boot: decrements unconfirmed attempt counter
    /// and automatically rolls back to the healthy fallback slot on exhaustion.
    pub fn select_boot_slot(&mut self) -> Result<(Slot, bool), AbError> {
        let idx = self.active as usize;
        if self.slots[idx].bootable {
            if self.slots[idx].successful {
                return Ok((self.active, false));
            }
            if self.slots[idx].attempts_left > 0 {
                self.slots[idx].attempts_left -= 1;
                self.generation = self.generation.saturating_add(1);
                return Ok((self.active, false));
            }
            // Attempts exhausted without success -> mark unbootable and roll back!
            self.slots[idx].bootable = false;
        }
        let fallback = self.active.other();
        let fb_idx = fallback as usize;
        if self.slots[fb_idx].bootable && self.slots[fb_idx].successful {
            self.active = fallback;
            self.generation = self.generation.saturating_add(1);
            return Ok((fallback, true));
        }
        Err(AbError::NoBootableSlot)
    }

    /// Mark the currently booted active slot as confirmed healthy.
    pub fn mark_boot_successful(&mut self) {
        let idx = self.active as usize;
        self.slots[idx].successful = true;
        self.slots[idx].bootable = true;
        self.slots[idx].attempts_left = DEFAULT_BOOT_ATTEMPTS;
        self.generation = self.generation.saturating_add(1);
    }

    pub fn to_sector(&self) -> [u8; 512] {
        let mut sec = [0u8; 512];
        sec[0..4].copy_from_slice(&AB_MAGIC);
        sec[4..6].copy_from_slice(&AB_VERSION.to_le_bytes());
        sec[6] = self.active as u8;
        sec[8..16].copy_from_slice(&self.generation.to_le_bytes());
        for i in 0..2 {
            let base = 16 + i * 16;
            let s = self.slots[i];
            sec[base] = s.bootable as u8;
            sec[base + 1] = s.successful as u8;
            sec[base + 2] = s.attempts_left;
            sec[base + 4..base + 8].copy_from_slice(&s.version_code.to_le_bytes());
            sec[base + 8..base + 12].copy_from_slice(&s.rollback_index.to_le_bytes());
        }
        let hash = nc_crypto::sha256(&sec[0..480]);
        sec[480..512].copy_from_slice(&hash);
        sec
    }

    pub fn from_sector(sec: &[u8; 512]) -> Result<Self, AbError> {
        if sec[0..4] != AB_MAGIC || u16::from_le_bytes([sec[4], sec[5]]) != AB_VERSION {
            return Err(AbError::CorruptedMetadata);
        }
        let expected = nc_crypto::sha256(&sec[0..480]);
        if !nc_crypto::constant_time_eq(&expected, &sec[480..512]) {
            return Err(AbError::CorruptedMetadata);
        }
        let active = match sec[6] {
            0 => Slot::A,
            1 => Slot::B,
            _ => return Err(AbError::CorruptedMetadata),
        };
        let generation = u64::from_le_bytes([
            sec[8], sec[9], sec[10], sec[11], sec[12], sec[13], sec[14], sec[15],
        ]);
        let mut slots = [SlotMeta::EMPTY; 2];
        for i in 0..2 {
            let base = 16 + i * 16;
            slots[i] = SlotMeta {
                bootable: sec[base] != 0,
                successful: sec[base + 1] != 0,
                attempts_left: sec[base + 2],
                version_code: u32::from_le_bytes([
                    sec[base + 4], sec[base + 5], sec[base + 6], sec[base + 7],
                ]),
                rollback_index: u32::from_le_bytes([
                    sec[base + 8], sec[base + 9], sec[base + 10], sec[base + 11],
                ]),
            };
        }
        Ok(Self { generation, active, slots })
    }

    /// Load from redundant primary/backup sectors, selecting the highest valid generation.
    pub fn load_redundant(primary: &[u8; 512], backup: &[u8; 512]) -> Result<Self, AbError> {
        match (Self::from_sector(primary), Self::from_sector(backup)) {
            (Ok(a), Ok(b)) => Ok(if a.generation >= b.generation { a } else { b }),
            (Ok(a), Err(_)) => Ok(a),
            (Err(_), Ok(b)) => Ok(b),
            (Err(_), Err(_)) => Err(AbError::CorruptedMetadata),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ab_slot_switch_rollback_and_redundant_recovery() {
        let mut bc = BootControl::new_initial(100, 1);

        // Downgrade rollback index is rejected.
        assert_eq!(bc.stage_update(99, 0), Err(AbError::RollbackRejected));

        // Stage update to inactive Slot B.
        assert_eq!(bc.stage_update(101, 2), Ok(Slot::B));
        let sec_b = bc.to_sector();

        // Simulate 3 failed boots of Slot B -> 4th boot automatically rolls back to Slot A!
        assert_eq!(bc.select_boot_slot(), Ok((Slot::B, false)));
        assert_eq!(bc.select_boot_slot(), Ok((Slot::B, false)));
        assert_eq!(bc.select_boot_slot(), Ok((Slot::B, false)));
        assert_eq!(bc.select_boot_slot(), Ok((Slot::A, true)));
        assert_eq!(bc.active, Slot::A);

        // Corrupt primary sector -> redundant loader falls back to intact backup sector.
        let mut corrupt = sec_b;
        corrupt[10] ^= 0xFF;
        let recovered = BootControl::load_redundant(&corrupt, &sec_b).expect("backup recovery");
        assert_eq!(recovered.active, Slot::B);
        assert_eq!(recovered.slots[1].version_code, 101);
    }
}
