//! Sistem çağrısı sarmalayıcıları (`int 0x80`). Numaralar: `nc_abi::SYS_*`.
//! Ana makinede derlenir ama çağrılmaz; testler `run()` ile sistem çağrısız çalışır.

use nc_abi::{
    SYS_APP_DONE, SYS_EXIT, SYS_FILE_CLOSE, SYS_FILE_OPEN, SYS_FILE_READ, SYS_FILE_WRITE,
    SYS_GUI_BACKDROP, SYS_GUI_DRAW, SYS_GUI_PRESENT, SYS_PKG_INSTALL, SYS_TEXT_WIDTH, SYS_VFS_CALL, SYS_WRITE,
};

/// # Safety
/// Argümanlar çekirdeğin beklediği değerler olmalı (işaretçiler geçerli kullanıcı belleğini göstermeli).
#[inline(always)]
pub unsafe fn syscall3(number: u64, a: u64, b: u64, c: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "int 0x80",
        inlateout("rax") number as i64 => ret,
        in("rdi") a,
        in("rsi") b,
        in("rdx") c,
        options(nostack)
    );
    ret
}

/// Olayı bitirir. Dönmez.
pub fn app_done(flags: u32) -> ! {
    unsafe {
        syscall3(SYS_APP_DONE, flags as u64, 0, 0);
    }
    exit(0)
}

pub fn exit(code: i32) -> ! {
    loop {
        unsafe {
            syscall3(SYS_EXIT, code as u64, 0, 0);
        }
    }
}

/// Konsola (seri günlük) yazar.
pub fn log(text: &str) -> i64 {
    unsafe { syscall3(SYS_WRITE, 1, text.as_ptr() as u64, text.len() as u64) }
}

pub fn gui_draw(surface: u64, stream: &[u8]) -> i64 {
    unsafe { syscall3(SYS_GUI_DRAW, surface, stream.as_ptr() as u64, stream.len() as u64) }
}

pub fn text_width(text: &str, cap_height: i32) -> i64 {
    unsafe { syscall3(SYS_TEXT_WIDTH, text.as_ptr() as u64, text.len() as u64, cap_height as u64) }
}

pub fn file_open(path: &str, write: bool) -> i64 {
    let mode = if write { nc_abi::FILE_MODE_WRITE } else { nc_abi::FILE_MODE_READ };
    unsafe { syscall3(SYS_FILE_OPEN, path.as_ptr() as u64, path.len() as u64, mode) }
}

pub fn file_read(handle: i64, buffer: &mut [u8]) -> i64 {
    unsafe { syscall3(SYS_FILE_READ, handle as u64, buffer.as_mut_ptr() as u64, buffer.len() as u64) }
}

pub fn file_write(handle: i64, data: &[u8]) -> i64 {
    unsafe { syscall3(SYS_FILE_WRITE, handle as u64, data.as_ptr() as u64, data.len() as u64) }
}

pub fn file_close(handle: i64) -> i64 {
    unsafe { syscall3(SYS_FILE_CLOSE, handle as u64, 0, 0) }
}

#[inline(always)]
unsafe fn syscall4(number: u64, a: u64, b: u64, c: u64, d: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "int 0x80",
        inlateout("rax") number as i64 => ret,
        in("rdi") a,
        in("rsi") b,
        in("rdx") c,
        in("r10") d,
        options(nostack)
    );
    ret
}

/// Masaüstünü (pencere alanı) `pixels`'e kopyalar; yalnızca OP_DRAW sırasında. `pixels` önceden dokunulmuş olmalı.
pub fn gui_backdrop(surface: u64, pixels: &mut [u32], width: usize, height: usize) -> i64 {
    unsafe { syscall4(SYS_GUI_BACKDROP, surface, pixels.as_mut_ptr() as u64, width as u64, height as u64) }
}

/// `pixels`'i pencereye (kırparak) basar; yalnızca OP_DRAW sırasında.
pub fn gui_present(surface: u64, pixels: &[u32], width: usize, height: usize) -> i64 {
    unsafe { syscall4(SYS_GUI_PRESENT, surface, pixels.as_ptr() as u64, width as u64, height as u64) }
}

/// Çekirdeğe `/data/app/...` altındaki bir paketi yeniden doğrulatıp kurdurur.
pub fn pkg_install(path: &str) -> i64 {
    unsafe { syscall3(SYS_PKG_INSTALL, path.as_ptr() as u64, path.len() as u64, 0) }
}

/// VFS hizmeti: `request` (nc_abi::vfs) gönderir, yanıtı `response`'a alır. Dönüş: yanıt uzunluğu ya da -errno.
pub fn vfs_call(request: &[u8], response: &mut [u8]) -> i64 {
    unsafe { syscall4(SYS_VFS_CALL, request.as_ptr() as u64, request.len() as u64, response.as_mut_ptr() as u64, response.len() as u64) }
}

pub fn panic_exit(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    struct Buf { b: [u8; 200], n: usize }
    impl Write for Buf {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let k = s.len().min(self.b.len() - self.n);
            self.b[self.n..self.n + k].copy_from_slice(&s.as_bytes()[..k]);
            self.n += k;
            Ok(())
        }
    }
    let mut w = Buf { b: [0; 200], n: 0 };
    let _ = w.write_str("[r3 panic] ");
    if let Some(l) = info.location() {
        let _ = write!(w, "{}:{} ", l.file(), l.line());
    }
    let _ = write!(w, "{}\n", info.message());
    let text = match core::str::from_utf8(&w.b[..w.n]) {
        Ok(s) => s,
        Err(e) => core::str::from_utf8(&w.b[..e.valid_up_to()]).unwrap_or(""),
    };
    log(text);
    exit(101)
}