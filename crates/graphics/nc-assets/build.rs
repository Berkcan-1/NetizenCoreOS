use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_SIDE: u32 = 192;

struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let directory = match env::var("NC_LOGO_DIR") {
        Ok(value) if !value.is_empty() => PathBuf::from(value),
        _ => manifest.join("../../../assets/logo"),
    };
    println!("cargo:rerun-if-env-changed=NC_LOGO_DIR");
    println!("cargo:rerun-if-changed={}", directory.display());

    let image = find_png(&directory).and_then(|path| decode(&path)).map(fit);
    if image.is_none() {
        println!("cargo:warning=logo png bulunamadi veya okunamadi: {}", directory.display());
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("logo.raw"), encode(image)).unwrap();

    let sound_dir = match env::var("NC_SOUND_DIR") {
        Ok(value) if !value.is_empty() => PathBuf::from(value),
        _ => manifest.join("../../../assets/sound"),
    };
    println!("cargo:rerun-if-env-changed=NC_SOUND_DIR");
    println!("cargo:rerun-if-changed={}", sound_dir.display());
    let sound = find_startup_wav(&sound_dir).and_then(|path| fs::read(path).ok()).and_then(|bytes| parse_wav(&bytes));
    if sound.is_none() {
        println!("cargo:warning=baslangic sesi (wav, PCM16) bulunamadi veya okunamadi: {}", sound_dir.display());
    }
    fs::write(out_dir.join("startup.snd"), encode_sound(sound)).unwrap();

    let wallpaper_dir = match env::var("NC_WALLPAPER_DIR") {
        Ok(value) if !value.is_empty() => PathBuf::from(value),
        _ => manifest.join("../../../assets/wallpapers"),
    };
    println!("cargo:rerun-if-env-changed=NC_WALLPAPER_DIR");
    println!("cargo:rerun-if-changed={}", wallpaper_dir.display());
    let wallpapers = build_wallpapers(&wallpaper_dir);
    if wallpapers.len() < WALLPAPER_MAX {
        println!(
            "cargo:warning=assets/wallpapers icinde {} resim bulundu (en fazla {}); eksik kalanlar duz gradyan olarak cizilir",
            wallpapers.len(),
            WALLPAPER_MAX
        );
    }
    fs::write(out_dir.join("wallpapers.bin"), encode_wallpapers(&wallpapers)).unwrap();
}

const WALLPAPER_MAX: usize = 6;
#[allow(dead_code)]
const WALLPAPER_SIDE_W: u32 = 1920;
#[allow(dead_code)]
const WALLPAPER_SIDE_H: u32 = 1080;

struct Wallpaper {
    top: u32,
    bottom: u32,
    jpeg: Vec<u8>,
}

fn list_wallpapers(directory: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = match fs::read_dir(directory) {
        Ok(entries) => entries.filter_map(|entry| entry.ok()).map(|entry| entry.path()).collect(),
        Err(_) => return Vec::new(),
    };
    files.retain(|path| {
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| {
                extension.eq_ignore_ascii_case("png") || extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg")
            })
            .unwrap_or(false)
    });
    files.sort();
    files.truncate(WALLPAPER_MAX);
    files
}

fn load_rgb(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let is_png = path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("png")).unwrap_or(false);
    if is_png {
        let image = decode(path)?;
        let mut rgb = Vec::with_capacity(image.rgba.len() / 4 * 3);
        for pixel in image.rgba.chunks_exact(4) {
            rgb.extend_from_slice(&pixel[0..3]);
        }
        return Some((image.width, image.height, rgb));
    }
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let data = fs::read(path).ok()?;
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(&data), options);
    let pixels = decoder.decode().ok()?;
    let (width, height) = decoder.dimensions()?;
    if pixels.len() != width * height * 3 {
        return None;
    }
    Some((width as u32, height as u32, pixels))
}

