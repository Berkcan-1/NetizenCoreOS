use alloc::vec;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use nc_app::Icon;
use nc_gfx::{lerp_color, rgb, Canvas, Rect};
use nc_math::{cos_milli, sin_milli};

const MASTER: usize = 256;
const LEVELS: usize = 4;
const KINDS: usize = 7;
const Q: i32 = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ArtKind {
    Folder = 0,
    Globe,
    Gear,
    Laptop,
    Clock,
    App,
    Doc,
}

pub fn kind_for(icon: Icon) -> ArtKind {
    match icon {
        Icon::Folder => ArtKind::Folder,
        Icon::Globe => ArtKind::Globe,
        Icon::Sliders => ArtKind::Gear,
        Icon::Monitor => ArtKind::Laptop,
        Icon::Stopwatch => ArtKind::Clock,
        Icon::Generic => ArtKind::App,
    }
}

#[derive(Clone, Copy)]
enum Paint {
    Solid(u32),
    Vertical { y0: i32, y1: i32, top: u32, bottom: u32 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Over,
    Erase,
    Atop,
}

fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn length(dx: i32, dy: i32) -> i32 {
    let sum = (dx as i64 * dx as i64 + dy as i64 * dy as i64) as u64;
    isqrt(sum) as i32
}

fn q(value: i32) -> i32 {
    value * Q
}

fn rrect(cx: i32, cy: i32, hw: i32, hh: i32, r: i32) -> impl Fn(i32, i32) -> i32 {
    let (cx, cy, hw, hh, r) = (q(cx), q(cy), q(hw), q(hh), q(r));
    move |px, py| {
        let qx = (px - cx).abs() - (hw - r);
        let qy = (py - cy).abs() - (hh - r);
        length(qx.max(0), qy.max(0)) + qx.max(qy).min(0) - r
    }
}

fn circle(cx: i32, cy: i32, r: i32) -> impl Fn(i32, i32) -> i32 {
    let (cx, cy, r) = (q(cx), q(cy), q(r));
    move |px, py| length(px - cx, py - cy) - r
}

fn ring(cx: i32, cy: i32, r: i32, width: i32) -> impl Fn(i32, i32) -> i32 {
    let (cx, cy, r, half) = (q(cx), q(cy), q(r), q(width) / 2);
    move |px, py| (length(px - cx, py - cy) - r).abs() - half
}

fn ellipse_ring(cx: i32, cy: i32, rx: i32, ry: i32, width: i32) -> impl Fn(i32, i32) -> i32 {
    let (cx, cy, rx, ry, half) = (q(cx), q(cy), q(rx).max(1), q(ry).max(1), q(width) / 2);
    move |px, py| {
        let u = ((px - cx) as i64 * ry as i64 / rx as i64) as i32;
        let v = py - cy;
        let d = length(u, v) - ry;
        ((d as i64 * rx.min(ry) as i64 / ry as i64) as i32).abs() - half
    }
}

fn capsule(ax: i32, ay: i32, bx: i32, by: i32, r: i32) -> impl Fn(i32, i32) -> i32 {
    let (ax, ay, bx, by, r) = (q(ax), q(ay), q(bx), q(by), q(r));
    move |px, py| {
        let (pax, pay) = ((px - ax) as i64, (py - ay) as i64);
        let (bax, bay) = ((bx - ax) as i64, (by - ay) as i64);
        let den = bax * bax + bay * bay;
        let h = if den == 0 { 0 } else { (pax * bax + pay * bay).clamp(0, den) * 1024 / den };
        let cx = ax as i64 + bax * h / 1024;
        let cy = ay as i64 + bay * h / 1024;
        length(px - cx as i32, py - cy as i32) - r
    }
}

fn convex(points: &'static [(i32, i32)]) -> impl Fn(i32, i32) -> i32 {
    move |px, py| {
        let count = points.len();
        let (mut sx, mut sy) = (0i64, 0i64);
        for &(x, y) in points {
            sx += q(x) as i64;
            sy += q(y) as i64;
        }
        let (gx, gy) = ((sx / count as i64) as i32, (sy / count as i64) as i32);
        let mut worst = i32::MIN;
        for index in 0..count {
            let (x0, y0) = (q(points[index].0), q(points[index].1));
            let (x1, y1) = (q(points[(index + 1) % count].0), q(points[(index + 1) % count].1));
            let edge = length(x1 - x0, y1 - y0).max(1) as i64;
            let side = |x: i32, y: i32| -> i64 {
                (x as i64 - x0 as i64) * (y1 - y0) as i64 - (y as i64 - y0 as i64) * (x1 - x0) as i64
            };
            let sign = if side(gx, gy) > 0 { -1 } else { 1 };
            let distance = (side(px, py) * sign as i64 / edge) as i32;
            worst = worst.max(distance);
        }
        worst
    }
}

struct Art {
    px: Vec<u32>,
}

impl Art {
    fn new() -> Self {
        Self { px: vec![0; MASTER * MASTER] }
    }

