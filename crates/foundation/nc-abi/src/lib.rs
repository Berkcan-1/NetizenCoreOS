//! NetizenCore Ring 3 ABI: çekirdek ile kullanıcı uygulamaları arasındaki TEK sözleşme.
//!
//! `no_std`, bağımlılıksız. Hem çekirdek hem (ileride) SDK uygulamaları bunu kullanır.
//! Model: OLAY-TABANLI. Çekirdek her olayda uygulamayı `e_entry`'den, yazmaçlar sıfır ve yığın
//! boş olarak girer; uygulama durumu kendi belleğinde (.data/.bss/heap) kalır. Olay, `USER_EVENT_PAGE`
//! adresindeki salt-okunur `EventBlock`'tan okunur. Uygulama işini bitirince `SYS_APP_DONE` çağırır.
#![no_std]

pub mod compositor;
pub mod draw;
pub mod dysonsphere;
pub mod event;
pub mod ipc;
pub mod notify;
pub mod perm;
pub mod ctx;
pub mod key;
pub mod vfs;
pub mod net;
pub mod shell;
pub mod sys;

pub const ABI_VERSION_MAJOR: u16 = 1;
pub const ABI_VERSION_MINOR: u16 = 1;
pub const ABI_VERSION_PATCH: u16 = 0;
pub const ABI_VERSION: u32 = 1;

pub const ABI_INFO_MAGIC: u32 = 0x4942_414E; // "NABI"
pub const ARCH_X86_64: u32 = 1;
pub const ARCH_AARCH64: u32 = 2;

pub const FEAT_SPARK_SEC: u64 = 1 << 0;
pub const FEAT_VFS_CALL: u64 = 1 << 1;
pub const FEAT_APP_LAUNCH: u64 = 1 << 2;
pub const FEAT_SYS_CALL: u64 = 1 << 3;
pub const FEAT_NET_CALL: u64 = 1 << 4;
pub const FEAT_SHELL_CALL: u64 = 1 << 5;
pub const FEAT_FDE_STORAGE: u64 = 1 << 6;
pub const FEAT_HW_CSPRNG: u64 = 1 << 7;
pub const FEAT_SMP: u64 = 1 << 8;
pub const FEAT_IPC_INTENTS: u64 = 1 << 9;

/// Versioned ABI discovery structure returned by `SYS_ABI_INFO` (32 bytes, fixed layout).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbiInfo {
    pub magic: u32,
    pub struct_size: u32,
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub reserved: u16,
    pub arch_id: u32,
    pub max_syscall: u32,
    pub features: u64,
}

impl AbiInfo {
    pub const SIZE: usize = 32;

    pub const fn new(arch_id: u32, max_syscall: u32, features: u64) -> Self {
        Self {
            magic: ABI_INFO_MAGIC,
            struct_size: Self::SIZE as u32,
            major: ABI_VERSION_MAJOR,
            minor: ABI_VERSION_MINOR,
            patch: ABI_VERSION_PATCH,
            reserved: 0,
            arch_id,
            max_syscall,
            features,
        }
    }

    pub fn to_le_bytes(&self) -> [u8; Self::SIZE] {
        let mut out = [0u8; Self::SIZE];
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..8].copy_from_slice(&self.struct_size.to_le_bytes());
        out[8..10].copy_from_slice(&self.major.to_le_bytes());
        out[10..12].copy_from_slice(&self.minor.to_le_bytes());
        out[12..14].copy_from_slice(&self.patch.to_le_bytes());
        out[14..16].copy_from_slice(&self.reserved.to_le_bytes());
        out[16..20].copy_from_slice(&self.arch_id.to_le_bytes());
        out[20..24].copy_from_slice(&self.max_syscall.to_le_bytes());
        out[24..32].copy_from_slice(&self.features.to_le_bytes());
        out
    }

    pub fn from_le_bytes(raw: &[u8]) -> Option<Self> {
        if raw.len() < Self::SIZE {
            return None;
        }
        let magic = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        let struct_size = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
        if magic != ABI_INFO_MAGIC || (struct_size as usize) < Self::SIZE {
            return None;
        }
        Some(Self {
            magic,
            struct_size,
            major: u16::from_le_bytes([raw[8], raw[9]]),
            minor: u16::from_le_bytes([raw[10], raw[11]]),
            patch: u16::from_le_bytes([raw[12], raw[13]]),
            reserved: u16::from_le_bytes([raw[14], raw[15]]),
            arch_id: u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]),
            max_syscall: u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]),
            features: u64::from_le_bytes([
                raw[24], raw[25], raw[26], raw[27], raw[28], raw[29], raw[30], raw[31],
            ]),
        })
    }

    pub const fn is_compatible_with(&self, required_major: u16, min_minor: u16) -> bool {
        self.magic == ABI_INFO_MAGIC && self.major == required_major && self.minor >= min_minor
    }
}

