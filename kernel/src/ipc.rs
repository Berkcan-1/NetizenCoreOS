// kernel/src/ipc.rs
#![allow(dead_code)]

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use nc_mem::{
    frame_ref_count, free_frame, retain_frame, AddressSpace, PagePermissions, PAGE_SIZE,
    USER_SPACE_END, USER_SPACE_START,
};

/// Zero-Copy Page Granting için alıcı süreçte varsayılan pencere tabanı (+3 MiB)
pub const USER_IPC_GRANT_BASE: u64 = USER_SPACE_START + 0x0030_0000;

// --- FAZ 4: OBJECT-CAPABILITY HAK BİT MASKELERİ (CapRights) ---
pub const RIGHT_NONE: u16     = 0;
pub const RIGHT_READ: u16     = 1 << 0;
pub const RIGHT_WRITE: u16    = 1 << 1;
pub const RIGHT_EXECUTE: u16  = 1 << 2;
pub const RIGHT_SEND: u16     = 1 << 3;
pub const RIGHT_RECV: u16     = 1 << 4;
pub const RIGHT_GRANT: u16    = 1 << 5;
pub const RIGHT_DELEGATE: u16 = 1 << 6;
pub const RIGHT_REVOKE: u16   = 1 << 7;
pub const RIGHT_SIGNAL: u16   = 1 << 8;
pub const RIGHT_WAIT: u16     = 1 << 9;

pub const RIGHT_ALL_IPC: u16 =
    RIGHT_READ | RIGHT_WRITE | RIGHT_SEND | RIGHT_RECV | RIGHT_GRANT | RIGHT_DELEGATE | RIGHT_REVOKE;
pub const RIGHT_ALL_NOTIFY: u16 =
    RIGHT_READ | RIGHT_WRITE | RIGHT_SIGNAL | RIGHT_WAIT | RIGHT_DELEGATE | RIGHT_REVOKE;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CapObjectKind {
    Empty,
    IpcEndpoint(usize),
    Notification(usize),
    SharedPageFrame(u64),
    GuiSurface(u32),
    FileResource(u32),
}

/// Taklit edilemez Capability Yuvası (C-Space Slot)
#[derive(Clone, Copy, Debug)]
pub struct CapabilitySlot {
    pub kind: CapObjectKind,
    pub rights: u16,
    pub generation: u16,
    pub badge: u32,
    pub parent_slot: Option<u16>,
    pub revoked: bool,
}

impl CapabilitySlot {
    pub const EMPTY: Self = Self {
        kind: CapObjectKind::Empty,
        rights: RIGHT_NONE,
        generation: 0,
        badge: 0,
        parent_slot: None,
        revoked: false,
    };

    #[inline]
    pub fn is_active(&self) -> bool {
        !self.revoked && self.kind != CapObjectKind::Empty
    }
}

/// Kullanıcı sürecine verilen 64-bit Taklit Edilemez Capability Referansı (CapRef)
/// Bit [15:0]  = Slot İndeksi
/// Bit [31:16] = Generation (Use-after-revoke koruması)
/// Bit [47:32] = Hak Özet Maskesi (Doğrulama için)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CapRef(pub u64);

impl CapRef {
    #[inline]
    pub fn encode(slot_idx: u16, generation: u16, rights: u16) -> Self {
        let raw = (slot_idx as u64)
            | ((generation as u64) << 16)
            | ((rights as u64) << 32);
        Self(raw)
    }

    #[inline]
    pub fn slot_index(self) -> usize {
        (self.0 & 0xFFFF) as usize
    }

    #[inline]
    pub fn generation(self) -> u16 {
        ((self.0 >> 16) & 0xFFFF) as u16
    }
}

pub const MAX_CSPACE_SLOTS: usize = 64;

/// Süreç Başına Capability Uzayı (Per-Process C-Space)
#[derive(Clone)]
pub struct CSpace {
    slots: [CapabilitySlot; MAX_CSPACE_SLOTS],
}

impl CSpace {
    pub const fn new() -> Self {
        Self {
            slots: [CapabilitySlot::EMPTY; MAX_CSPACE_SLOTS],
        }
    }

    pub fn insert(
        &mut self,
        kind: CapObjectKind,
        rights: u16,
        badge: u32,
        parent_slot: Option<u16>,
    ) -> Option<CapRef> {
        for (idx, slot) in self.slots.iter_mut().enumerate() {
            if !slot.is_active() {
                let next_gen = slot.generation.wrapping_add(1).max(1);
                *slot = CapabilitySlot {
                    kind,
                    rights,
                    generation: next_gen,
                    badge,
                    parent_slot,
                    revoked: false,
                };
                return Some(CapRef::encode(idx as u16, next_gen, rights));
            }
        }
        None
    }