    fn shape<F: Fn(i32, i32) -> i32>(&mut self, bbox: [i32; 4], sdf: F, paint: Paint, alpha: u32, mode: Mode) {
        let x0 = (bbox[0] - 1).max(0) as usize;
        let y0 = (bbox[1] - 1).max(0) as usize;
        let x1 = ((bbox[2] + 1).max(0) as usize).min(MASTER);
        let y1 = ((bbox[3] + 1).max(0) as usize).min(MASTER);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = sdf(x as i32 * Q + Q / 2, y as i32 * Q + Q / 2);
                let coverage = ((Q / 2 - d) * 16).clamp(0, 256) as u32;
                if coverage == 0 {
                    continue;
                }
                let index = y * MASTER + x;
                let dst = self.px[index];
                if mode == Mode::Erase {
                    let keep = 256 - coverage;
                    let ch = |shift: u32| ((((dst >> shift) & 0xFF) * keep) >> 8) << shift;
                    self.px[index] = ch(24) | ch(16) | ch(8) | ch(0);
                    continue;
                }
                let color = match paint {
                    Paint::Solid(c) => c,
                    Paint::Vertical { y0, y1, top, bottom } => {
                        let span = (y1 - y0).max(1);
                        let t = (((y as i32 - y0) * 256) / span).clamp(0, 256) as u32;
                        lerp_color(top, bottom, t)
                    }
                };
                let mut sa = (coverage * alpha.min(255) / 256).min(255);
                let (cr, cg, cb) = ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF);
                let (da, dr, dg, db) = ((dst >> 24) & 0xFF, (dst >> 16) & 0xFF, (dst >> 8) & 0xFF, dst & 0xFF);
                if mode == Mode::Atop {
                    sa = sa * da / 255;
                    let inv_a = 255 - sa;
                    let nr = (dr * inv_a / 255 + cr * sa / 255 * da / 255).min(da);
                    let ng = (dg * inv_a / 255 + cg * sa / 255 * da / 255).min(da);
                    let nb = (db * inv_a / 255 + cb * sa / 255 * da / 255).min(da);
                    self.px[index] = (da << 24) | (nr << 16) | (ng << 8) | nb;
                } else {
                    let inv = 255 - sa;
                    let na = sa + da * inv / 255;
                    let nr = cr * sa / 255 + dr * inv / 255;
                    let ng = cg * sa / 255 + dg * inv / 255;
                    let nb = cb * sa / 255 + db * inv / 255;
                    self.px[index] = (na.min(255) << 24) | (nr.min(255) << 16) | (ng.min(255) << 8) | nb.min(255);
                }
            }
        }
    }

    fn shadow_and_finish(mut self, dy: usize, blur: usize, strength: u32) -> Vec<u32> {
        let n = MASTER;
        let mut mask = vec![0u8; n * n];
        for y in 0..n {
            for x in 0..n {
                mask[y * n + x] = (self.px[y * n + x] >> 24) as u8;
            }
        }
        let mut shifted = vec![0u8; n * n];
        for y in dy..n {
            let from = (y - dy) * n;
            shifted[y * n..y * n + n].copy_from_slice(&mask[from..from + n]);
        }
        let mut scratch = vec![0u8; n * n];
        for _ in 0..3 {
            box_blur(&shifted, &mut scratch, n, blur, true);
            box_blur(&scratch, &mut shifted, n, blur, false);
        }
        for index in 0..n * n {
            let art = self.px[index];
            let art_a = art >> 24;
            let shadow_a = shifted[index] as u32 * strength / 256;
            if shadow_a == 0 {
                continue;
            }
            let inv = 255 - art_a;
            let sh_a = shadow_a * inv / 255;
            let sh_r = 16 * sh_a / 255;
            let sh_g = 22 * sh_a / 255;
            let sh_b = 44 * sh_a / 255;
            let a = (art_a + sh_a).min(255);
            let r = (((art >> 16) & 0xFF) + sh_r).min(255);
            let g = (((art >> 8) & 0xFF) + sh_g).min(255);
            let b = ((art & 0xFF) + sh_b).min(255);
            self.px[index] = (a << 24) | (r << 16) | (g << 8) | b;
        }
        self.px
    }
}

