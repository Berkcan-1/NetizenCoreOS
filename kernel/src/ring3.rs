// kernel/src/ring3.rs
//
// Olay-tabanli Ring 3 uygulama calisma zamani (`ring3-apps` ozelligi).
// Sozlesme: crates/nc-abi. Yukleme: crates/nc-elf + ProcessManager::spawn_user_elf.
// Cizim: crates/nc-ui::remote. Dosya: sinirli VFS koprusu (asagida).
#![allow(dead_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use nc_abi::ctx::CTX_SIZE;
use nc_abi::draw;
use nc_abi::vfs as wire;
use nc_abi::event::{EventBlock, OP_CLOSE, OP_DRAW, OP_INIT, OP_UPDATE};
use nc_abi::{
    EACCES, EAGAIN, EBADF, EFAULT, EFBIG, EINVAL, EMFILE, ENOENT, ENOMEM, ENOSYS, EPERM,
    FILE_MAX_BYTES, FILE_MAX_OPEN, FILE_MAX_PATH, FILE_MODE_READ, FILE_MODE_WRITE,
    SYS_APP_DONE, SYS_FILE_CLOSE, SYS_FILE_OPEN, SYS_FILE_READ, SYS_FILE_WRITE, SYS_GUI_BACKDROP, SYS_GUI_DRAW,
    SYS_GUI_PRESENT, SYS_PKG_INSTALL, SYS_TEXT_WIDTH, SYS_VFS_CALL,
};
use nc_arch::idt::{abort_ring3_to_kernel, SyscallFrame};
use nc_gfx::{Canvas, Rect};
use nc_mem::AddressSpace;

use crate::process::{
    active_user_pcb, active_user_pid, ProcessManager, SecurityIdentity, TrustLevel,
    CAP_FS_USER_READ, CAP_FS_USER_WRITE,
};
use crate::syscall::{HandleKind, ProcessHandle};

const EIO: i64 = -5;

// --- Watchdog bütçesi ------------------------------------------------------------------------

static TSC_PER_MS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static BUDGET_MS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(200);

/// `Clock::calibrate()` sonrası bir kez çağrılır (0 = kalibre edilemedi; Ring 3 olayları reddedilir).
pub fn set_tsc_per_ms(value: u64) {
    TSC_PER_MS.store(value, core::sync::atomic::Ordering::Relaxed);
}

/// Tek bir olayın (INIT/UPDATE/DRAW/CLOSE) en fazla ne kadar sürebileceği.
pub fn set_event_budget_ms(value: u64) {
    BUDGET_MS.store(value.max(1), core::sync::atomic::Ordering::Relaxed);
}

pub fn event_budget_cycles() -> Option<u64> {
    let per_ms = TSC_PER_MS.load(core::sync::atomic::Ordering::Relaxed);
    if per_ms == 0 {
        None
    } else {
        Some(per_ms.saturating_mul(BUDGET_MS.load(core::sync::atomic::Ordering::Relaxed)))
    }
}

struct OpenFile {
    handle: u64,
    write: bool,
    path: String,
    data: Vec<u8>,
    pos: usize,
}

pub struct AppRuntime {
    pid: usize,
    event_frame: u64,
    entry: u64,
    surface: u64,
    ret_flags: u32,
    files: Vec<OpenFile>,
    ctx_bytes: Option<[u8; CTX_SIZE]>,
    pending_package: Option<Vec<u8>>,
    write_stage: Option<WriteStage>,
}

struct WriteStage {
    path: String,
    data: Vec<u8>,
    total: u64,
}

impl AppRuntime {
    pub fn new(pid: usize, event_frame: u64, entry: u64, surface: u64) -> Self {
        Self { pid, event_frame, entry, surface, ret_flags: 0, files: Vec::new(), ctx_bytes: None, pending_package: None, write_stage: None }
    }
}

static mut APPS: Vec<AppRuntime> = Vec::new();
static mut GUI: Option<(*mut Canvas<'static>, Rect)> = None;
static mut VFS: Option<nc_fs::VfsManager> = None;

fn apps() -> &'static mut Vec<AppRuntime> {
    unsafe { &mut *core::ptr::addr_of_mut!(APPS) }
}

fn runtime_mut(pid: usize) -> Option<&'static mut AppRuntime> {
    apps().iter_mut().find(|a| a.pid == pid)
}

pub fn register(runtime: AppRuntime) {
    apps().push(runtime);
}

pub fn unregister(pid: usize) {
    apps().retain(|a| a.pid != pid);
}

pub fn runtime_info(pid: usize) -> Option<(u64, u64)> {
    runtime_mut(pid).map(|a| (a.event_frame, a.entry))
}