fn shrink(width: u32, height: u32, rgb: &[u8], max_w: u32, max_h: u32) -> (u32, u32, Vec<u8>) {
    if width <= max_w && height <= max_h {
        return (width, height, rgb.to_vec());
    }
    let scale = (max_w as f64 / width as f64).min(max_h as f64 / height as f64);
    let new_w = ((width as f64 * scale).round() as u32).max(1);
    let new_h = ((height as f64 * scale).round() as u32).max(1);
    let mut out = vec![0u8; (new_w * new_h * 3) as usize];
    for y in 0..new_h {
        let sy0 = (y as u64 * height as u64 / new_h as u64) as u32;
        let sy1 = (((y + 1) as u64 * height as u64 / new_h as u64) as u32).max(sy0 + 1).min(height);
        for x in 0..new_w {
            let sx0 = (x as u64 * width as u64 / new_w as u64) as u32;
            let sx1 = (((x + 1) as u64 * width as u64 / new_w as u64) as u32).max(sx0 + 1).min(width);
            let mut sum = [0u64; 3];
            let mut count = 0u64;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let index = ((sy * width + sx) * 3) as usize;
                    sum[0] += rgb[index] as u64;
                    sum[1] += rgb[index + 1] as u64;
                    sum[2] += rgb[index + 2] as u64;
                    count += 1;
                }
            }
            let index = ((y * new_w + x) * 3) as usize;
            for channel in 0..3 {
                out[index + channel] = (sum[channel] / count.max(1)) as u8;
            }
        }
    }
    (new_w, new_h, out)
}

fn average(width: u32, height: u32, rgb: &[u8], from_y: u32, to_y: u32) -> u32 {
    let mut sum = [0u64; 3];
    let mut count = 0u64;
    for y in from_y..to_y.min(height) {
        for x in (0..width).step_by(4) {
            let index = ((y * width + x) * 3) as usize;
            sum[0] += rgb[index] as u64;
            sum[1] += rgb[index + 1] as u64;
            sum[2] += rgb[index + 2] as u64;
            count += 1;
        }
    }
    let count = count.max(1);
    (((sum[0] / count) as u32) << 16) | (((sum[1] / count) as u32) << 8) | (sum[2] / count) as u32
}

const WALLPAPER_BUDGET: usize = 2_200_000;
const WALLPAPER_STEPS: [(u32, u32, u8); 5] = [(1920, 1080, 88), (1920, 1080, 78), (1600, 900, 76), (1366, 768, 72), (1280, 720, 66)];

fn build_wallpapers(directory: &Path) -> Vec<Wallpaper> {
    let mut sources = Vec::new();
    for path in list_wallpapers(directory) {
        match load_rgb(&path) {
            Some(image) => sources.push(image),
            None => println!("cargo:warning=duvar kagidi okunamadi: {}", path.display()),
        }
    }
    let mut result = Vec::new();
    for (step_index, (max_w, max_h, quality)) in WALLPAPER_STEPS.iter().enumerate() {
        let mut attempt = Vec::new();
        let mut total = 0usize;
        for (width, height, rgb) in sources.iter() {
            let (w, h, pixels) = shrink(*width, *height, rgb, *max_w, *max_h);
            let top = average(w, h, &pixels, 0, h * 2 / 5);
            let bottom = average(w, h, &pixels, h * 3 / 5, h);
            let mut jpeg = Vec::new();
            let encoder = jpeg_encoder::Encoder::new(&mut jpeg, *quality);
            if encoder.encode(&pixels, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).is_err() {
                continue;
            }
            total += jpeg.len();
            attempt.push(Wallpaper { top, bottom, jpeg });
        }
        result = attempt;
        if total <= WALLPAPER_BUDGET {
            break;
        }
        if step_index + 1 == WALLPAPER_STEPS.len() {
            println!("cargo:warning=duvar kagitlari {} bayt; cekirdek siniri icin kucuk resim kullanin", total);
        }
    }
    result
}