fn box_blur(src: &[u8], dst: &mut [u8], n: usize, radius: usize, horizontal: bool) {
    let window = (radius * 2 + 1) as u32;
    for line in 0..n {
        let mut sum: u32 = 0;
        let get = |i: isize| -> u32 {
            if i < 0 || i >= n as isize {
                0
            } else if horizontal {
                src[line * n + i as usize] as u32
            } else {
                src[i as usize * n + line] as u32
            }
        };
        for i in -(radius as isize)..=(radius as isize) {
            sum += get(i);
        }
        for i in 0..n {
            let value = (sum / window) as u8;
            if horizontal {
                dst[line * n + i] = value;
            } else {
                dst[i * n + line] = value;
            }
            sum += get(i as isize + radius as isize + 1);
            sum -= get(i as isize - radius as isize);
        }
    }
}

fn build_mips(master: Vec<u32>) -> [Vec<u32>; LEVELS] {
    let mut levels: [Vec<u32>; LEVELS] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    levels[0] = master;
    for level in 1..LEVELS {
        let src_n = MASTER >> (level - 1);
        let n = src_n / 2;
        let mut out = vec![0u32; n * n];
        for y in 0..n {
            for x in 0..n {
                let mut acc = [0u32; 4];
                for (ox, oy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = levels[level - 1][(y * 2 + oy) * src_n + x * 2 + ox];
                    acc[0] += p >> 24;
                    acc[1] += (p >> 16) & 0xFF;
                    acc[2] += (p >> 8) & 0xFF;
                    acc[3] += p & 0xFF;
                }
                out[y * n + x] = ((acc[0] / 4) << 24) | ((acc[1] / 4) << 16) | ((acc[2] / 4) << 8) | (acc[3] / 4);
            }
        }
        levels[level] = out;
    }
    levels
}

const FULL: [i32; 4] = [0, 0, 256, 256];

fn vg(y0: i32, y1: i32, top: u32, bottom: u32) -> Paint {
    Paint::Vertical { y0, y1, top, bottom }
}

fn gloss(art: &mut Art, bbox: [i32; 4], shape: impl Fn(i32, i32) -> i32, strength: u32) {
    art.shape(bbox, shape, vg(bbox[1], bbox[3], rgb(255, 255, 255), rgb(255, 255, 255)), strength, Mode::Atop);
}

