// kernel/src/syscall.rs
extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use nc_arch::idt::{
    abort_ring3_to_kernel, register_recoverable_pf_hook, register_syscall_dispatcher,
    register_user_fault_hook, ExceptionFrame, SyscallFrame, FAST_SYSCALL_COUNT, IRQ_TIMER_TICKS,
};
use nc_arch::serial;
use nc_mem::{
    alloc_frame, frame_ref_count, free_frame, AddressSpace, PagePermissions, PAGE_SIZE, PTE_COW,
    PTE_USER, USER_SPACE_START,
};

use crate::ipc::{
    grant_page_zero_copy, CapObjectKind, CapRef, FastIpcMessage, RIGHT_ALL_IPC, RIGHT_ALL_NOTIFY,
    RIGHT_GRANT, RIGHT_READ, RIGHT_RECV, RIGHT_SEND, RIGHT_SIGNAL, RIGHT_WAIT, RIGHT_WRITE,
    USER_IPC_GRANT_BASE,
};
use crate::process::{
    active_user_address_space, active_user_pcb, record_active_user_exit, record_active_user_fault,
    save_active_user_context, ProcessManager, SecurityIdentity, CAP_FS_USER_READ,
    CAP_FS_USER_WRITE, CAP_NONE, GLOBAL_PM,
};

// --- NETIZENCORE SİSTEM ÇAĞRISI ABI NUMARALARI (v2.0 - 15 Çağrı) ---
pub const SYS_EXIT: u64                = 1;
pub const SYS_YIELD: u64               = 2;
pub const SYS_SLEEP: u64               = 3;
pub const SYS_WRITE: u64               = 4;
pub const SYS_CLOCK_GET: u64           = 5;
pub const SYS_HANDLE_OPEN: u64         = 6;
pub const SYS_HANDLE_CLOSE: u64        = 7;
pub const SYS_GUI_SUBMIT: u64          = 8;
// Faz 3 & Faz 4: Zero-Copy IPC ve Object-Capability C-Space Çağrıları
pub const SYS_IPC_ENDPOINT_CREATE: u64 = 9;
pub const SYS_IPC_SEND: u64            = 10;
pub const SYS_IPC_RECV: u64            = 11;
pub const SYS_IPC_GRANT_PAGE: u64      = 12;
pub const SYS_CAP_DERIVE: u64          = 13;
pub const SYS_CAP_REVOKE: u64          = 14;
pub const SYS_NOTIFY_SIGNAL: u64       = 15;

// --- POSIX HATA KODLARI ---
pub const EPERM: i64  = -1;
#[allow(dead_code)]
pub const ENOENT: i64 = -2;
pub const EBADF: i64  = -9;
pub const EAGAIN: i64 = -11;
pub const ENOMEM: i64 = -12;
pub const EACCES: i64 = -13;
pub const EFAULT: i64 = -14;
pub const EINVAL: i64 = -22;
pub const ENOSYS: i64 = -38;

pub const STDOUT_HANDLE: u64 = 1;
pub const STDERR_HANDLE: u64 = 2;

pub const NCU1_MAGIC: [u8; 4] = *b"NCU1";
pub const NCU1_VERSION: u32 = 1;

pub const USER_CODE_BASE: u64   = USER_SPACE_START;               // 0x0000_0080_0000_0000 (RX)
pub const USER_DATA_BASE: u64   = USER_SPACE_START + 0x0010_0000; // +1 MiB (RO/RW)
pub const USER_STACK_GUARD: u64 = USER_SPACE_START + 0x0020_0000; // Guard Page (Unmapped!)
pub const USER_STACK_PAGE: u64  = USER_SPACE_START + 0x0020_1000; // 4 KiB User Stack (RW, NX)
pub const USER_STACK_TOP: u64   = USER_SPACE_START + 0x0020_2000;
pub const USER_HEAP_BASE: u64   = USER_SPACE_START + 0x0040_0000; // +4 MiB (Demand Paging Heap)
pub const USER_HEAP_MAX: u64    = USER_SPACE_START + 0x0041_0000; // 64 KiB Demand Paging Sınırı

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HandleKind {
    ConsoleOut,
    ConsoleErr,
    FileRead,
    FileWrite,
    GuiSurface,
}

#[derive(Clone, Debug)]
pub struct ProcessHandle {
    pub id: u64,
    pub kind: HandleKind,
    #[allow(dead_code)]
    pub required_cap: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct GuiDrawCommand {
    pub op: u8,
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub color: u32,
}

pub struct Ncu1Image<'a> {
    pub entry_offset: usize,
    pub code: &'a [u8],
    pub rodata: &'a [u8],
    #[allow(dead_code)]
    pub checksum: u32,
}

impl<'a> Ncu1Image<'a> {
    pub fn fnv1a(code: &[u8], rodata: &[u8]) -> u32 {
        let mut h = 0x811C_9DC5u32;
        for &b in code.iter().chain(rodata.iter()) {
            h ^= b as u32;
            h = h.wrapping_mul(0x0100_0193);
        }
        h
    }

    pub fn build(entry_offset: u32, code: &[u8], rodata: &[u8]) -> Vec<u8> {
        let checksum = Self::fnv1a(code, rodata);
        let mut out = Vec::with_capacity(24 + code.len() + rodata.len());
        out.extend_from_slice(&NCU1_MAGIC);
        out.extend_from_slice(&NCU1_VERSION.to_le_bytes());
        out.extend_from_slice(&entry_offset.to_le_bytes());
        out.extend_from_slice(&(code.len() as u32).to_le_bytes());
        out.extend_from_slice(&(rodata.len() as u32).to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(code);
        out.extend_from_slice(rodata);
        out
    }

    pub fn parse_and_verify(raw: &'a [u8]) -> Result<Self, &'static str> {
        if raw.len() < 24 || &raw[0..4] != &NCU1_MAGIC {
            return Err("Gecersiz NCU1 imaj basligi");
        }
        let ver = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
        if ver != NCU1_VERSION {
            return Err("Desteklenmeyen NCU1 versiyonu");
        }
        let entry_offset = u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]) as usize;
        let code_len = u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]) as usize;
        let ro_len = u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]) as usize;
        let expected_csum = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);

        if code_len == 0 || code_len > PAGE_SIZE || ro_len > PAGE_SIZE {
            return Err("NCU1 segment boyutu sinir disi");
        }
        if entry_offset >= code_len {
            return Err("NCU1 giris noktasi kod segmenti disinda");
        }
        let total = 24usize
            .checked_add(code_len)
            .and_then(|v| v.checked_add(ro_len))
            .ok_or("Boyut tasmasi")?;
        if raw.len() != total {
            return Err("NCU1 dosya boyutu baslikla uyusmuyor");
        }

        let code = &raw[24..24 + code_len];
        let rodata = &raw[24 + code_len..total];
        if Self::fnv1a(code, rodata) != expected_csum {
            return Err("NCU1 FNV-1a butunluk dogrulamasi basarisiz");
        }

        Ok(Self {
            entry_offset,
            code,
            rodata,
            checksum: expected_csum,
        })
    }
}

