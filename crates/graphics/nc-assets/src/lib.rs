#![no_std]

static LOGO_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo.raw"));

static SOUND_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/startup.snd"));

pub fn startup_sound() -> Option<&'static [u8]> {
    if SOUND_BLOB.len() < 24 || &SOUND_BLOB[0..4] != b"NCSN" {
        return None;
    }
    Some(SOUND_BLOB)
}

static WALLPAPER_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wallpapers.bin"));

pub struct EmbeddedWallpaper {
    pub top: u32,
    pub bottom: u32,
    pub jpeg: &'static [u8],
}

pub fn wallpaper_count() -> usize {
    if WALLPAPER_BLOB.len() < 6 || &WALLPAPER_BLOB[0..4] != b"NCWP" || WALLPAPER_BLOB[4] != 1 {
        return 0;
    }
    WALLPAPER_BLOB[5] as usize
}

pub fn wallpaper(index: usize) -> Option<EmbeddedWallpaper> {
    let count = wallpaper_count();
    if index >= count {
        return None;
    }
    let table = 6 + index * 12;
    let top = read_u32(WALLPAPER_BLOB, table);
    let bottom = read_u32(WALLPAPER_BLOB, table + 4);
    let length = read_u32(WALLPAPER_BLOB, table + 8) as usize;
    let mut offset = 6 + count * 12;
    for earlier in 0..index {
        offset += read_u32(WALLPAPER_BLOB, 6 + earlier * 12 + 8) as usize;
    }
    let jpeg = WALLPAPER_BLOB.get(offset..offset.checked_add(length)?)?;
    Some(EmbeddedWallpaper { top, bottom, jpeg })
}

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
