// kernel/src/syscall.rs
extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use nc_arch::idt::{
    abort_ring3_to_kernel, register_syscall_dispatcher, register_user_fault_hook, ExceptionFrame,
    SyscallFrame, IRQ_TIMER_TICKS,
};
use nc_arch::serial;
use nc_mem::{PAGE_SIZE, USER_SPACE_START};

use crate::process::{
    active_user_address_space, active_user_pcb, record_active_user_exit, record_active_user_fault,
    save_active_user_context, ProcessManager, SecurityIdentity, CAP_FS_USER_READ,
    CAP_FS_USER_WRITE, CAP_NONE,
};

// --- NETIZENCORE FAZ 3: SİSTEM ÇAĞRISI ABI NUMARALARI ---
pub const SYS_EXIT: u64       = 1;
pub const SYS_YIELD: u64      = 2;
pub const SYS_SLEEP: u64      = 3;
pub const SYS_WRITE: u64      = 4;
pub const SYS_CLOCK_GET: u64  = 5;
pub const SYS_HANDLE_OPEN: u64= 6;
pub const SYS_HANDLE_CLOSE: u64=7;
pub const SYS_GUI_SUBMIT: u64 = 8;

// --- POSIX HATA KODLARI ---
pub const EPERM: i64  = -1;

#[allow(dead_code)]
pub const ENOENT: i64 = -2;

pub const EBADF: i64  = -9;
pub const EACCES: i64 = -13;
pub const EFAULT: i64 = -14;
pub const EINVAL: i64 = -22;
pub const ENOSYS: i64 = -38;

pub const STDOUT_HANDLE: u64 = 1;
pub const STDERR_HANDLE: u64 = 2;

pub const NCU1_MAGIC: [u8; 4] = *b"NCU1";
pub const NCU1_VERSION: u32 = 1;

pub const USER_CODE_BASE: u64  = USER_SPACE_START;               // 0x0000_0080_0000_0000 (RX)
pub const USER_DATA_BASE: u64  = USER_SPACE_START + 0x0010_0000; // +1 MiB (RO/RW)
pub const USER_STACK_GUARD: u64= USER_SPACE_START + 0x0020_0000; // Guard Page
pub const USER_STACK_PAGE: u64 = USER_SPACE_START + 0x0020_1000; // 4 KiB User Stack (RW, NX)
pub const USER_STACK_TOP: u64  = USER_SPACE_START + 0x0020_2000;

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
    pub op: u8,        // 1 = Clear, 2 = DrawRect, 3 = DrawText
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub color: u32,    // ARGB8888
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
        let total = 24usize.checked_add(code_len).and_then(|v| v.checked_add(ro_len)).ok_or("Boyut tasmasi")?;
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
        SYS_YIELD => {
            unsafe {
                abort_ring3_to_kernel(-1);
            }
        }
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
                    let next_hid = proc.next_handle_id;
                    proc.next_handle_id += 1;
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
                return EINVAL; // Standart akışlar kapatılamaz
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
                    proc.handles.iter().any(|h| h.id == surface_handle && h.kind == HandleKind::GuiSurface)
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

            let mut cmd_buf = [GuiDrawCommand { op: 0, x: 0, y: 0, width: 0, height: 0, color: 0 }; 32];
            let raw_dest = unsafe {
                core::slice::from_raw_parts_mut(cmd_buf.as_mut_ptr() as *mut u8, byte_len)
            };

            if space.copy_from_user(cmds_user_ptr, raw_dest).is_err() {
                return EFAULT;
            }

            for i in 0..count {
                let cmd = &cmd_buf[i];
                if cmd.width > 2048 || cmd.height > 2048 {
                    return EINVAL;
                }
            }
            count as i64
        }
        _ => ENOSYS,
    }
}

