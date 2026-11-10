#![no_std]

static LOGO_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo.raw"));

pub struct Logo {
    pub width: u32,
    pub height: u32,
    pub pixels: &'static [u8],
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
}

pub fn logo() -> Option<Logo> {
    let blob = LOGO_BLOB;
    if blob.len() < 12 || blob[0] != b'N' || blob[1] != b'C' || blob[2] != b'L' || blob[3] != b'G' {
        return None;
    }
    let width = read_u32(blob, 4);
    let height = read_u32(blob, 8);
    let needed = width as usize * height as usize * 4;
    if width == 0 || height == 0 || blob.len() < 12 + needed {
        return None;
    }
    Some(Logo { width, height, pixels: &blob[12..12 + needed] })
}
