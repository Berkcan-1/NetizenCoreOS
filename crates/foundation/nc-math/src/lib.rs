#![no_std]

const SIN_TABLE: [i32; 19] = [
    0, 87, 174, 259, 342, 423, 500, 574, 643, 707, 766, 819, 866, 906, 940, 966, 985, 996, 1000,
];

pub fn sin_milli(degrees: i32) -> i32 {
    let d = degrees.rem_euclid(360);
    let (index, sign) = if d <= 90 {
        (d / 5, 1)
    } else if d <= 180 {
        ((180 - d) / 5, 1)
    } else if d <= 270 {
        ((d - 180) / 5, -1)
    } else {
        ((360 - d) / 5, -1)
    };
    sign * SIN_TABLE[index as usize]
}

pub fn cos_milli(degrees: i32) -> i32 {
    sin_milli(degrees + 90)
}

pub fn integer_sqrt(value: u64) -> u64 {
    if value < 2 {
        return value;
    }
    let mut x = 1u64 << ((64 - value.leading_zeros() + 1) / 2);
    loop {
        let y = (x + value / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

pub fn smoothstep256(t: u32) -> u32 {
    let t = t.min(256);
    (t * t * (768 - 2 * t)) >> 16
}

pub fn ratio256(value: u32, total: u32) -> u32 {
    if total == 0 {
        return 256;
    }
    ((value as u64 * 256) / total as u64).min(256) as u32
}

pub fn lerp_i32(from: i32, to: i32, t: u32) -> i32 {
    from + ((to - from) as i64 * t.min(256) as i64 / 256) as i32
}