pub fn begin_event(pid: usize, gui: Option<(*mut Canvas<'static>, Rect)>) {
    if let Some(rt) = runtime_mut(pid) {
        rt.ret_flags = 0;
    }
    unsafe {
        *core::ptr::addr_of_mut!(GUI) = gui;
    }
}

pub fn end_event() {
    unsafe {
        *core::ptr::addr_of_mut!(GUI) = None;
    }
}

pub fn take_ret(pid: usize) -> u32 {
    runtime_mut(pid).map(|a| a.ret_flags).unwrap_or(0)
}

pub fn set_context(pid: usize, bytes: [u8; CTX_SIZE]) {
    if let Some(rt) = runtime_mut(pid) {
        rt.ctx_bytes = Some(bytes);
    }
}

pub fn context_bytes(pid: usize) -> Option<[u8; CTX_SIZE]> {
    runtime_mut(pid).and_then(|rt| rt.ctx_bytes)
}

/// Uygulamanın `SYS_PKG_INSTALL` ile onayladığı (çekirdeğin zaten doğruladığı) paket baytları.
pub fn take_pending_package(pid: usize) -> Option<Vec<u8>> {
    runtime_mut(pid).and_then(|rt| rt.pending_package.take())
}

pub fn is_file_handle(handle: u64) -> bool {
    runtime_mut(active_user_pid()).map_or(false, |rt| rt.files.iter().any(|f| f.handle == handle))
}

/// VFS'i Ring 3'e girmeden ONCE (genis cekirdek yiginiyla) olusturur; syscall icinde degil.
pub fn ensure_vfs() {
    unsafe {
        let slot = &mut *core::ptr::addr_of_mut!(VFS);
        if slot.is_none() {
            *slot = Some(nc_fs::VfsManager::new());
        }
    }
}

fn vfs() -> Option<&'static mut nc_fs::VfsManager> {
    unsafe { (*core::ptr::addr_of_mut!(VFS)).as_mut() }
}

// --- Yol politikasi ------------------------------------------------------------------------

