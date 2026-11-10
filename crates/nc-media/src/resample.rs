use alloc::vec::Vec;

use crate::MediaError;

const MAX_SUPPORT_PIXELS: i64 = 24;

pub type RowFn<'a> = &'a mut dyn FnMut(u32, u32, &mut [u32]) -> Result<(), MediaError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitMode {
    Auto,
    Fill,
    Fit,
}

struct AxisTaps {
    first: Vec<u32>,
    offset: Vec<u32>,
    count: Vec<u32>,
    weights: Vec<u32>,
    max_count: usize,
}

fn build_axis(window: u32, destination: u32) -> Result<AxisTaps, MediaError> {
    if window == 0 || destination == 0 {
        return Err(MediaError::InvalidDimensions);
    }
    let window = window as i64;
    let dst = destination as i64;
    let ratio = window * 256 / dst;
    let support = ratio.max(256).min(MAX_SUPPORT_PIXELS * 256);

    let mut taps = AxisTaps {
        first: Vec::with_capacity(destination as usize),
        offset: Vec::with_capacity(destination as usize),
        count: Vec::with_capacity(destination as usize),
        weights: Vec::new(),
        max_count: 1,
    };

    let mut scratch: Vec<(i64, i64)> = Vec::new();
    let mut index = 0i64;
    while index < dst {
        let center = ((2 * index + 1) * window * 256) / (2 * dst) - 128;
        let low = (center - support + 255).div_euclid(256).max(0);
        let high = (center + support).div_euclid(256).min(window - 1);

        scratch.clear();
        let mut source = low;
        while source <= high {
            let weight = support - (source * 256 - center).abs();
            if weight > 0 {
                scratch.push((source, weight));
            }
            source += 1;
        }
        if scratch.is_empty() {
            let nearest = ((center + 128).div_euclid(256)).max(0).min(window - 1);
            scratch.push((nearest, 1));
        }

        let total: i64 = scratch.iter().map(|(_, w)| *w).sum();
        taps.first.push(scratch[0].0 as u32);
        taps.offset.push(taps.weights.len() as u32);
        taps.count.push(scratch.len() as u32);
        if scratch.len() > taps.max_count {
            taps.max_count = scratch.len();
        }
        let mut sum = 0i64;
        let last = scratch.len() - 1;
        for (position, (_, weight)) in scratch.iter().enumerate() {
            let normalized = if position == last {
                65536 - sum
            } else {
                weight * 65536 / total
            };
            sum += normalized;
            taps.weights.push(normalized.max(0) as u32);
        }
        index += 1;
    }
    Ok(taps)
}

fn reserve<T: Clone>(count: usize, value: T) -> Result<Vec<T>, MediaError> {
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(count).map_err(|_| MediaError::ResourceLimit)?;
    buffer.resize(count, value);
    Ok(buffer)
}

struct CachedRow {
    index: u32,
    stamp: u64,
    valid: bool,
    data: Vec<u32>,
}

