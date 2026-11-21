//! ARMv8-A mobil uygulama simgeleri (`aarch64crates/assets/icons/*.png`).
#![no_std]

#[repr(C, align(4))]
struct Align4<T: ?Sized>(T);

static ICON_BLOB: &Align4<[u8]> = &Align4(*include_bytes!(concat!(env!("OUT_DIR"), "/icons.bin")));
static LOGO_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo.raw"));

pub struct Logo {
    pub width: u32,
    pub height: u32,
    pub pixels: &'static [u8],
}

pub fn logo() -> Option<Logo> {
    let blob = LOGO_BLOB;
    if blob.len() < 12 || &blob[0..4] != b"NCLG" {
        return None;
    }
    let width = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]);
    let height = u32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]);
    let needed = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
    if width == 0 || height == 0 || blob.len() < 12 + needed {
        return None;
    }
    Some(Logo { width, height, pixels: &blob[12..12 + needed] })
}

/// Bir simgenin seçilen boyuttaki, önceden çarpılmış ARGB (0xAARRGGBB) piksel kümesi.
pub struct Icon {
    pub size: u32,
    pub pixels: &'static [u32],
}

fn icon_word(index: usize) -> Option<u32> {
    let bytes = &ICON_BLOB.0;
    let at = index.checked_mul(4)?;
    let chunk = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
}

/// `id`: 1'den başlar (`nc_abi::draw::ICON_*`). `want`: hedef kenar uzunluğu (piksel); istenenden küçük olmayan
/// en küçük hazır boyut seçilir (yoksa en büyüğü).
/// Özel PNG henüz sağlanmamışsa güvenli olarak genel simgeye (`ICON_GENERIC = 6`) düşer.
pub fn icon(id: u32, want: u32) -> Option<Icon> {
    if let Some(found) = icon_exact(id, want) {
        return Some(found);
    }
    const GENERIC_ID: u32 = 6;
    if id != GENERIC_ID {
        return icon_exact(GENERIC_ID, want);
    }
    None
}

fn icon_exact(id: u32, want: u32) -> Option<Icon> {
    if icon_word(0)? != 0x4349_434E || icon_word(1)? != 1 {
        return None;
    }
    let count = icon_word(2)? as usize;
    let mips = icon_word(3)? as usize;
    if id == 0 || id as usize > count || mips == 0 || mips > 8 {
        return None;
    }
    let table = 4 + mips;
    let entry = |m: usize| -> Option<(usize, u32)> {
        let slot = table + ((id as usize - 1) * mips + m) * 2;
        let offset = icon_word(slot)? as usize;
        let size = icon_word(slot + 1)?;
        if size == 0 { None } else { Some((offset, size)) }
    };
    let mut best: Option<(usize, u32)> = None;
    for m in 0..mips {
        let Some((offset, size)) = entry(m) else { continue };
        let better = match best {
            None => true,
            Some((_, cur)) => (size >= want && (cur < want || size < cur)) || (cur < want && size > cur),
        };
        if better {
            best = Some((offset, size));
        }
    }
    let (offset, size) = best?;
    let count_px = (size as usize) * (size as usize);
    let bytes = &ICON_BLOB.0;
    let start = offset.checked_mul(4)?;
    let end = start.checked_add(count_px.checked_mul(4)?)?;
    if end > bytes.len() {
        return None;
    }
    let pixels = unsafe { core::slice::from_raw_parts(bytes.as_ptr().add(start) as *const u32, count_px) };
    Some(Icon { size, pixels })
}