fn path_well_formed(path: &str) -> bool {
    if !path.starts_with('/') || path.len() > FILE_MAX_PATH {
        return false;
    }
    if path.bytes().any(|b| b == 0 || b == b'\\') {
        return false;
    }
    path[1..].split('/').all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

fn read_allowed(path: &str) -> bool {
    path.starts_with("/ram/") || path.starts_with("/data/documents/") || path.starts_with("/data/desktop/")
}

fn write_allowed(path: &str) -> bool {
    path.starts_with("/ram/") || path.starts_with("/data/documents/")
}

// --- Syscall'lar -----------------------------------------------------------------------------

pub fn dispatch(frame: &mut SyscallFrame, space: &AddressSpace) -> i64 {
    match frame.rax {
        SYS_APP_DONE => app_done(frame.rdi),
        SYS_FILE_OPEN => file_open(space, frame.rdi, frame.rsi as usize, frame.rdx),
        SYS_FILE_READ => file_read(space, frame.rdi, frame.rsi, frame.rdx as usize),
        SYS_FILE_WRITE => file_write(space, frame.rdi, frame.rsi, frame.rdx as usize),
        SYS_FILE_CLOSE => file_close(frame.rdi),
        SYS_GUI_DRAW => gui_draw(space, frame.rdi, frame.rsi, frame.rdx as usize),
        SYS_TEXT_WIDTH => text_width_call(space, frame.rdi, frame.rsi as usize, frame.rdx as i64),
        SYS_PKG_INSTALL => pkg_install(space, frame.rdi, frame.rsi as usize),
        SYS_GUI_BACKDROP => gui_surface_copy(space, frame.rdi, frame.rsi, frame.rdx as usize, frame.r10 as usize, false),
        SYS_GUI_PRESENT => gui_surface_copy(space, frame.rdi, frame.rsi, frame.rdx as usize, frame.r10 as usize, true),
        SYS_VFS_CALL => vfs_call(space, frame.rdi, frame.rsi as usize, frame.rdx, frame.r10 as usize),
        _ => ENOSYS,
    }
}

fn app_done(flags: u64) -> i64 {
    unsafe {
        let Some(rt) = runtime_mut(active_user_pid()) else {
            return EPERM;
        };
        rt.ret_flags = flags as u32;
        abort_ring3_to_kernel(0)
    }
}

fn file_open(space: &AddressSpace, path_ptr: u64, path_len: usize, mode: u64) -> i64 {
    if mode != FILE_MODE_READ && mode != FILE_MODE_WRITE {
        return EINVAL;
    }
    if path_len == 0 || path_len > FILE_MAX_PATH {
        return EINVAL;
    }
    let mut buf = [0u8; FILE_MAX_PATH];
    if space.copy_from_user(path_ptr, &mut buf[..path_len]).is_err() {
        return EFAULT;
    }
    let Ok(path) = core::str::from_utf8(&buf[..path_len]) else {
        return EINVAL;
    };
    if !path_well_formed(path) {
        return EINVAL;
    }
    let write = mode == FILE_MODE_WRITE;
    let required = if write { CAP_FS_USER_WRITE } else { CAP_FS_USER_READ };
    let allowed = if write { write_allowed(path) } else { read_allowed(path) };

    unsafe {
        let Some(pcb) = active_user_pcb() else {
            return EPERM;
        };
        if (pcb.identity.capabilities & required) != required || !allowed {
            return EACCES;
        }
        let Some(rt) = runtime_mut(pcb.pid) else {
            return EPERM;
        };
        if rt.files.len() >= FILE_MAX_OPEN {
            return EMFILE;
        }

        let mut data: Vec<u8> = Vec::new();
        if !write {
            let Some(manager) = vfs() else {
                return EIO;
            };
            match manager.read_file_path(path) {
                Ok(bytes) => {
                    if bytes.len() > FILE_MAX_BYTES {
                        return EFBIG;
                    }
                    data = bytes;
                }
                Err(_) => return ENOENT,
            }
        }

        if rt.files.try_reserve(1).is_err() || pcb.handles.try_reserve(1).is_err() {
            return ENOMEM;
        }
        let id = pcb.next_handle_id;
        let Some(next_id) = id.checked_add(1) else {
            return EINVAL;
        };
        let mut owned_path = String::new();
        if owned_path.try_reserve(path.len()).is_err() {
            return ENOMEM;
        }
        owned_path.push_str(path);

        pcb.next_handle_id = next_id;
        pcb.handles.push(ProcessHandle {
            id,
            kind: if write { HandleKind::FileWrite } else { HandleKind::FileRead },
            required_cap: required,
        });
        rt.files.push(OpenFile { handle: id, write, path: owned_path, data, pos: 0 });
        id as i64
    }
}

fn file_read(space: &AddressSpace, handle: u64, buf_ptr: u64, len: usize) -> i64 {
    if len == 0 {
        return 0;
    }
    unsafe {
        let Some(rt) = runtime_mut(active_user_pid()) else {
            return EPERM;
        };
        let Some(file) = rt.files.iter_mut().find(|f| f.handle == handle && !f.write) else {
            return EBADF;
        };
        let count = len.min(64 * 1024).min(file.data.len() - file.pos);
        if count == 0 {
            return 0;
        }
        if space.copy_to_user(buf_ptr, &file.data[file.pos..file.pos + count]).is_err() {
            return EFAULT;
        }
        file.pos += count;
        count as i64
    }
}

fn file_write(space: &AddressSpace, handle: u64, buf_ptr: u64, len: usize) -> i64 {
    if len == 0 {
        return 0;
    }
    if len > 64 * 1024 {
        return EINVAL;
    }
    unsafe {
        let Some(rt) = runtime_mut(active_user_pid()) else {
            return EPERM;
        };
        let Some(file) = rt.files.iter_mut().find(|f| f.handle == handle && f.write) else {
            return EBADF;
        };
        if file.data.len().checked_add(len).map_or(true, |total| total > FILE_MAX_BYTES) {
            return EFBIG;
        }
        let mut chunk: Vec<u8> = Vec::new();
        if chunk.try_reserve_exact(len).is_err() {
            return ENOMEM;
        }
        chunk.resize(len, 0);
        if space.copy_from_user(buf_ptr, &mut chunk).is_err() {
            return EFAULT;
        }
        if file.data.try_reserve(len).is_err() {
            return ENOMEM;
        }
        file.data.extend_from_slice(&chunk);
        len as i64
    }
}

fn file_close(handle: u64) -> i64 {
    unsafe {
        let Some(rt) = runtime_mut(active_user_pid()) else {
            return EPERM;
        };
        let Some(position) = rt.files.iter().position(|f| f.handle == handle) else {
            return EBADF;
        };
        let file = rt.files.remove(position);
        if let Some(pcb) = active_user_pcb() {
            if let Some(slot) = pcb.handles.iter().position(|h| h.id == handle) {
                pcb.handles.remove(slot);
            }
        }
        if !file.write {
            return 0;
        }
        let Some(manager) = vfs() else {
            return EIO;
        };
        let previous = nc_fs::get_vfs_caller();
        nc_fs::set_vfs_caller(nc_fs::CALLER_USER_APP);
        let result = manager.write_file_path(&file.path, &file.data);
        nc_fs::set_vfs_caller(previous);
        match result {
            Ok(()) => 0,
            Err(nc_fs::FsError::PermissionDenied) | Err(nc_fs::FsError::ReadOnly) => EACCES,
            Err(_) => EIO,
        }
    }
}

fn gui_draw(space: &AddressSpace, handle: u64, ptr: u64, len: usize) -> i64 {
    if len == 0 || len > draw::MAX_STREAM_BYTES {
        return EINVAL;
    }
    let has_surface = unsafe {
        active_user_pcb().map_or(false, |p| {
            p.handles.iter().any(|h| h.id == handle && h.kind == HandleKind::GuiSurface)
        })
    };
    if !has_surface {
        return EBADF;
    }
    let target = unsafe { *core::ptr::addr_of!(GUI) };
    let Some((canvas_ptr, area)) = target else {
        return EAGAIN;
    };
    let mut data: Vec<u8> = Vec::new();
    if data.try_reserve_exact(len).is_err() {
        return ENOMEM;
    }
    data.resize(len, 0);
    if space.copy_from_user(ptr, &mut data).is_err() {
        return EFAULT;
    }
    let canvas = unsafe { &mut *canvas_ptr };
    match nc_ui::remote::replay(canvas, area, &data) {
        Ok(records) => records as i64,
        Err(_) => EINVAL,
    }
}

fn text_width_call(space: &AddressSpace, ptr: u64, len: usize, cap_height: i64) -> i64 {
    if len > draw::MAX_TEXT_BYTES {
        return EINVAL;
    }
    let mut buf = [0u8; draw::MAX_TEXT_BYTES];
    if space.copy_from_user(ptr, &mut buf[..len]).is_err() {
        return EFAULT;
    }
    let Ok(text) = core::str::from_utf8(&buf[..len]) else {
        return EINVAL;
    };
    let cap = cap_height.clamp(1, draw::MAX_CAP_HEIGHT as i64) as i32;
    nc_ui::remote::measure_text(text, cap) as i64
}


// --- Piksel yüzeyi (ana-bilgisayar çalışma zamanı) ---------------------------------------------

fn bytes_of(row: &[u32]) -> &[u8] {
    unsafe { core::slice::from_raw_parts(row.as_ptr() as *const u8, row.len() * 4) }
}

fn bytes_of_mut(row: &mut [u32]) -> &mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(row.as_mut_ptr() as *mut u8, row.len() * 4) }
}