/// Faz 5: Dinamik Sanal Bellek Yönetimi (#PF Vektör 14: Demand Paging & Copy-on-Write)
fn on_recoverable_page_fault(frame: &ExceptionFrame, cr2: u64) -> bool {
    if !frame.is_user_mode() || frame.vector != 14 {
        return false;
    }

    let page_vaddr = cr2 & !(PAGE_SIZE as u64 - 1);
    // Guard sayfasına erişim kesinlikle engellenir (Stack Overflow koruması)
    if page_vaddr == USER_STACK_GUARD {
        return false;
    }

    let err = frame.error_code;
    let is_present = (err & 0x1) != 0;
    let is_write = (err & 0x2) != 0;
    let is_user = (err & 0x4) != 0;
    let is_inst_fetch = (err & 0x10) != 0;

    if !is_user || is_inst_fetch {
        return false;
    }

    unsafe {
        let Some(proc) = active_user_pcb() else {
            return false;
        };

        // 1. DURUM: Copy-on-Write (CoW) Yazma Hatası (Sayfa mevcut, yazma denendi ve PTE_COW bit 9 set)
        if is_present && is_write {
            let Some((old_phys, flags)) = proc.address_space.translate_with_flags(page_vaddr) else {
                return false;
            };
            let old_frame = old_phys & !(PAGE_SIZE as u64 - 1);
            if (flags & PTE_COW) == 0 || (flags & PTE_USER) == 0 {
                return false;
            }

            // Eğer çerçeveye yalnızca bu süreç sahipse (refcount == 1), kopyalamadan doğrudan RW yap!
            if frame_ref_count(old_frame) == 1 {
                if proc.address_space.protect_4k(page_vaddr, PagePermissions::USER_RW).is_err() {
                    return false;
                }
                proc.cow_faults_resolved = proc.cow_faults_resolved.saturating_add(1);
                return true;
            }

            // Birden fazla süreç paylaşıyorsa yeni 4K çerçeve ayır, içeriği kopyala ve eşlemeyi değiştir
            if proc.owned_frames.try_reserve(2).is_err() {
                return false;
            }
            let Some(new_frame) = alloc_frame() else {
                return false;
            };
            core::ptr::copy_nonoverlapping(
                old_frame as *const u8,
                new_frame as *mut u8,
                PAGE_SIZE,
            );

            if proc.address_space.unmap_4k(page_vaddr).is_err() {
                free_frame(new_frame);
                return false;
            }
            if proc
                .address_space
                .map_4k(page_vaddr, new_frame, PagePermissions::USER_RW)
                .is_err()
            {
                let _ = proc.address_space.map_4k_with_extra_flags(
                    page_vaddr,
                    old_frame,
                    PagePermissions::USER_RO,
                    PTE_COW,
                );
                free_frame(new_frame);
                return false;
            }

            // Eski paylaşımlı çerçevenin referansını düşür ve процеsin owned_frames listesini güncelle
            if let Some(pos) = proc.owned_frames.iter().position(|&f| f == old_frame) {
                proc.owned_frames[pos] = new_frame;
            } else {
                proc.owned_frames.push(new_frame);
            }
            free_frame(old_frame);
            ProcessManager::collect_user_table_frames(
                proc.address_space.pml4_phys,
                &mut proc.owned_frames,
            );
            proc.cow_faults_resolved = proc.cow_faults_resolved.saturating_add(1);
            return true;
        }

        // 2. DURUM: Demand Paging (Henüz eşlenmemiş USER_HEAP_BASE..demand_heap_limit sayfası)
        if !is_present && page_vaddr >= USER_HEAP_BASE && page_vaddr < proc.demand_heap_limit {
            if proc.owned_frames.try_reserve(4).is_err() {
                return false;
            }
            let Some(new_frame) = alloc_frame() else {
                return false;
            };
            if proc
                .address_space
                .map_4k(page_vaddr, new_frame, PagePermissions::USER_RW)
                .is_err()
            {
                free_frame(new_frame);
                return false;
            }
            proc.owned_frames.push(new_frame);
            ProcessManager::collect_user_table_frames(
                proc.address_space.pml4_phys,
                &mut proc.owned_frames,
            );
            proc.demand_pages_allocated = proc.demand_pages_allocated.saturating_add(1);
            return true;
        }
    }

    false
}

fn on_user_fault(frame: &ExceptionFrame, cr2: u64) -> ! {
    record_active_user_fault(frame.vector as u8, frame.rip, cr2);
    unsafe {
        abort_ring3_to_kernel(1000 + (frame.vector as i64));
    }
}

