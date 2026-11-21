//! Sistem çağrısı sarmalayıcıları (x86_64: `int 0x80`; aarch64: `svc #0`, numara x8). Numaralar: `nc_abi::SYS_*`.
//! Ana makinede derlenir ama çağrılmaz; testler `run()` ile sistem çağrısız çalışır.

use nc_abi::{
    SYS_APP_DONE, SYS_APP_LAUNCH, SYS_EXIT, SYS_FILE_CLOSE, SYS_FILE_OPEN, SYS_FILE_READ, SYS_FILE_WRITE,
    SYS_GUI_BACKDROP, SYS_GUI_DRAW, SYS_GUI_PRESENT, SYS_NET_CALL, SYS_PKG_INSTALL, SYS_SHELL_CALL, SYS_SYS_CALL, SYS_TEXT_WIDTH, SYS_VFS_CALL,
    SYS_WRITE,
};

/// # Safety
/// Argümanlar çekirdeğin beklediği değerler olmalı (işaretçiler geçerli kullanıcı belleğini göstermeli).
#[inline(always)]
pub unsafe fn syscall3(number: u64, a: u64, b: u64, c: u64) -> i64 {
    #[cfg(target_arch = "x86_64")]
    {
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
    #[cfg(target_arch = "aarch64")]
    {
        let ret: u64;
        core::arch::asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a => ret,
            in("x1") b,
            in("x2") c,
            options(nostack)
        );
        ret as i64
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = (number, a, b, c);
        -38
    }
}

/// Olayı bitirir. Dönmez.
pub fn app_done(flags: u32) -> ! {
    unsafe {
        syscall3(SYS_APP_DONE, flags as u64, 0, 0);
    }
    exit(0)
}

/// Saat okur. `kind`: 0 = çekirdek tikleri (100 Hz), 1 = duvar saati (UTC Unix saniyesi; bilinmiyorsa 0).
pub fn clock_get(kind: u64) -> i64 {
    unsafe { syscall3(nc_abi::SYS_CLOCK_GET, kind, 0, 0) }
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
    #[cfg(target_arch = "x86_64")]
    {
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
    #[cfg(target_arch = "aarch64")]
    {
        let ret: u64;
        core::arch::asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a => ret,
            in("x1") b,
            in("x2") c,
            in("x3") d,
            options(nostack)
        );
        ret as i64
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = (number, a, b, c, d);
        -38
    }
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

/// Uygulamayı başlatır ya da arka planda duraklatılmışsa ön plana getirir (`OP_RESUME`).
pub fn app_launch(app_id: &str) -> i64 {
    unsafe { syscall3(SYS_APP_LAUNCH, app_id.as_ptr() as u64, app_id.len() as u64, 0) }
}

/// Arka plandaki uygulamayı sonlandırır ve fiziksel bellek karelerini serbest bırakır.
pub fn app_stop(app_id: &str) -> i64 {
    unsafe { syscall3(SYS_APP_LAUNCH, app_id.as_ptr() as u64, app_id.len() as u64, 1) }
}

/// Ring 3 pencere yöneticisi (`nc.home`): ön plandaki uygulamayı duraklatır (`OP_PAUSE`) ve ana ekrana döner.
pub fn wm_pause_to_home() -> i64 {
    let id = "nc.home";
    unsafe { syscall3(SYS_APP_LAUNCH, id.as_ptr() as u64, id.len() as u64, 2) }
}

/// Ring 3 pencere yöneticisi (`nc.home`): ön plandaki alt uygulamaya Geri (`key::ESCAPE`) gönderir.
pub fn wm_send_back() -> i64 {
    let id = "nc.home";
    unsafe { syscall3(SYS_APP_LAUNCH, id.as_ptr() as u64, id.len() as u64, 3) }
}

/// Ring 3 pencere yöneticisi (`nc.home`): dokunma olayını ön plandaki alt uygulamaya iletir.
pub fn wm_forward_touch(x: i32, y: i32, pointer_flags: u32) -> i64 {
    let packed_xy = (((x.max(0) as u64) & 0xFFFF) << 16) | ((y.max(0) as u64) & 0xFFFF);
    unsafe { syscall3(SYS_APP_LAUNCH, packed_xy, pointer_flags as u64, 4) }
}

/// Ring 3 pencere yöneticisi (`nc.home`): alt uygulamanın görünüm alanı yüksekliğini (`top_inset`, `bottom_inset`) bildirir.
pub fn wm_set_viewport(top_inset: i32, bottom_inset: i32) -> i64 {
    let packed = (((top_inset.max(0) as u64) & 0xFFFF) << 16) | ((bottom_inset.max(0) as u64) & 0xFFFF);
    unsafe { syscall3(SYS_APP_LAUNCH, packed, 0, 5) }
}

/// Mobil sistem ayarını (`nc_abi::sys::MOBILE_SET_*`) günceller ve `/data` bölümüne kalıcı yazar.
pub fn mobile_set(sub_op: u32, val: u32) -> bool {
    let mut req = [0u8; 32];
    let mut resp = [0u8; 32];
    let Some(n) = nc_abi::sys::encode_request(&mut req, nc_abi::sys::OP_MOBILE_SET, sub_op, val, 0, &[]) else {
        return false;
    };
    let r = sys_call(&req[..n], &mut resp);
    r >= nc_abi::sys::RESP_HEADER as i64
}

/// VFS hizmeti: `request` (nc_abi::vfs) gönderir, yanıtı `response`'a alır. Dönüş: yanıt uzunluğu ya da -errno.
pub fn vfs_call(request: &[u8], response: &mut [u8]) -> i64 {
    unsafe { syscall4(SYS_VFS_CALL, request.as_ptr() as u64, request.len() as u64, response.as_mut_ptr() as u64, response.len() as u64) }
}

/// HTTP hizmeti: `request` (nc_abi::net) gönderir, yanıtı `response`'a alır. Dönüş: yanıt uzunluğu ya da -errno.
pub fn net_call(request: &[u8], response: &mut [u8]) -> i64 {
    unsafe { syscall4(SYS_NET_CALL, request.as_ptr() as u64, request.len() as u64, response.as_mut_ptr() as u64, response.len() as u64) }
}

/// Sistem hizmeti (Ayarlar): `request` (nc_abi::sys) gönderir, yanıtı `response`'a alır. Dönüş: yanıt uzunluğu ya da -errno.
pub fn sys_call(request: &[u8], response: &mut [u8]) -> i64 {
    unsafe { syscall4(SYS_SYS_CALL, request.as_ptr() as u64, request.len() as u64, response.as_mut_ptr() as u64, response.len() as u64) }
}

/// Kabuk hizmeti (uygulama sesi, bildirimler): `request` (nc_abi::shell) gönderir, yanıtı `response`'a alır. Dönüş: yanıt uzunluğu ya da -errno.
#[cfg(feature = "host")]
pub fn shell_call(_request: &[u8], _response: &mut [u8]) -> i64 {
    -38
}

#[cfg(not(feature = "host"))]
pub fn shell_call(request: &[u8], response: &mut [u8]) -> i64 {
    unsafe { syscall4(SYS_SHELL_CALL, request.as_ptr() as u64, request.len() as u64, response.as_mut_ptr() as u64, response.len() as u64) }
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