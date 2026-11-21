//! Storage Quotas, Recoverable Trash, and TRIM/Discard Validation (Feature 27).

pub const MAX_QUOTA_APPS: usize = 64;
pub const MAX_TRASH_ITEMS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaError {
    TableFull,
    QuotaExceeded,
    NotFound,
    PermissionDenied,
    Unsupported,
    InvalidRange,
}

#[derive(Clone, Copy)]
struct QuotaEntry {
    in_use: bool,
    app_len: u8,
    app_id: [u8; 32],
    used_bytes: u64,
    limit_bytes: u64,
}

impl QuotaEntry {
    const EMPTY: Self = Self {
        in_use: false,
        app_len: 0,
        app_id: [0; 32],
        used_bytes: 0,
        limit_bytes: 0,
    };

    fn matches(&self, app_id: &str) -> bool {
        let b = app_id.as_bytes();
        self.app_len as usize == b.len() && &self.app_id[..b.len()] == b
    }
}

pub struct QuotaTable {
    entries: [QuotaEntry; MAX_QUOTA_APPS],
}

impl QuotaTable {
    pub const fn new() -> Self {
        Self {
            entries: [QuotaEntry::EMPTY; MAX_QUOTA_APPS],
        }
    }

    pub fn set_limit(&mut self, app_id: &str, limit_bytes: u64) -> Result<(), QuotaError> {
        let b = app_id.as_bytes();
        if b.is_empty() || b.len() > 32 {
            return Err(QuotaError::InvalidRange);
        }
        if let Some(e) = self.entries.iter_mut().find(|e| e.in_use && e.matches(app_id)) {
            e.limit_bytes = limit_bytes;
            return Ok(());
        }
        let slot = self.entries.iter_mut().find(|e| !e.in_use).ok_or(QuotaError::TableFull)?;
        *slot = QuotaEntry::EMPTY;
        slot.in_use = true;
        slot.app_len = b.len() as u8;
        slot.app_id[..b.len()].copy_from_slice(b);
        slot.limit_bytes = limit_bytes;
        Ok(())
    }

    pub fn try_consume(&mut self, app_id: &str, add_bytes: u64) -> Result<u64, QuotaError> {
        let e = self
            .entries
            .iter_mut()
            .find(|e| e.in_use && e.matches(app_id))
            .ok_or(QuotaError::NotFound)?;
        let next = e.used_bytes.checked_add(add_bytes).ok_or(QuotaError::QuotaExceeded)?;
        if next > e.limit_bytes {
            return Err(QuotaError::QuotaExceeded);
        }
        e.used_bytes = next;
        Ok(next)
    }

    pub fn release(&mut self, app_id: &str, sub_bytes: u64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.in_use && e.matches(app_id)) {
            e.used_bytes = e.used_bytes.saturating_sub(sub_bytes);
        }
    }
}

#[derive(Clone, Copy)]
struct TrashItem {
    in_use: bool,
    trash_id: u32,
    owner_len: u8,
    owner_id: [u8; 32],
    size_bytes: u64,
    expires_at_sec: u64,
}

impl TrashItem {
    const EMPTY: Self = Self {
        in_use: false,
        trash_id: 0,
        owner_len: 0,
        owner_id: [0; 32],
        size_bytes: 0,
        expires_at_sec: 0,
    };

    fn matches_owner(&self, app_id: &str) -> bool {
        let b = app_id.as_bytes();
        self.owner_len as usize == b.len() && &self.owner_id[..b.len()] == b
    }
}

pub struct TrashRegistry {
    items: [TrashItem; MAX_TRASH_ITEMS],
    next_id: u32,
}

impl TrashRegistry {
    pub const fn new() -> Self {
        Self {
            items: [TrashItem::EMPTY; MAX_TRASH_ITEMS],
            next_id: 1,
        }
    }

    pub fn move_to_trash(
        &mut self,
        owner_app_id: &str,
        size_bytes: u64,
        expires_at_sec: u64,
    ) -> Result<u32, QuotaError> {
        let b = owner_app_id.as_bytes();
        if b.is_empty() || b.len() > 32 {
            return Err(QuotaError::InvalidRange);
        }
        let slot = self.items.iter_mut().find(|i| !i.in_use).ok_or(QuotaError::TableFull)?;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        *slot = TrashItem::EMPTY;
        slot.in_use = true;
        slot.trash_id = id;
        slot.owner_len = b.len() as u8;
        slot.owner_id[..b.len()].copy_from_slice(b);
        slot.size_bytes = size_bytes;
        slot.expires_at_sec = expires_at_sec;
        Ok(id)
    }

    pub fn restore(&mut self, caller_app_id: &str, trash_id: u32) -> Result<u64, QuotaError> {
        let item = self
            .items
            .iter_mut()
            .find(|i| i.in_use && i.trash_id == trash_id)
            .ok_or(QuotaError::NotFound)?;
        if !item.matches_owner(caller_app_id) {
            return Err(QuotaError::PermissionDenied);
        }
        let size = item.size_bytes;
        *item = TrashItem::EMPTY;
        Ok(size)
    }

    pub fn purge_expired(&mut self, now_sec: u64) -> u64 {
        let mut freed = 0u64;
        for item in self.items.iter_mut() {
            if item.in_use && now_sec >= item.expires_at_sec {
                freed = freed.saturating_add(item.size_bytes);
                *item = TrashItem::EMPTY;
            }
        }
        freed
    }
}

/// Validate a block-device TRIM/discard request before issuing hardware commands.
/// Never advertises or issues TRIM when `device_supports_discard == false`.
pub fn validate_discard_range(
    lba: u64,
    sectors: u64,
    device_sectors: u64,
    align_sectors: u64,
    device_supports_discard: bool,
) -> Result<(), QuotaError> {
    if !device_supports_discard {
        return Err(QuotaError::Unsupported);
    }
    if sectors == 0 || align_sectors == 0 {
        return Err(QuotaError::InvalidRange);
    }
    if (lba % align_sectors) != 0 || (sectors % align_sectors) != 0 {
        return Err(QuotaError::InvalidRange);
    }
    let end = lba.checked_add(sectors).ok_or(QuotaError::InvalidRange)?;
    if end > device_sectors {
        return Err(QuotaError::InvalidRange);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotas_trash_isolation_and_trim_validation() {
        let mut q = QuotaTable::new();
        q.set_limit("nc.notes", 4096).unwrap();
        assert_eq!(q.try_consume("nc.notes", 3000), Ok(3000));
        assert_eq!(q.try_consume("nc.notes", 2000), Err(QuotaError::QuotaExceeded));
        q.release("nc.notes", 1000);
        assert_eq!(q.try_consume("nc.notes", 2000), Ok(4000));

        let mut trash = TrashRegistry::new();
        let tid = trash.move_to_trash("nc.notes", 1024, 1000).unwrap();
        // Other app cannot restore nc.notes's private trashed item.
        assert_eq!(trash.restore("nc.evil", tid), Err(QuotaError::PermissionDenied));
        assert_eq!(trash.restore("nc.notes", tid), Ok(1024));

        let _tid2 = trash.move_to_trash("nc.notes", 2048, 500).unwrap();
        assert_eq!(trash.purge_expired(499), 0);
        assert_eq!(trash.purge_expired(500), 2048);

        assert_eq!(validate_discard_range(0, 8, 1024, 8, false), Err(QuotaError::Unsupported));
        assert_eq!(validate_discard_range(4, 8, 1024, 8, true), Err(QuotaError::InvalidRange));
        assert_eq!(validate_discard_range(8, 16, 1024, 8, true), Ok(()));
    }
}