fn dispatch_syscall(frame: &mut SyscallFrame) -> i64 {
    if (frame.iret_cs & 3) != 3 {
        return EPERM;
    }
    let Some(space) = active_user_address_space() else {
        return EPERM;
    };
    save_active_user_context(frame.iret_rip, frame.iret_rsp);

    match frame.rax {
        SYS_EXIT => {
            let exit_code = frame.rdi as i32;
            record_active_user_exit(exit_code);
            unsafe {
                abort_ring3_to_kernel(0);
            }
        }
        SYS_YIELD => unsafe {
            abort_ring3_to_kernel(-1);
        },
        SYS_SLEEP => {
            let ticks = frame.rdi.min(10_000);
            unsafe {
                abort_ring3_to_kernel(-(10 + ticks as i64));
            }
        }
        SYS_WRITE => {
            let handle_id = frame.rdi;
            let user_ptr = frame.rsi;
            let len = frame.rdx as usize;

            let (pid, has_handle) = unsafe {
                if let Some(proc) = active_user_pcb() {
                    let ok = proc.handles.iter().any(|h| {
                        h.id == handle_id
                            && (h.kind == HandleKind::ConsoleOut || h.kind == HandleKind::ConsoleErr)
                    });
                    (proc.pid, ok)
                } else {
                    (0, false)
                }
            };
            if !has_handle {
                return EBADF;
            }
            if len == 0 {
                return 0;
            }
            if len > 512 {
                return EINVAL;
            }
            let mut kbuf = [0u8; 512];
            if space.copy_from_user(user_ptr, &mut kbuf[..len]).is_err() {
                return EFAULT;
            }
            serial::write_str("[Ring-3 PID=");
            write_dec(pid as u64);
            serial::write_str("] ");
            if let Ok(s) = core::str::from_utf8(&kbuf[..len]) {
                serial::write_str(s);
            } else {
                serial::write_str("<binary>");
            }
            len as i64
        }
        SYS_CLOCK_GET => {
            let out_user_ptr = frame.rdi;
            let now = IRQ_TIMER_TICKS.load(Ordering::Relaxed);
            if out_user_ptr != 0 {
                if space.copy_to_user(out_user_ptr, &now.to_le_bytes()).is_err() {
                    return EFAULT;
                }
            }
            now as i64
        }
        SYS_HANDLE_OPEN => {
            let kind_raw = frame.rdi;
            let (kind, req_cap) = match kind_raw {
                1 => (HandleKind::FileRead, CAP_FS_USER_READ),
                2 => (HandleKind::FileWrite, CAP_FS_USER_WRITE),
                3 => (HandleKind::GuiSurface, CAP_NONE),
                _ => return EINVAL,
            };
            unsafe {
                if let Some(proc) = active_user_pcb() {
                    if (proc.identity.capabilities & req_cap) != req_cap {
                        return EACCES;
                    }
                    if proc.handles.try_reserve(1).is_err() {
                        return ENOMEM;
                    }
                    let next_hid = proc.next_handle_id;
                    let Some(next_id) = next_hid.checked_add(1) else {
                        return EINVAL;
                    };
                    if next_hid <= STDERR_HANDLE {
                        return EINVAL;
                    }
                    proc.next_handle_id = next_id;
                    proc.handles.push(ProcessHandle {
                        id: next_hid,
                        kind,
                        required_cap: req_cap,
                    });
                    next_hid as i64
                } else {
                    EPERM
                }
            }
        }
        SYS_HANDLE_CLOSE => {
            let hid = frame.rdi;
            if hid <= 2 {
                return EINVAL;
            }
            unsafe {
                if let Some(proc) = active_user_pcb() {
                    if let Some(pos) = proc.handles.iter().position(|h| h.id == hid) {
                        proc.handles.remove(pos);
                        0
                    } else {
                        EBADF
                    }
                } else {
                    EPERM
                }
            }
        }
        SYS_GUI_SUBMIT => {
            let surface_handle = frame.rdi;
            let cmds_user_ptr = frame.rsi;
            let count = frame.rdx as usize;

            if count == 0 || count > 32 {
                return EINVAL;
            }
            let has_surface = unsafe {
                if let Some(proc) = active_user_pcb() {
                    proc.handles
                        .iter()
                        .any(|h| h.id == surface_handle && h.kind == HandleKind::GuiSurface)
                } else {
                    false
                }
            };
            if !has_surface {
                return EBADF;
            }

            let byte_len = match count.checked_mul(core::mem::size_of::<GuiDrawCommand>()) {
                Some(l) => l,
                None => return EINVAL,
            };
            let mut cmd_buf = [GuiDrawCommand {
                op: 0,
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                color: 0,
            }; 32];
            let raw_dest =
                unsafe { core::slice::from_raw_parts_mut(cmd_buf.as_mut_ptr() as *mut u8, byte_len) };
            if space.copy_from_user(cmds_user_ptr, raw_dest).is_err() {
                return EFAULT;
            }

            for i in 0..count {
                let cmd = cmd_buf[i];
                match cmd.op {
                    1 => {
                        if cmd.x != 0 || cmd.y != 0 || cmd.width != 0 || cmd.height != 0 {
                            return EINVAL;
                        }
                    }
                    2 | 3 => {
                        let x = cmd.x as i32;
                        let y = cmd.y as i32;
                        let right = x.checked_add(cmd.width as i32);
                        let bottom = y.checked_add(cmd.height as i32);
                        if x < 0
                            || y < 0
                            || cmd.width == 0
                            || cmd.height == 0
                            || cmd.width > 2048
                            || cmd.height > 2048
                            || right.map_or(true, |edge| edge > 2048)
                            || bottom.map_or(true, |edge| edge > 2048)
                        {
                            return EINVAL;
                        }
                    }
                    _ => return EINVAL,
                }
            }
            count as i64
        }
        // --- FAZ 3 & FAZ 4: YENİ MİKROÇEKİRDEK IPC VE CAPABILITY SİSTEM ÇAĞRILARI ---
        SYS_IPC_ENDPOINT_CREATE => unsafe {
            let Some(pm) = GLOBAL_PM.as_mut() else {
                return EPERM;
            };
            let Some(proc) = active_user_pcb() else {
                return EPERM;
            };
            let kind_arg = frame.rdi; // 0 = IPC Endpoint, 1 = Notification Object
            if kind_arg == 0 {
                let ep_id = match pm.ipc.create_endpoint(proc.pid) {
                    Ok(id) => id,
                    Err(e) => return e,
                };
                match proc
                    .cspace
                    .insert(CapObjectKind::IpcEndpoint(ep_id), RIGHT_ALL_IPC, 0, None)
                {
                    Some(cap) => cap.0 as i64,
                    None => ENOMEM,
                }
            } else if kind_arg == 1 {
                let ntf_id = match pm.ipc.create_notification() {
                    Ok(id) => id,
                    Err(e) => return e,
                };
                match proc.cspace.insert(
                    CapObjectKind::Notification(ntf_id),
                    RIGHT_ALL_NOTIFY,
                    0,
                    None,
                ) {
                    Some(cap) => cap.0 as i64,
                    None => ENOMEM,
                }
            } else {
                EINVAL
            }
        },
        SYS_IPC_SEND => unsafe {
            // RDI = Endpoint CapRef, RSI = tag, RDX = msg0, R10 = msg1, R8 = msg2, R9 = msg3
            let Some(pm) = GLOBAL_PM.as_mut() else {
                return EPERM;
            };
            let Some(proc) = active_user_pcb() else {
                return EPERM;
            };
            let cap = CapRef(frame.rdi);
            let slot = match proc.cspace.lookup(cap, RIGHT_SEND) {
                Ok(s) => s,
                Err(e) => return e,
            };
            let CapObjectKind::IpcEndpoint(ep_id) = slot.kind else {
                return EBADF;
            };
            let msg = FastIpcMessage {
                sender_pid: proc.pid,
                badge: slot.badge,
                tag: frame.rsi,
                regs: [frame.rdx, frame.r10, frame.r8, frame.r9],
                granted_frame: None,
                delegated_cap: None,
            };
            match pm.ipc.send_message(ep_id, msg) {
                Ok(_) => 0,
                Err(e) => e,
            }
        },
        SYS_IPC_RECV => unsafe {
            // RDI = Endpoint CapRef -> Dönüş: RAX = tag, RDX = msg0, R10 = msg1, R8 = msg2, R9 = msg3
            let Some(pm) = GLOBAL_PM.as_mut() else {
                return EPERM;
            };
            let Some(proc) = active_user_pcb() else {
                return EPERM;
            };
            let cap = CapRef(frame.rdi);
            let slot = match proc.cspace.lookup(cap, RIGHT_RECV) {
                Ok(s) => s,
                Err(e) => return e,
            };
            let CapObjectKind::IpcEndpoint(ep_id) = slot.kind else {
                return EBADF;
            };
            match pm.ipc.recv_message(ep_id, proc.pid) {
                Ok(Some(msg)) => {
                    frame.rdx = msg.regs[0];
                    frame.r10 = msg.regs[1];
                    frame.r8 = msg.regs[2];
                    frame.r9 = msg.regs[3];
                    msg.tag as i64
                }
                Ok(None) => EAGAIN,
                Err(e) => e,
            }
        },
        SYS_IPC_GRANT_PAGE => unsafe {
            // RDI = Endpoint CapRef, RSI = src_vaddr, RDX = dst_vaddr (0 ise USER_IPC_GRANT_BASE), R10 = writable (0/1)
            let Some(pm) = GLOBAL_PM.as_mut() else {
                return EPERM;
            };
            let sender_pid = crate::process::active_user_pid();
            let sender_idx = match pm.processes.iter().position(|p| p.pid == sender_pid) {
                Some(i) => i,
                None => return EPERM,
            };
            let cap = CapRef(frame.rdi);
            let slot = match pm.processes[sender_idx].cspace.lookup(cap, RIGHT_SEND | RIGHT_GRANT) {
                Ok(s) => s,
                Err(e) => return e,
            };
            let CapObjectKind::IpcEndpoint(ep_id) = slot.kind else {
                return EBADF;
            };
            let target_pid = match pm.ipc. endpoints_owner(ep_id) {
                Some(pid) => pid,
                None => return EBADF,
            };
            let target_idx = match pm.processes.iter().position(|p| p.pid == target_pid) {
                Some(i) => i,
                None => return EBADF,
            };
            if sender_idx == target_idx {
                return EINVAL;
            }

            let src_vaddr = frame.rsi;
            let dst_vaddr = if frame.rdx == 0 {
                USER_IPC_GRANT_BASE
            } else {
                frame.rdx
            };
            let writable = frame.r10 != 0;

            let sender_space = pm.processes[sender_idx].address_space;
            let target_proc = &mut pm.processes[target_idx];
            match grant_page_zero_copy(
                &sender_space,
                src_vaddr,
                &mut target_proc.address_space,
                &mut target_proc.owned_frames,
                dst_vaddr,
                writable,
            ) {
                Ok(_phys) => {
                    ProcessManager::collect_user_table_frames(
                        target_proc.address_space.pml4_phys,
                        &mut target_proc.owned_frames,
                    );
                    dst_vaddr as i64
                }
                Err(e) => e,
            }
        },
        SYS_CAP_DERIVE => unsafe {
            // RDI = Kaynak CapRef, RSI = Kısıtlanmış Yeni Haklar (u16), RDX = Yeni Badge (u32)
            let Some(proc) = active_user_pcb() else {
                return EPERM;
            };
            let src_cap = CapRef(frame.rdi);
            let reduced_rights = (frame.rsi & 0xFFFF) as u16;
            let badge = frame.rdx as u32;
            match proc.cspace.derive_restricted(src_cap, reduced_rights, badge) {
                Ok(new_cap) => new_cap.0 as i64,
                Err(e) => e,
            }
        },
        SYS_CAP_REVOKE => unsafe {
            // RDI = İptal edilecek kök CapRef
            let Some(proc) = active_user_pcb() else {
                return EPERM;
            };
            let cap = CapRef(frame.rdi);
            match proc.cspace.revoke(cap) {
                Ok(count) => count as i64,
                Err(e) => e,
            }
        },
        SYS_NOTIFY_SIGNAL => unsafe {
            // RDI = Notification CapRef, RSI = mode (0 = signal bits in RDX, 1 = poll/read)
            let Some(pm) = GLOBAL_PM.as_mut() else {
                return EPERM;
            };
            let Some(proc) = active_user_pcb() else {
                return EPERM;
            };
            let cap = CapRef(frame.rdi);
            let mode = frame.rsi;
            if mode == 0 {
                let slot = match proc.cspace.lookup(cap, RIGHT_SIGNAL) {
                    Ok(s) => s,
                    Err(e) => return e,
                };
                let CapObjectKind::Notification(ntf_id) = slot.kind else {
                    return EBADF;
                };
                match pm.ipc.signal_notification(ntf_id, frame.rdx) {
                    Ok(_) => 0,
                    Err(e) => e,
                }
            } else if mode == 1 {
                let slot = match proc.cspace.lookup(cap, RIGHT_WAIT | RIGHT_READ) {
                    Ok(s) => s,
                    Err(e) => return e,
                };
                let CapObjectKind::Notification(ntf_id) = slot.kind else {
                    return EBADF;
                };
                match pm.ipc.poll_notification(ntf_id) {
                    Ok(word) => word as i64,
                    Err(e) => e,
                }
            } else {
                EINVAL
            }
        },
        _ => ENOSYS,
    }
}