fn paint_folder() -> Art {
    let mut a = Art::new();
    a.shape([36, 44, 130, 100], rrect(83, 74, 47, 26, 12), vg(44, 100, rgb(70, 132, 232), rgb(46, 104, 214)), 255, Mode::Over);
    a.shape([28, 62, 228, 214], rrect(128, 138, 100, 76, 22), vg(62, 214, rgb(92, 160, 252), rgb(48, 110, 226)), 255, Mode::Over);
    a.shape([48, 74, 208, 124], rrect(128, 99, 80, 25, 8), vg(74, 124, rgb(255, 255, 255), rgb(226, 234, 248)), 240, Mode::Over);
    a.shape([28, 96, 228, 214], rrect(128, 155, 100, 59, 22), vg(96, 214, rgb(150, 206, 255), rgb(66, 136, 246)), 255, Mode::Over);
    gloss(&mut a, [28, 96, 228, 150], rrect(128, 123, 100, 27, 22), 70);
    a.shape([30, 98, 226, 104], rrect(128, 101, 96, 3, 3), Paint::Solid(rgb(255, 255, 255)), 120, Mode::Atop);
    a
}

fn paint_globe() -> Art {
    let mut a = Art::new();
    a.shape([32, 32, 224, 224], circle(128, 128, 94), vg(34, 222, rgb(122, 208, 255), rgb(36, 108, 232)), 255, Mode::Over);
    for rx in [34, 66] {
        a.shape([34, 34, 222, 222], ellipse_ring(128, 128, rx, 94, 5), Paint::Solid(rgb(255, 255, 255)), 120, Mode::Atop);
    }
    a.shape([34, 34, 222, 222], capsule(128, 34, 128, 222, 2), Paint::Solid(rgb(255, 255, 255)), 120, Mode::Atop);
    for (y, half) in [(128, 94), (88, 86), (168, 86), (56, 58), (200, 58)] {
        a.shape([34, y - 4, 222, y + 4], capsule(128 - half, y, 128 + half, y, 2), Paint::Solid(rgb(255, 255, 255)), 110, Mode::Atop);
    }
    a.shape([34, 34, 222, 222], ring(128, 128, 93, 4), Paint::Solid(rgb(20, 70, 170)), 130, Mode::Atop);
    gloss(&mut a, [40, 34, 216, 130], ellipse_ring(128, 78, 70, 44, 90), 60);
    a
}

fn paint_gear() -> Art {
    let mut a = Art::new();
    let silver = vg(28, 228, rgb(232, 236, 244), rgb(132, 140, 158));
    for step in 0..12 {
        let deg = step * 30;
        let (c, s) = (cos_milli(deg), sin_milli(deg));
        let (x0, y0) = (128 + c * 66 / 1000, 128 + s * 66 / 1000);
        let (x1, y1) = (128 + c * 93 / 1000, 128 + s * 93 / 1000);
        a.shape(FULL, capsule(x0, y0, x1, y1, 11), silver, 255, Mode::Over);
    }
    a.shape(FULL, circle(128, 128, 76), silver, 255, Mode::Over);
    a.shape(FULL, ring(128, 128, 70, 3), Paint::Solid(rgb(255, 255, 255)), 150, Mode::Atop);
    a.shape(FULL, circle(128, 128, 38), vg(90, 166, rgb(92, 100, 118), rgb(44, 50, 64)), 255, Mode::Over);
    a.shape(FULL, circle(128, 128, 20), vg(108, 148, rgb(206, 212, 224), rgb(150, 158, 174)), 255, Mode::Over);
    gloss(&mut a, [30, 30, 226, 120], ellipse_ring(128, 70, 80, 40, 80), 50);
    a
}

fn paint_laptop() -> Art {
    let mut a = Art::new();
    a.shape(FULL, rrect(128, 112, 96, 62, 14), vg(50, 174, rgb(74, 80, 96), rgb(30, 34, 44)), 255, Mode::Over);
    a.shape(FULL, rrect(128, 112, 88, 54, 8), vg(58, 166, rgb(84, 170, 255), rgb(132, 96, 240)), 255, Mode::Over);
    gloss(&mut a, [40, 58, 216, 112], rrect(128, 85, 88, 27, 8), 46);
    a.shape(FULL, circle(128, 56, 2), Paint::Solid(rgb(20, 22, 28)), 255, Mode::Over);
    a.shape(FULL, rrect(128, 190, 112, 11, 8), vg(180, 202, rgb(226, 230, 238), rgb(150, 158, 174)), 255, Mode::Over);
    a.shape(FULL, rrect(128, 184, 18, 3, 3), Paint::Solid(rgb(110, 118, 134)), 200, Mode::Over);
    a
}

