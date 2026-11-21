//! Bounded In-Memory Compressed Swap (ZRAM) — Feature 7.
//!
//! Stores swapped-out 4 KiB pages in a bounded pool using fast zero-run /
//! byte-run compression with CRC32 corruption detection. Incompressible pages
//! are rejected cleanly (never wasting CPU or expanding storage).

pub const ZRAM_PAGE_SIZE: usize = 4096;
pub const ZRAM_MAX_SLOTS: usize = 256;
pub const ZRAM_POOL_BYTES: usize = 256 * 1024; // 256 KiB bounded pool
const MAX_COMPRESSED_PAGE: usize = 3584; // Require >= 12.5% savings

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZramError {
    OutOfSlots,
    OutOfMemory,
    Incompressible,
    InvalidSlot,
    Corrupted,
}

#[derive(Clone, Copy)]
struct ZramSlot {
    in_use: bool,
    offset: u32,
    comp_len: u16,
    crc32: u32,
}

impl ZramSlot {
    const EMPTY: Self = Self {
        in_use: false,
        offset: 0,
        comp_len: 0,
        crc32: 0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZramStats {
    pub stored_pages: usize,
    pub original_bytes: usize,
    pub compressed_bytes: usize,
    pub pool_capacity: usize,
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Compact byte-run compressor:
/// - `0x00..=0x7F` (len = tag + 1): literal run of `len` bytes
/// - `0x80..=0xFF` (len = (tag & 0x7F) + 3): repeat next single byte `len` times
fn compress_page(src: &[u8; ZRAM_PAGE_SIZE], dst: &mut [u8]) -> Option<usize> {
    let mut i = 0usize;
    let mut out = 0usize;
    while i < src.len() {
        let b = src[i];
        let mut run = 1usize;
        while i + run < src.len() && src[i + run] == b && run < 130 {
            run += 1;
        }
        if run >= 3 {
            if out + 2 > dst.len() {
                return None;
            }
            dst[out] = 0x80 | ((run - 3) as u8);
            dst[out + 1] = b;
            out += 2;
            i += run;
        } else {
            let lit_start = i;
            let mut lit_len = 0usize;
            while i < src.len() && lit_len < 128 {
                let c = src[i];
                let mut r = 1usize;
                while i + r < src.len() && src[i + r] == c && r < 3 {
                    r += 1;
                }
                if r >= 3 {
                    break;
                }
                i += 1;
                lit_len += 1;
            }
            if out + 1 + lit_len > dst.len() {
                return None;
            }
            dst[out] = (lit_len - 1) as u8;
            out += 1;
            dst[out..out + lit_len].copy_from_slice(&src[lit_start..lit_start + lit_len]);
            out += lit_len;
        }
    }
    Some(out)
}

fn decompress_page(src: &[u8], dst: &mut [u8; ZRAM_PAGE_SIZE]) -> bool {
    let mut i = 0usize;
    let mut out = 0usize;
    while i < src.len() {
        let tag = src[i];
        i += 1;
        if tag & 0x80 != 0 {
            if i >= src.len() {
                return false;
            }
            let count = (tag & 0x7F) as usize + 3;
            let val = src[i];
            i += 1;
            if out + count > dst.len() {
                return false;
            }
            for b in &mut dst[out..out + count] {
                *b = val;
            }
            out += count;
        } else {
            let count = (tag as usize) + 1;
            if i + count > src.len() || out + count > dst.len() {
                return false;
            }
            dst[out..out + count].copy_from_slice(&src[i..i + count]);
            i += count;
            out += count;
        }
    }
    out == dst.len()
}

pub struct ZramPool {
    slots: [ZramSlot; ZRAM_MAX_SLOTS],
    pool: [u8; ZRAM_POOL_BYTES],
    tail: usize,
    stored_pages: usize,
    compressed_bytes: usize,
}

impl ZramPool {
    pub const fn new() -> Self {
        Self {
            slots: [ZramSlot::EMPTY; ZRAM_MAX_SLOTS],
            pool: [0u8; ZRAM_POOL_BYTES],
            tail: 0,
            stored_pages: 0,
            compressed_bytes: 0,
        }
    }

    pub fn stats(&self) -> ZramStats {
        ZramStats {
            stored_pages: self.stored_pages,
            original_bytes: self.stored_pages * ZRAM_PAGE_SIZE,
            compressed_bytes: self.compressed_bytes,
            pool_capacity: ZRAM_POOL_BYTES,
        }
    }

    fn compact(&mut self) {
        let mut cursor = 0usize;
        for slot in self.slots.iter_mut() {
            if slot.in_use {
                let off = slot.offset as usize;
                let len = slot.comp_len as usize;
                if off != cursor {
                    self.pool.copy_within(off..off + len, cursor);
                    slot.offset = cursor as u32;
                }
                cursor += len;
            }
        }
        self.tail = cursor;
    }

    pub fn store_page(&mut self, page: &[u8; ZRAM_PAGE_SIZE]) -> Result<usize, ZramError> {
        let slot_idx = self
            .slots
            .iter()
            .position(|s| !s.in_use)
            .ok_or(ZramError::OutOfSlots)?;

        let mut scratch = [0u8; MAX_COMPRESSED_PAGE];
        let comp_len = compress_page(page, &mut scratch).ok_or(ZramError::Incompressible)?;

        if self.tail + comp_len > ZRAM_POOL_BYTES {
            self.compact();
            if self.tail + comp_len > ZRAM_POOL_BYTES {
                return Err(ZramError::OutOfMemory);
            }
        }

        let offset = self.tail;
        self.pool[offset..offset + comp_len].copy_from_slice(&scratch[..comp_len]);
        self.tail += comp_len;

        self.slots[slot_idx] = ZramSlot {
            in_use: true,
            offset: offset as u32,
            comp_len: comp_len as u16,
            crc32: crc32(page),
        };
        self.stored_pages += 1;
        self.compressed_bytes += comp_len;
        Ok(slot_idx)
    }

    pub fn load_page(&self, slot_idx: usize, out: &mut [u8; ZRAM_PAGE_SIZE]) -> Result<(), ZramError> {
        let slot = self.slots.get(slot_idx).ok_or(ZramError::InvalidSlot)?;
        if !slot.in_use {
            return Err(ZramError::InvalidSlot);
        }
        let start = slot.offset as usize;
        let end = start + slot.comp_len as usize;
        if end > ZRAM_POOL_BYTES {
            return Err(ZramError::Corrupted);
        }
        if !decompress_page(&self.pool[start..end], out) {
            out.fill(0);
            return Err(ZramError::Corrupted);
        }
        if crc32(out) != slot.crc32 {
            out.fill(0);
            return Err(ZramError::Corrupted);
        }
        Ok(())
    }

    pub fn free_slot(&mut self, slot_idx: usize) -> Result<(), ZramError> {
        let slot = self.slots.get_mut(slot_idx).ok_or(ZramError::InvalidSlot)?;
        if !slot.in_use {
            return Err(ZramError::InvalidSlot);
        }
        let start = slot.offset as usize;
        let len = slot.comp_len as usize;
        for b in &mut self.pool[start..start + len] {
            *b = 0;
        }
        *slot = ZramSlot::EMPTY;
        self.stored_pages -= 1;
        self.compressed_bytes -= len;
        if self.stored_pages == 0 {
            self.tail = 0;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zram_store_load_corrupt_and_reclaim() {
        let mut zram = ZramPool::new();
        let mut page = [0u8; ZRAM_PAGE_SIZE];
        for i in 0..256 {
            page[i] = (i % 16) as u8;
        }
        // Remaining 3840 bytes are zero -> highly compressible.
        let slot = zram.store_page(&page).expect("compress and store");
        let stats = zram.stats();
        assert_eq!(stats.stored_pages, 1);
        assert!(stats.compressed_bytes < 512, "expected high compression ratio");

        let mut restored = [0xAAu8; ZRAM_PAGE_SIZE];
        zram.load_page(slot, &mut restored).expect("decompress and verify CRC");
        assert_eq!(restored, page);

        // Corrupt 1 byte inside compressed pool -> CRC32 must catch it and zeroize output.
        let off = zram.slots[slot].offset as usize;
        zram.pool[off + 2] ^= 0x01;
        restored.fill(0x55);
        assert_eq!(zram.load_page(slot, &mut restored), Err(ZramError::Corrupted));
        assert!(restored.iter().all(|&b| b == 0));

        zram.free_slot(slot).expect("free slot");
        assert_eq!(zram.stats().stored_pages, 0);
        assert_eq!(zram.stats().compressed_bytes, 0);
    }

    #[test]
    fn zram_rejects_incompressible_noise() {
        let mut zram = ZramPool::new();
        let mut noise = [0u8; ZRAM_PAGE_SIZE];
        let mut x = 0x1234_5678_9ABC_DEF1u64;
        for b in noise.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        assert_eq!(zram.store_page(&noise), Err(ZramError::Incompressible));
    }
}