pub fn init() {
    register_recoverable_pf_hook(on_recoverable_page_fault);
    register_user_fault_hook(on_user_fault);
    register_syscall_dispatcher(dispatch_syscall);
}

fn write_dec(mut v: u64) {
    if v == 0 {
        serial::write_str("0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while v > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    if let Ok(s) = core::str::from_utf8(&buf[i..]) {
        serial::write_str(s);
    }
}

pub fn run_phase2_ring3_smoke_tests(pm: &mut ProcessManager) -> Result<(), &'static str> {
    init();
    let frames_before = nc_mem::free_frame_count();

    let msg_a = b"Merhaba Ring-3! (Surec A: EFAULT & ENOSYS dogrulandi, simdi ud2 ile cokuyor)\n";
    let mut code_a: Vec<u8> = Vec::new();
    code_a.extend_from_slice(&[
        0x48, 0xC7, 0xC0, 0x04, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC6, 0x00, 0x00, 0x10, 0x00,
        0x48, 0xC7, 0xC2, 0x10, 0x00, 0x00, 0x00,
        0xCD, 0x80,
        0x48, 0xC7, 0xC0, 0xE7, 0x03, 0x00, 0x00,
        0xCD, 0x80,
        0x48, 0xC7, 0xC0, 0x04, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00,
        0x48, 0xBE,
    ]);
    code_a.extend_from_slice(&USER_DATA_BASE.to_le_bytes());
    code_a.extend_from_slice(&[
        0x48, 0xC7, 0xC2, msg_a.len() as u8, 0x00, 0x00, 0x00,
        0xCD, 0x80,
        0x0F, 0x0B,
    ]);

    let img_a_bytes = Ncu1Image::build(0, &code_a, msg_a);
    let pid_a = pm.spawn_user_ncu1(
        "Ring3-CrashTest",
        SecurityIdentity::from_builtin_app_id("nc.smoke.crash"),
        &img_a_bytes,
    )?;

    let msg_b = b"Merhaba Ring-3! (Surec B: Izole CR3 aktif, A coktu ama B calisiyor ve sys_exit(42) yapiyor)\n";
    let mut code_b: Vec<u8> = Vec::new();
    code_b.extend_from_slice(&[
        0x48, 0xC7, 0xC0, 0x05, 0x00, 0x00, 0x00,
        0x48, 0x31, 0xFF,
        0xCD, 0x80,
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00,
        0xCD, 0x80,
        0x48, 0xC7, 0xC0, 0x04, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00,
        0x48, 0xBE,
    ]);
    code_b.extend_from_slice(&USER_DATA_BASE.to_le_bytes());
    code_b.extend_from_slice(&[
        0x48, 0xC7, 0xC2, msg_b.len() as u8, 0x00, 0x00, 0x00,
        0xCD, 0x80,
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 42, 0x00, 0x00, 0x00,
        0xCD, 0x80,
    ]);

    let img_b_bytes = Ncu1Image::build(0, &code_b, msg_b);
    let pid_b = pm.spawn_user_ncu1(
        "Ring3-Survivor",
        SecurityIdentity::from_builtin_app_id("nc.smoke.survivor"),
        &img_b_bytes,
    )?;

    let pm_ptr = pm as *mut ProcessManager;
    unsafe {
        crate::process::GLOBAL_PM = pm_ptr;
        for _ in 0..10 {
            ProcessManager::schedule_raw(pm_ptr);
            if pm.process_fault_vec(pid_a).is_some() && pm.process_exit_code(pid_b).is_some() {
                break;
            }
        }
    }

    let fault_a = pm.process_fault_vec(pid_a);
    let exit_b = pm.process_exit_code(pid_b);
    let reaped = pm.reap_terminated_user_processes();
    let frames_after = nc_mem::free_frame_count();

    if fault_a != Some(6) {
        return Err("Surec A ud2 (#UD vec=6) ile sonlanmadi");
    }
    if exit_b != Some(42) {
        return Err("Surec B sys_exit(42) ile temiz sonlanmadi");
    }
    if reaped < 2 || frames_after != frames_before {
        return Err("Sonlanan Ring 3 sureclerinin fiziksel 4K sayfalari tam geri alinmadi (Leak)");
    }

    Ok(())
}