pub fn resample_window(
    row: RowFn<'_>,
    window_x: u32,
    window_y: u32,
    window_w: u32,
    window_h: u32,
    dest_w: u32,
    dest_h: u32,
) -> Result<Vec<u32>, MediaError> {
    if window_w == 0 || window_h == 0 || dest_w == 0 || dest_h == 0 {
        return Err(MediaError::InvalidDimensions);
    }
    let total = (dest_w as usize)
        .checked_mul(dest_h as usize)
        .ok_or(MediaError::ResourceLimit)?;

    if window_w == dest_w && window_h == dest_h {
        let mut output = reserve(total, 0u32)?;
        for y in 0..dest_h {
            let start = y as usize * dest_w as usize;
            row(window_y + y, window_x, &mut output[start..start + dest_w as usize])?;
        }
        return Ok(output);
    }

    let xs = build_axis(window_w, dest_w)?;
    let ys = build_axis(window_h, dest_h)?;
    let width = dest_w as usize;

    let mut source_row = reserve(window_w as usize, 0u32)?;
    let slots = ys.max_count + 1;
    let mut cache: Vec<CachedRow> = Vec::new();
    cache.try_reserve_exact(slots).map_err(|_| MediaError::ResourceLimit)?;
    let mut slot_index = 0;
    while slot_index < slots {
        cache.push(CachedRow {
            index: 0,
            stamp: 0,
            valid: false,
            data: reserve(width * 4, 0u32)?,
        });
        slot_index += 1;
    }

    let mut output = reserve(total, 0u32)?;
    let mut accumulator = reserve(width * 4, 0u64)?;
    let mut clock = 0u64;

    for y in 0..dest_h as usize {
        let first = ys.first[y];
        let count = ys.count[y] as usize;
        let offset = ys.offset[y] as usize;

        for tap in 0..count {
            let wanted = first + tap as u32;
            if cache.iter().any(|slot| slot.valid && slot.index == wanted) {
                continue;
            }
            let mut victim = 0usize;
            let mut best = u64::MAX;
            for (position, slot) in cache.iter().enumerate() {
                if !slot.valid {
                    victim = position;
                    break;
                }
                if slot.stamp < best {
                    best = slot.stamp;
                    victim = position;
                }
            }
            row(window_y + wanted, window_x, &mut source_row)?;
            for pixel in source_row.iter_mut() {
                let alpha = *pixel >> 24;
                if alpha != 255 {
                    let r = ((*pixel >> 16) & 255) * alpha / 255;
                    let g = ((*pixel >> 8) & 255) * alpha / 255;
                    let b = (*pixel & 255) * alpha / 255;
                    *pixel = (alpha << 24) | (r << 16) | (g << 8) | b;
                }
            }
            clock += 1;
            let slot = &mut cache[victim];
            slot.valid = true;
            slot.index = wanted;
            slot.stamp = clock;
            for x in 0..width {
                let tap_first = xs.first[x] as usize;
                let tap_count = xs.count[x] as usize;
                let tap_offset = xs.offset[x] as usize;
                let mut a = 0u32;
                let mut r = 0u32;
                let mut g = 0u32;
                let mut b = 0u32;
                let mut k = 0usize;
                while k < tap_count {
                    let weight = xs.weights[tap_offset + k];
                    let pixel = source_row[tap_first + k];
                    a += weight * (pixel >> 24);
                    r += weight * ((pixel >> 16) & 255);
                    g += weight * ((pixel >> 8) & 255);
                    b += weight * (pixel & 255);
                    k += 1;
                }
                slot.data[x * 4] = a >> 8;
                slot.data[x * 4 + 1] = r >> 8;
                slot.data[x * 4 + 2] = g >> 8;
                slot.data[x * 4 + 3] = b >> 8;
            }
        }

        for value in accumulator.iter_mut() {
            *value = 0;
        }
        for tap in 0..count {
            let wanted = first + tap as u32;
            let weight = ys.weights[offset + tap] as u64;
            let slot = match cache.iter().find(|slot| slot.valid && slot.index == wanted) {
                Some(slot) => slot,
                None => return Err(MediaError::DecodeFailed),
            };
            for (target, value) in accumulator.iter_mut().zip(slot.data.iter()) {
                *target += weight * (*value as u64);
            }
        }

        let line = y * width;
        for x in 0..width {
            let a = (((accumulator[x * 4] >> 16) + 128) >> 8).min(255) as u32;
            let mut r = (((accumulator[x * 4 + 1] >> 16) + 128) >> 8).min(255) as u32;
            let mut g = (((accumulator[x * 4 + 2] >> 16) + 128) >> 8).min(255) as u32;
            let mut b = (((accumulator[x * 4 + 3] >> 16) + 128) >> 8).min(255) as u32;
            if a != 255 {
                if a == 0 {
                    r = 0;
                    g = 0;
                    b = 0;
                } else {
                    r = (r * 255 + a / 2) / a;
                    g = (g * 255 + a / 2) / a;
                    b = (b * 255 + a / 2) / a;
                    r = r.min(255);
                    g = g.min(255);
                    b = b.min(255);
                }
            }
            output[line + x] = (a << 24) | (r << 16) | (g << 8) | b;
        }
    }
    Ok(output)
}

