//! Capability-Controlled Shared Memory Registry (Feature 5).
//!
//! Mediates shared memory handles between isolated processes. Each object is
//! bound to its creator task slot, explicit authorized participant list,
//! read/write permissions, and revocation state.

pub const MAX_SHM_OBJECTS: usize = 32;
pub const MAX_SHM_PARTICIPANTS: usize = 8;

pub const SHM_PERM_READ: u32 = 1 << 0;
pub const SHM_PERM_WRITE: u32 = 1 << 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShmError {
    TableFull,
    InvalidHandle,
    PermissionDenied,
    InvalidSize,
    ParticipantsFull,
    Revoked,
}

#[derive(Clone, Copy)]
struct Participant {
    in_use: bool,
    task_id: u32,
    perms: u32,
}

impl Participant {
    const EMPTY: Self = Self {
        in_use: false,
        task_id: 0,
        perms: 0,
    };
}

#[derive(Clone, Copy)]
pub struct ShmObject {
    in_use: bool,
    revoked: bool,
    handle: u32,
    creator_task: u32,
    phys_base: u64,
    page_count: u32,
    participants: [Participant; MAX_SHM_PARTICIPANTS],
}

impl ShmObject {
    const EMPTY: Self = Self {
        in_use: false,
        revoked: false,
        handle: 0,
        creator_task: 0,
        phys_base: 0,
        page_count: 0,
        participants: [Participant::EMPTY; MAX_SHM_PARTICIPANTS],
    };

    pub fn phys_base(&self) -> u64 {
        self.phys_base
    }

    pub fn page_count(&self) -> u32 {
        self.page_count
    }
}

pub struct ShmRegistry {
    objects: [ShmObject; MAX_SHM_OBJECTS],
    next_handle: u32,
}

impl ShmRegistry {
    pub const fn new() -> Self {
        Self {
            objects: [ShmObject::EMPTY; MAX_SHM_OBJECTS],
            next_handle: 1,
        }
    }

    pub fn create(
        &mut self,
        creator_task: u32,
        phys_base: u64,
        page_count: u32,
    ) -> Result<u32, ShmError> {
        if page_count == 0 || page_count > 1024 || phys_base & 0xFFF != 0 {
            return Err(ShmError::InvalidSize);
        }
        let slot = self
            .objects
            .iter_mut()
            .find(|o| !o.in_use)
            .ok_or(ShmError::TableFull)?;
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1).max(1);
        *slot = ShmObject::EMPTY;
        slot.in_use = true;
        slot.handle = handle;
        slot.creator_task = creator_task;
        slot.phys_base = phys_base;
        slot.page_count = page_count;
        slot.participants[0] = Participant {
            in_use: true,
            task_id: creator_task,
            perms: SHM_PERM_READ | SHM_PERM_WRITE,
        };
        Ok(handle)
    }

    pub fn grant(
        &mut self,
        caller_task: u32,
        handle: u32,
        target_task: u32,
        perms: u32,
    ) -> Result<(), ShmError> {
        let obj = self
            .objects
            .iter_mut()
            .find(|o| o.in_use && o.handle == handle)
            .ok_or(ShmError::InvalidHandle)?;
        if obj.revoked {
            return Err(ShmError::Revoked);
        }
        if obj.creator_task != caller_task {
            return Err(ShmError::PermissionDenied);
        }
        let clamped = perms & (SHM_PERM_READ | SHM_PERM_WRITE);
        if let Some(existing) = obj
            .participants
            .iter_mut()
            .find(|p| p.in_use && p.task_id == target_task)
        {
            existing.perms = clamped;
            return Ok(());
        }
        let free = obj
            .participants
            .iter_mut()
            .find(|p| !p.in_use)
            .ok_or(ShmError::ParticipantsFull)?;
        *free = Participant {
            in_use: true,
            task_id: target_task,
            perms: clamped,
        };
        Ok(())
    }

    pub fn authorize_map(
        &self,
        caller_task: u32,
        handle: u32,
        required_perms: u32,
    ) -> Result<(u64, u32), ShmError> {
        let obj = self
            .objects
            .iter()
            .find(|o| o.in_use && o.handle == handle)
            .ok_or(ShmError::InvalidHandle)?;
        if obj.revoked {
            return Err(ShmError::Revoked);
        }
        let participant = obj
            .participants
            .iter()
            .find(|p| p.in_use && p.task_id == caller_task)
            .ok_or(ShmError::PermissionDenied)?;
        if (participant.perms & required_perms) != required_perms {
            return Err(ShmError::PermissionDenied);
        }
        Ok((obj.phys_base, obj.page_count))
    }

    pub fn revoke(&mut self, caller_task: u32, handle: u32) -> Result<(), ShmError> {
        let obj = self
            .objects
            .iter_mut()
            .find(|o| o.in_use && o.handle == handle)
            .ok_or(ShmError::InvalidHandle)?;
        if obj.creator_task != caller_task {
            return Err(ShmError::PermissionDenied);
        }
        obj.revoked = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shm_enforces_creator_grants_permissions_and_revocation() {
        let mut reg = ShmRegistry::new();
        let h = reg.create(10, 0x5020_0000, 4).expect("create shm");

        // Creator can map RW.
        assert_eq!(reg.authorize_map(10, h, SHM_PERM_READ | SHM_PERM_WRITE), Ok((0x5020_0000, 4)));

        // Unauthorized task 20 cannot map.
        assert_eq!(reg.authorize_map(20, h, SHM_PERM_READ), Err(ShmError::PermissionDenied));

        // Non-creator cannot grant.
        assert_eq!(reg.grant(20, h, 30, SHM_PERM_READ), Err(ShmError::PermissionDenied));

        // Creator grants read-only to task 20.
        reg.grant(10, h, 20, SHM_PERM_READ).expect("grant RO");
        assert_eq!(reg.authorize_map(20, h, SHM_PERM_READ), Ok((0x5020_0000, 4)));
        assert_eq!(reg.authorize_map(20, h, SHM_PERM_WRITE), Err(ShmError::PermissionDenied));

        // Revocation denies all further mappings.
        reg.revoke(10, h).expect("revoke");
        assert_eq!(reg.authorize_map(20, h, SHM_PERM_READ), Err(ShmError::Revoked));
    }
}