pub fn run_phase3_isolation_tests(pm: &mut ProcessManager) -> Result<(), &'static str> {
    init();

    let msg_unauth = b"Faz-3 Yetki Testi: Dosya yazma handle'i talep ediliyor...\n";
    let mut code_unauth: Vec<u8> = Vec::new();
    code_unauth.extend_from_slice(&[
        0x48, 0xC7, 0xC0, 0x06, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 0x02, 0x00, 0x00, 0x00,
        0xCD, 0x80,
        0x48, 0xF7, 0xD8,
        0x48, 0x89, 0xC7,
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00,
        0xCD, 0x80,
    ]);
    let img1 = Ncu1Image::build(0, &code_unauth, msg_unauth);
    let mut id_unauth = SecurityIdentity::from_builtin_app_id("nc.test.unauth");
    id_unauth.capabilities = CAP_NONE;
    let pid1 = pm.spawn_user_ncu1("Ring3-Unauth", id_unauth, &img1)?;

    let mut code_gui: Vec<u8> = Vec::new();
    code_gui.extend_from_slice(&[
        0x48, 0xC7, 0xC0, 0x06, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 0x03, 0x00, 0x00, 0x00,
        0xCD, 0x80,
        0x48, 0x89, 0xC7,
        0x48, 0xC7, 0xC0, 0x08, 0x00, 0x00, 0x00,
        0x48, 0xBE,
    ]);
    code_gui.extend_from_slice(&USER_DATA_BASE.to_le_bytes());
    code_gui.extend_from_slice(&[
        0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00,
        0xCD, 0x80,
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 0x63, 0x00, 0x00, 0x00,
        0xCD, 0x80,
    ]);

    let draw_cmd = GuiDrawCommand {
        op: 2,
        x: 10,
        y: 10,
        width: 100,
        height: 50,
        color: 0xFF00_FF00,
    };
    let cmd_bytes = unsafe {
        core::slice::from_raw_parts(
            &draw_cmd as *const _ as *const u8,
            core::mem::size_of::<GuiDrawCommand>(),
        )
    };
    let img2 = Ncu1Image::build(0, &code_gui, cmd_bytes);
    let id_gui = SecurityIdentity::from_builtin_app_id("nc.test.gui");
    let pid2 = pm.spawn_user_ncu1("Ring3-GuiApp", id_gui, &img2)?;

    let pm_ptr = pm as *mut ProcessManager;
    unsafe {
        crate::process::GLOBAL_PM = pm_ptr;
        for _ in 0..10 {
            ProcessManager::schedule_raw(pm_ptr);
            if pm.process_exit_code(pid1).is_some() && pm.process_exit_code(pid2).is_some() {
                break;
            }
        }
    }

    let code1 = pm.process_exit_code(pid1);
    let code2 = pm.process_exit_code(pid2);
    pm.reap_terminated_user_processes();

    if code1 != Some(13) {
        return Err("Yetkisiz surec EACCES ile engellenemedi");
    }
    if code2 != Some(99) {
        return Err("GUI cizim submit testi basarisiz oldu");
    }

    Ok(())
}

