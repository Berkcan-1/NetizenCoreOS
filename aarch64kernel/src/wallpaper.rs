use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::addr_of_mut;

use nc_fs::{FileSystem, FileType};
use nc_gfx::Canvas;
use nc_media::{decode_jpeg_wallpaper, decode_png_wallpaper, inspect, FitMode, MediaKind};

use crate::storage;

const MEDIA_DIR: &str = "/media";
const ASSET_WALLPAPER_DIR: &str = "/assets/wallpapers";
const MAX_NAME_BYTES: usize = 40;
const MAX_FILE_BYTES: usize = 12 << 20;
const MAX_DECODE_PIXELS: u64 = 3_000_000;
pub const MAX_CANDIDATES: usize = 6;
pub const PRESET_COUNT: usize = 5;
pub const PRESET_FILES: [&str; PRESET_COUNT] = [
    "wallpaper_01.png",
    "wallpaper_02.png",
    "wallpaper_03.png",
    "wallpaper_04.png",
    "wallpaper_05.png",
];
const RGB_MASK: u32 = 0x00FF_FFFF;

struct State {
    name: Option<String>,
    pixels: Vec<u32>,
}

struct PresetCache {
    index: u8,
    width: usize,
    height: usize,
    attempted: bool,
    pixels: Vec<u32>,
}

static mut GENERATION: u32 = 0;

/// Duvar kağıdı her değiştiğinde artar (çekirdek, aynı kareyi atlamadan önce bunu da imzaya katar).
pub fn generation() -> u32 {
    unsafe { *core::ptr::addr_of!(GENERATION) }
}

fn bump() {
    unsafe {
        *core::ptr::addr_of_mut!(GENERATION) = (*core::ptr::addr_of!(GENERATION)).wrapping_add(1);
    }
}

static mut STATE: State = State { name: None, pixels: Vec::new() };
static mut PRESET: PresetCache = PresetCache {
    index: 0xFF,
    width: 0,
    height: 0,
    attempted: false,
    pixels: Vec::new(),
};

fn state() -> &'static mut State {
    unsafe { &mut *addr_of_mut!(STATE) }
}

fn preset_cache() -> &'static mut PresetCache {
    unsafe { &mut *addr_of_mut!(PRESET) }
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && !name.contains(['/', '\\'])
        && !name.starts_with('.')
        && name.chars().all(|c| !c.is_control())
}

fn is_image_name(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((_, ext)) => {
            ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg")
        }
        None => false,
    }
}

pub fn candidates() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let Some(fs) = storage::data_fs() else { return names };
    let Ok(entries) = fs.list_dir(MEDIA_DIR) else { return names };
    for entry in entries.iter() {
        if !matches!(entry.file_type, FileType::RegularFile) {
            continue;
        }
        let name: &str = &entry.name;
        if !valid_name(name) || !is_image_name(name) {
            continue;
        }
        if entry.size == 0 || entry.size > MAX_FILE_BYTES as u64 {
            continue;
        }
        names.push(String::from(name));
    }
    names.sort();
    names.truncate(MAX_CANDIDATES);
    names
}

fn decode_bytes(bytes: &[u8], width: usize, height: usize) -> Option<Vec<u32>> {
    if bytes.is_empty() || bytes.len() > MAX_FILE_BYTES {
        return None;
    }
    let info = inspect(bytes).ok()?;
    let (sw, sh) = (info.width? as u64, info.height? as u64);
    if sw == 0 || sh == 0 || sw.saturating_mul(sh) > MAX_DECODE_PIXELS {
        return None;
    }
    let raster = match info.kind {
        MediaKind::Png => decode_png_wallpaper(bytes, width as u32, height as u32, FitMode::Fill),
        MediaKind::Jpeg => decode_jpeg_wallpaper(bytes, width as u32, height as u32, FitMode::Fill),
        _ => return None,
    }
    .ok()?;
    if raster.width as usize != width || raster.height as usize != height {
        return None;
    }
    let mut pixels = raster.pixels_argb;
    for pixel in pixels.iter_mut() {
        *pixel &= RGB_MASK;
    }
    Some(pixels)
}

fn decode_data_media(name: &str, width: usize, height: usize) -> Option<Vec<u32>> {
    if !valid_name(name) {
        return None;
    }
    let fs = storage::data_fs()?;
    let mut path = String::from(MEDIA_DIR);
    path.push('/');
    path.push_str(name);
    let bytes = fs.read_file(&path).ok()?;
    decode_bytes(&bytes, width, height)
}

fn decode_system_preset(index: u8, width: usize, height: usize) -> Option<Vec<u32>> {
    let fs = storage::system_fs()?;
    let preferred = (index as usize).min(PRESET_COUNT - 1);

    // Önce istenen wallpaper_0N.png dosyasını dene, yoksa ilk mevcut PNG duvar kağıdını yükle.
    for step in 0..PRESET_COUNT {
        let slot = (preferred + step) % PRESET_COUNT;
        let mut path = String::from(ASSET_WALLPAPER_DIR);
        path.push('/');
        path.push_str(PRESET_FILES[slot]);
        if let Ok(bytes) = fs.read_file(&path) {
            if let Some(pixels) = decode_bytes(&bytes, width, height) {
                return Some(pixels);
            }
        }
    }
    None
}

pub fn set(name: &str, width: usize, height: usize) -> bool {
    if !candidates().iter().any(|candidate| candidate == name) {
        return false;
    }
    match decode_data_media(name, width, height) {
        Some(pixels) => {
            let current = state();
            current.name = Some(String::from(name));
            current.pixels = pixels;
            bump();
            true
        }
        None => false,
    }
}

pub fn restore_pending(name: &str) {
    if valid_name(name) {
        let current = state();
        current.name = Some(String::from(name));
        current.pixels = Vec::new();
        bump();
    }
}

pub fn clear() {
    let current = state();
    current.name = None;
    current.pixels = Vec::new();
    bump();
}

pub fn current_name() -> Option<String> {
    state().name.clone()
}

pub fn has_image() -> bool {
    state().name.is_some()
}

/// `/data/media/` altındaki kullanıcı seçimi duvar kağıdını ekrana aktarır.
pub fn blit(canvas: &mut Canvas<'_>) -> bool {
    let width = canvas.width();
    let height = canvas.height();
    let current = state();
    if current.pixels.len() != width * height {
        let Some(name) = current.name.clone() else { return false };
        match decode_data_media(&name, width, height) {
            Some(pixels) => current.pixels = pixels,
            None => return false,
        }
    }
    canvas.pixels_mut().copy_from_slice(&current.pixels);
    true
}

/// `/system/assets/wallpapers/wallpaper_01..05.png` duvar kağıdını önbellekten (veya ilk seferde çözerek) ekrana aktarır.
pub fn blit_preset(index: u8, canvas: &mut Canvas<'_>) -> bool {
    let width = canvas.width();
    let height = canvas.height();
    let idx = index.min((PRESET_COUNT - 1) as u8);
    let cache = preset_cache();
    if cache.index != idx || cache.width != width || cache.height != height || !cache.attempted {
        cache.index = idx;
        cache.width = width;
        cache.height = height;
        cache.attempted = true;
        cache.pixels = decode_system_preset(idx, width, height).unwrap_or_default();
        bump();
    }
    if cache.pixels.len() != width * height {
        return false;
    }
    canvas.pixels_mut().copy_from_slice(&cache.pixels);
    true
}
