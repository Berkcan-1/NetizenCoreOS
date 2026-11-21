//! Shared event-driven motion primitives for NetizenCore mobile Ring 3 apps.
//!
//! Uses monotonic `EventBlock.uptime_ms` (wraparound-safe), cubic Bézier easing,
//! and explicit reduced-motion support so completed animations settle cleanly
//! and immediately stop requesting `RET_ANIMATING` frames.

pub const ONE: i32 = 256;
const Q: i64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bez(i64, i64, i64, i64);

impl Bez {
    pub const fn new(x1: i32, y1: i32, x2: i32, y2: i32) -> Bez {
        Bez(
            x1 as i64 * Q / 1000,
            y1 as i64 * Q / 1000,
            x2 as i64 * Q / 1000,
            y2 as i64 * Q / 1000,
        )
    }
}

pub const LINEAR: Bez = Bez::new(0, 0, 1000, 1000);
pub const STANDARD: Bez = Bez::new(220, 1000, 360, 1000);
pub const DECEL: Bez = Bez::new(160, 1000, 300, 1000);
pub const ACCEL: Bez = Bez::new(400, 0, 1000, 1000);
pub const SPRING: Bez = Bez::new(260, 1120, 420, 1000);

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

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub in_ms: u32,
    pub out_ms: u32,
    pub in_c: Bez,
    pub out_c: Bez,
}

/// Short tactile button / control feedback (90–120 ms).
pub const SPEC_PRESS: Spec = Spec { in_ms: 90, out_ms: 120, in_c: DECEL, out_c: DECEL };
/// Selection indicator / segmented tab glide (170 ms).
pub const SPEC_TAB: Spec = Spec { in_ms: 170, out_ms: 170, in_c: DECEL, out_c: DECEL };
/// Modal / action sheet enter (210 ms) & exit (160 ms).
pub const SPEC_SHEET: Spec = Spec { in_ms: 210, out_ms: 160, in_c: DECEL, out_c: ACCEL };
/// Page / detail / viewer transitions (200 ms / 150 ms).
pub const SPEC_PAGE: Spec = Spec { in_ms: 200, out_ms: 150, in_c: DECEL, out_c: ACCEL };
/// Subtle overlay fade (150 ms / 130 ms).
pub const SPEC_FADE: Spec = Spec { in_ms: 150, out_ms: 130, in_c: DECEL, out_c: ACCEL };

#[derive(Clone, Copy, Debug)]
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

    pub const fn with_value(v: i32) -> Self {
        Self { from: v, to: v, start_ms: 0, dur_ms: 1, curve: LINEAR, value: v }
    }

    pub const fn value(&self) -> i32 {
        self.value
    }

    pub const fn target(&self) -> i32 {
        self.to
    }

    pub const fn settled(&self) -> bool {
        self.value == self.to
    }

    pub fn set(&mut self, v: i32) {
        self.from = v;
        self.to = v;
        self.value = v;
    }

    pub fn step(&mut self, now_ms: u32, target: i32, spec: &Spec, reduced_motion: bool) -> i32 {
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
        let over = (span.abs() / 6) as i32;
        let lo = self.from.min(self.to) - over;
        let hi = self.from.max(self.to) + over;
        self.value = (v as i32).clamp(lo, hi);
        self.value
    }
}

/// Timed ephemeral toast state (auto-dismisses cleanly without spinning idle frames).
#[derive(Clone, Copy)]
pub struct ToastState {
    pub buf: [u8; 72],
    pub len: usize,
    pub color: u32,
    pub shown_at_ms: u32,
    pub duration_ms: u32,
    pub active: bool,
}

impl ToastState {
    pub const fn new() -> Self {
        Self {
            buf: [0; 72],
            len: 0,
            color: 0,
            shown_at_ms: 0,
            duration_ms: 2600,
            active: false,
        }
    }

    pub fn show(&mut self, now_ms: u32, color: u32, msg: &str) {
        let b = msg.as_bytes();
        let n = b.len().min(self.buf.len());
        self.buf[..n].copy_from_slice(&b[..n]);
        self.len = n;
        self.color = color;
        self.shown_at_ms = now_ms;
        self.active = n > 0;
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.len = 0;
    }

    /// Returns `true` if the toast just expired and needs a redraw.
    pub fn tick(&mut self, now_ms: u32) -> bool {
        if self.active && now_ms.wrapping_sub(self.shown_at_ms) >= self.duration_ms {
            self.active = false;
            self.len = 0;
            return true;
        }
        false
    }

    pub fn text(&self) -> &str {
        if !self.active {
            return "";
        }
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tween_settles_and_handles_wraparound() {
        let mut tw = Tween::new();
        assert!(tw.settled());
        let start = u32::MAX - 50;
        let v0 = tw.step(start, ONE, &SPEC_TAB, false);
        assert_eq!(v0, 0);
        assert!(!tw.settled());
        let mid = start.wrapping_add(85);
        let vm = tw.step(mid, ONE, &SPEC_TAB, false);
        assert!(vm > 0 && vm < ONE);
        let end = start.wrapping_add(200);
        let ve = tw.step(end, ONE, &SPEC_TAB, false);
        assert_eq!(ve, ONE);
        assert!(tw.settled());
    }

    #[test]
    fn reduced_motion_settles_immediately() {
        let mut tw = Tween::new();
        let v = tw.step(1000, ONE, &SPEC_SHEET, true);
        assert_eq!(v, ONE);
        assert!(tw.settled());
    }
}
