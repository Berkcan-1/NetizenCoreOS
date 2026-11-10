//! Klavye kodları (`EventBlock::key`). Kararlı ABI: yeni değerler eklenir, mevcutlar değişmez.

pub const NONE: u32 = 0;
pub const ENTER: u32 = 1;
pub const ESCAPE: u32 = 2;
pub const SPACE: u32 = 3;
pub const TAB: u32 = 4;
pub const BACKSPACE: u32 = 5;
pub const UP: u32 = 6;
pub const DOWN: u32 = 7;
pub const LEFT: u32 = 8;
pub const RIGHT: u32 = 9;
pub const UNKNOWN: u32 = 10;

/// Karakter tuşları: bu bit + Unicode skaler değeri.
pub const CHAR_FLAG: u32 = 0x8000_0000;

pub fn from_char(c: char) -> u32 {
    CHAR_FLAG | c as u32
}

pub fn as_char(code: u32) -> Option<char> {
    if code & CHAR_FLAG == 0 {
        return None;
    }
    char::from_u32(code & !CHAR_FLAG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_codes_roundtrip() {
        for c in ['a', 'Z', '0', 'ş', 'İ', '\u{1F600}'] {
            assert_eq!(as_char(from_char(c)), Some(c));
        }
        assert_eq!(as_char(ENTER), None);
        assert_eq!(as_char(NONE), None);
        assert_eq!(as_char(CHAR_FLAG | 0xD800), None, "vekil kod noktasi gecersiz");
    }

    #[test]
    fn named_keys_are_distinct_and_below_char_flag() {
        let all = [NONE, ENTER, ESCAPE, SPACE, TAB, BACKSPACE, UP, DOWN, LEFT, RIGHT, UNKNOWN];
        for (i, a) in all.iter().enumerate() {
            assert!(*a < CHAR_FLAG);
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