fn paint_clock() -> Art {
    let mut a = Art::new();
    a.shape(FULL, rrect(128, 42, 16, 12, 5), vg(30, 54, rgb(190, 196, 208), rgb(130, 138, 154)), 255, Mode::Over);
    a.shape(FULL, circle(128, 142, 88), vg(54, 230, rgb(255, 176, 84), rgb(236, 88, 52)), 255, Mode::Over);
    a.shape(FULL, circle(128, 142, 76), vg(66, 218, rgb(255, 255, 255), rgb(226, 230, 240)), 255, Mode::Over);
    for tick in 0..12 {
        let deg = tick * 30;
        let (c, s) = (cos_milli(deg), sin_milli(deg));
        let (inner, outer) = if tick % 3 == 0 { (56, 68) } else { (62, 68) };
        a.shape(FULL, capsule(128 + c * inner / 1000, 142 + s * inner / 1000, 128 + c * outer / 1000, 142 + s * outer / 1000, 2), Paint::Solid(rgb(60, 66, 82)), 230, Mode::Over);
    }
    a.shape(FULL, capsule(128, 142, 128 + 38, 142 - 30, 4), Paint::Solid(rgb(52, 58, 74)), 255, Mode::Over);
    a.shape(FULL, capsule(128, 142, 128 - 8, 142 - 52, 3), Paint::Solid(rgb(236, 88, 52)), 255, Mode::Over);
    a.shape(FULL, circle(128, 142, 7), Paint::Solid(rgb(236, 88, 52)), 255, Mode::Over);
    gloss(&mut a, [44, 56, 212, 140], ellipse_ring(128, 100, 66, 36, 70), 46);
    a
}

fn paint_app() -> Art {
    let mut a = Art::new();
    a.shape(FULL, rrect(128, 128, 92, 82, 26), vg(46, 210, rgb(206, 214, 232), rgb(148, 160, 190)), 255, Mode::Over);
    a.shape([36, 46, 220, 82], rrect(128, 64, 92, 18, 18), vg(46, 82, rgb(110, 124, 156), rgb(84, 96, 128)), 255, Mode::Atop);
    for (i, c) in [rgb(255, 110, 100), rgb(255, 196, 72), rgb(90, 214, 120)].iter().enumerate() {
        a.shape(FULL, circle(62 + i as i32 * 18, 64, 5), Paint::Solid(*c), 255, Mode::Over);
    }
    a.shape(FULL, rrect(100, 112, 40, 6, 6), Paint::Solid(rgb(255, 255, 255)), 200, Mode::Over);
    a.shape(FULL, rrect(112, 136, 52, 6, 6), Paint::Solid(rgb(255, 255, 255)), 150, Mode::Over);
    a.shape(FULL, rrect(92, 160, 32, 6, 6), Paint::Solid(rgb(255, 255, 255)), 150, Mode::Over);
    gloss(&mut a, [36, 46, 220, 120], rrect(128, 83, 92, 37, 26), 38);
    a
}

static DOC_FOLD: [(i32, i32); 3] = [(150, 30), (204, 84), (150, 84)];

