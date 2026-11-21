use crate::{host, storage, task};
use nc_abi::{
    vfs as vfs_wire, AbiInfo, ARCH_AARCH64, FEAT_APP_LAUNCH, FEAT_HW_CSPRNG, FEAT_SMP,
    FEAT_SPARK_SEC, FEAT_SYS_CALL, FEAT_VFS_CALL, SYS_ABI_INFO, SYS_APP_DONE, SYS_APP_LAUNCH,
    SYS_CLOCK_GET, SYS_EXIT, SYS_GUI_DRAW, SYS_PKG_INSTALL, SYS_SLEEP, SYS_SYS_CALL,
    SYS_TEXT_WIDTH, SYS_VFS_CALL, SYS_WRITE, SYS_YIELD,
};
use nc_arch::exceptions::TrapFrame;

const EBADF: i64 = -9;
const EACCES: i64 = -13;
const EFAULT: i64 = -14;
const EINVAL: i64 = -22;
const ENOSYS: i64 = -38;
const MAX_WRITE: u64 = 4096;
const MAX_VFS_RESP: usize = 131_072;

fn write_bytes(bytes: &[u8]) {
    for &byte in bytes {
        if byte == b'\n' {
            nc_arch::serial::write_byte(b'\r');
        }
        nc_arch::serial::write_byte(byte);
    }
}

fn sys_write(handle: u64, pointer: u64, length: u64) -> i64 {
    if handle != 1 && handle != 2 {
        return EBADF;
    }
    if length > MAX_WRITE {
        return EINVAL;
    }
    let mut buffer = [0u8; 256];
    let mut done = 0u64;
    while done < length {
        let chunk = (length - done).min(buffer.len() as u64) as usize;
        let ok = task::with_current_space(|space| space.copy_from_user(pointer + done, &mut buffer[..chunk])).unwrap_or(false);
        if !ok {
            return EFAULT;
        }
        write_bytes(&buffer[..chunk]);
        done += chunk as u64;
    }
    length as i64
}

fn sys_clock_get(kind: u64) -> i64 {
    match kind {
        0 => task::ticks() as i64,
        1 => nc_arch::rtc::unix_time_now().unwrap_or(0) as i64,
        _ => EINVAL,
    }
}

fn sys_vfs_call(req_ptr: u64, req_len: u64, resp_ptr: u64, resp_cap: u64) -> i64 {
    let max_req = vfs_wire::REQ_HEADER + vfs_wire::MAX_PATH + vfs_wire::MAX_CHUNK;
    if req_len < vfs_wire::REQ_HEADER as u64 || req_len as usize > max_req || resp_cap < vfs_wire::RESP_HEADER as u64 {
        return EINVAL;
    }
    let cap = (resp_cap as usize).min(MAX_VFS_RESP);
    let mut req_buf = alloc::vec![0u8; req_len as usize];
    let ok = task::with_current_space(|space| space.copy_from_user(req_ptr, &mut req_buf)).unwrap_or(false);
    if !ok {
        return EFAULT;
    }
    let mut resp_buf = alloc::vec![0u8; cap];
    let mut id_buf = [0u8; 32];
    let id_len = task::current_app_id(&mut id_buf);
    let app_id = core::str::from_utf8(&id_buf[..id_len]).unwrap_or("");
    let caps = task::current_caps();
    let written = storage::vfs_call(app_id, caps, &req_buf, &mut resp_buf);
    if written < 0 {
        return written;
    }
    let n = written as usize;
    let copied = task::with_current_space(|space| space.copy_to_user(resp_ptr, &resp_buf[..n])).unwrap_or(false);
    if !copied {
        return EFAULT;
    }
    written
}

fn sys_pkg_install(path_ptr: u64, path_len: u64) -> i64 {
    let mut id_buf = [0u8; 32];
    let id_len = task::current_app_id(&mut id_buf);
    let caller_id = core::str::from_utf8(&id_buf[..id_len]).unwrap_or("");
    if nc_spark_sec::ds_authorize(caller_id, nc_caps::PACKAGE_INSTALL as u64)
        != nc_spark_sec::AUTH_ALLOW
    {
        return EACCES;
    }
    if path_len == 0 || path_len as usize > nc_abi::FILE_MAX_PATH {
        return EINVAL;
    }
    let mut path_buf = [0u8; nc_abi::FILE_MAX_PATH];
    let ok = task::with_current_space(|space| space.copy_from_user(path_ptr, &mut path_buf[..path_len as usize])).unwrap_or(false);
    if !ok {
        return EFAULT;
    }
    let Ok(path) = core::str::from_utf8(&path_buf[..path_len as usize]) else {
        return EINVAL;
    };
    storage::install_package_from_path(task::current_caps(), path)
}

fn sys_abi_info(out_ptr: u64, out_len: u64) -> i64 {
    if (out_len as usize) < AbiInfo::SIZE {
        return EINVAL;
    }
    let mut features = FEAT_SPARK_SEC | FEAT_VFS_CALL | FEAT_APP_LAUNCH | FEAT_SYS_CALL;
    if nc_arch::rand::rndr_supported() {
        features |= FEAT_HW_CSPRNG;
    }
    if crate::smp::online_count() > 1 {
        features |= FEAT_SMP;
    }
    let info = AbiInfo::new(ARCH_AARCH64, SYS_ABI_INFO as u32, features);
    let raw = info.to_le_bytes();
    let copied = task::with_current_space(|space| space.copy_to_user(out_ptr, &raw)).unwrap_or(false);
    if !copied {
        return EFAULT;
    }
    AbiInfo::SIZE as i64
}

pub fn dispatch(frame: &mut TrapFrame) {
    let number = frame.regs[8];
    let a0 = frame.regs[0];
    let a1 = frame.regs[1];
    let a2 = frame.regs[2];
    let a3 = frame.regs[3];
    match number {
        SYS_EXIT => task::exit_current(frame, a0 as i64),
        SYS_YIELD => {
            frame.regs[0] = 0;
            task::yield_now(frame);
        }
        SYS_SLEEP => {
            frame.regs[0] = 0;
            task::sleep_current(frame, a0);
        }
        SYS_WRITE => frame.regs[0] = sys_write(a0, a1, a2) as u64,
        SYS_CLOCK_GET => frame.regs[0] = sys_clock_get(a0) as u64,
        SYS_APP_DONE => task::app_done(frame, a0 as u32),
        SYS_GUI_DRAW => frame.regs[0] = host::gui_draw(a0, a1, a2) as u64,
        SYS_TEXT_WIDTH => frame.regs[0] = host::text_width(a0, a1, a2) as u64,
        SYS_PKG_INSTALL => frame.regs[0] = sys_pkg_install(a0, a1) as u64,
        SYS_VFS_CALL => frame.regs[0] = sys_vfs_call(a0, a1, a2, a3) as u64,
        SYS_SYS_CALL => frame.regs[0] = host::sys_call(a0, a1, a2, a3) as u64,
        SYS_APP_LAUNCH => frame.regs[0] = host::app_launch(a0, a1, a2) as u64,
        SYS_ABI_INFO => frame.regs[0] = sys_abi_info(a0, a1) as u64,
        _ => frame.regs[0] = ENOSYS as u64,
    }
}
