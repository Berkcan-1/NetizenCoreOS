//! Ayırıcısız sabit tampon: sayı/metin birleştirme (`nc_app::text::Line` ile aynı davranış).

pub struct Line {
    bytes: [u8; 128],
    length: usize,
}

impl Line {
    pub const fn new() -> Line {
        Line { bytes: [0; 128], length: 0 }
    }

    pub fn text(&mut self, value: &str) -> &mut Line {
        for ch in value.chars() {
            let size = ch.len_utf8();
            if self.length + size > self.bytes.len() {
                break;
            }
            ch.encode_utf8(&mut self.bytes[self.length..self.length + size]);
            self.length += size;
        }
        self
    }

    pub fn number(&mut self, value: u32, digits: usize) -> &mut Line {
        let mut divisor = 1u32;
        let mut needed = 1usize;
        while value / divisor >= 10 {
            divisor *= 10;
            needed += 1;
        }
        let mut padding = digits.saturating_sub(needed);
        while padding > 0 && self.length < self.bytes.len() {
            self.bytes[self.length] = b'0';
            self.length += 1;
            padding -= 1;
        }
        while divisor > 0 && self.length < self.bytes.len() {
            self.bytes[self.length] = b'0' + (value / divisor % 10) as u8;
            self.length += 1;
            divisor /= 10;
        }
        self
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.length]).unwrap_or("")
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_stopwatch_display() {
        let mut line = Line::new();
        line.number(7, 2).text(":").number(5, 2).text(".").number(9, 1);
        assert_eq!(line.as_str(), "07:05.9");
        let mut line = Line::new();
        line.number(0, 2).text(":").number(59, 2).text(".").number(0, 1);
        assert_eq!(line.as_str(), "00:59.0");
        let mut line = Line::new();
        line.number(1234, 2);
        assert_eq!(line.as_str(), "1234");
        let mut line = Line::new();
        line.number(u32::MAX, 1);
        assert_eq!(line.as_str(), "4294967295");
    }

    #[test]
    fn never_overflows_its_buffer() {
        let mut line = Line::new();
        for _ in 0..100 {
            line.text("ş").number(12345, 8);
        }
        assert!(line.as_str().len() <= 128);
    }
}