/// `present = false`: masaüstü (pencere alanı) -> kullanıcı tamponu; `true`: kullanıcı tamponu -> pencere (kırparak).
fn gui_surface_copy(space: &AddressSpace, handle: u64, ptr: u64, w: usize, h: usize, present: bool) -> i64 {
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return EINVAL;
    }
    let has_surface = unsafe {
        active_user_pcb().map_or(false, |p| p.handles.iter().any(|x| x.id == handle && x.kind == HandleKind::GuiSurface))
    };
    if !has_surface {
        return EBADF;
    }
    let Some((canvas_ptr, area)) = (unsafe { *core::ptr::addr_of!(GUI) }) else {
        return EAGAIN;
    };
    let canvas = unsafe { &mut *canvas_ptr };
    let (cw, ch) = (canvas.width() as i32, canvas.height() as i32);
    let clip = canvas.clip_rect();
    // Kopyalanacak sütun aralığı (pencere ∩ tuval ∩ tampon); present'te ayrıca tuval klibi.
    let mut left = area.x.max(0);
    let mut right = (area.x + area.width).min(cw).min(area.x + w as i32);
    let (mut top, mut bottom) = (area.y.max(0), (area.y + area.height).min(ch).min(area.y + h as i32));
    if present {
        left = left.max(clip.x);
        right = right.min(clip.x + clip.width);
        top = top.max(clip.y);
        bottom = bottom.min(clip.y + clip.height);
    }
    let mut row: Vec<u32> = Vec::new();
    if row.try_reserve_exact(w).is_err() {
        return ENOMEM;
    }
    row.resize(w, 0);
    for y in 0..h as i32 {
        let screen_y = area.y + y;
        let user_row = ptr + (y as u64) * (w as u64) * 4;
        let inside_y = screen_y >= top && screen_y < bottom;
        if present {
            if !inside_y || left >= right {
                continue;
            }
            if space.copy_from_user(user_row, bytes_of_mut(&mut row)).is_err() {
                return EFAULT;
            }
            let (x0, x1) = ((left - area.x) as usize, (right - area.x) as usize);
            let base = screen_y as usize * cw as usize + left as usize;
            canvas.pixels_mut()[base..base + (x1 - x0)].copy_from_slice(&row[x0..x1]);
        } else {
            row.iter_mut().for_each(|p| *p = 0);
            if inside_y && left < right {
                let (x0, x1) = ((left - area.x) as usize, (right - area.x) as usize);
                let base = screen_y as usize * cw as usize + left as usize;
                row[x0..x1].copy_from_slice(&canvas.pixels()[base..base + (x1 - x0)]);
            }
            if space.copy_to_user(user_row, bytes_of(&row)).is_err() {
                return EFAULT;
            }
        }
    }
    0
}

// --- Paket kurma: çekirdek paketi KENDİSİ doğrular -------------------------------------------

fn identity_is_files() -> bool {
    unsafe {
        active_user_pcb().map_or(false, |p| p.identity.trust == TrustLevel::BuiltinSystemApp && p.identity.app_id == "nc.files")
    }
}

