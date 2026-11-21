//! `EventBlock`: çekirdeğin her olayda `USER_EVENT_PAGE`'e yazdığı 96 baytlık, SABİT düzenli blok.
//! Düzen elle (little-endian) kodlanır; `repr(C)` varsayımına dayanmaz.

pub const EVENT_MAGIC: u32 = 0x5645_434E; // "NCEV"
pub const EVENT_SIZE: usize = 96;

pub const OP_INIT: u32 = 0;
pub const OP_UPDATE: u32 = 1;
pub const OP_DRAW: u32 = 2;
pub const OP_CLOSE: u32 = 3;
/// Uygulama arka plana alindi (onPause); durumu belleginde korunur, cizim gonderilmez.
pub const OP_PAUSE: u32 = 4;
/// Arka plandaki uygulama yeniden on plana getirildi (onResume); RET_REDRAW donmelidir.
pub const OP_RESUME: u32 = 5;

/// `EventBlock.flags` kodlamasi (mobil kabuk ve uygulamalar icin gercek cekirdek durumu):
/// bit 0..3: duvar kagidi temasi (0..3 hazir gradyan, 4 = kullanicinin resmi; cekirdek resmi uygulamanin altina cizer)
/// bit 4..7: arka planda/calisan uygulama maskesi (bit 0 = nc.files, bit 1 = nc.settings)
/// bit 8..11: on plandaki uygulama kimligi (0 = Home, 1 = nc.files, 2 = nc.settings)
/// bit 12..19: saat dilimi ofseti (saat + 12; 0 ise varsayilan UTC+3)
/// bit 20..23: kurulu .ntzc paket maskesi (bit 0 = nc.files, bit 1 = nc.settings)
/// bit 24..27: parlaklik adimi (2..=10 -> %20..=%100; 0 ise %100)
/// bit 28: Wi-Fi acik (1) / kapali (0)
/// bit 29: Bluetooth acik (1) / kapali (0)
/// bit 30: Sistem ekran kilidi aktif (1) / acik (0)
/// bit 31: Karanlik tema aktif (1) / acik tema (0)
#[allow(clippy::too_many_arguments)]
pub const fn pack_mobile_state(
    wallpaper: u8,
    running_mask: u8,
    fg_app: u8,
    tz_hours: i8,
    installed_mask: u8,
    brightness_pct: u8,
    wifi: bool,
    bt: bool,
) -> u32 {
    pack_mobile_state_v2(
        wallpaper,
        running_mask,
        fg_app,
        tz_hours,
        installed_mask,
        brightness_pct,
        wifi,
        bt,
        false,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub const fn pack_mobile_state_v2(
    wallpaper: u8,
    running_mask: u8,
    fg_app: u8,
    tz_hours: i8,
    installed_mask: u8,
    brightness_pct: u8,
    wifi: bool,
    bt: bool,
    locked: bool,
    dark_theme: bool,
) -> u32 {
    let wp = (wallpaper & 0x0F) as u32;
    let run = ((running_mask & 0x0F) as u32) << 4;
    let fg = ((fg_app & 0x0F) as u32) << 8;
    let raw = tz_hours as i32 + 12;
    let clamped = if raw < 0 { 0 } else if raw > 26 { 26 } else { raw };
    let tz = (clamped as u32) << 12;
    let inst = ((installed_mask & 0x0F) as u32) << 20;
    let step = (brightness_pct / 10) as i32;
    let b_step = if step < 2 { 2 } else if step > 10 { 10 } else { step };
    let br = ((b_step as u32) & 0x0F) << 24;
    let wf = (wifi as u32) << 28;
    let btf = (bt as u32) << 29;
    let lck = (locked as u32) << 30;
    let drk = (dark_theme as u32) << 31;
    wp | run | fg | tz | inst | br | wf | btf | lck | drk
}

pub const fn pack_mobile_flags_v2(wallpaper: u8, running_mask: u8, tz_hours: i8, installed_mask: u8) -> u32 {
    pack_mobile_state(wallpaper, running_mask, 0, tz_hours, installed_mask, 100, true, false)
}

pub const fn pack_mobile_flags(wallpaper: u8, running_mask: u8, tz_hours: i8) -> u32 {
    pack_mobile_flags_v2(wallpaper, running_mask, tz_hours, 0)
}

pub const fn unpack_wallpaper(flags: u32) -> u8 {
    (flags & 0x0F) as u8
}

pub const fn unpack_running_mask(flags: u32) -> u8 {
    ((flags >> 4) & 0x0F) as u8
}

pub const fn unpack_fg_app(flags: u32) -> u8 {
    ((flags >> 8) & 0x0F) as u8
}

pub const fn unpack_tz_hours(flags: u32) -> i32 {
    let raw = ((flags >> 12) & 0xFF) as i32;
    if raw == 0 {
        3
    } else {
        raw - 12
    }
}

pub const fn unpack_installed_mask(flags: u32) -> u8 {
    ((flags >> 20) & 0x0F) as u8
}

pub const fn unpack_brightness(flags: u32) -> u8 {
    let step = ((flags >> 24) & 0x0F) as u8;
    if step < 2 || step > 10 {
        100
    } else {
        step * 10
    }
}

pub const fn unpack_wifi(flags: u32) -> bool {
    (flags >> 28) & 1 != 0
}

pub const fn unpack_bt(flags: u32) -> bool {
    (flags >> 29) & 1 != 0
}

pub const fn unpack_locked(flags: u32) -> bool {
    (flags >> 30) & 1 != 0
}

pub const fn unpack_dark_theme(flags: u32) -> bool {
    (flags >> 31) & 1 != 0
}

pub const PTR_HAS: u32 = 1;
pub const PTR_DOWN: u32 = 2;
pub const PTR_CLICKED: u32 = 4;
pub const PTR_RIGHT_CLICKED: u32 = 8;

pub const RET_NONE: u32 = 0;
pub const RET_REDRAW: u32 = 1;
pub const RET_CLOSE: u32 = 2;
/// Uygulama `SYS_PKG_INSTALL` ile bir paket kurulumu başlattı; çekirdek paketi yeniden doğrulayıp uygulamayı bağlar.
pub const RET_INSTALL: u32 = 4;
/// Uygulama aktif bir geçiş/animasyon yürütüyor; çekirdek bir sonraki kareyi 50 Hz hızında tetikler.
pub const RET_ANIMATING: u32 = 8;

pub const OFF_MAGIC: usize = 0;
pub const OFF_VERSION: usize = 4;
pub const OFF_OP: usize = 8;
pub const OFF_FLAGS: usize = 12;
pub const OFF_SURFACE: usize = 16;
pub const OFF_AREA_X: usize = 24;
pub const OFF_AREA_Y: usize = 28;
pub const OFF_AREA_W: usize = 32;
pub const OFF_AREA_H: usize = 36;
pub const OFF_UNIT: usize = 40;
pub const OFF_UPTIME_MS: usize = 44;
pub const OFF_ACCENT: usize = 48;
pub const OFF_POINTER_FLAGS: usize = 52;
pub const OFF_POINTER_X: usize = 56;
pub const OFF_POINTER_Y: usize = 60;
pub const OFF_KEY: usize = 64;
pub const OFF_LANG: usize = 68;
pub const OFF_HEAP_BASE: usize = 72;
pub const OFF_HEAP_SIZE: usize = 80;
pub const OFF_SEQ: usize = 88;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct EventBlock {
    pub op: u32,
    pub flags: u32,
    pub surface: u64,
    pub area_x: i32,
    pub area_y: i32,
    pub area_w: i32,
    pub area_h: i32,
    pub unit: i32,
    pub uptime_ms: u32,
    pub accent: u32,
    pub pointer_flags: u32,
    pub pointer_x: i32,
    pub pointer_y: i32,
    pub key: u32,
    pub lang: u32,
    pub heap_base: u64,
    pub heap_size: u64,
    pub seq: u64,
}

fn put_u32(out: &mut [u8; EVENT_SIZE], off: usize, v: u32) {
    out[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut [u8; EVENT_SIZE], off: usize, v: u64) {
    out[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn get_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn get_u64(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        b[off], b[off + 1], b[off + 2], b[off + 3], b[off + 4], b[off + 5], b[off + 6], b[off + 7],
    ])
}

impl EventBlock {
    pub fn encode(&self) -> [u8; EVENT_SIZE] {
        let mut out = [0u8; EVENT_SIZE];
        put_u32(&mut out, OFF_MAGIC, EVENT_MAGIC);
        put_u32(&mut out, OFF_VERSION, crate::ABI_VERSION);
        put_u32(&mut out, OFF_OP, self.op);
        put_u32(&mut out, OFF_FLAGS, self.flags);
        put_u64(&mut out, OFF_SURFACE, self.surface);
        put_u32(&mut out, OFF_AREA_X, self.area_x as u32);
        put_u32(&mut out, OFF_AREA_Y, self.area_y as u32);
        put_u32(&mut out, OFF_AREA_W, self.area_w as u32);
        put_u32(&mut out, OFF_AREA_H, self.area_h as u32);
        put_u32(&mut out, OFF_UNIT, self.unit as u32);
        put_u32(&mut out, OFF_UPTIME_MS, self.uptime_ms);
        put_u32(&mut out, OFF_ACCENT, self.accent);
        put_u32(&mut out, OFF_POINTER_FLAGS, self.pointer_flags);
        put_u32(&mut out, OFF_POINTER_X, self.pointer_x as u32);
        put_u32(&mut out, OFF_POINTER_Y, self.pointer_y as u32);
        put_u32(&mut out, OFF_KEY, self.key);
        put_u32(&mut out, OFF_LANG, self.lang);
        put_u64(&mut out, OFF_HEAP_BASE, self.heap_base);
        put_u64(&mut out, OFF_HEAP_SIZE, self.heap_size);
        put_u64(&mut out, OFF_SEQ, self.seq);
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<EventBlock> {
        if bytes.len() < EVENT_SIZE
            || get_u32(bytes, OFF_MAGIC) != EVENT_MAGIC
            || get_u32(bytes, OFF_VERSION) != crate::ABI_VERSION
        {
            return None;
        }
        Some(EventBlock {
            op: get_u32(bytes, OFF_OP),
            flags: get_u32(bytes, OFF_FLAGS),
            surface: get_u64(bytes, OFF_SURFACE),
            area_x: get_u32(bytes, OFF_AREA_X) as i32,
            area_y: get_u32(bytes, OFF_AREA_Y) as i32,
            area_w: get_u32(bytes, OFF_AREA_W) as i32,
            area_h: get_u32(bytes, OFF_AREA_H) as i32,
            unit: get_u32(bytes, OFF_UNIT) as i32,
            uptime_ms: get_u32(bytes, OFF_UPTIME_MS),
            accent: get_u32(bytes, OFF_ACCENT),
            pointer_flags: get_u32(bytes, OFF_POINTER_FLAGS),
            pointer_x: get_u32(bytes, OFF_POINTER_X) as i32,
            pointer_y: get_u32(bytes, OFF_POINTER_Y) as i32,
            key: get_u32(bytes, OFF_KEY),
            lang: get_u32(bytes, OFF_LANG),
            heap_base: get_u64(bytes, OFF_HEAP_BASE),
            heap_size: get_u64(bytes, OFF_HEAP_SIZE),
            seq: get_u64(bytes, OFF_SEQ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> EventBlock {
        EventBlock {
            op: OP_DRAW,
            flags: 0xA5,
            surface: 3,
            area_x: -7,
            area_y: 9,
            area_w: 640,
            area_h: 480,
            unit: 16,
            uptime_ms: 123_456,
            accent: 0x0030_80F0,
            pointer_flags: PTR_HAS | PTR_CLICKED,
            pointer_x: -1,
            pointer_y: 77,
            key: 65,
            lang: 1,
            heap_base: 0x80_0040_0000,
            heap_size: 4 << 20,
            seq: u64::MAX - 1,
        }
    }

    #[test]
    fn roundtrip_and_fixed_offsets() {
        let e = sample();
        let bytes = e.encode();
        assert_eq!(EventBlock::decode(&bytes), Some(e));
        assert_eq!(u32::from_le_bytes(bytes[OFF_OP..OFF_OP + 4].try_into().unwrap()), OP_DRAW);
        assert_eq!(u64::from_le_bytes(bytes[OFF_SURFACE..OFF_SURFACE + 8].try_into().unwrap()), 3);
        assert_eq!(i32::from_le_bytes(bytes[OFF_AREA_X..OFF_AREA_X + 4].try_into().unwrap()), -7);
        assert_eq!(u64::from_le_bytes(bytes[OFF_SEQ..OFF_SEQ + 8].try_into().unwrap()), u64::MAX - 1);
        assert_eq!(OFF_SEQ + 8, EVENT_SIZE);
    }

    #[test]
    fn rejects_bad_header() {
        let mut bytes = sample().encode();
        assert!(EventBlock::decode(&bytes[..EVENT_SIZE - 1]).is_none());
        bytes[0] ^= 1;
        assert!(EventBlock::decode(&bytes).is_none());
        let mut bytes = sample().encode();
        bytes[OFF_VERSION] = 99;
        assert!(EventBlock::decode(&bytes).is_none());
    }

    #[test]
    fn offsets_do_not_overlap() {
        let fields: [(usize, usize); 20] = [
            (OFF_MAGIC, 4), (OFF_VERSION, 4), (OFF_OP, 4), (OFF_FLAGS, 4), (OFF_SURFACE, 8),
            (OFF_AREA_X, 4), (OFF_AREA_Y, 4), (OFF_AREA_W, 4), (OFF_AREA_H, 4), (OFF_UNIT, 4),
            (OFF_UPTIME_MS, 4), (OFF_ACCENT, 4), (OFF_POINTER_FLAGS, 4), (OFF_POINTER_X, 4),
            (OFF_POINTER_Y, 4), (OFF_KEY, 4), (OFF_HEAP_BASE, 8), (OFF_HEAP_SIZE, 8), (OFF_SEQ, 8),
            (OFF_LANG, 4),
        ];
        for (i, a) in fields.iter().enumerate() {
            assert!(a.0 + a.1 <= EVENT_SIZE);
            for b in fields.iter().skip(i + 1) {
                assert!(a.0 + a.1 <= b.0 || b.0 + b.1 <= a.0, "çakışma: {:?} {:?}", a, b);
            }
        }
    }
}