    pub fn lookup(&self, cap: CapRef, required_rights: u16) -> Result<CapabilitySlot, i64> {
        let idx = cap.slot_index();
        if idx >= MAX_CSPACE_SLOTS {
            return Err(-9); // EBADF
        }
        let slot = self.slots[idx];
        if !slot.is_active() || slot.generation != cap.generation() {
            return Err(-9); // EBADF
        }
        if (slot.rights & required_rights) != required_rights {
            return Err(-13); // EACCES (Hak yetersiz!)
        }
        Ok(slot)
    }

    /// Mevcut bir yetkiden daha kısıtlı (alt küme) haklara sahip yeni bir yetki türetir (Mint/Derive).
    /// Hak yükseltme (Privilege Escalation) donanımsal/mantıksal olarak reddedilir!
    pub fn derive_restricted(
        &mut self,
        src_cap: CapRef,
        reduced_rights: u16,
        new_badge: u32,
    ) -> Result<CapRef, i64> {
        let src_slot = self.lookup(src_cap, RIGHT_DELEGATE)?;
        // Yeni haklar kaynak hakların kesin alt kümesi olmalıdır
        if (reduced_rights & !src_slot.rights) != 0 {
            return Err(-13); // EACCES: Sahip olunmayan yetki türetilemez!
        }
        let parent_idx = src_cap.slot_index() as u16;
        if let CapObjectKind::SharedPageFrame(phys) = src_slot.kind {
            let _ = retain_frame(phys);
        }
        self.insert(src_slot.kind, reduced_rights, new_badge, Some(parent_idx))
            .ok_or(-12) // ENOMEM
    }

    /// Yetkiyi ve ondan türetilmiş (child) tüm alt yetkileri iptal eder (Revocation Tree).
    pub fn revoke(&mut self, cap: CapRef) -> Result<usize, i64> {
        let _ = self.lookup(cap, RIGHT_REVOKE)?;
        let root_idx = cap.slot_index() as u16;
        let mut revoked_count = 0usize;

        for idx in 0..MAX_CSPACE_SLOTS {
            if self.slots[idx].is_active()
                && (idx as u16 == root_idx || self.slots[idx].parent_slot == Some(root_idx))
            {
                if let CapObjectKind::SharedPageFrame(phys) = self.slots[idx].kind {
                    unsafe {
                        free_frame(phys);
                    }
                }
                self.slots[idx].revoked = true;
                self.slots[idx].kind = CapObjectKind::Empty;
                self.slots[idx].generation = self.slots[idx].generation.wrapping_add(1);
                revoked_count += 1;
            }
        }
        Ok(revoked_count)
    }

    pub fn release_all(&mut self) {
        for slot in self.slots.iter_mut() {
            if slot.is_active() {
                if let CapObjectKind::SharedPageFrame(phys) = slot.kind {
                    unsafe {
                        free_frame(phys);
                    }
                }
                *slot = CapabilitySlot::EMPTY;
            }
        }
    }
}

// --- FAZ 3: ZERO-COPY IPC MESAJ VE ENDPOINT MOTORU ---

#[derive(Clone, Copy, Debug)]
pub struct FastIpcMessage {
    pub sender_pid: usize,
    pub badge: u32,
    pub tag: u64,
    pub regs: [u64; 4], // Yazmaç tabanlı (R10, R8, R9, RDX) Zero-Copy yükü
    pub granted_frame: Option<(u64, u16)>, // (Fiziksel 4K Çerçeve, İzinler)
    pub delegated_cap: Option<CapabilitySlot>,
}

pub struct IpcEndpoint {
    pub id: usize,
    pub owner_pid: usize,
    pub queue: VecDeque<FastIpcMessage>,
    pub waiting_receiver_pid: Option<usize>,
}

pub struct NotificationObject {
    pub id: usize,
    pub word: u64,
    pub waiting_pid: Option<usize>,
}

pub struct IpcManager {
    endpoints: Vec<IpcEndpoint>,
    notifications: Vec<NotificationObject>,
    next_ep_id: usize,
    next_ntf_id: usize,
}

impl IpcManager {
    pub const fn new() -> Self {
        Self {
            endpoints: Vec::new(),
            notifications: Vec::new(),
            next_ep_id: 1,
            next_ntf_id: 1,
        }
    }