fn pkg_install(space: &AddressSpace, path_ptr: u64, path_len: usize) -> i64 {
    if !identity_is_files() {
        return EACCES;
    }
    if path_len == 0 || path_len > FILE_MAX_PATH {
        return EINVAL;
    }
    let mut buf = [0u8; FILE_MAX_PATH];
    if space.copy_from_user(path_ptr, &mut buf[..path_len]).is_err() {
        return EFAULT;
    }
    let Ok(path) = core::str::from_utf8(&buf[..path_len]) else {
        return EINVAL;
    };
    if !path_well_formed(path) || !path.starts_with("/data/app/") || !path.ends_with(".ntzc") {
        return EINVAL;
    }
    let Some(manager) = vfs() else {
        return EIO;
    };
    let previous = nc_fs::get_vfs_caller();
    nc_fs::set_vfs_caller(nc_fs::CALLER_FILES);
    let bytes = manager.read_file_path(path);
    nc_fs::set_vfs_caller(previous);
    let Ok(bytes) = bytes else {
        return ENOENT;
    };
    // Ring 3 Dosyalar'ın "imzalı" iddiasına güvenilmez: imza + abi + ELF burada yeniden doğrulanır.
    match nc_pkg::NtzcPackage::parse(&bytes) {
        Some(pkg) if pkg.runnable() => {}
        _ => return EINVAL,
    }
    unsafe {
        let Some(rt) = runtime_mut(active_user_pid()) else {
            return EPERM;
        };
        rt.pending_package = Some(bytes);
    }
    0
}

// --- VFS hizmeti (Dosyalar) ----------------------------------------------------------------

fn fs_status(error: &nc_fs::FsError) -> i32 {
    use nc_fs::FsError as E;
    match error {
        E::DiskError => wire::ERR_DISK,
        E::InvalidSuperblock => wire::ERR_INVALID_SUPERBLOCK,
        E::NotFound => wire::ERR_NOT_FOUND,
        E::NotADirectory => wire::ERR_NOT_A_DIRECTORY,
        E::NotAFile => wire::ERR_NOT_A_FILE,
        E::Corrupted => wire::ERR_CORRUPTED,
        E::AlreadyExists => wire::ERR_ALREADY_EXISTS,
        E::ReadOnly => wire::ERR_READ_ONLY,
        E::PermissionDenied => wire::ERR_PERMISSION_DENIED,
        E::InvalidPath => wire::ERR_INVALID_PATH,
        E::StorageFull => wire::ERR_STORAGE_FULL,
    }
}

const VFS_RESPONSE_MAX: usize = 128 * 1024;

fn vfs_call(space: &AddressSpace, req_ptr: u64, req_len: usize, resp_ptr: u64, resp_cap: usize) -> i64 {
    if !identity_is_files() {
        return EACCES;
    }
    if req_len < wire::REQ_HEADER || req_len > wire::REQ_HEADER + wire::MAX_PATH + wire::MAX_CHUNK {
        return EINVAL;
    }
    if resp_cap < wire::RESP_HEADER || resp_cap > VFS_RESPONSE_MAX {
        return EINVAL;
    }
    let mut raw: Vec<u8> = Vec::new();
    if raw.try_reserve_exact(req_len).is_err() {
        return ENOMEM;
    }
    raw.resize(req_len, 0);
    if space.copy_from_user(req_ptr, &mut raw).is_err() {
        return EFAULT;
    }
    let Ok(request) = wire::parse_request(&raw) else {
        return EINVAL;
    };
    let Some(manager) = vfs() else {
        return EIO;
    };

    let previous = nc_fs::get_vfs_caller();
    nc_fs::set_vfs_caller(nc_fs::CALLER_FILES);
    let outcome = vfs_execute(manager, &request, resp_cap);
    nc_fs::set_vfs_caller(previous);

    let (status, payload) = match outcome {
        Ok(payload) => (0, payload),
        Err(error) => (fs_status(&error), Vec::new()),
    };
    let mut response: Vec<u8> = Vec::new();
    if response.try_reserve_exact(wire::RESP_HEADER + payload.len()).is_err() {
        return ENOMEM;
    }
    response.resize(wire::RESP_HEADER, 0);
    if wire::encode_response_header(&mut response, status, payload.len()).is_none() {
        return EINVAL;
    }
    response.extend_from_slice(&payload);
    if response.len() > resp_cap {
        return EFBIG;
    }
    if space.copy_to_user(resp_ptr, &response).is_err() {
        return EFAULT;
    }
    response.len() as i64
}

