use nc_app::theme::{WALLPAPERS, WALLPAPER_COUNT};
use nc_gfx::{lerp_color, rgb, Canvas};
use nc_math::{cos_milli, sin_milli};

use crate::paint::soft_sphere;
use crate::theme::lighten;

fn noise(x: u32, y: u32) -> i32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 7) as i32 - 3
}

pub fn draw_wallpaper(canvas: &mut Canvas<'_>, index: usize) {
    canvas.reset_clip();
    let width = canvas.width() as i32;
    let height = canvas.height() as i32;
    let wallpaper = &WALLPAPERS[index % WALLPAPER_COUNT];
    let c0 = wallpaper.top;
    let c1 = wallpaper.bottom;
    let mid = lighten(lerp_color(c0, c1, 128), 24);
    let w = width.max(1) as u64;
    let h = height.max(1) as u64;
    let stride = width as usize;
    {
        let px = canvas.pixels_mut();
        for y in 0..height as usize {
            let ty = y as u64 * 153 / h;
            for x in 0..stride {
                let t = (x as u64 * 102 / w + ty).min(255) as u32;
                px[y * stride + x] = if t < 128 {
                    lerp_color(c0, mid, t * 2)
                } else {
                    lerp_color(mid, c1, (t - 128) * 2)
                };
            }
        }
    }

    soft_sphere(
        canvas,
        width * 20 / 100,
        height * 30 / 100,
        height * 46 / 100,
        lighten(c0, 170),
        lighten(c0, 60),
        150,
    );
    soft_sphere(
        canvas,
        width * 82 / 100,
        height * 68 / 100,
        height * 58 / 100,
        lighten(c1, 150),
        lighten(c1, 50),
        150,
    );
    soft_sphere(
        canvas,
        width * 56 / 100,
        height * 4 / 100,
        height * 32 / 100,
        lighten(lerp_color(c0, c1, 140), 190),
        lighten(lerp_color(c0, c1, 140), 80),
        110,
    );

    for glow in wallpaper.glows.iter() {
        canvas.blend_glow(
            width * glow.x / 100,
            height * glow.y / 100,
            height * glow.radius / 100,
            glow.color,
            glow.strength,
        );
    }
    canvas.blend_glow(
        width / 2,
        height * 46 / 100,
        height * 38 / 100,
        rgb(198, 244, 255),
        44,
    );

    let px = canvas.pixels_mut();
    for y in 0..height as usize {
        for x in 0..stride {
            let p = px[y * stride + x];
            let n = noise(x as u32, y as u32);
            let r = (((p >> 16) & 0xFF) as i32 + n).clamp(0, 255) as u32;
            let g = (((p >> 8) & 0xFF) as i32 + n).clamp(0, 255) as u32;
            let b = ((p & 0xFF) as i32 + n).clamp(0, 255) as u32;
            px[y * stride + x] = (r << 16) | (g << 8) | b;
        }
    }
}

pub fn draw_motion(canvas: &mut Canvas<'_>, uptime_ms: u32, speed: u32, index: usize) {
    let w = canvas.width() as i32;
    let h = canvas.height() as i32;
    let wallpaper = &WALLPAPERS[index % WALLPAPER_COUNT];
    let speed = speed.max(1);
    let deg1 = ((uptime_ms / (24 / speed).max(4)) % 360) as i32;
    let deg2 = ((uptime_ms / (30 / speed).max(5) + 140) % 360) as i32;

    let ox1 = w * 26 / 100 + (cos_milli(deg1) * (w / 9) / 1000);
    let oy1 = h * 32 / 100 + (sin_milli(deg1) * (h / 10) / 1000);
    soft_sphere(
        canvas,
        ox1,
        oy1,
        h * 18 / 100,
        lighten(wallpaper.top, 180),
        lighten(wallpaper.top, 70),
        62,
    );

    let ox2 = w * 74 / 100 + (sin_milli(deg2) * (w / 8) / 1000);
    let oy2 = h * 64 / 100 + (cos_milli(deg2) * (h / 9) / 1000);
    soft_sphere(
        canvas,
        ox2,
        oy2,
        h * 21 / 100,
        lighten(wallpaper.bottom, 170),
        lighten(wallpaper.bottom, 60),
        58,
    );
}
