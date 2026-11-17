//! NetizenCore yetki (capability) bitleri için tek doğruluk kaynağı (F-05).
//!
//! Eskiden `kernel/src/tasking/process.rs` (`CAP_*`) ve `nc-pkg` (`Capability`) aynı yetkiler için FARKLI
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
pub const AUDIO_PLAY: u32 = 1 << 15;
pub const NOTIFY_POST: u32 = 1 << 16;
pub const FS_ALL_READ: u32 = 1 << 17;
pub const FS_ALL_WRITE: u32 = 1 << 18;
pub const PROCESS_LAUNCH: u32 = 1 << 19;
pub const PACKAGE_REMOVE: u32 = 1 << 20;

/// Yalnızca çekirdek içi kimlik için (tüm bitler). Üçüncü taraf/uygulama bu değeri ASLA alamaz.
pub const KERNEL_INTERNAL: u32 = 0xFFFF_FFFF;

/// Tanımlı tüm tekil yetkiler.
pub const ALL: [u32; 21] = [
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
    AUDIO_PLAY,
    NOTIFY_POST,
    FS_ALL_READ,
    FS_ALL_WRITE,
    PROCESS_LAUNCH,
    PACKAGE_REMOVE,
];

/// Sonradan kurulan (kullanıcı katmanı) uygulamaların alabileceği EN FAZLA yetkiler. Manifest daha fazlasını isterse fazlası
/// sessizce düşer ve kurulum ekranında "verilmeyen" olarak gösterilir. Sistem katmanı (Ayarlar, Dosyalar, Paket Kurucu) bu
/// maskeye bağlı değildir; ama o katmana yalnızca platform anahtarıyla imzalı VE salt-okunur sistem bölümünden gelen paket girer.
pub const USER_ALLOWED: u32 = SETTINGS_READ
    | FS_USER_READ
    | FS_USER_WRITE
    | FS_USER_DELETE
    | NETWORK_CLIENT
    | IPC_ENDPOINT
    | AUDIO_PLAY
    | NOTIFY_POST;

pub const SYSTEM_ONLY: u32 = KERNEL_INTERNAL & !USER_ALLOWED;

pub const fn clamp_user(declared: u32) -> u32 {
    declared & USER_ALLOWED
}

pub const fn dropped_for_user(declared: u32) -> u32 {
    declared & !USER_ALLOWED
}

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
    fn user_tier_never_gets_privileged_bits() {
        let everything = ALL.iter().fold(0u32, |acc, c| acc | c);
        let granted = clamp_user(everything);
        assert_eq!(granted & PACKAGE_INSTALL, 0);
        assert_eq!(granted & PACKAGE_REMOVE, 0);
        assert_eq!(granted & PROCESS_LAUNCH, 0);
        assert_eq!(granted & FS_ALL_READ, 0);
        assert_eq!(granted & FS_ALL_WRITE, 0);
        assert_eq!(granted & SETTINGS_WRITE_DRIVERS, 0);
        assert_eq!(granted & FS_GRANT_WRITE, 0);
        assert_ne!(granted & NETWORK_CLIENT, 0);
    }

    #[test]
    fn kernel_internal_covers_every_capability() {
        for c in ALL {
            assert_eq!(KERNEL_INTERNAL & c, c);
        }
    }
}
