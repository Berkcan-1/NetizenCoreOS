//! NetizenCore Ring 3 ABI: çekirdek ile kullanıcı uygulamaları arasındaki TEK sözleşme.
//!
//! `no_std`, bağımlılıksız. Hem çekirdek hem (ileride) SDK uygulamaları bunu kullanır.
//! Model: OLAY-TABANLI. Çekirdek her olayda uygulamayı `e_entry`'den, yazmaçlar sıfır ve yığın
//! boş olarak girer; uygulama durumu kendi belleğinde (.data/.bss/heap) kalır. Olay, `USER_EVENT_PAGE`
//! adresindeki salt-okunur `EventBlock`'tan okunur. Uygulama işini bitirince `SYS_APP_DONE` çağırır.
#![no_std]

pub mod draw;
pub mod event;
pub mod ctx;
pub mod key;
pub mod vfs;

pub const ABI_VERSION: u32 = 1;

pub const SYS_EXIT: u64 = 1;
pub const SYS_YIELD: u64 = 2;
pub const SYS_SLEEP: u64 = 3;
pub const SYS_WRITE: u64 = 4;
pub const SYS_CLOCK_GET: u64 = 5;
pub const SYS_HANDLE_OPEN: u64 = 6;
pub const SYS_HANDLE_CLOSE: u64 = 7;
pub const SYS_GUI_SUBMIT: u64 = 8;
pub const SYS_APP_DONE: u64 = 16;
pub const SYS_FILE_OPEN: u64 = 17;
pub const SYS_FILE_READ: u64 = 18;
pub const SYS_FILE_WRITE: u64 = 19;
pub const SYS_GUI_DRAW: u64 = 20;
pub const SYS_TEXT_WIDTH: u64 = 21;
pub const SYS_FILE_CLOSE: u64 = 22;
/// rdi = yol (`/data/app/...`), rsi = uzunluk: çekirdek paketi KENDİSİ doğrular; olay sonunda RET_INSTALL ile bildirilir.
pub const SYS_PKG_INSTALL: u64 = 23;
/// rdi = yüzey, rsi = tampon (u32 piksel), rdx = genişlik, r10 = yükseklik: masaüstünü tampona kopyalar (yalnızca OP_DRAW).
pub const SYS_GUI_BACKDROP: u64 = 24;
/// Aynı argümanlar: tamponu pencereye (kırparak) basar (yalnızca OP_DRAW).
pub const SYS_GUI_PRESENT: u64 = 25;
/// rdi = istek, rsi = uzunluk, rdx = yanıt tamponu, r10 = kapasite: VFS hizmeti (`nc_abi::vfs`).
pub const SYS_VFS_CALL: u64 = 26;

pub const EPERM: i64 = -1;
pub const ENOENT: i64 = -2;
pub const EBADF: i64 = -9;
pub const EAGAIN: i64 = -11;
pub const ENOMEM: i64 = -12;
pub const EACCES: i64 = -13;
pub const EFAULT: i64 = -14;
pub const EINVAL: i64 = -22;
pub const EMFILE: i64 = -24;
pub const EFBIG: i64 = -27;
pub const ENOSYS: i64 = -38;

pub const FILE_MODE_READ: u64 = 1;
pub const FILE_MODE_WRITE: u64 = 2;
pub const FILE_MAX_BYTES: usize = 1024 * 1024;
pub const FILE_MAX_OPEN: usize = 8;
pub const FILE_MAX_PATH: usize = 256;

pub const USER_SPACE_START: u64 = 0x0000_0080_0000_0000;
pub const PAGE_SIZE: u64 = 4096;

pub const USER_HEAP_BASE: u64 = USER_SPACE_START + 0x0040_0000;
pub const APP_HEAP_BYTES: u64 = 8 * 1024 * 1024;
pub const USER_EVENT_PAGE: u64 = USER_SPACE_START + 0x00F0_0000;
pub const USER_IMAGE_MIN: u64 = USER_SPACE_START + 0x0100_0000;
pub const USER_IMAGE_MAX: u64 = USER_SPACE_START + 0x0800_0000;
pub const USER_APP_STACK_GUARD: u64 = USER_SPACE_START + 0x0900_0000;
pub const APP_STACK_PAGES: u64 = 32;
pub const USER_APP_STACK_BOTTOM: u64 = USER_APP_STACK_GUARD + PAGE_SIZE;
pub const USER_APP_STACK_TOP: u64 = USER_APP_STACK_BOTTOM + APP_STACK_PAGES * PAGE_SIZE;
/// Girişte RSP, `call` edilmiş gibi 8 (mod 16) olur: RSP = USER_APP_STACK_TOP - 8.
pub const USER_ENTRY_RSP: u64 = USER_APP_STACK_TOP - 8;

/// Kritik: bu bölgeler birbirine girmemeli (derleme zamanı denetimi).
const _: () = {
    assert!(USER_HEAP_BASE + APP_HEAP_BYTES <= USER_EVENT_PAGE);
    assert!(USER_EVENT_PAGE + PAGE_SIZE <= USER_IMAGE_MIN);
    assert!(USER_IMAGE_MAX <= USER_APP_STACK_GUARD);
    assert!(USER_ENTRY_RSP % 16 == 8);
    assert!(USER_APP_STACK_TOP < USER_SPACE_START + 0x7F00_0000_0000);
};

pub const ELF_MAX_SEGMENTS: usize = 8;
pub const ELF_MAX_PAGES: u64 = 4 * 1024;