fn encode_wallpapers(items: &[Wallpaper]) -> Vec<u8> {
    let mut blob = Vec::new();
    blob.extend_from_slice(b"NCWP");
    blob.push(1);
    blob.push(items.len() as u8);
    for item in items {
        blob.extend_from_slice(&item.top.to_le_bytes());
        blob.extend_from_slice(&item.bottom.to_le_bytes());
        blob.extend_from_slice(&(item.jpeg.len() as u32).to_le_bytes());
    }
    for item in items {
        blob.extend_from_slice(&item.jpeg);
    }
    blob
}

struct Wav {
    rate: u32,
    left: Vec<i16>,
    right: Vec<i16>,
}

fn find_startup_wav(directory: &Path) -> Option<PathBuf> {
    let preferred = directory.join("startup.wav");
    if preferred.is_file() {
        return Some(preferred);
    }
    let mut files: Vec<PathBuf> = fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("wav"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    files.into_iter().next()
}

fn parse_wav(bytes: &[u8]) -> Option<Wav> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut cursor = 12usize;
    let mut format: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    while cursor + 8 <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let size = u32::from_le_bytes([bytes[cursor + 4], bytes[cursor + 5], bytes[cursor + 6], bytes[cursor + 7]]) as usize;
        let body_start = cursor + 8;
        let body_end = body_start.saturating_add(size).min(bytes.len());
        if id == b"fmt " && body_end - body_start >= 16 {
            let body = &bytes[body_start..body_end];
            format = Some((
                u16::from_le_bytes([body[0], body[1]]),
                u16::from_le_bytes([body[2], body[3]]),
                u32::from_le_bytes([body[4], body[5], body[6], body[7]]),
                u16::from_le_bytes([body[14], body[15]]),
            ));
        } else if id == b"data" {
            data = Some(&bytes[body_start..body_end]);
        }
        cursor = body_start.saturating_add(size).saturating_add(size & 1);
    }
    let (tag, channels, rate, bits) = format?;
    let data = data?;
    if tag != 1 || bits != 16 || !(channels == 1 || channels == 2) || rate < 8_000 || rate > 96_000 {
        return None;
    }
    let frame_bytes = channels as usize * 2;
    let frames = data.len() / frame_bytes;
    if frames == 0 {
        return None;
    }
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for frame in data.chunks_exact(frame_bytes) {
        let l = i16::from_le_bytes([frame[0], frame[1]]);
        let r = if channels == 2 { i16::from_le_bytes([frame[2], frame[3]]) } else { l };
        left.push(l);
        right.push(r);
    }
    Some(Wav { rate, left, right })
}

const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73,
    80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494,
    544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499,
    2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442,
    11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const INDEX_TABLE: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

fn encode_nibble(sample: i32, predictor: &mut i32, index: &mut i32) -> u8 {
    let mut step = STEP_TABLE[*index as usize];
    let mut diff = sample - *predictor;
    let negative = diff < 0;
    if negative {
        diff = -diff;
    }
    let mut nibble = 0u8;
    let mut delta = step >> 3;
    if diff >= step {
        nibble |= 4;
        diff -= step;
        delta += step;
    }
    step >>= 1;
    if diff >= step {
        nibble |= 2;
        diff -= step;
        delta += step;
    }
    step >>= 1;
    if diff >= step {
        nibble |= 1;
        delta += step;
    }
    if negative {
        *predictor -= delta;
        nibble |= 8;
    } else {
        *predictor += delta;
    }
    *predictor = (*predictor).clamp(-32768, 32767);
    *index = (*index + INDEX_TABLE[nibble as usize]).clamp(0, 88);
    nibble
}

