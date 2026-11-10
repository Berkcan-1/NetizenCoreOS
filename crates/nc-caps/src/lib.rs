//! NetizenCore yetki (capability) bitleri için tek doğruluk kaynağı (F-05).
//!
//! Eskiden `kernel/src/process.rs` (`CAP_*`) ve `nc-pkg` (`Capability`) aynı yetkiler için FARKLI
//! bit değerleri kullanıyordu (ör. FS okuma: 1<<5 ve 1<<6). İkisi de artık buradan türetilir.
//! Numaralandırma `nc-pkg`'nin önceki (daha eksiksiz) düzenidir; çekirdek tarafı buna uyarlandı.
//!
//! DURUM: derlenmedi/çalıştırılmadı (yazıldığı ortamda cargo yoktu). Çakışma kontrolü derleme
//! zamanındadır: iki yetki aynı biti paylaşırsa veya tek bit değilse DERLEME HATASI verir.
#![no_std]

pub const NONE: u32 = 0;
pub const SETTINGS_READ: u32 = 1 << 0;
pub const SETTINGS_WRITE_DISPLAY: u32 = 1 << 1;
pub const SETTINGS_WRITE_LOCALE: u32 = 1 << 2;
pub const SETTINGS_WRITE_KEYBOARD: u32 = 1 << 3;
pub const SETTINGS_WRITE_NETWORK: u32 = 1 << 4;
pub const SETTINGS_WRITE_DRIVERS: u32 = 1 << 5;
pub const FS_USER_READ: u32 = 1 << 6;
pub const FS_USER_WRITE: u32 = 1 << 7;
pub const FS_USER_DELETE: u32 = 1 << 8;
pub const FS_GRANT_READ: u32 = 1 << 9;
pub const FS_GRANT_WRITE: u32 = 1 << 10;
pub const PACKAGE_INSTALL: u32 = 1 << 11;
pub const NETWORK_CLIENT: u32 = 1 << 12;
pub const SYSTEM_AUDIT_READ: u32 = 1 << 13;
pub const IPC_ENDPOINT: u32 = 1 << 14;

/// Yalnızca çekirdek içi kimlik için (tüm bitler). Üçüncü taraf/uygulama bu değeri ASLA alamaz.
pub const KERNEL_INTERNAL: u32 = 0xFFFF_FFFF;

/// Tanımlı tüm tekil yetkiler.
pub const ALL: [u32; 15] = [
    SETTINGS_READ,
    SETTINGS_WRITE_DISPLAY,
    SETTINGS_WRITE_LOCALE,
    SETTINGS_WRITE_KEYBOARD,
    SETTINGS_WRITE_NETWORK,
    SETTINGS_WRITE_DRIVERS,
    FS_USER_READ,
    FS_USER_WRITE,
    FS_USER_DELETE,
    FS_GRANT_READ,
    FS_GRANT_WRITE,
    PACKAGE_INSTALL,
    NETWORK_CLIENT,
    SYSTEM_AUDIT_READ,
    IPC_ENDPOINT,
];

// Derleme zamanı: her yetki tek bit olmalı ve hiçbir ikisi çakışmamalı.
const _: () = {
    let mut i = 0;
    let mut seen = 0u32;
    while i < ALL.len() {
        assert!(ALL[i].count_ones() == 1, "capability must be a single bit");
        assert!(seen & ALL[i] == 0, "two capabilities share the same bit");
        seen |= ALL[i];
        i += 1;
    }
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_read_and_write_are_distinct_single_bits() {
        assert_ne!(FS_USER_READ, FS_USER_WRITE);
        assert_eq!(FS_USER_READ, 1 << 6);
        assert_eq!(FS_USER_WRITE, 1 << 7);
    }

    #[test]
    fn kernel_internal_covers_every_capability() {
        for c in ALL {
            assert_eq!(KERNEL_INTERNAL & c, c);
        }
    }
}