fn vfs_execute(manager: &mut nc_fs::VfsManager, req: &wire::Request<'_>, resp_cap: usize) -> Result<Vec<u8>, nc_fs::FsError> {
    use nc_fs::FsError;
    let mut out: Vec<u8> = Vec::new();
    match req.op {
        wire::OP_LIST => {
            let entries = manager.list_dir_path(req.path)?;
            let budget = resp_cap.saturating_sub(wire::RESP_HEADER + 8);
            out.resize(8, 0);
            let mut count = 0u32;
            let mut scratch = [0u8; 24 + 264];
            for entry in entries.iter().take(wire::MAX_LIST_ENTRIES) {
                if entry.name.len() > 255 {
                    continue;
                }
                let kind = match entry.file_type {
                    nc_fs::FileType::RegularFile => wire::KIND_FILE,
                    nc_fs::FileType::Directory => wire::KIND_DIR,
                    nc_fs::FileType::Unknown => wire::KIND_UNKNOWN,
                };
                let Some(n) = wire::encode_dir_entry(&mut scratch, kind, entry.size, entry.id, &entry.name) else {
                    continue;
                };
                if out.len() - 8 + n > budget {
                    break;
                }
                out.extend_from_slice(&scratch[..n]);
                count += 1;
            }
            out[0..4].copy_from_slice(&count.to_le_bytes());
        }
        wire::OP_STAT => {
            let stat = manager.file_stat_path(req.path)?;
            out.extend_from_slice(&stat.size.to_le_bytes());
            out.extend_from_slice(&[stat.is_dir as u8, stat.readonly as u8, 0, 0, 0, 0, 0, 0]);
        }
        wire::OP_READ_RANGE => {
            let wanted = (req.arg1 as usize).min(wire::MAX_CHUNK).min(resp_cap.saturating_sub(wire::RESP_HEADER));
            out.try_reserve_exact(wanted).map_err(|_| FsError::StorageFull)?;
            out.resize(wanted, 0);
            let read = manager.read_file_range_path(req.path, req.arg0, &mut out)?;
            out.truncate(read);
        }
        wire::OP_WRITE => {
            if req.arg1 > wire::MAX_WRITE_TOTAL {
                return Err(FsError::StorageFull);
            }
            let rt = runtime_mut(active_user_pid()).ok_or(FsError::PermissionDenied)?;
            if req.arg0 == 0 {
                rt.write_stage = Some(WriteStage { path: String::from(req.path), data: Vec::new(), total: req.arg1 });
            }
            let stage = rt.write_stage.as_mut().ok_or(FsError::InvalidPath)?;
            if stage.path != req.path || stage.total != req.arg1 || stage.data.len() as u64 != req.arg0 {
                rt.write_stage = None;
                return Err(FsError::InvalidPath);
            }
            stage.data.try_reserve(req.data.len()).map_err(|_| FsError::StorageFull)?;
            stage.data.extend_from_slice(req.data);
            if stage.data.len() as u64 == stage.total {
                let done = rt.write_stage.take();
                if let Some(done) = done {
                    manager.write_file_path(&done.path, &done.data)?;
                }
            } else if stage.data.len() as u64 > stage.total {
                rt.write_stage = None;
                return Err(FsError::InvalidPath);
            }
        }
        wire::OP_MKDIR => manager.create_dir_path(req.path)?,
        wire::OP_REMOVE => manager.remove_path(req.path)?,
        wire::OP_USB_RESCAN => out.push(manager.rescan_usb() as u8),
        wire::OP_USB_ONLINE => out.push(manager.is_usb_online() as u8),
        wire::OP_USB_LABEL => out.extend_from_slice(manager.usb_label().as_bytes()),
        _ => return Err(FsError::InvalidPath),
    }
    Ok(out)
}

// --- Faz 6: Ring 3 ELF uygulama yolu uctan uca testi --------------------------------------

static FAZ6_APP: &[u8] = include_bytes!("../faz6/faz6_app.elf");

fn faz6_identity() -> SecurityIdentity {
    SecurityIdentity {
        app_id: String::from("faz6.test"),
        uid: 1001,
        trust: TrustLevel::VerifiedApp,
        capabilities: CAP_FS_USER_READ | CAP_FS_USER_WRITE,
    }
}

fn faz6_event(op: u32, flags: u32, seq: u64, area: Rect) -> EventBlock {
    EventBlock {
        op,
        flags,
        surface: 3,
        area_x: area.x,
        area_y: area.y,
        area_w: area.width,
        area_h: area.height,
        unit: 16,
        accent: 0x0030_80F0,
        heap_base: nc_abi::USER_HEAP_BASE,
        heap_size: nc_abi::APP_HEAP_BYTES,
        seq,
        ..Default::default()
    }
}