fn encode_sound(sound: Option<Wav>) -> Vec<u8> {
    let Some(wav) = sound else { return Vec::new(); };
    let peak = wav.left.iter().chain(wav.right.iter()).map(|v| (*v as i32).abs()).max().unwrap_or(0).max(1);
    let gain = (22_000.0f64 / peak as f64).min(3.0);
    let scale = |v: i16| -> i32 { ((v as f64) * gain).round().clamp(-32768.0, 32767.0) as i32 };
    let frames = wav.left.len();
    let mut blob = Vec::with_capacity(24 + frames);
    blob.extend_from_slice(b"NCSN");
    blob.push(1);
    blob.push(2);
    blob.extend_from_slice(&[0, 0]);
    blob.extend_from_slice(&wav.rate.to_le_bytes());
    blob.extend_from_slice(&(frames as u32).to_le_bytes());
    let first_l = scale(wav.left[0]);
    let first_r = scale(wav.right[0]);
    blob.extend_from_slice(&(first_l as i16).to_le_bytes());
    blob.push(0);
    blob.push(0);
    blob.extend_from_slice(&(first_r as i16).to_le_bytes());
    blob.push(0);
    blob.push(0);
    let (mut pred_l, mut idx_l) = (first_l, 0i32);
    let (mut pred_r, mut idx_r) = (first_r, 0i32);
    for frame in 0..frames {
        let l = encode_nibble(scale(wav.left[frame]), &mut pred_l, &mut idx_l);
        let r = encode_nibble(scale(wav.right[frame]), &mut pred_r, &mut idx_r);
        blob.push(l | (r << 4));
    }
    blob
}

fn find_png(directory: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    files.into_iter().next()
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

fn fit(image: Image) -> Image {
    let longest = image.width.max(image.height);
    if longest <= MAX_SIDE {
        return image;
    }
    let new_width = ((image.width as u64 * MAX_SIDE as u64) / longest as u64).max(1) as u32;
    let new_height = ((image.height as u64 * MAX_SIDE as u64) / longest as u64).max(1) as u32;
    let mut rgba = Vec::with_capacity(new_width as usize * new_height as usize * 4);
    for dy in 0..new_height {
        let sy0 = (dy as u64 * image.height as u64 / new_height as u64) as u32;
        let sy1 = (((dy as u64 + 1) * image.height as u64 + new_height as u64 - 1) / new_height as u64) as u32;
        let sy1 = sy1.max(sy0 + 1).min(image.height);
        for dx in 0..new_width {
            let sx0 = (dx as u64 * image.width as u64 / new_width as u64) as u32;
            let sx1 = (((dx as u64 + 1) * image.width as u64 + new_width as u64 - 1) / new_width as u64) as u32;
            let sx1 = sx1.max(sx0 + 1).min(image.width);
            let mut sum_red = 0u64;
            let mut sum_green = 0u64;
            let mut sum_blue = 0u64;
            let mut sum_alpha = 0u64;
            let mut count = 0u64;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let index = (sy as usize * image.width as usize + sx as usize) * 4;
                    let alpha = image.rgba[index + 3] as u64;
                    sum_red += image.rgba[index] as u64 * alpha;
                    sum_green += image.rgba[index + 1] as u64 * alpha;
                    sum_blue += image.rgba[index + 2] as u64 * alpha;
                    sum_alpha += alpha;
                    count += 1;
                }
            }
            if sum_alpha == 0 || count == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                rgba.push((sum_red / sum_alpha) as u8);
                rgba.push((sum_green / sum_alpha) as u8);
                rgba.push((sum_blue / sum_alpha) as u8);
                rgba.push((sum_alpha / count) as u8);
            }
        }
    }
    Image { width: new_width, height: new_height, rgba }
}

fn encode(image: Option<Image>) -> Vec<u8> {
    let mut blob = Vec::new();
    blob.extend_from_slice(b"NCLG");
    match image {
        Some(image) => {
            blob.extend_from_slice(&image.width.to_le_bytes());
            blob.extend_from_slice(&image.height.to_le_bytes());
            blob.extend_from_slice(&image.rgba);
        }
        None => {
            blob.extend_from_slice(&0u32.to_le_bytes());
            blob.extend_from_slice(&0u32.to_le_bytes());
        }
    }
    blob
}