fn paint_doc() -> Art {
    let mut a = Art::new();
    a.shape(FULL, rrect(128, 128, 74, 98, 14), vg(30, 226, rgb(255, 255, 255), rgb(216, 224, 240)), 255, Mode::Over);
    a.shape([150, 30, 206, 86], rrect(180, 58, 30, 30, 12), Paint::Solid(rgb(0, 0, 0)), 255, Mode::Erase);
    a.shape([148, 28, 206, 86], convex(&DOC_FOLD), vg(30, 86, rgb(196, 206, 228), rgb(160, 174, 204)), 255, Mode::Over);
    for (i, w) in [46, 52, 36, 48].iter().enumerate() {
        a.shape(FULL, rrect(84 + *w / 2, 118 + i as i32 * 24, *w / 2, 4, 4), Paint::Solid(rgb(120, 136, 172)), 200, Mode::Over);
    }
    a
}

struct Slot {
    lock: AtomicBool,
    value: UnsafeCell<Option<[Option<[Vec<u32>; LEVELS]>; KINDS]>>,
}

unsafe impl Sync for Slot {}

static CACHE: Slot = Slot { lock: AtomicBool::new(false), value: UnsafeCell::new(None) };

fn with_cache<R>(run: impl FnOnce(&mut [Option<[Vec<u32>; LEVELS]>; KINDS]) -> R) -> R {
    while CACHE.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let slot = unsafe { &mut *CACHE.value.get() };
    let cache = slot.get_or_insert_with(|| [None, None, None, None, None, None, None]);
    let result = run(cache);
    CACHE.lock.store(false, Ordering::Release);
    result
}

fn render(kind: ArtKind) -> [Vec<u32>; LEVELS] {
    let art = match kind {
        ArtKind::Folder => paint_folder(),
        ArtKind::Globe => paint_globe(),
        ArtKind::Gear => paint_gear(),
        ArtKind::Laptop => paint_laptop(),
        ArtKind::Clock => paint_clock(),
        ArtKind::App => paint_app(),
        ArtKind::Doc => paint_doc(),
    };
    build_mips(art.shadow_and_finish(7, 5, 110))
}

pub fn art_rect_for_tile(tile: Rect) -> Rect {
    let grow = tile.width * 40 / 216;
    Rect::new(tile.x - grow / 2, tile.y - grow / 2 - tile.height / 24, tile.width + grow, tile.height + grow)
}

pub fn draw_art(canvas: &mut Canvas<'_>, kind: ArtKind, rect: Rect, alpha: u32) {
    if rect.width < 4 || rect.height < 4 || alpha == 0 {
        return;
    }
    with_cache(|cache| {
        let slot = &mut cache[kind as usize];
        if slot.is_none() {
            *slot = Some(render(kind));
        }
        let levels = slot.as_ref().unwrap();
        let mut level = 0;
        while level + 1 < LEVELS && (MASTER >> (level + 1)) as i32 >= rect.width {
            level += 1;
        }
        let n = MASTER >> level;
        blit(canvas, &levels[level], n, rect, alpha.min(256));
    });
}