// Test uygulamasi her olayda sayaci artirir ve dondurulen degerin 16..24. bitlerine koyar: durum
// uygulamanin BELLEGINDE kalir (her olayda yazmaclar sifirlanir, giris noktasi bastan calisir).
fn expect_ret(ret: u32, counter: u32, mask: u32, want: u32, what: &'static str) -> Result<(), &'static str> {
    if ret & 0x8000_0000 != 0 || ((ret >> 16) & 0xFF) != counter || (ret & mask) != want {
        return Err(what);
    }
    Ok(())
}

pub fn run_phase6_ring3_app_test(pm: &mut ProcessManager) -> Result<(), &'static str> {
    crate::syscall::init();
    let frames_before = nc_mem::free_frame_count();

    if pm.spawn_user_elf("bad", faz6_identity(), b"bu bir ELF degil, rastgele baytlar").is_ok() {
        return Err("rastgele baytlar ELF olarak kabul edildi");
    }
    if pm.spawn_user_elf("bad", faz6_identity(), &FAZ6_APP[..100]).is_ok() {
        return Err("kesik ELF kabul edildi");
    }
    let mut wx = FAZ6_APP.to_vec();
    wx[68] = 7;
    if pm.spawn_user_elf("bad", faz6_identity(), &wx).is_ok() {
        return Err("hem yazilabilir hem yurutulebilir segmentli ELF kabul edildi");
    }
    if nc_mem::free_frame_count() != frames_before {
        return Err("reddedilen ELF imajlari bellek sizdirdi");
    }

    let pid = pm.spawn_user_elf("faz6", faz6_identity(), FAZ6_APP)?;
    let area = Rect::new(16, 16, 96, 64);

    let ret = pm.run_app_event(pid, &faz6_event(OP_INIT, 0, 1, area), None)?;
    expect_ret(ret, 1, 0xFFFF, 0x0701, "INIT: EAGAIN/metin genisligi/dosya gidis-donus denetimi basarisiz")?;

    let stored = match vfs() {
        Some(manager) => manager.read_file_path("/ram/faz6.txt").map_err(|_| "VFS'te uygulamanin yazdigi dosya yok")?,
        None => return Err("VFS olusturulamadi"),
    };
    if stored.as_slice() != b"NetizenCore Ring3" {
        return Err("VFS'teki dosya icerigi uygulamanin yazdigiyla ayni degil");
    }
    // /ram koprusu: BASKA bir VfsManager ornegi (Ring 0 uygulamalarinin kullandigi turden) ayni dosyayi gormeli.
    {
        let mut other = nc_fs::VfsManager::new();
        let seen = other
            .read_file_path("/ram/faz6.txt")
            .map_err(|_| "/ram kopru: Ring 3'un yazdigi dosya baska VfsManager ornegince gorulmuyor")?;
        if seen.as_slice() != b"NetizenCore Ring3" {
            return Err("/ram kopru: baska VfsManager ornegi farkli icerik goruyor");
        }
    }

    let ret = pm.run_app_event(pid, &faz6_event(OP_UPDATE, 0, 2, area), None)?;
    expect_ret(ret, 2, 0xFFFF, 0x0001, "UPDATE: durum olaylar arasinda korunmadi veya RET_REDRAW yok")?;

    const BG: u32 = 0x0011_2233;
    const BLUE: u32 = 0x0000_00FF;
    const RED: u32 = 0x00FF_0000;
    const WHITE: u32 = 0x00FF_FFFF;
    let mut pixels = vec![BG; 128 * 96];
    {
        let mut canvas = Canvas::new(&mut pixels, 128, 96).ok_or("Faz 6 canvas olusturulamadi")?;
        let ret = pm.run_app_event(pid, &faz6_event(OP_DRAW, 0, 3, area), Some((&mut canvas, area)))?;
        expect_ret(ret, 3, 0xFFFF, 0x0001, "DRAW: SYS_GUI_DRAW 6 kayit kabul etmedi")?;
    }
    let at = |x: usize, y: usize| pixels[y * 128 + x];
    if at(5, 5) != BG || at(112, 30) != BG || at(20, 80) != BG {
        return Err("DRAW: uygulama penceresinin disina cizdi (klip ihlali)");
    }
    if at(17, 17) != BLUE || at(111, 30) != BLUE {
        return Err("DRAW: arka plan dolgusu pencere icinde beklenen yerde degil");
    }
    if at(16 + 40, 16 + 20) != RED {
        return Err("DRAW: kirmizi dikdortgen pencere-gorece konumda degil");
    }
    if at(16 + 15, 16 + 40) != WHITE {
        return Err("DRAW: panel icerigi beklenen renkte degil");
    }
    if at(16 + 50, 16 + 45) == BLUE {
        return Err("DRAW: nokta cizilmedi");
    }
    if at(16 + 66, 16 + 44) != 0x0030_80F0 {
        return Err("DRAW: tema dugmesi (OP_BUTTON, accent) cizilmedi");
    }
    let mut centered_left = 0;
    for y in 16 + 54..16 + 64 {
        for x in 16 + 36..16 + 47 {
            if at(x, y) != BLUE {
                centered_left += 1;
            }
        }
    }
    if centered_left == 0 {
        return Err("DRAW: ortali metin (align=1) x'in soluna tasmadi; sola hizali cizilmis olabilir");
    }
    let mut text_pixels = 0;
    for y in 16 + 2..16 + 16 {
        for x in 16 + 4..16 + 30 {
            let p = at(x, y);
            if p != BLUE && p != RED {
                text_pixels += 1;
            }
        }
    }
    if text_pixels < 4 {
        return Err("DRAW: metin cizilmedi");
    }

    let mut scratch = vec![BG; 128 * 96];
    {
        let mut canvas = Canvas::new(&mut scratch, 128, 96).ok_or("Faz 6 canvas olusturulamadi")?;
        let ret = pm.run_app_event(pid, &faz6_event(OP_DRAW, 3, 4, area), Some((&mut canvas, area)))?;
        expect_ret(ret, 4, 0xFFFF, 0xF001, "kotu cizim akislari (EINVAL/EINVAL/EFAULT/EBADF) beklendigi gibi reddedilmedi")?;
    }
    if scratch.iter().any(|&p| p != BG) {
        return Err("reddedilen cizim akislari yine de piksel degistirdi");
    }

    let ret = pm.run_app_event(pid, &faz6_event(OP_UPDATE, 4, 5, area), None)?;
    expect_ret(ret, 5, 0xFFFF, 0x0F01, "kotu dosya cagrilari (EACCES/EINVAL/EINVAL/EBADF) beklendigi gibi reddedilmedi")?;

    let ret = pm.run_app_event(pid, &faz6_event(OP_CLOSE, 0, 6, area), None)?;
    expect_ret(ret, 6, 0xFFFF, 0x0002, "CLOSE: RET_CLOSE donmedi")?;

    for (selector, expected_vector, what) in [
        (1u32, 6u8, "ud2 (#UD) cokmesi uygun sekilde izole edilmedi"),
        (2u32, 14u8, "salt-okunur olay sayfasina yazma (#PF) uygun sekilde izole edilmedi"),
    ] {
        let victim = pm.spawn_user_elf("faz6-fault", faz6_identity(), FAZ6_APP)?;
        if pm.run_app_event(victim, &faz6_event(OP_UPDATE, selector, 1, area), None).is_ok() {
            return Err(what);
        }
        if pm.process_fault_vec(victim) != Some(expected_vector) {
            return Err(what);
        }
        if pm.run_app_event(victim, &faz6_event(OP_UPDATE, 0, 2, area), None).is_ok() {
            return Err("sonlanmis uygulamaya olay teslim edilebildi");
        }
        if !pm.kill_app(victim) {
            return Err("cokmus uygulama temizlenemedi");
        }
    }

    // Faz 6b: watchdog. Sonsuz dongu ve sistem cagrisi yagdirma kisa surede sonlandirilmali;
    // cekirdek ve zamanlayici kesmesi sagliklı kalmali. (Burada takilirsa watchdog calismiyor demektir.)
    nc_arch::serial::write_str("[Faz 6b] watchdog testi basliyor (sonsuz dongu ~30 ms icinde sonlandirilmali)...\n");
    set_event_budget_ms(30);
    let ticks_before = nc_arch::idt::IRQ_TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    for (selector, what) in [
        (5u32, "watchdog sonsuz donguyu sonlandirmadi"),
        (6u32, "watchdog sistem cagrisi yagdiran uygulamayi sonlandirmadi"),
    ] {
        let victim = match pm.spawn_user_elf("faz6-hang", faz6_identity(), FAZ6_APP) {
            Ok(v) => v,
            Err(e) => {
                set_event_budget_ms(200);
                return Err(e);
            }
        };
        let outcome = pm.run_app_event(victim, &faz6_event(OP_UPDATE, selector, 1, area), None);
        if outcome != Err(crate::process::APP_WATCHDOG_ERROR) {
            set_event_budget_ms(200);
            return Err(what);
        }
        if !pm.kill_app(victim) {
            set_event_budget_ms(200);
            return Err("watchdog'un sonlandirdigi uygulama temizlenemedi");
        }
    }
    set_event_budget_ms(200);
    if nc_arch::idt::watchdog_expired() || nc_arch::idt::WATCHDOG_DEADLINE_TSC.load(core::sync::atomic::Ordering::SeqCst) != 0 {
        return Err("watchdog test sonrasi kurulu kaldi");
    }
    let wait_until = nc_arch::time::read_tsc().saturating_add(
        TSC_PER_MS.load(core::sync::atomic::Ordering::Relaxed).saturating_mul(20),
    );
    while nc_arch::time::read_tsc() < wait_until {
        core::hint::spin_loop();
    }
    if nc_arch::idt::IRQ_TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) <= ticks_before {
        return Err("watchdog sonrasi zamanlayici kesmesi durdu (EOI/IF bozuldu)");
    }
    nc_arch::serial::write_str("[Faz 6b] watchdog iki senaryoda da sonlandirdi; zamanlayici saglikli.\n");

    if !pm.kill_app(pid) {
        return Err("uygulama sonlandirilamadi");
    }
    if let Some(manager) = vfs() {
        let _ = manager.remove_path("/ram/faz6.txt");
    }
    if nc_mem::free_frame_count() != frames_before {
        return Err("Ring 3 uygulama yasam dongusu fiziksel cerceve sizdirdi");
    }
    Ok(())
}
