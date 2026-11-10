use crate::{Canvas, Rect};

pub fn lerp_packed(from: u32, to: u32, t: u32) -> u32 {
    let t = t.min(256);
    let red_blue =
        (((from & 0x00FF_00FF) * (256 - t) + (to & 0x00FF_00FF) * t) >> 8) & 0x00FF_00FF;
    let green = (((from & 0x0000_FF00) * (256 - t) + (to & 0x0000_FF00) * t) >> 8) & 0x0000_FF00;
    red_blue | green
}

fn scale_packed(color: u32, factor: u32) -> u32 {
    let red_blue = (((color & 0x00FF_00FF) * factor) >> 8) & 0x00FF_00FF;
    let green = (((color & 0x0000_FF00) * factor) >> 8) & 0x0000_FF00;
    red_blue | green
}

fn multiply_channel(base: u32, ink: u32) -> u32 {
    (base * (256 - ink)) >> 8
}

fn multiply_packed(base: u32, ink: u32) -> u32 {
    (multiply_channel((base >> 16) & 0xFF, (ink >> 16) & 0xFF) << 16)
        | (multiply_channel((base >> 8) & 0xFF, (ink >> 8) & 0xFF) << 8)
        | multiply_channel(base & 0xFF, ink & 0xFF)
}

pub fn compose_ink(
    destination: &mut Canvas<'_>,
    destination_x: i32,
    destination_y: i32,
    base: &Canvas<'_>,
    ink_a: &Canvas<'_>,
    ink_b: &Canvas<'_>,
    mix: u32,
    opacity: u32,
    area: Rect,
) {
    let mix = mix.min(256);
    let opacity = opacity.min(256);
    let stage_width = base.width() as i32;
    let stage_height = base.height() as i32;
    let destination_width = destination.width() as i32;
    let destination_height = destination.height() as i32;
    let x_start = area.x.max(0);
    let y_start = area.y.max(0);
    let x_end = (area.x + area.width).min(stage_width);
    let y_end = (area.y + area.height).min(stage_height);
    let base_pixels = base.pixels();
    let a_pixels = ink_a.pixels();
    let b_pixels = ink_b.pixels();
    let output = destination.pixels_mut();
    for y in y_start..y_end {
        let target_y = destination_y + y;
        if target_y < 0 || target_y >= destination_height {
            continue;
        }
        for x in x_start..x_end {
            let target_x = destination_x + x;
            if target_x < 0 || target_x >= destination_width {
                continue;
            }
            let index = (y * stage_width + x) as usize;
            let mut ink = a_pixels[index];
            if mix != 0 {
                ink = lerp_packed(ink, b_pixels[index], mix);
            }
            if opacity != 256 {
                ink = scale_packed(ink, opacity);
            }
            output[(target_y * destination_width + target_x) as usize] =
                multiply_packed(base_pixels[index], ink);
        }
    }
}

pub fn lerp_canvases(destination: &mut Canvas<'_>, from: &Canvas<'_>, to: &Canvas<'_>, t: u32) {
    let from_pixels = from.pixels();
    let to_pixels = to.pixels();
    let output = destination.pixels_mut();
    let count = output.len().min(from_pixels.len()).min(to_pixels.len());
    for index in 0..count {
        output[index] = lerp_packed(from_pixels[index], to_pixels[index], t);
    }
}
