use std::env;
use std::fs;
use std::path::{Path, PathBuf};

struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

// Mobil simge adları: sıra = kimlik - 1 (nc-abi::draw::ICON_* ile aynı sırada).
// Mevcut 1..=7 kimlikleri korunur; 8..=12 yeni sistem uygulamalari icin sona eklenir.
const ICON_NAMES: [&str; 12] = [
    "ic_files",
    "ic_settings",
    "ic_browser",
    "ic_system",
    "ic_stopwatch",
    "ic_generic",
    "ic_keyboard",
    "ic_gallery",
    "ic_messages",
    "ic_phone",
    "ic_contacts",
    "ic_camera",
];
const ICON_SIZES: [u32; 4] = [48, 96, 144, 192];
const LOGO_MAX_SIDE: u32 = 160;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let icon_dir = match env::var("NC_MOBILE_ICON_DIR") {
        Ok(value) if !value.is_empty() => PathBuf::from(value),
        _ => manifest.join("../../assets/icons"),
    };
    println!("cargo:rerun-if-env-changed=NC_MOBILE_ICON_DIR");
    println!("cargo:rerun-if-changed={}", icon_dir.display());
    for name in ICON_NAMES {
        println!("cargo:rerun-if-changed={}", icon_dir.join(format!("{}.png", name)).display());
    }
    let logo_dir = match env::var("NC_MOBILE_LOGO_DIR") {
        Ok(value) if !value.is_empty() => PathBuf::from(value),
        _ => manifest.join("../../assets/logo"),
    };
    println!("cargo:rerun-if-env-changed=NC_MOBILE_LOGO_DIR");
    println!("cargo:rerun-if-changed={}", logo_dir.display());
    let raw_logo = find_png(&logo_dir).and_then(|p| decode(&p));
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("icons.bin"), build_icons(&icon_dir, raw_logo.as_ref())).unwrap();
    let logo_img = raw_logo.map(fit_logo);
    fs::write(out_dir.join("logo.raw"), encode_logo(logo_img)).unwrap();
}

fn find_png(directory: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(directory)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("png")).unwrap_or(false))
        .collect();
    files.sort();
    files.into_iter().next()
}

fn fit_logo(image: Image) -> Image {
    let longest = image.width.max(image.height);
    if longest <= LOGO_MAX_SIDE {
        return image;
    }
    let new_w = ((image.width as u64 * LOGO_MAX_SIDE as u64) / longest as u64).max(1) as u32;
    let new_h = ((image.height as u64 * LOGO_MAX_SIDE as u64) / longest as u64).max(1) as u32;
    let mut rgba = Vec::with_capacity((new_w * new_h * 4) as usize);
    for dy in 0..new_h {
        let sy0 = (dy as u64 * image.height as u64 / new_h as u64) as u32;
        let sy1 = (((dy as u64 + 1) * image.height as u64 + new_h as u64 - 1) / new_h as u64) as u32;
        let sy1 = sy1.max(sy0 + 1).min(image.height);
        for dx in 0..new_w {
            let sx0 = (dx as u64 * image.width as u64 / new_w as u64) as u32;
            let sx1 = (((dx as u64 + 1) * image.width as u64 + new_w as u64 - 1) / new_w as u64) as u32;
            let sx1 = sx1.max(sx0 + 1).min(image.width);
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let i = (sy as usize * image.width as usize + sx as usize) * 4;
                    let alpha = image.rgba[i + 3] as u64;
                    r += image.rgba[i] as u64 * alpha;
                    g += image.rgba[i + 1] as u64 * alpha;
                    b += image.rgba[i + 2] as u64 * alpha;
                    a += alpha;
                    n += 1;
                }
            }
            if a == 0 || n == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                rgba.push((r / a) as u8);
                rgba.push((g / a) as u8);
                rgba.push((b / a) as u8);
                rgba.push((a / n) as u8);
            }
        }
    }
    Image { width: new_w, height: new_h, rgba }
}