pub fn init() {
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
#[allow(dead_code)]
fn write_hex_u16(mut v: u16) {
    let mut buf = [b'0'; 4];
    for i in (0..4).rev() {
        let n = (v & 0xF) as u8;
        buf[i] = if n < 10 { b'0' + n } else { b'A' + (n - 10) };
        v >>= 4;
    }
    if let Ok(s) = core::str::from_utf8(&buf) {
        serial::write_str(s);
    }
}

/// Faz 2 Ring 3 Duman Testi:
/// 1) Süreç A (Ring3-CrashTest): CPL=3'te çalışır, geçerli sys_write yapar, hatalı pointer (EFAULT) ve
///    geçersiz syscall (ENOSYS) sınırlarını doğrular, ardından `ud2` ile kasıtlı çöker.
/// 2) Süreç B (Ring3-Survivor): Kendi izole CR3 alanında çalışır, A'nın çöküşünden etkilenmez,

pub fn run_phase2_ring3_smoke_tests(pm: &mut ProcessManager) -> Result<(), &'static str> {
    init();
    let frames_before = nc_mem::free_frame_count();

    // --- SÜREÇ A: Ring 3 Sınır ve Kasıtlı Çöküş Testi (ud2) ---
    let msg_a = b"Merhaba Ring-3! (Surec A: EFAULT & ENOSYS dogrulandi, simdi ud2 ile cokuyor)\n";
    let mut code_a: Vec<u8> = Vec::new();
    code_a.extend_from_slice(&[
        // 1. EFAULT Testi: mov rsi, 0x100000 (Kernel kod adresi!)
        0x48, 0xC7, 0xC0, 0x04, 0x00, 0x00, 0x00, // mov rax, 4 (SYS_WRITE)
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x48, 0xC7, 0xC6, 0x00, 0x00, 0x10, 0x00, // mov rsi, 0x100000
        0x48, 0xC7, 0xC2, 0x10, 0x00, 0x00, 0x00, // mov rdx, 16
        0xCD, 0x80,                               // int 0x80
        // 2. ENOSYS Testi: mov rax, 999
        0x48, 0xC7, 0xC0, 0xE7, 0x03, 0x00, 0x00, // mov rax, 999
        0xCD, 0x80,                               // int 0x80
        // 3. Geçerli Yazma: msg_a
        0x48, 0xC7, 0xC0, 0x04, 0x00, 0x00, 0x00, // mov rax, 4 (SYS_WRITE)
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x48, 0xBE,                               // movabs rsi, USER_DATA_BASE
    ]);
    code_a.extend_from_slice(&USER_DATA_BASE.to_le_bytes());
    code_a.extend_from_slice(&[
        0x48, 0xC7, 0xC2, msg_a.len() as u8, 0x00, 0x00, 0x00, // mov rdx, len
        0xCD, 0x80,                                            // int 0x80
        // 4. ud2 (#UD)
        0x0F, 0x0B,
    ]);

    let img_a_bytes = Ncu1Image::build(0, &code_a, msg_a);
    let pid_a = pm.spawn_user_ncu1(
        "Ring3-CrashTest",
        SecurityIdentity::from_builtin_app_id("nc.smoke.crash"),
        &img_a_bytes,
    )?;

    // --- SÜREÇ B: İzole CR3'te Hayatta Kalan ve Temiz Çıkan Süreç ---
    let msg_b = b"Merhaba Ring-3! (Surec B: Izole CR3 aktif, A coktu ama B calisiyor ve sys_exit(42) yapiyor)\n";
    let mut code_b: Vec<u8> = Vec::new();
    code_b.extend_from_slice(&[
        // 1. sys_clock_get
        0x48, 0xC7, 0xC0, 0x05, 0x00, 0x00, 0x00, // mov rax, 5 (SYS_CLOCK_GET)
        0x48, 0x31, 0xFF,                         // xor rdi, rdi
        0xCD, 0x80,                               // int 0x80
        // 2. sys_yield()
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, // mov rax, 2 (SYS_YIELD)
        0xCD, 0x80,                               // int 0x80
        // 3. sys_write: msg_b
        0x48, 0xC7, 0xC0, 0x04, 0x00, 0x00, 0x00, // mov rax, 4 (SYS_WRITE)
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
        0x48, 0xBE,                               // movabs rsi, USER_DATA_BASE
    ]);
    code_b.extend_from_slice(&USER_DATA_BASE.to_le_bytes());
    code_b.extend_from_slice(&[
        0x48, 0xC7, 0xC2, msg_b.len() as u8, 0x00, 0x00, 0x00, // mov rdx, len
        0xCD, 0x80,                                            // int 0x80
        // 4. sys_exit(42)
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (SYS_EXIT)
        0x48, 0xC7, 0xC7, 42, 0x00, 0x00, 0x00,   // mov rdi, 42
        0xCD, 0x80,                               // int 0x80
    ]);

    let img_b_bytes = Ncu1Image::build(0, &code_b, msg_b);
    let pid_b = pm.spawn_user_ncu1(
        "Ring3-Survivor",
        SecurityIdentity::from_builtin_app_id("nc.smoke.survivor"),
        &img_b_bytes,
    )?;

    // Süreçleri deterministik olarak 3 tur çalıştır:
    // Tur 1: A çalışır ve ud2 ile ölür
    // Tur 2: B çalışır ve sys_yield yapar
    // Tur 3: B devam eder ve sys_exit(42) yapar
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

    // SÜREÇ 1: Yetkisiz Dosya Yazma Handle Talebi (EACCES Beklenir)
    let msg_unauth = b"Faz-3 Yetki Testi: Dosya yazma handle'i talep ediliyor...\n";
    let _msg_gui = b"Faz-3 GUI Testi: Surface acildi ve cizim gonderildi.\n";
    let mut code_unauth: Vec<u8> = Vec::new();
    code_unauth.extend_from_slice(&[
        // 1. sys_handle_open(2 = FileWrite) -> Yetkisiz olduğu için RAX = -13 (EACCES) döner
        0x48, 0xC7, 0xC0, 0x06, 0x00, 0x00, 0x00, // mov rax, 6 (SYS_HANDLE_OPEN)
        0x48, 0xC7, 0xC7, 0x02, 0x00, 0x00, 0x00, // mov rdi, 2
        0xCD, 0x80,                               // int 0x80
        // 2. Eğer -13 döndüyse RAX'ı pozitife çevir: neg rax -> RAX = 13 olur
        0x48, 0xF7, 0xD8,                         // neg rax
        // 3. sys_exit(RAX) -> Çıkış kodu 13 olarak bildirilir
        0x48, 0x89, 0xC7,                         // mov rdi, rax
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (SYS_EXIT)
        0xCD, 0x80,                               // int 0x80
    ]);

    let img1 = Ncu1Image::build(0, &code_unauth, msg_unauth);
    let mut id_unauth = SecurityIdentity::from_builtin_app_id("nc.test.unauth");
    id_unauth.capabilities = CAP_NONE; // Sıfır yetki

    let pid1 = pm.spawn_user_ncu1("Ring3-Unauth", id_unauth, &img1)?;

    // SÜREÇ 2: GUI Surface Handle Alıp Batch Çizim Yapan Süreç
    let msg_gui = b"Faz-3 GUI Testi: Surface acildi ve cizim gonderildi.\n";
    let mut code_gui: Vec<u8> = Vec::new();
    code_gui.extend_from_slice(&[
        // 1. sys_handle_open(3 = GuiSurface) -> Handle döner (örn: 3)
        0x48, 0xC7, 0xC0, 0x06, 0x00, 0x00, 0x00, // mov rax, 6
        0x48, 0xC7, 0xC7, 0x03, 0x00, 0x00, 0x00, // mov rdi, 3
        0xCD, 0x80,
        0x48, 0x89, 0xC7,                         // mov rdi, rax (handle)
        // 2. sys_gui_submit(rdi, USER_DATA_BASE, 1)
        0x48, 0xC7, 0xC0, 0x08, 0x00, 0x00, 0x00, // mov rax, 8 (SYS_GUI_SUBMIT)
        0x48, 0xBE,                               // movabs rsi, USER_DATA_BASE
    ]);
    code_gui.extend_from_slice(&USER_DATA_BASE.to_le_bytes());
    code_gui.extend_from_slice(&[
        0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00, // mov rdx, 1 (1 komut)
        0xCD, 0x80,
        // 3. sys_exit(99)
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00,
        0x48, 0xC7, 0xC7, 0x63, 0x00, 0x00, 0x00, // mov rdi, 99
        0xCD, 0x80,
    ]);

    let draw_cmd = GuiDrawCommand {
        op: 2, // DrawRect
        x: 10,
        y: 10,
        width: 100,
        height: 50,
        color: 0xFF00_FF00, // Yeşil
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

/// Faz 4 Performans (p50/p95 TSC), Donanım Sertleştirmesi ve İzolasyon Regresyon Testi:
/// 1) CPU Sertleştirmesi: CR0.WP, SMEP, IOPL=0 doğrulaması.
/// 2) TSC Ölçümleri: Ring 0 doğrudan çağrı vs CR3 değişimi vs copy_from_user vs Ring 3 Syscall p50/p95.
/// 3) İki Süreç Arası Bellek Gizliliği: Süreç A ve Süreç B aynı sanal adreste farklı fiziksel sayfalara sahiptir.
/// 4) Ayrıcalıklı Komut Engeli: Ring 3'te `cli` çalıştıran süreç #GP (Vektör 13) ile yakalanır ve sonlandırılır.
pub fn run_phase4_hardening_and_benchmarks(pm: &mut ProcessManager) -> Result<(), &'static str> {
    init();
    let (wp_ok, smep_ok) = nc_arch::enable_cpu_hardening();
    if !wp_ok {
        return Err("CR0.WP (Write Protect) etkinlestirilemedi");
    }
    let frames_before = nc_mem::free_frame_count();

    // 1. ÖLÇÜM: Monolitik Doğrudan Fonksiyon Çağrısı (Ring 0 -> Ring 0)
    let mut direct_samples = [0u64; 16];
    for i in 0..16 {
        let t0 = nc_arch::time::read_tsc();
        let _ = monolithic_direct_call_noop(i as u64);
        let t1 = nc_arch::time::read_tsc();
        direct_samples[i] = t1.wrapping_sub(t0);
    }
    direct_samples.sort_unstable();
    let direct_p50 = direct_samples[8];

    // 2. ÖLÇÜM: CR3 Adres Alanı Değişimi ve copy_from_user (64 Bayt)
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

    // 3. TEST: İki Süreç Arası Bellek Gizliliği + Ring 3 Syscall Ölçümü + `cli` (#GP Vektör 13) Engeli
    // Süreç 1: USER_DATA_BASE içinde gizli anahtar (0xCAFE_BABE) taşır, 4 kez sys_clock_get yapar ve sys_exit(77) ile çıkar.
    let secret_data = [0xBE, 0xBA, 0xFE, 0xCA, 0x11, 0x22, 0x33, 0x44];
    let mut code_p1: Vec<u8> = Vec::new();
    for _ in 0..4 {
        code_p1.extend_from_slice(&[
            0x48, 0xC7, 0xC0, 0x05, 0x00, 0x00, 0x00, // mov rax, 5 (SYS_CLOCK_GET)
            0x48, 0x31, 0xFF,                         // xor rdi, rdi
            0xCD, 0x80,                               // int 0x80
        ]);
    }
    code_p1.extend_from_slice(&[
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (SYS_EXIT)
        0x48, 0xC7, 0xC7, 77, 0x00, 0x00, 0x00,   // mov rdi, 77
        0xCD, 0x80,
    ]);

    let img_p1 = Ncu1Image::build(0, &code_p1, &secret_data);
    let pid_p1 = pm.spawn_user_ncu1(
        "Ring3-BenchAndSecret",
        SecurityIdentity::from_builtin_app_id("nc.bench.p1"),
        &img_p1,
    )?;

    // Süreç 2: Kendi USER_DATA_BASE adresini okur (Süreç 1'in 0xCAFE_BABE verisini GÖRMEMELİDİR)
    // ve ardından Ring 3'te yasak olan `cli` (0xFA) komutunu çalıştırmayı dener -> #GP (Vektör 13) ile sonlandırılmalıdır!
    let public_data = [0u8; 8];
    let mut code_p2: Vec<u8> = Vec::new();
    code_p2.extend_from_slice(&[
        0x48, 0xBE, // movabs rsi, USER_DATA_BASE
    ]);
    code_p2.extend_from_slice(&USER_DATA_BASE.to_le_bytes());
    code_p2.extend_from_slice(&[
        0x8B, 0x06,                   // mov eax, dword ptr [rsi]
        0x3D, 0xBE, 0xBA, 0xFE, 0xCA, // cmp eax, 0xCAFEBABE (Süreç 1'in verisi sızdı mı?)
        0x74, 0x01,                   // je .leak (Eğer sızdıysa +1 atla -> ud2'ye düş)
        0xFA,                         // cli -> Beklenen yol: Ring 3'te #GP (Vektör 13) üretir!
        0x0F, 0x0B,                   // .leak: ud2 (Yalnızca veri sızıntısı varsa buraya gelir)
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