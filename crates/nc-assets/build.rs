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
        _ => manifest.join("../../assets/logo"),
    };
    println!("cargo:rerun-if-env-changed=NC_LOGO_DIR");
    println!("cargo:rerun-if-changed={}", directory.display());

    let image = find_png(&directory).and_then(|path| decode(&path)).map(fit);
    if image.is_none() {
        println!("cargo:warning=logo png bulunamadi veya okunamadi: {}", directory.display());
    }

    let output = PathBuf::from(env::var("OUT_DIR").unwrap()).join("logo.raw");
    fs::write(output, encode(image)).unwrap();
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