fn encode_logo(image: Option<Image>) -> Vec<u8> {
    let mut blob = Vec::new();
    blob.extend_from_slice(b"NCLG");
    match image {
        Some(img) => {
            blob.extend_from_slice(&img.width.to_le_bytes());
            blob.extend_from_slice(&img.height.to_le_bytes());
            blob.extend_from_slice(&img.rgba);
        }
        None => {
            blob.extend_from_slice(&0u32.to_le_bytes());
            blob.extend_from_slice(&0u32.to_le_bytes());
        }
    }
    blob
}

/// Kaynak PNG'yi (kare olmayansa ortadan kırpılır) `side` x `side` boyutuna kutu süzgeciyle küçültür, köşelerini
/// kenar yumuşatmalı yuvarlar ve önceden çarpılmış ARGB olarak döndürür.
fn render_icon(image: &Image, side: u32) -> Vec<u32> {
    let crop = image.width.min(image.height);
    let ox = (image.width - crop) / 2;
    let oy = (image.height - crop) / 2;
    let half = side as f32 / 2.0;
    let radius = side as f32 * 0.27;
    let mut out = Vec::with_capacity((side * side) as usize);
    for y in 0..side {
        let sy0 = oy + (y as u64 * crop as u64 / side as u64) as u32;
        let sy1 = (oy + ((y + 1) as u64 * crop as u64 / side as u64) as u32).max(sy0 + 1).min(oy + crop);
        for x in 0..side {
            let sx0 = ox + (x as u64 * crop as u64 / side as u64) as u32;
            let sx1 = (ox + ((x + 1) as u64 * crop as u64 / side as u64) as u32).max(sx0 + 1).min(ox + crop);
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let i = ((sy * image.width + sx) * 4) as usize;
                    let alpha = image.rgba[i + 3] as u64;
                    r += image.rgba[i] as u64 * alpha;
                    g += image.rgba[i + 1] as u64 * alpha;
                    b += image.rgba[i + 2] as u64 * alpha;
                    a += alpha;
                    n += 1;
                }
            }
            let n = n.max(1);
            let alpha_avg = a as f32 / n as f32;
            let (pr, pg, pb) = if a == 0 {
                (0.0, 0.0, 0.0)
            } else {
                (r as f32 / (n as f32 * 255.0), g as f32 / (n as f32 * 255.0), b as f32 / (n as f32 * 255.0))
            };
            let px = (x as f32 + 0.5 - half).abs();
            let py = (y as f32 + 0.5 - half).abs();
            let qx = px - (half - radius);
            let qy = py - (half - radius);
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
            let coverage = (0.5 - outside).clamp(0.0, 1.0);
            let alpha = alpha_avg / 255.0 * coverage;
            let to8 = |v: f32| (v.clamp(0.0, 255.0) + 0.5) as u32;
            let (ar, ag, ab) = (to8(pr * coverage), to8(pg * coverage), to8(pb * coverage));
            let aa = to8(alpha * 255.0);
            out.push((aa << 24) | (ar.min(aa) << 16) | (ag.min(aa) << 8) | ab.min(aa));
        }
    }
    out
}