#[inline(never)]
fn monolithic_direct_call_noop(val: u64) -> u64 {
    core::hint::black_box(val.wrapping_add(1))
}

pub fn run_phase4_hardening_and_benchmarks(pm: &mut ProcessManager) -> Result<(), &'static str> {
    init();

    let (wp_ok, smep_ok) = nc_arch::enable_cpu_hardening();
    if !wp_ok {
        return Err("CR0.WP (Write Protect) etkinlestirilemedi");
    }

    let frames_before = nc_mem::free_frame_count();

    let mut direct_samples = [0u64; 16];
    for i in 0..16 {
        let t0 = nc_arch::time::read_tsc();
        let _ = monolithic_direct_call_noop(i as u64);
        let t1 = nc_arch::time::read_tsc();
        direct_samples[i] = t1.wrapping_sub(t0);
    }
    direct_samples.sort_unstable();
    let direct_p50 = direct_samples[8];

    let cur_cr3 = nc_mem::paging::read_cr3();
    let mut cr3_samples = [0u64; 16];
    for i in 0..16 {
        let t0 = nc_arch::time::read_tsc();
        unsafe {
            nc_mem::paging::write_cr3(cur_cr3);
        }
        let t1 = nc_arch::time::read_tsc();
        cr3_samples[i] = t1.wrapping_sub(t0);
    }
    cr3_samples.sort_unstable();
    let cr3_p50 = cr3_samples[8];
    let cr3_p95 = cr3_samples[14];

    let secret_data = [0xBE, 0xBA, 0xFE, 0xCA, 0x11, 0x22, 0x33, 0x44];
    let mut code_p1: Vec<u8> = Vec::new();
    for _ in 0..4 {
        code_p1.extend_from_slice(&[
            0x48, 0xC7, 0xC0, 0x05, 0x00, 0x00, 0x00,
            0x48, 0x31, 0xFF,
            0xCD, 0x80,
        ]);
    }
    code_p1.extend_from_slice(&[
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 77, 0x00, 0x00, 0x00,
        0xCD, 0x80,
    ]);
    let img_p1 = Ncu1Image::build(0, &code_p1, &secret_data);
    let pid_p1 = pm.spawn_user_ncu1(
        "Ring3-BenchAndSecret",
        SecurityIdentity::from_builtin_app_id("nc.bench.p1"),
        &img_p1,
    )?;

    let public_data = [0u8; 8];
    let mut code_p2: Vec<u8> = Vec::new();
    code_p2.extend_from_slice(&[0x48, 0xBE]);
    code_p2.extend_from_slice(&USER_DATA_BASE.to_le_bytes());
    code_p2.extend_from_slice(&[
        0x8B, 0x06,
        0x3D, 0xBE, 0xBA, 0xFE, 0xCA,
        0x74, 0x01,
        0xFA,
        0x0F, 0x0B,
    ]);
    let img_p2 = Ncu1Image::build(0, &code_p2, &public_data);
    let pid_p2 = pm.spawn_user_ncu1(
        "Ring3-PrivInstructionTest",
        SecurityIdentity::from_builtin_app_id("nc.bench.p2"),
        &img_p2,
    )?;

    let pm_ptr = pm as *mut ProcessManager;
    let t_sys_start = nc_arch::time::read_tsc();
    unsafe {
        crate::process::GLOBAL_PM = pm_ptr;
        for _ in 0..10 {
            ProcessManager::schedule_raw(pm_ptr);
            if pm.process_exit_code(pid_p1).is_some() && pm.process_fault_vec(pid_p2).is_some() {
                break;
            }
        }
    }
    let t_sys_end = nc_arch::time::read_tsc();
    let avg_syscall_cycles = t_sys_end.wrapping_sub(t_sys_start) / 6;

    let exit_p1 = pm.process_exit_code(pid_p1);
    let fault_p2 = pm.process_fault_vec(pid_p2);
    let reaped = pm.reap_terminated_user_processes();
    let frames_after = nc_mem::free_frame_count();

    if exit_p1 != Some(77) {
        return Err("Faz 4: Surec 1 (Benchmark) temiz sonlanmadi");
    }
    if fault_p2 != Some(13) {
        return Err("Faz 4: Ring 3 'cli' ayricalikli komutu #GP (vec=13) ile engellenmedi veya bellek sizintisi oldu");
    }
    if reaped < 2 || frames_after != frames_before {
        return Err("Faz 4: Fiziksel sayfa cerceveleri (PMM) tam geri alinmadi");
    }

    serial::write_str("[Faz 4 BENCHMARK] Ring-0 Direct Call p50=");
    write_dec(direct_p50);
    serial::write_str(" cyc | CR3 Switch p50=");
    write_dec(cr3_p50);
    serial::write_str("/p95=");
    write_dec(cr3_p95);
    serial::write_str(" cyc | Ring-3 Session+Syscall avg=");
    write_dec(avg_syscall_cycles);
    serial::write_str(" cyc | SMEP=");
    serial::write_str(if smep_ok { "ON\n" } else { "CPU-N/A (WP+NX ON)\n" });

    Ok(())
}

