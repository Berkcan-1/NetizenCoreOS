use crate::data::{Entry, OPS, PARTS};

const MAX_SEGMENTS: usize = 192;
const MAX_KNOTS: usize = 24;

#[derive(Clone, Copy)]
enum Op {
    Move(i32, i32),
    Line(i32, i32),
    Quad(i32, i32, i32, i32),
    Cubic(i32, i32, i32, i32, i32, i32),
}

fn walk(entry: &Entry, visit: &mut dyn FnMut(Op)) {
    let first = entry.first_part as usize;
    let last = first + entry.part_count as usize;
    for part in &PARTS[first..last] {
        let dx = part.dx as i32;
        let dy = part.dy as i32;
        let end = part.start as usize + part.len as usize;
        let mut index = part.start as usize;
        while index < end {
            let code = OPS[index];
            if code == 1 {
                visit(Op::Move(OPS[index + 1] as i32 + dx, OPS[index + 2] as i32 + dy));
                index += 3;
            } else if code == 2 {
                visit(Op::Line(OPS[index + 1] as i32 + dx, OPS[index + 2] as i32 + dy));
                index += 3;
            } else if code == 3 {
                visit(Op::Quad(
                    OPS[index + 1] as i32 + dx,
                    OPS[index + 2] as i32 + dy,
                    OPS[index + 3] as i32 + dx,
                    OPS[index + 4] as i32 + dy,
                ));
                index += 5;
            } else if code == 4 {
                visit(Op::Cubic(
                    OPS[index + 1] as i32 + dx,
                    OPS[index + 2] as i32 + dy,
                    OPS[index + 3] as i32 + dx,
                    OPS[index + 4] as i32 + dy,
                    OPS[index + 5] as i32 + dx,
                    OPS[index + 6] as i32 + dy,
                ));
                index += 7;
            } else {
                index = end;
            }
        }
    }
}

fn integer_sqrt(value: u64) -> u64 {
    let mut remaining = value;
    let mut result: u64 = 0;
    let mut bit: u64 = 1 << 62;
    while bit > value {
        bit >>= 2;
    }
    while bit != 0 {
        if remaining >= result + bit {
            remaining -= result + bit;
            result = (result >> 1) + bit;
        } else {
            result >>= 1;
        }
        bit >>= 2;
    }
    result
}

#[derive(Clone, Copy)]
struct Axis {
    count: usize,
    unit: [i32; MAX_KNOTS],
    snapped: [i64; MAX_KNOTS],
    base: i64,
    scale: i64,
    grid: i64,
}

impl Axis {
    fn new(base: i64, scale: i64, grid: i64) -> Axis {
        Axis { count: 0, unit: [0; MAX_KNOTS], snapped: [0; MAX_KNOTS], base, scale, grid }
    }

    fn add(&mut self, value: i32) {
        let mut position = 0;
        while position < self.count && self.unit[position] < value {
            position += 1;
        }
        if position < self.count && self.unit[position] == value {
            return;
        }
        if self.count == MAX_KNOTS {
            return;
        }
        let mut shift = self.count;
        while shift > position {
            self.unit[shift] = self.unit[shift - 1];
            shift -= 1;
        }
        self.unit[position] = value;
        self.count += 1;
    }

    fn plain(&self, value: i32) -> i64 {
        self.base + (value as i64 * self.scale).div_euclid(1000)
    }

    fn snap(&self, position: i64) -> i64 {
        (position - self.grid + 128).div_euclid(256) * 256 + self.grid
    }

    fn finish(&mut self) {
        for index in 0..self.count {
            let plain = self.plain(self.unit[index]);
            self.snapped[index] = self.snap(plain);
        }
    }

    fn map(&self, value: i32) -> i64 {
        if self.count == 0 {
            return self.plain(value);
        }
        let last = self.count - 1;
        if value <= self.unit[0] {
            return self.snapped[0] + self.plain(value) - self.plain(self.unit[0]);
        }
        if value >= self.unit[last] {
            return self.snapped[last] + self.plain(value) - self.plain(self.unit[last]);
        }
        let mut index = 0;
        while self.unit[index + 1] <= value {
            index += 1;
        }
        let span = (self.unit[index + 1] - self.unit[index]) as i64;
        let along = (value - self.unit[index]) as i64;
        self.snapped[index] + ((self.snapped[index + 1] - self.snapped[index]) * along).div_euclid(span)
    }
}

