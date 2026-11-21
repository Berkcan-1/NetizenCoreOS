#![allow(dead_code)]

pub const ONE: i32 = 256;
const Q: i64 = 4096;

#[derive(Clone, Copy)]
pub struct Bez(i64, i64, i64, i64);

impl Bez {
    pub const fn new(x1: i32, y1: i32, x2: i32, y2: i32) -> Bez {
        Bez(x1 as i64 * Q / 1000, y1 as i64 * Q / 1000, x2 as i64 * Q / 1000, y2 as i64 * Q / 1000)
    }
}

pub const LINEAR: Bez = Bez::new(0, 0, 1000, 1000);
pub const STANDARD: Bez = Bez::new(220, 1000, 360, 1000);
pub const DECEL: Bez = Bez::new(160, 1000, 300, 1000);
pub const ACCEL: Bez = Bez::new(400, 0, 1000, 1000);
pub const SPRING: Bez = Bez::new(260, 1140, 420, 1000);

fn bez_x(b: &Bez, t: i64) -> i64 {
    let u = Q - t;
    (3 * u * u * t * b.0 + 3 * u * t * t * b.2 + t * t * t * Q) / (Q * Q * Q)
}

fn bez_y(b: &Bez, t: i64) -> i64 {
    let u = Q - t;
    (3 * u * u * t * b.1 + 3 * u * t * t * b.3 + t * t * t * Q) / (Q * Q * Q)
}

pub fn ease(b: Bez, p: i64) -> i64 {
    let p = p.clamp(0, Q);
    let (mut lo, mut hi) = (0i64, Q);
    for _ in 0..12 {
        let mid = (lo + hi) / 2;
        if bez_x(&b, mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez_y(&b, (lo + hi) / 2)
}

pub fn curve(b: Bez, v: i32) -> i32 {
    (ease(b, v.clamp(0, ONE) as i64 * Q / ONE as i64) * ONE as i64 / Q) as i32
}

pub fn lerp(a: i32, b: i32, t: i32) -> i32 {
    a + (b - a) * t / ONE
}

#[derive(Clone, Copy)]
pub struct Spec {
    pub in_ms: u32,
    pub out_ms: u32,
    pub in_c: Bez,
    pub out_c: Bez,
}

pub const S_APP: Spec = Spec { in_ms: 230, out_ms: 190, in_c: DECEL, out_c: DECEL };
pub const S_NAV: Spec = Spec { in_ms: 180, out_ms: 140, in_c: DECEL, out_c: ACCEL };
pub const S_CC: Spec = Spec { in_ms: 260, out_ms: 180, in_c: DECEL, out_c: STANDARD };
pub const S_REC: Spec = Spec { in_ms: 250, out_ms: 170, in_c: DECEL, out_c: STANDARD };
pub const S_DRAWER: Spec = Spec { in_ms: 250, out_ms: 180, in_c: DECEL, out_c: STANDARD };
pub const S_FOLDER: Spec = Spec { in_ms: 260, out_ms: 170, in_c: SPRING, out_c: ACCEL };
pub const S_MENU: Spec = Spec { in_ms: 190, out_ms: 140, in_c: DECEL, out_c: ACCEL };
pub const S_SEARCH: Spec = Spec { in_ms: 210, out_ms: 150, in_c: DECEL, out_c: ACCEL };
pub const S_WIDGETS: Spec = Spec { in_ms: 240, out_ms: 170, in_c: DECEL, out_c: STANDARD };
pub const S_LIFT: Spec = Spec { in_ms: 150, out_ms: 130, in_c: DECEL, out_c: DECEL };

#[derive(Clone, Copy)]
pub struct Tween {
    from: i32,
    to: i32,
    start_ms: u32,
    dur_ms: u32,
    curve: Bez,
    value: i32,
}

impl Tween {
    pub const fn new() -> Self {
        Self { from: 0, to: 0, start_ms: 0, dur_ms: 1, curve: LINEAR, value: 0 }
    }

    pub fn value(&self) -> i32 {
        self.value
    }

    pub fn settled(&self) -> bool {
        self.value == self.to
    }

    pub fn set(&mut self, v: i32) {
        self.from = v;
        self.to = v;
        self.value = v;
    }

    pub fn step(&mut self, now_ms: u32, target: i32, spec: &Spec) -> i32 {
        self.step_ex(now_ms, target, spec, false)
    }

    pub fn step_ex(&mut self, now_ms: u32, target: i32, spec: &Spec, reduced_motion: bool) -> i32 {
        if reduced_motion {
            self.set(target);
            return self.value;
        }
        if target != self.to {
            let rising = target > self.value;
            self.from = self.value;
            self.to = target;
            self.start_ms = now_ms;
            self.dur_ms = if rising { spec.in_ms } else { spec.out_ms }.max(1);
            self.curve = if rising { spec.in_c } else { spec.out_c };
        }
        if self.value == self.to {
            return self.value;
        }
        let elapsed = now_ms.wrapping_sub(self.start_ms);
        if elapsed >= self.dur_ms {
            self.value = self.to;
            return self.value;
        }
        let p = elapsed as i64 * Q / self.dur_ms as i64;
        let s = ease(self.curve, p);
        let span = (self.to - self.from) as i64;
        let v = self.from as i64 + span * s / Q;
        let over = (span.abs() / 5) as i32;
        let lo = self.from.min(self.to) - over;
        let hi = self.from.max(self.to) + over;
        self.value = (v as i32).clamp(lo, hi);
        self.value
    }
}