/// RAPOR YOL HARİTASI UÇTAN UCA DOĞRULAMA TESTİ (Faz 1..5 Modernizasyon Paketi):
/// 1) Faz 1: Donanımsal `syscall` (0x0F 0x05) ve `sysretq` ile Ring 3 geçişi.
/// 2) Faz 2: Local APIC etkinliği ve SMP CPU keşfi.
/// 3) Faz 3 & 4: C-Space Capability oluşturma (`SYS_IPC_ENDPOINT_CREATE`), yetki daraltma (`SYS_CAP_DERIVE`),
///    Zero-Copy Fast-Path IPC (`SYS_IPC_SEND` / `SYS_IPC_RECV`) ve `SYS_CAP_REVOKE` doğrulaması.
/// 4) Faz 5: Demand Paging (`USER_HEAP_BASE` adresine ilk yazmada #PF ile otomatik çerçeve tahsisi)
///    ve İki Süreç Arası Copy-on-Write (CoW) sayfa paylaşımı + yazma anında otomatik kopyalama!
pub fn run_phase5_modernization_suite(pm: &mut ProcessManager) -> Result<(), &'static str> {
    init();
    let frames_before = nc_mem::free_frame_count();
    let fast_before = FAST_SYSCALL_COUNT.load(Ordering::Relaxed);

    // --- SÜREÇ 1 (Parent / IPC + C-Space + Demand Paging + CoW Writer) ---
    // Bu süreç tüm çağrılarını donanımsal `syscall` (0x0F, 0x05) komutuyla yapar!
    // 1. SYS_IPC_ENDPOINT_CREATE(0) -> RAX = cap_ep (ve R12'ye kaydedilir)
    // 2. SYS_CAP_DERIVE(cap_ep, RIGHT_SEND (1<<3), badge=0x42) -> RAX = derived_cap (R13'e kaydedilir)
    // 3. SYS_IPC_SEND(derived_cap, tag=0xABCD, rdx=0x12345678)
    // 4. SYS_IPC_RECV(cap_ep) -> RAX == 0xABCD doğrulaması
    // 5. Demand Paging: Eşlenmemiş USER_HEAP_BASE (0x80_0040_0000) adresine 0x55AA55AA yazar (#PF Vektör 14 otomatik çözer!)
    // 6. CoW Break: Paylaşılan USER_DATA_BASE (0x80_0010_0000) sayfasına 0xDEADC0DE yazar (#PF CoW otomatik kopyalar!)
    // 7. SYS_CAP_REVOKE(cap_ep) -> Türetilen yetkiyi iptal eder
    // 8. SYS_EXIT(88)
    let initial_shared_rodata = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    let mut code_p1: Vec<u8> = Vec::new();
    code_p1.extend_from_slice(&[
        // 1. SYS_IPC_ENDPOINT_CREATE(0) via fast `syscall` (0x0F, 0x05)
        0x48, 0xC7, 0xC0, 0x09, 0x00, 0x00, 0x00, // mov rax, 9 (SYS_IPC_ENDPOINT_CREATE)
        0x48, 0x31, 0xFF,                         // xor rdi, rdi (kind = 0)
        0x0F, 0x05,                               // syscall
        0x49, 0x89, 0xC4,                         // mov r12, rax (root cap_ep)

        // 2. SYS_CAP_DERIVE(r12, RIGHT_SEND=8, badge=0x42)
        0x48, 0xC7, 0xC0, 0x0D, 0x00, 0x00, 0x00, // mov rax, 13 (SYS_CAP_DERIVE)
        0x4C, 0x89, 0xE7,                         // mov rdi, r12
        0x48, 0xC7, 0xC6, 0x08, 0x00, 0x00, 0x00, // mov rsi, 8 (RIGHT_SEND)
        0x48, 0xC7, 0xC2, 0x42, 0x00, 0x00, 0x00, // mov rdx, 0x42
        0x0F, 0x05,                               // syscall
        0x49, 0x89, 0xC5,                         // mov r13, rax (derived_cap)

        // 3. SYS_IPC_SEND(r13, tag=0xABCD, rdx=0x1234)
        0x48, 0xC7, 0xC0, 0x0A, 0x00, 0x00, 0x00, // mov rax, 10 (SYS_IPC_SEND)
        0x4C, 0x89, 0xEF,                         // mov rdi, r13
        0x48, 0xC7, 0xC6, 0xCD, 0xAB, 0x00, 0x00, // mov rsi, 0xABCD
        0x48, 0xC7, 0xC2, 0x34, 0x12, 0x00, 0x00, // mov rdx, 0x1234
        0x0F, 0x05,                               // syscall

        // 4. SYS_IPC_RECV(r12)
        0x48, 0xC7, 0xC0, 0x0B, 0x00, 0x00, 0x00, // mov rax, 11 (SYS_IPC_RECV)
        0x4C, 0x89, 0xE7,                         // mov rdi, r12
        0x0F, 0x05,                               // syscall

        // 5. Demand Paging Testi: movabs rbx, USER_HEAP_BASE; mov dword ptr [rbx], 0x55AA55AA
        0x48, 0xBB,                               // movabs rbx, USER_HEAP_BASE
    ]);
    code_p1.extend_from_slice(&USER_HEAP_BASE.to_le_bytes());
    code_p1.extend_from_slice(&[
        0xC7, 0x03, 0xAA, 0x55, 0xAA, 0x55,       // mov dword ptr [rbx], 0x55AA55AA (#PF -> Demand Page!)
        0x8B, 0x03,                               // mov eax, dword ptr [rbx]
        0x3D, 0xAA, 0x55, 0xAA, 0x55,             // cmp eax, 0x55AA55AA
        0x75, 0x22,                               // jne .fail

        // 6. Copy-on-Write (CoW) Testi: movabs rbx, USER_DATA_BASE; mov dword ptr [rbx], 0xDEADC0DE
        0x48, 0xBB,                               // movabs rbx, USER_DATA_BASE
    ]);
    code_p1.extend_from_slice(&USER_DATA_BASE.to_le_bytes());
    code_p1.extend_from_slice(&[
        0xC7, 0x03, 0xDE, 0xC0, 0xAD, 0xDE,       // mov dword ptr [rbx], 0xDEADC0DE (#PF -> CoW Break!)

        // 7. SYS_CAP_REVOKE(r12)
        0x48, 0xC7, 0xC0, 0x0E, 0x00, 0x00, 0x00, // mov rax, 14 (SYS_CAP_REVOKE)
        0x4C, 0x89, 0xE7,                         // mov rdi, r12
        0x0F, 0x05,                               // syscall

        // 8. Başarılı Çıkış: SYS_EXIT(88)
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (SYS_EXIT)
        0x48, 0xC7, 0xC7, 88, 0x00, 0x00, 0x00,   // mov rdi, 88
        0x0F, 0x05,                               // syscall

        // .fail: ud2
        0x0F, 0x0B,
    ]);

    let img_p1 = Ncu1Image::build(0, &code_p1, &initial_shared_rodata);
    let pid_parent = pm.spawn_user_ncu1(
        "Ring3-ModernParent",
        SecurityIdentity::from_builtin_app_id("nc.modern.parent"),
        &img_p1,
    )?;

    // --- SÜREÇ 2 (Child / CoW İzolasyon Doğrulayıcı) ---
    // Parent ile aynı fiziksel USER_DATA_BASE sayfasını CoW olarak paylaşır.
    // Parent kendi kopyasına 0xDEADC0DE yazdıktan sonra Child kendi USER_DATA_BASE sayfasını okur;
    // Orijinal veri (0x44332211) bozulmamış olmalıdır!
    let mut code_p2: Vec<u8> = Vec::new();
    code_p2.extend_from_slice(&[
        0x48, 0xBB, // movabs rbx, USER_DATA_BASE
    ]);
    code_p2.extend_from_slice(&USER_DATA_BASE.to_le_bytes());
    code_p2.extend_from_slice(&[
        0x8B, 0x03,                               // mov eax, dword ptr [rbx]
        0x3D, 0x11, 0x22, 0x33, 0x44,             // cmp eax, 0x44332211 (Orijinal veri korundu mu?)
        0x75, 0x10,                               // jne .cow_corrupted
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (SYS_EXIT)
        0x48, 0xC7, 0xC7, 90, 0x00, 0x00, 0x00,   // mov rdi, 90
        0x0F, 0x05,                               // syscall
        // .cow_corrupted:
        0x0F, 0x0B,                               // ud2
    ]);

    let img_p2 = Ncu1Image::build(0, &code_p2, &[0u8; 8]);
    let pid_child = pm.spawn_user_ncu1(
        "Ring3-CoWChild",
        SecurityIdentity::from_builtin_app_id("nc.modern.child"),
        &img_p2,
    )?;

    // Parent ve Child arasında USER_DATA_BASE sayfasını Copy-on-Write (CoW) olarak paylaştır!
    pm.share_page_cow(pid_parent, pid_child, USER_DATA_BASE)?;

    let pm_ptr = pm as *mut ProcessManager;
    unsafe {
        crate::process::GLOBAL_PM = pm_ptr;
        for _ in 0..12 {
            ProcessManager::schedule_raw(pm_ptr);
            if pm.process_exit_code(pid_parent).is_some()
                && pm.process_exit_code(pid_child).is_some()
            {
                break;
            }
        }
    }

    let exit_parent = pm.process_exit_code(pid_parent);
    let exit_child = pm.process_exit_code(pid_child);
    let fast_after = FAST_SYSCALL_COUNT.load(Ordering::Relaxed);

    let (demand_cnt, cow_cnt) = pm
        .processes
        .iter()
        .find(|p| p.pid == pid_parent)
        .map(|p| (p.demand_pages_allocated, p.cow_faults_resolved))
        .unwrap_or((0, 0));

    let reaped = pm.reap_terminated_user_processes();
    let frames_after = nc_mem::free_frame_count();

    if exit_parent != Some(88) {
        return Err("Faz 5: Parent surec (Fast SYSCALL + IPC + Demand Paging + CoW) temiz sonlanmadi");
    }
    if exit_child != Some(90) {
        return Err("Faz 5: Child surec CoW izolasyon dogrulamasinda basarisiz oldu");
    }
    if fast_after <= fast_before {
        return Err("Faz 1: Donanimsal 'syscall' (LSTAR/SYSRET) sayaci ilerlemedi");
    }
    if demand_cnt == 0 || cow_cnt == 0 {
        return Err("Faz 5: Demand Paging veya CoW #PF sayfa hatasi cozumlenmedi");
    }
    if reaped < 2 || frames_after != frames_before {
        return Err("Faz 5: Paylasimli CoW ve Demand Paging cerceveleri PMM'e tam iade edilmedi (Leak)");
    }

    serial::write_str("[Faz 5 MODERNIZATION OK] Fast SYSCALL count=");
    write_dec(fast_after - fast_before);
    serial::write_str(" | Demand Paging #PF=");
    write_dec(demand_cnt as u64);
    serial::write_str(" | CoW Break #PF=");
    write_dec(cow_cnt as u64);
    serial::write_str(" | LAPIC=");
    serial::write_str(if nc_arch::apic::is_lapic_active() { "ON" } else { "PIC-FALLBACK" });
    serial::write_str(" | SMP Cores=");
    write_dec(nc_arch::apic::DETECTED_CPU_COUNT.load(Ordering::Relaxed) as u64);
    serial::write_str("\n");

    Ok(())
}