struct Scan {
    x: Axis,
    y: Axis,
    current: (i32, i32),
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
}

impl Scan {
    fn include(&mut self, x: i32, y: i32) {
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_y = self.min_y.min(y);
        self.max_y = self.max_y.max(y);
    }

    fn visit(&mut self, op: Op) {
        match op {
            Op::Move(x, y) => {
                self.current = (x, y);
                self.include(x, y);
            }
            Op::Line(x, y) => {
                if x == self.current.0 {
                    self.x.add(x);
                }
                if y == self.current.1 {
                    self.y.add(y);
                }
                self.current = (x, y);
                self.include(x, y);
            }
            Op::Quad(cx, cy, x, y) => {
                self.include(cx, cy);
                self.include(x, y);
                self.current = (x, y);
            }
            Op::Cubic(ax, ay, bx, by, x, y) => {
                self.include(ax, ay);
                self.include(bx, by);
                self.include(x, y);
                self.current = (x, y);
            }
        }
    }
}

struct Shaper {
    x: Axis,
    y: Axis,
    origin_x: i64,
    origin_y: i64,
    current: (i32, i32),
    segments: [[i32; 4]; MAX_SEGMENTS],
    count: usize,
}

fn steps(points: &[(i32, i32)]) -> i64 {
    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    for &(x, y) in points {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let extent = ((max_x - min_x).max(max_y - min_y) / 256) as u64;
    let count = integer_sqrt(6 * extent) as i64 + 1;
    count.max(3).min(14)
}

impl Shaper {
    fn point(&self, x: i32, y: i32) -> (i32, i32) {
        ((self.origin_x + self.x.map(x)) as i32, (self.origin_y + self.y.map(y)) as i32)
    }

    fn push(&mut self, from: (i32, i32), to: (i32, i32)) {
        if self.count < MAX_SEGMENTS {
            self.segments[self.count] = [from.0, from.1, to.0, to.1];
            self.count += 1;
        }
    }

    fn visit(&mut self, op: Op) {
        match op {
            Op::Move(x, y) => {
                self.current = self.point(x, y);
            }
            Op::Line(x, y) => {
                let to = self.point(x, y);
                let from = self.current;
                self.push(from, to);
                self.current = to;
            }
            Op::Quad(cx, cy, x, y) => {
                let start = self.current;
                let control = self.point(cx, cy);
                let end = self.point(x, y);
                let count = steps(&[start, control, end]);
                let denominator = count * count;
                let mut previous = start;
                for index in 1..=count {
                    let rest = count - index;
                    let px = (rest * rest * start.0 as i64 + 2 * rest * index * control.0 as i64 + index * index * end.0 as i64)
                        .div_euclid(denominator);
                    let py = (rest * rest * start.1 as i64 + 2 * rest * index * control.1 as i64 + index * index * end.1 as i64)
                        .div_euclid(denominator);
                    let next = (px as i32, py as i32);
                    self.push(previous, next);
                    previous = next;
                }
                self.current = end;
            }
            Op::Cubic(ax, ay, bx, by, x, y) => {
                let start = self.current;
                let first = self.point(ax, ay);
                let second = self.point(bx, by);
                let end = self.point(x, y);
                let count = steps(&[start, first, second, end]);
                let denominator = count * count * count;
                let mut previous = start;
                for index in 1..=count {
                    let rest = count - index;
                    let w0 = rest * rest * rest;
                    let w1 = 3 * rest * rest * index;
                    let w2 = 3 * rest * index * index;
                    let w3 = index * index * index;
                    let px = (w0 * start.0 as i64 + w1 * first.0 as i64 + w2 * second.0 as i64 + w3 * end.0 as i64)
                        .div_euclid(denominator);
                    let py = (w0 * start.1 as i64 + w1 * first.1 as i64 + w2 * second.1 as i64 + w3 * end.1 as i64)
                        .div_euclid(denominator);
                    let next = (px as i32, py as i32);
                    self.push(previous, next);
                    previous = next;
                }
                self.current = end;
            }
        }
    }
}

fn distance_squared(px: i64, py: i64, segment: &[i32; 4]) -> i64 {
    let ax = segment[0] as i64;
    let ay = segment[1] as i64;
    let dx = segment[2] as i64 - ax;
    let dy = segment[3] as i64 - ay;
    let length_squared = dx * dx + dy * dy;
    let (qx, qy) = if length_squared == 0 {
        (ax, ay)
    } else {
        let t = (px - ax) * dx + (py - ay) * dy;
        if t <= 0 {
            (ax, ay)
        } else if t >= length_squared {
            (segment[2] as i64, segment[3] as i64)
        } else {
            (ax + dx * t / length_squared, ay + dy * t / length_squared)
        }
    };
    let ex = px - qx;
    let ey = py - qy;
    ex * ex + ey * ey
}

pub fn draw_glyph(
    entry: &Entry,
    origin_x: i32,
    origin_y: i32,
    cap: i32,
    stroke_sixteenths: i32,
    plot: &mut dyn FnMut(i32, i32, u8),
) {
    if entry.part_count == 0 {
        return;
    }
    let stroke_pixels = ((stroke_sixteenths + 8) / 16).max(1).min((cap / 2).max(1)) as i64;
    let grid: i64 = if stroke_pixels % 2 == 1 { 128 } else { 0 };
    let cap64 = cap as i64;
    let mut scan = Scan {
        x: Axis::new(0, cap64 * 256, grid),
        y: Axis::new(stroke_pixels * 128, (cap64 - stroke_pixels) * 256, grid),
        current: (0, 0),
        min_x: i32::MAX,
        max_x: i32::MIN,
        min_y: i32::MAX,
        max_y: i32::MIN,
    };
    walk(entry, &mut |op| scan.visit(op));
    if scan.min_x > scan.max_x {
        return;
    }
    let (low_x, high_x, low_y, high_y) = (scan.min_x, scan.max_x, scan.min_y, scan.max_y);
    scan.x.add(low_x);
    scan.x.add(high_x);
    scan.y.add(low_y);
    scan.y.add(high_y);
    scan.x.finish();
    scan.y.finish();
    let mut shaper = Shaper {
        x: scan.x,
        y: scan.y,
        origin_x: origin_x as i64 * 256,
        origin_y: origin_y as i64 * 256,
        current: (0, 0),
        segments: [[0; 4]; MAX_SEGMENTS],
        count: 0,
    };
    walk(entry, &mut |op| shaper.visit(op));
    if shaper.count == 0 {
        return;
    }
    let half = stroke_sixteenths.max(1) * 8;
    let reach = half + 128;
    let reach_squared = reach as i64 * reach as i64;
    let mut left = i32::MAX;
    let mut right = i32::MIN;
    let mut top = i32::MAX;
    let mut bottom = i32::MIN;
    for index in 0..shaper.count {
        let segment = &shaper.segments[index];
        left = left.min(segment[0]).min(segment[2]);
        right = right.max(segment[0]).max(segment[2]);
        top = top.min(segment[1]).min(segment[3]);
        bottom = bottom.max(segment[1]).max(segment[3]);
    }
    let min_x = (left - reach).div_euclid(256);
    let max_x = (right + reach).div_euclid(256);
    let min_y = (top - reach).div_euclid(256);
    let max_y = (bottom + reach).div_euclid(256);
    let mut active = [0u8; MAX_SEGMENTS];
    for py in min_y..=max_y {
        let sample_y = py * 256 + 128;
        let mut active_count = 0;
        for index in 0..shaper.count {
            let segment = &shaper.segments[index];
            let low = segment[1].min(segment[3]) - reach;
            let high = segment[1].max(segment[3]) + reach;
            if sample_y >= low && sample_y <= high {
                active[active_count] = index as u8;
                active_count += 1;
            }
        }
        if active_count == 0 {
            continue;
        }
        for px in min_x..=max_x {
            let sample_x = px * 256 + 128;
            let mut best = reach_squared;
            for slot in 0..active_count {
                let segment = &shaper.segments[active[slot] as usize];
                let low = segment[0].min(segment[2]) - reach;
                let high = segment[0].max(segment[2]) + reach;
                if sample_x < low || sample_x > high {
                    continue;
                }
                let distance = distance_squared(sample_x as i64, sample_y as i64, segment);
                if distance < best {
                    best = distance;
                }
            }
            if best < reach_squared {
                let distance = integer_sqrt(best as u64) as i32;
                let coverage = reach - distance;
                if coverage > 0 {
                    plot(px, py, coverage.min(255) as u8);
                }
            }
        }
    }
}
