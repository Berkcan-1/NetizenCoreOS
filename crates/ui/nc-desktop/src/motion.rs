pub const ONE: i32 = 4096;

pub fn clamp01(t: i32) -> i32 {
    t.clamp(0, ONE)
}

pub fn lerp(a: i32, b: i32, t: i32) -> i32 {
    a + (((b - a) as i64) * (t as i64) / (ONE as i64)) as i32
}

pub fn ease_out_cubic(t: i32) -> i32 {
    let u = (ONE - clamp01(t)) as i64;
    ONE - (u * u * u / ((ONE as i64) * (ONE as i64))) as i32
}

pub fn ease_out_quart(t: i32) -> i32 {
    let u = (ONE - clamp01(t)) as i64;
    let one = ONE as i64;
    ONE - (u * u * u * u / (one * one * one)) as i32
}

pub fn ease_in_out_cubic(t: i32) -> i32 {
    let t = clamp01(t) as i64;
    let one = ONE as i64;
    if t < one / 2 {
        (4 * t * t * t / (one * one)) as i32
    } else {
        let u = 2 * one - 2 * t;
        (one - u * u * u / (2 * one * one)) as i32
    }
}

pub fn ease_out_back(t: i32) -> i32 {
    let one = ONE as i64;
    let c1 = one * 9 / 10;
    let c3 = c1 + one;
    let x = clamp01(t) as i64 - one;
    let cube = x * x * x / (one * one);
    let sq = x * x / one;
    (one + c3 * cube / one + c1 * sq / one) as i32
}

pub fn approach(current: i32, target: i32, dt_ms: u32, duration_ms: u32) -> i32 {
    let step = ((dt_ms as i64 * ONE as i64) / (duration_ms.max(1) as i64)).max(1) as i32;
    if current < target {
        (current + step).min(target)
    } else if current > target {
        (current - step).max(target)
    } else {
        current
    }
}

pub fn staggered(t: i32, index: usize, count: usize, span: i32) -> i32 {
    let n = count.max(1) as i64;
    let delay = (span as i64) * (index as i64) / n;
    let window = ((ONE - span).max(ONE / 4)) as i64;
    let local = (t as i64 - delay) * (ONE as i64) / window;
    clamp01(local.clamp(0, ONE as i64) as i32)
}

pub fn scale_u32(value: u32, t: i32) -> u32 {
    (value as u64 * clamp01(t) as u64 / ONE as u64) as u32
}

pub fn scale_rect_about(x: i32, y: i32, w: i32, h: i32, scale: i32, cx: i32, cy: i32) -> (i32, i32, i32, i32) {
    let nw = (w as i64 * scale as i64 / ONE as i64) as i32;
    let nh = (h as i64 * scale as i64 / ONE as i64) as i32;
    let nx = cx + ((x - cx) as i64 * scale as i64 / ONE as i64) as i32;
    let ny = cy + ((y - cy) as i64 * scale as i64 / ONE as i64) as i32;
    (nx, ny, nw, nh)
}