fn blit(canvas: &mut Canvas<'_>, source: &[u32], n: usize, rect: Rect, alpha: u32) {
    let clip = canvas.clip_rect();
    let width = canvas.width();
    let x_from = rect.x.max(clip.x);
    let x_to = (rect.x + rect.width).min(clip.x + clip.width);
    let y_from = rect.y.max(clip.y);
    let y_to = (rect.y + rect.height).min(clip.y + clip.height);
    if x_from >= x_to || y_from >= y_to {
        return;
    }
    let pixels = canvas.pixels_mut();
    let max = n as i64 - 1;
    for y in y_from..y_to {
        let v = ((((y - rect.y) as i64 * 2 + 1) * n as i64) << 15) / rect.height as i64 - (1 << 15);
        let (y0, fy) = ((v >> 16).clamp(0, max), ((v & 0xFFFF) >> 8) as u32);
        let y1 = (y0 + 1).min(max);
        for x in x_from..x_to {
            let u = ((((x - rect.x) as i64 * 2 + 1) * n as i64) << 15) / rect.width as i64 - (1 << 15);
            let (x0, fx) = ((u >> 16).clamp(0, max), ((u & 0xFFFF) >> 8) as u32);
            let x1 = (x0 + 1).min(max);
            let t = |xx: i64, yy: i64| source[yy as usize * n + xx as usize];
            let (p00, p10, p01, p11) = (t(x0, y0), t(x1, y0), t(x0, y1), t(x1, y1));
            let mix = |shift: u32| -> u32 {
                let c = |p: u32| (p >> shift) & 0xFF;
                let top = c(p00) * (256 - fx) + c(p10) * fx;
                let bottom = c(p01) * (256 - fx) + c(p11) * fx;
                (top * (256 - fy) + bottom * fy) >> 16
            };
            let sa = mix(24) * alpha / 256;
            if sa == 0 {
                continue;
            }
            let (sr, sg, sb) = (mix(16) * alpha / 256, mix(8) * alpha / 256, mix(0) * alpha / 256);
            let index = y as usize * width + x as usize;
            let dst = pixels[index];
            let inv = 255 - sa.min(255);
            let r = (sr + ((dst >> 16) & 0xFF) * inv / 255).min(255);
            let g = (sg + ((dst >> 8) & 0xFF) * inv / 255).min(255);
            let b = (sb + (dst & 0xFF) * inv / 255).min(255);
            pixels[index] = (r << 16) | (g << 8) | b;
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    const KINDS_ALL: [ArtKind; KINDS] = [
        ArtKind::Folder,
        ArtKind::Globe,
        ArtKind::Gear,
        ArtKind::Laptop,
        ArtKind::Clock,
        ArtKind::App,
        ArtKind::Doc,
    ];

    #[test]
    fn every_icon_has_content_and_transparent_corners() {
        for kind in KINDS_ALL {
            let levels = render(kind);
            let master = &levels[0];
            let covered = master.iter().filter(|p| (**p >> 24) > 200).count();
            assert!(covered > 12_000, "ikon cok bos: {}", covered);
            assert!(covered < 52_000, "ikon cok dolu: {}", covered);
            assert_eq!(master[0] >> 24, 0);
            assert_eq!(master[MASTER * MASTER - 1] >> 24, 0);
            for p in master.iter() {
                let a = p >> 24;
                assert!(((p >> 16) & 0xFF) <= a.max(1) + 1 && ((p >> 8) & 0xFF) <= a.max(1) + 1 && (p & 0xFF) <= a.max(1) + 1, "premultiplied bozuk");
            }
        }
    }

    #[test]
    fn dump_icon_sheet_when_requested() {
        let Ok(path) = std::env::var("NC_ART_DUMP") else { return; };
        let (w, h) = (7 * 280usize, 560usize);
        let mut pixels = vec![0u32; w * h];
        for y in 0..h {
            for x in 0..w {
                let t = (y * 255 / h) as u32;
                pixels[y * w + x] = if y < h / 2 { lerp_color(rgb(236, 240, 247), rgb(214, 222, 236), t * 2 / 2) } else { lerp_color(rgb(34, 38, 48), rgb(18, 20, 28), t) };
            }
        }
        let mut canvas = Canvas::new(&mut pixels, w, h).unwrap();
        for (i, kind) in KINDS_ALL.iter().enumerate() {
            draw_art(&mut canvas, *kind, Rect::new(i as i32 * 280 + 12, 12, 256, 256), 256);
            draw_art(&mut canvas, *kind, Rect::new(i as i32 * 280 + 12, 292, 256, 256), 256);
            draw_art(&mut canvas, *kind, Rect::new(i as i32 * 280 + 100, 200, 64, 64), 256);
            draw_art(&mut canvas, *kind, Rect::new(i as i32 * 280 + 180, 220, 48, 48), 256);
            draw_art(&mut canvas, *kind, Rect::new(i as i32 * 280 + 100, 480, 64, 64), 256);
            draw_art(&mut canvas, *kind, Rect::new(i as i32 * 280 + 180, 500, 32, 32), 256);
        }
        let mut out = std::format!("P6\n{} {}\n255\n", w, h).into_bytes();
        for p in &pixels {
            out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        std::fs::write(path, out).unwrap();
    }
}