    pub fn create_endpoint(&mut self, owner_pid: usize) -> Result<usize, i64> {
        self.endpoints.try_reserve(1).map_err(|_| -12i64)?;
        let id = self.next_ep_id;
        self.next_ep_id += 1;
        self.endpoints.push(IpcEndpoint {
            id,
            owner_pid,
            queue: VecDeque::new(),
            waiting_receiver_pid: None,
        });
        Ok(id)
    }

    pub fn endpoints_owner(&self, ep_id: usize) -> Option<usize> {
        self.endpoints.iter().find(|e| e.id == ep_id).map(|e| e.owner_pid)
    }

    pub fn create_notification(&mut self) -> Result<usize, i64> {
        self.notifications.try_reserve(1).map_err(|_| -12i64)?;
        let id = self.next_ntf_id;
        self.next_ntf_id += 1;
        self.notifications.push(NotificationObject {
            id,
            word: 0,
            waiting_pid: None,
        });
        Ok(id)
    }

    pub fn send_message(&mut self, ep_id: usize, msg: FastIpcMessage) -> Result<Option<usize>, i64> {
        let ep = self.endpoints.iter_mut().find(|e| e.id == ep_id).ok_or(-9i64)?;
        if ep.queue.len() >= 32 {
            return Err(-12); // ENOMEM (Kuyruk dolu)
        }
        ep.queue.try_reserve(1).map_err(|_| -12i64)?;
        ep.queue.push_back(msg);
        Ok(ep.waiting_receiver_pid.take())
    }

    pub fn recv_message(&mut self, ep_id: usize, receiver_pid: usize) -> Result<Option<FastIpcMessage>, i64> {
        let ep = self.endpoints.iter_mut().find(|e| e.id == ep_id).ok_or(-9i64)?;
        if let Some(msg) = ep.queue.pop_front() {
            Ok(Some(msg))
        } else {
            ep.waiting_receiver_pid = Some(receiver_pid);
            Ok(None)
        }
    }

    pub fn signal_notification(&mut self, ntf_id: usize, bits: u64) -> Result<Option<usize>, i64> {
        let ntf = self.notifications.iter_mut().find(|n| n.id == ntf_id).ok_or(-9i64)?;
        ntf.word |= bits;
        Ok(ntf.waiting_pid.take())
    }

    pub fn poll_notification(&mut self, ntf_id: usize) -> Result<u64, i64> {
        let ntf = self.notifications.iter_mut().find(|n| n.id == ntf_id).ok_or(-9i64)?;
        let val = ntf.word;
        ntf.word = 0; // Atomik oku ve sıfırla
        Ok(val)
    }
}

/// Zero-Copy Page Granting / Lending:
/// Göndericinin `src_vaddr` sayfasındaki fiziksel çerçeveyi kopyalamadan (`retain_frame` ile)
/// alıcının `dst_vaddr` sanal adresine doğrudan eşler!
pub fn grant_page_zero_copy(
    sender_space: &AddressSpace,
    src_vaddr: u64,
    receiver_space: &mut AddressSpace,
     receiver_owned_frames: &mut Vec<u64>,
    dst_vaddr: u64,
    writable: bool,
) -> Result<u64, i64> {
    if (src_vaddr & 0xFFF) != 0 || (dst_vaddr & 0xFFF) != 0 {
        return Err(-22); // EINVAL
    }
    if src_vaddr < USER_SPACE_START
        || dst_vaddr < USER_SPACE_START
        || dst_vaddr.saturating_add(PAGE_SIZE as u64 - 1) > USER_SPACE_END
    {
        return Err(-14); // EFAULT
    }

    let (phys_frame, flags) = sender_space.translate_with_flags(src_vaddr).ok_or(-14i64)?;
    let frame_base = phys_frame & !0xFFFu64;
    if (flags & nc_mem::PTE_USER) == 0 {
        return Err(-1); // EPERM
    }
    if writable && (flags & nc_mem::PTE_WRITABLE) == 0 {
        return Err(-13); // EACCES: Salt-okunur sayfa yazılabilir olarak hibe edilemez
    }

    receiver_owned_frames.try_reserve(1).map_err(|_| -12i64)?;
    retain_frame(frame_base).map_err(|_| -12i64)?;

    let perms = if writable {
        PagePermissions::USER_RW
    } else {
        PagePermissions::USER_RO
    };

    if receiver_space.map_4k(dst_vaddr, frame_base, perms).is_err() {
        unsafe {
            free_frame(frame_base);
        }
        return Err(-22); // EINVAL / AlreadyMapped
    }

    receiver_owned_frames.push(frame_base);
    debug_assert!(frame_ref_count(frame_base) >= 2);
    Ok(frame_base)
}
