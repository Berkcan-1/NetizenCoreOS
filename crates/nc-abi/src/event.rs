//! `EventBlock`: çekirdeğin her olayda `USER_EVENT_PAGE`'e yazdığı 96 baytlık, SABİT düzenli blok.
//! Düzen elle (little-endian) kodlanır; `repr(C)` varsayımına dayanmaz.

pub const EVENT_MAGIC: u32 = 0x5645_434E; // "NCEV"
pub const EVENT_SIZE: usize = 96;

pub const OP_INIT: u32 = 0;
pub const OP_UPDATE: u32 = 1;
pub const OP_DRAW: u32 = 2;
pub const OP_CLOSE: u32 = 3;

pub const PTR_HAS: u32 = 1;
pub const PTR_DOWN: u32 = 2;
pub const PTR_CLICKED: u32 = 4;
pub const PTR_RIGHT_CLICKED: u32 = 8;

pub const RET_NONE: u32 = 0;
pub const RET_REDRAW: u32 = 1;
pub const RET_CLOSE: u32 = 2;
/// Uygulama `SYS_PKG_INSTALL` ile bir paket kurulumu başlattı; çekirdek paketi yeniden doğrulayıp uygulamayı bağlar.
pub const RET_INSTALL: u32 = 4;

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