const _: () = {
    assert!(core::mem::size_of::<AbiInfo>() == AbiInfo::SIZE);
    assert!(core::mem::align_of::<AbiInfo>() == 8);
};

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
/// HTTP hizmeti (`nc_abi::net`): Tarayici icin; TCP/TLS cekirdekte kalir.
pub const SYS_NET_CALL: u64 = 27;
/// Sistem hizmeti (`nc_abi::sys`): yalnizca Ayarlar icin; kablosuz, Ethernet, suruculer, PCI, kilit, ayar geri yazimi.
pub const SYS_SYS_CALL: u64 = 28;
/// Kabuk hizmeti (`nc_abi::shell`): uygulama sesi ve bildirimler; yetki `audio.play` / `notification.post`.
pub const SYS_SHELL_CALL: u64 = 29;
/// Uygulama yasam dongusu (`x0 = kimlik_ptr, x1 = kimlik_len, x2 = islem`: 0 = baslat/one getir, 1 = durdur).
pub const SYS_APP_LAUNCH: u64 = 30;
/// ABI surum ve ozellik kesfi (`x0 = out_ptr, x1 = out_len` -> yazilan bayt sayisi veya -EINVAL/-EFAULT).
pub const SYS_ABI_INFO: u64 = 31;

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
/// Büyük heap isteyen uygulamalar (Tarayıcı) için yığının ÜSTÜNDE ayrı bölge; talep-sayfalamayla dolar.
pub const USER_HEAP_HIGH_BASE: u64 = USER_SPACE_START + 0x1000_0000;
pub const BIG_HEAP_BYTES: u64 = 96 * 1024 * 1024;

/// (heap tabanı, heap boyutu): yerleşik uygulama kimliğine göre.
pub fn heap_for(app_id: &str) -> (u64, u64) {
    match app_id {
        "nc.browser" => (USER_HEAP_HIGH_BASE, BIG_HEAP_BYTES),
        _ => (USER_HEAP_BASE, APP_HEAP_BYTES),
    }
}
pub const USER_EVENT_PAGE: u64 = USER_SPACE_START + 0x00F0_0000;
pub const USER_IMAGE_MIN: u64 = USER_SPACE_START + 0x0100_0000;
pub const USER_IMAGE_MAX: u64 = USER_SPACE_START + 0x0800_0000;
pub const USER_APP_STACK_GUARD: u64 = USER_SPACE_START + 0x0900_0000;
pub const APP_STACK_PAGES: u64 = 512;
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
    assert!(USER_APP_STACK_TOP < USER_HEAP_HIGH_BASE);
    assert!(USER_APP_STACK_TOP < USER_SPACE_START + 0x7F00_0000_0000);
};

pub const ELF_MAX_SEGMENTS: usize = 8;
pub const ELF_MAX_PAGES: u64 = 4 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_info_roundtrip_and_compatibility() {
        let info = AbiInfo::new(
            ARCH_AARCH64,
            SYS_ABI_INFO as u32,
            FEAT_SPARK_SEC | FEAT_VFS_CALL | FEAT_APP_LAUNCH,
        );
        let raw = info.to_le_bytes();
        assert_eq!(raw.len(), AbiInfo::SIZE);
        let decoded = AbiInfo::from_le_bytes(&raw).expect("valid AbiInfo");
        assert_eq!(decoded, info);
        assert!(decoded.is_compatible_with(1, 0));
        assert!(decoded.is_compatible_with(1, 1));
        assert!(!decoded.is_compatible_with(2, 0));
        assert!(!decoded.is_compatible_with(1, 2));
    }

    #[test]
    fn abi_info_rejects_bad_magic_and_short_buffer() {
        assert!(AbiInfo::from_le_bytes(&[0u8; 16]).is_none());
        let mut bad = AbiInfo::new(ARCH_X86_64, 31, 0).to_le_bytes();
        bad[0] ^= 0xFF;
        assert!(AbiInfo::from_le_bytes(&bad).is_none());
    }

    #[test]
    fn syscall_numbers_remain_stable() {
        assert_eq!(ABI_VERSION, 1);
        assert_eq!(SYS_EXIT, 1);
        assert_eq!(SYS_YIELD, 2);
        assert_eq!(SYS_SLEEP, 3);
        assert_eq!(SYS_WRITE, 4);
        assert_eq!(SYS_CLOCK_GET, 5);
        assert_eq!(SYS_APP_DONE, 16);
        assert_eq!(SYS_GUI_DRAW, 20);
        assert_eq!(SYS_TEXT_WIDTH, 21);
        assert_eq!(SYS_PKG_INSTALL, 23);
        assert_eq!(SYS_VFS_CALL, 26);
        assert_eq!(SYS_NET_CALL, 27);
        assert_eq!(SYS_SYS_CALL, 28);
        assert_eq!(SYS_SHELL_CALL, 29);
        assert_eq!(SYS_APP_LAUNCH, 30);
        assert_eq!(SYS_ABI_INFO, 31);
        assert_eq!(event::EVENT_SIZE, 96);
    }
}