/// Genel NetizenCore uygulama simgesi: varsa logo siluetini koyu arduvaz zemin uzerine ortalar,
/// yoksa temiz geometrik NetizenCore (NC) amblemi uretir.
fn synthesize_generic_icon(logo: Option<&Image>) -> Image {
    let side = 192u32;
    let mut rgba = vec![0u8; (side * side * 4) as usize];
    for y in 0..side {
        let t = y as f32 / (side - 1) as f32;
        let r = (24.0 + t * 14.0) as u8;
        let g = (38.0 + t * 44.0) as u8;
        let b = (68.0 + t * 110.0) as u8;
        for x in 0..side {
            let i = ((y * side + x) * 4) as usize;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }
    if let Some(src) = logo {
        let target = 112u32;
        let longest = src.width.max(src.height).max(1);
        let dw = ((src.width as u64 * target as u64) / longest as u64).max(1) as u32;
        let dh = ((src.height as u64 * target as u64) / longest as u64).max(1) as u32;
        let ox = (side - dw) / 2;
        let oy = (side - dh) / 2;
        for dy in 0..dh {
            let sy = (dy as u64 * src.height as u64 / dh as u64).min(src.height as u64 - 1) as u32;
            for dx in 0..dw {
                let sx = (dx as u64 * src.width as u64 / dw as u64).min(src.width as u64 - 1) as u32;
                let si = ((sy * src.width + sx) * 4) as usize;
                let a = src.rgba[si + 3] as u32;
                if a == 0 {
                    continue;
                }
                let di = (((oy + dy) * side + (ox + dx)) * 4) as usize;
                let inv = 255 - a;
                rgba[di] = ((src.rgba[si] as u32 * a + rgba[di] as u32 * inv) / 255) as u8;
                rgba[di + 1] = ((src.rgba[si + 1] as u32 * a + rgba[di + 1] as u32 * inv) / 255) as u8;
                rgba[di + 2] = ((src.rgba[si + 2] as u32 * a + rgba[di + 2] as u32 * inv) / 255) as u8;
            }
        }
    } else {
        // Minimal NC geometrik markasi (beyaz halka + ic nokta)
        let c = side as f32 / 2.0;
        for y in 0..side {
            for x in 0..side {
                let dx = x as f32 + 0.5 - c;
                let dy = y as f32 + 0.5 - c;
                let dist = (dx * dx + dy * dy).sqrt();
                if (dist >= 34.0 && dist <= 48.0) || dist <= 14.0 {
                    let i = ((y * side + x) * 4) as usize;
                    rgba[i] = 248;
                    rgba[i + 1] = 250;
                    rgba[i + 2] = 252;
                }
            }
        }
    }
    Image { width: side, height: side, rgba }
}

/// "NCIC": başlık [sihir, sürüm, simge sayısı, boyut sayısı], boyut listesi, (ofset, boyut) tablosu, piksel verisi.
fn build_icons(directory: &Path, logo: Option<&Image>) -> Vec<u8> {
    let count = ICON_NAMES.len();
    let mips = ICON_SIZES.len();
    let header_words = 4 + mips + count * mips * 2;
    let mut table = vec![0u32; count * mips * 2];
    let mut data: Vec<u32> = Vec::new();
    for (index, name) in ICON_NAMES.iter().enumerate() {
        let path = directory.join(format!("{}.png", name));
        let image = match decode(&path) {
            Some(img) => img,
            None if *name == "ic_generic" => synthesize_generic_icon(logo),
            None => continue,
        };
        for (m, &side) in ICON_SIZES.iter().enumerate() {
            let pixels = render_icon(&image, side);
            let offset = header_words + data.len();
            data.extend_from_slice(&pixels);
            table[(index * mips + m) * 2] = offset as u32;
            table[(index * mips + m) * 2 + 1] = side;
        }
    }
    let mut words: Vec<u32> = vec![0x4349_434E, 1, count as u32, mips as u32];
    words.extend_from_slice(&ICON_SIZES);
    words.extend_from_slice(&table);
    words.extend_from_slice(&data);
    let mut out = Vec::with_capacity(words.len() * 4);
    for w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

fn decode(path: &Path) -> Option<Image> {
    let file = fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    let data = &buffer[..info.buffer_size()];
    let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(data),
        png::ColorType::Rgb => {
            for pixel in data.chunks_exact(3) {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for pixel in data.chunks_exact(2) {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
        }
        png::ColorType::Grayscale => {
            for value in data {
                rgba.extend_from_slice(&[*value, *value, *value, 255]);
            }
        }
        png::ColorType::Indexed => return None,
    }
    Some(Image { width: info.width, height: info.height, rgba })
}