fn flatten_over_black(pixels: &mut [u32]) {
    for pixel in pixels.iter_mut() {
        let alpha = *pixel >> 24;
        if alpha == 255 {
            continue;
        }
        let r = ((*pixel >> 16) & 255) * alpha / 255;
        let g = ((*pixel >> 8) & 255) * alpha / 255;
        let b = (*pixel & 255) * alpha / 255;
        *pixel = 0xFF00_0000 | (r << 16) | (g << 8) | b;
    }
}

pub fn compose_screen(
    row: RowFn<'_>,
    source_w: u32,
    source_h: u32,
    screen_w: u32,
    screen_h: u32,
    mode: FitMode,
) -> Result<Vec<u32>, MediaError> {
    if source_w == 0 || source_h == 0 || screen_w == 0 || screen_h == 0 {
        return Err(MediaError::InvalidDimensions);
    }
    let sw = source_w as u64;
    let sh = source_h as u64;
    let dw = screen_w as u64;
    let dh = screen_h as u64;

    let (crop_w, crop_h) = if sw * dh >= sh * dw {
        (((sh * dw) / dh).max(1).min(sw), sh)
    } else {
        (sw, ((sw * dh) / dw).max(1).min(sh))
    };
    let crop_x = (sw - crop_w) / 2;
    let crop_y = (sh - crop_h) / 2;
    let difference = (sw * dh).abs_diff(sh * dw) * 100 / (sh * dw);
    let fill = match mode {
        FitMode::Fill => true,
        FitMode::Fit => false,
        FitMode::Auto => difference <= 12,
    };

    if fill {
        let mut pixels = resample_window(
            row,
            crop_x as u32,
            crop_y as u32,
            crop_w as u32,
            crop_h as u32,
            screen_w,
            screen_h,
        )?;
        flatten_over_black(&mut pixels);
        return Ok(pixels);
    }

    let (mut fit_w, mut fit_h) = if dw * sh <= dh * sw {
        (dw, ((sh * dw) / sw).max(1))
    } else {
        (((sw * dh) / sh).max(1), dh)
    };
    if fit_w > sw * 4 {
        fit_w = sw * 4;
        fit_h = (sh * 4).max(1);
    }
    fit_w = fit_w.min(dw);
    fit_h = fit_h.min(dh);

    let tiny_w = ((screen_w / 32).max(8)).min(screen_w);
    let tiny_h = ((screen_h / 32).max(8)).min(screen_h);
    let tiny = resample_window(
        row,
        crop_x as u32,
        crop_y as u32,
        crop_w as u32,
        crop_h as u32,
        tiny_w,
        tiny_h,
    )?;
    let mut tiny_rows = |y: u32, x0: u32, out: &mut [u32]| -> Result<(), MediaError> {
        let start = y as usize * tiny_w as usize + x0 as usize;
        let slice = tiny.get(start..start + out.len()).ok_or(MediaError::Truncated)?;
        out.copy_from_slice(slice);
        Ok(())
    };
    let mut background = resample_window(&mut tiny_rows, 0, 0, tiny_w, tiny_h, screen_w, screen_h)?;
    for pixel in background.iter_mut() {
        let r = (((*pixel >> 16) & 255) * 150) >> 8;
        let g = (((*pixel >> 8) & 255) * 150) >> 8;
        let b = ((*pixel & 255) * 150) >> 8;
        *pixel = 0xFF00_0000 | (r << 16) | (g << 8) | b;
    }

    let fitted = resample_window(row, 0, 0, source_w, source_h, fit_w as u32, fit_h as u32)?;
    let left = ((dw - fit_w) / 2) as usize;
    let top = ((dh - fit_h) / 2) as usize;
    for y in 0..fit_h as usize {
        for x in 0..fit_w as usize {
            let pixel = fitted[y * fit_w as usize + x];
            let alpha = pixel >> 24;
            let target = (top + y) * screen_w as usize + left + x;
            if alpha == 255 {
                background[target] = pixel;
            } else if alpha > 0 {
                let under = background[target];
                let mix = |s: u32, d: u32| (s * alpha + d * (255 - alpha)) / 255;
                let r = mix((pixel >> 16) & 255, (under >> 16) & 255);
                let g = mix((pixel >> 8) & 255, (under >> 8) & 255);
                let b = mix(pixel & 255, under & 255);
                background[target] = 0xFF00_0000 | (r << 16) | (g << 8) | b;
            }
        }
    }
    Ok(background)
}
