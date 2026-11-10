//! `ContextBlock`: çekirdeğin her olayda `USER_EVENT_PAGE + CTX_OFFSET`'e yazdığı, uygulamanın görmesi gereken
//! sistem bağlamı (ekran, bellek, ayarlar). Sabit, elle kodlanmış 128 baytlık düzen.

pub const CTX_OFFSET: usize = 256;
pub const CTX_SIZE: usize = 128;
pub const CTX_MAGIC: u32 = 0x5854_434E; // "NCTX"
pub const PLATFORM_MAX: usize = 48;
pub const NO_CUSTOM_WALLPAPER: u16 = 0xFFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextBlock {
    pub screen_w: i32,
    pub screen_h: i32,
    pub memory_mb: u32,
    pub wallpaper: u32,
    pub accent: u32,
    pub wallpaper_animated: bool,
    pub wallpaper_speed: u8,
    pub custom_wallpaper: u16,
    pub glass_blur: u8,
    pub dock_auto_hide: bool,
    pub launch_anim_speed: u8,
    pub wifi_enabled: bool,
    pub bluetooth_enabled: bool,
    pub language: u8,
    pub keyboard: u8,
    pub dark_theme: bool,
    pub platform_len: u8,
    pub platform: [u8; PLATFORM_MAX],
}

impl ContextBlock {
    pub fn platform_str(&self) -> &str {
        let len = (self.platform_len as usize).min(PLATFORM_MAX);
        core::str::from_utf8(&self.platform[..len]).unwrap_or("")
    }

    pub fn set_platform(&mut self, name: &str) {
        let mut len = name.len().min(PLATFORM_MAX);
        while len > 0 && !name.is_char_boundary(len) {
            len -= 1;
        }
        self.platform = [0; PLATFORM_MAX];
        self.platform[..len].copy_from_slice(&name.as_bytes()[..len]);
        self.platform_len = len as u8;
    }

    pub fn encode(&self) -> [u8; CTX_SIZE] {
        let mut b = [0u8; CTX_SIZE];
        b[0..4].copy_from_slice(&CTX_MAGIC.to_le_bytes());
        b[4..8].copy_from_slice(&self.screen_w.to_le_bytes());
        b[8..12].copy_from_slice(&self.screen_h.to_le_bytes());
        b[12..16].copy_from_slice(&self.memory_mb.to_le_bytes());
        b[16..20].copy_from_slice(&self.wallpaper.to_le_bytes());
        b[20..24].copy_from_slice(&self.accent.to_le_bytes());
        b[24] = self.wallpaper_animated as u8;
        b[25] = self.wallpaper_speed;
        b[26..28].copy_from_slice(&self.custom_wallpaper.to_le_bytes());
        b[28] = self.glass_blur;
        b[29] = self.dock_auto_hide as u8;
        b[30] = self.launch_anim_speed;
        b[31] = self.wifi_enabled as u8;
        b[32] = self.bluetooth_enabled as u8;
        b[33] = self.language;
        b[34] = self.keyboard;
        b[35] = self.dark_theme as u8;
        b[36] = self.platform_len;
        b[40..40 + PLATFORM_MAX].copy_from_slice(&self.platform);
        b
    }

    pub fn decode(b: &[u8]) -> Option<ContextBlock> {
        if b.len() < CTX_SIZE || u32::from_le_bytes([b[0], b[1], b[2], b[3]]) != CTX_MAGIC {
            return None;
        }
        let i32_at = |o: usize| i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let u32_at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        if b[36] as usize > PLATFORM_MAX {
            return None;
        }
        let mut platform = [0u8; PLATFORM_MAX];
        platform.copy_from_slice(&b[40..40 + PLATFORM_MAX]);
        Some(ContextBlock {
            screen_w: i32_at(4),
            screen_h: i32_at(8),
            memory_mb: u32_at(12),
            wallpaper: u32_at(16),
            accent: u32_at(20),
            wallpaper_animated: b[24] != 0,
            wallpaper_speed: b[25],
            custom_wallpaper: u16::from_le_bytes([b[26], b[27]]),
            glass_blur: b[28],
            dock_auto_hide: b[29] != 0,
            launch_anim_speed: b[30],
            wifi_enabled: b[31] != 0,
            bluetooth_enabled: b[32] != 0,
            language: b[33],
            keyboard: b[34],
            dark_theme: b[35] != 0,
            platform_len: b[36],
            platform,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ContextBlock {
        let mut c = ContextBlock {
            screen_w: 1280,
            screen_h: 720,
            memory_mb: 492,
            wallpaper: 3,
            accent: 2,
            wallpaper_animated: true,
            wallpaper_speed: 2,
            custom_wallpaper: NO_CUSTOM_WALLPAPER,
            glass_blur: 1,
            dock_auto_hide: true,
            launch_anim_speed: 3,
            wifi_enabled: true,
            bluetooth_enabled: false,
            language: 1,
            keyboard: 1,
            dark_theme: true,
            platform_len: 0,
            platform: [0; PLATFORM_MAX],
        };
        c.set_platform("QEMU / KVM Sanal Donanım");
        c
    }

    #[test]
    fn roundtrip() {
        let c = sample();
        assert_eq!(ContextBlock::decode(&c.encode()), Some(c));
        assert_eq!(c.platform_str(), "QEMU / KVM Sanal Donanım");
    }

    #[test]
    fn rejects_bad_input() {
        let mut b = sample().encode();
        assert!(ContextBlock::decode(&b[..CTX_SIZE - 1]).is_none());
        b[0] ^= 1;
        assert!(ContextBlock::decode(&b).is_none());
        let mut b = sample().encode();
        b[36] = 200;
        assert!(ContextBlock::decode(&b).is_none(), "platform_len sinir disi");
    }

    #[test]
    fn long_platform_is_truncated_on_char_boundary() {
        let mut c = sample();
        c.set_platform(&"ş".repeat(40));
        assert!(c.platform_len as usize <= PLATFORM_MAX);
        assert!(core::str::from_utf8(&c.platform[..c.platform_len as usize]).is_ok());
    }

    #[test]
    fn ctx_region_does_not_overlap_event_block() {
        assert!(crate::event::EVENT_SIZE <= CTX_OFFSET);
        assert!(CTX_OFFSET + CTX_SIZE <= crate::PAGE_SIZE as usize);
        assert!(40 + PLATFORM_MAX <= CTX_SIZE);
    }
}
