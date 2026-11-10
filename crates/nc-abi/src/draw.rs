//! Çizim komut akışı v1 (uygulama -> compositor).
//!
//! Tel biçimi (hepsi little-endian, her kayıt 4 bayta hizalı):
//!   kayıt = [op u8][bayraklar u8 = 0][len u16 = başlık dahil toplam bayt] + yük
//!   FILL_RECT(1): x y w h (i32) color (u32)                              yük 20
//!   PANEL(2):     x y w h radius (i32) fill fill_a border border_a (u32) yük 36
//!   DOT(3):       cx cy radius (i32) color strength (u32)                yük 20
//!   TEXT(4):      x y cap_h (i32) color (u32) text_len (u16) tracking (u8) align (u8) bayt[text_len] + sıfır dolgu
//!                 tracking: harf aralığı yüzdesi (0 = varsayılan 15); align: 0 sola, 1 x'e ORTALI
//!   BUTTON(5):    x y w h (i32) accent (u32) label_len (u16) style (u8) hover (u8) bayt[label_len] + sıfır dolgu
//!                 style: 0 normal, 1 vurgulu (accent rengiyle); çizimi compositor'un tema düğmesi yapar
//!
//! Koordinatlar pencere içeriğine görecedir; sınırlandırma (clip) çekirdekte yapılır. Her kayıt ve
//! akışın toplamı doğrulanır; bozuk bir akış HİÇ çizilmez (önce doğrula, sonra çiz).

pub const OP_FILL_RECT: u8 = 1;
pub const OP_PANEL: u8 = 2;
pub const OP_DOT: u8 = 3;
pub const OP_TEXT: u8 = 4;
pub const OP_BUTTON: u8 = 5;

pub const HEADER_LEN: usize = 4;
pub const MAX_STREAM_BYTES: usize = 16 * 1024;
pub const MAX_RECORDS: usize = 512;
pub const MAX_TEXT_BYTES: usize = 256;
pub const COORD_LIMIT: i32 = 16384;
pub const MAX_PANEL_DIM: i32 = 2048;
pub const MAX_RADIUS: i32 = 256;
pub const MAX_DOT_RADIUS: i32 = 512;
pub const MAX_STRENGTH: u32 = 1024;
pub const MAX_CAP_HEIGHT: i32 = 128;
pub const MAX_TRACKING: u8 = 100;
pub const MAX_LABEL_BYTES: usize = 64;
pub const MAX_WORK_PIXELS: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmdError {
    Truncated,
    BadLength,
    BadFlags,
    UnknownOp,
    OutOfRange,
    BadUtf8,
    StreamTooLarge,
    TooManyRecords,
    WorkBudget,
    BufferFull,
}

impl CmdError {
    pub fn as_str(self) -> &'static str {
        match self {
            CmdError::Truncated => "akis kesik",
            CmdError::BadLength => "gecersiz kayit uzunlugu",
            CmdError::BadFlags => "gecersiz bayrak",
            CmdError::UnknownOp => "bilinmeyen komut",
            CmdError::OutOfRange => "deger sinir disi",
            CmdError::BadUtf8 => "metin UTF-8 degil",
            CmdError::StreamTooLarge => "akis cok buyuk",
            CmdError::TooManyRecords => "cok fazla kayit",
            CmdError::WorkBudget => "cizim is butcesi asildi",
            CmdError::BufferFull => "yazma tamponu dolu",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd<'a> {
    FillRect { x: i32, y: i32, w: i32, h: i32, color: u32 },
    Panel { x: i32, y: i32, w: i32, h: i32, radius: i32, fill: u32, fill_alpha: u32, border: u32, border_alpha: u32 },
    Dot { cx: i32, cy: i32, radius: i32, color: u32, strength: u32 },
    Text { x: i32, y: i32, cap_height: i32, color: u32, tracking: u8, centered: bool, text: &'a str },
    Button { x: i32, y: i32, w: i32, h: i32, accent: u32, style: u8, hover: bool, label: &'a str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub records: usize,
    pub work_pixels: u64,
}

fn i32_at(b: &[u8], off: usize) -> i32 {
    i32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn coord_ok(v: i32) -> bool {
    (-COORD_LIMIT..=COORD_LIMIT).contains(&v)
}

fn padded(len: usize) -> usize {
    (len + 3) & !3
}

// Döndürür: (komut, kaydın toplam bayt uzunluğu, iş maliyeti).
fn parse_record(bytes: &[u8], off: usize) -> Result<(Cmd<'_>, usize, u64), CmdError> {
    let rest = bytes.get(off..).ok_or(CmdError::Truncated)?;
    if rest.len() < HEADER_LEN {
        return Err(CmdError::Truncated);
    }
    let op = rest[0];
    if rest[1] != 0 {
        return Err(CmdError::BadFlags);
    }
    let len = u16::from_le_bytes([rest[2], rest[3]]) as usize;
    if len < HEADER_LEN || len % 4 != 0 {
        return Err(CmdError::BadLength);
    }
    if len > rest.len() {
        return Err(CmdError::Truncated);
    }
    let p = &rest[HEADER_LEN..len];

    match op {
        OP_FILL_RECT => {
            if p.len() != 20 {
                return Err(CmdError::BadLength);
            }
            let (x, y, w, h) = (i32_at(p, 0), i32_at(p, 4), i32_at(p, 8), i32_at(p, 12));
            if !coord_ok(x) || !coord_ok(y) || !(0..=COORD_LIMIT).contains(&w) || !(0..=COORD_LIMIT).contains(&h) {
                return Err(CmdError::OutOfRange);
            }
            Ok((Cmd::FillRect { x, y, w, h, color: u32_at(p, 16) }, len, 0))
        }
        OP_PANEL => {
            if p.len() != 36 {
                return Err(CmdError::BadLength);
            }
            let (x, y, w, h, radius) = (i32_at(p, 0), i32_at(p, 4), i32_at(p, 8), i32_at(p, 12), i32_at(p, 16));
            let (fill, fill_alpha, border, border_alpha) = (u32_at(p, 20), u32_at(p, 24), u32_at(p, 28), u32_at(p, 32));
            if !coord_ok(x)
                || !coord_ok(y)
                || !(1..=MAX_PANEL_DIM).contains(&w)
                || !(1..=MAX_PANEL_DIM).contains(&h)
                || !(0..=MAX_RADIUS).contains(&radius)
                || radius * 2 > w.min(h)
                || fill_alpha > 255
                || border_alpha > 255
            {
                return Err(CmdError::OutOfRange);
            }
            let work = (w as u64) * (h as u64);
            Ok((Cmd::Panel { x, y, w, h, radius, fill, fill_alpha, border, border_alpha }, len, work))
        }
        OP_DOT => {
            if p.len() != 20 {
                return Err(CmdError::BadLength);
            }
            let (cx, cy, radius) = (i32_at(p, 0), i32_at(p, 4), i32_at(p, 8));
            let (color, strength) = (u32_at(p, 12), u32_at(p, 16));
            if !coord_ok(cx) || !coord_ok(cy) || !(0..=MAX_DOT_RADIUS).contains(&radius) || strength > MAX_STRENGTH {
                return Err(CmdError::OutOfRange);
            }
            let side = (2 * radius + 3) as u64;
            Ok((Cmd::Dot { cx, cy, radius, color, strength }, len, side * side))
        }
        OP_TEXT => {
            if p.len() < 20 {
                return Err(CmdError::BadLength);
            }
            let (x, y, cap_height, color) = (i32_at(p, 0), i32_at(p, 4), i32_at(p, 8), u32_at(p, 12));
            let text_len = u16::from_le_bytes([p[16], p[17]]) as usize;
            let (tracking, align) = (p[18], p[19]);
            if tracking > MAX_TRACKING || align > 1 {
                return Err(CmdError::OutOfRange);
            }
            if text_len > MAX_TEXT_BYTES {
                return Err(CmdError::OutOfRange);
            }
            if p.len() != 20 + padded(text_len) {
                return Err(CmdError::BadLength);
            }
            if p[20 + text_len..].iter().any(|&b| b != 0) {
                return Err(CmdError::BadFlags);
            }
            if !coord_ok(x) || !coord_ok(y) || !(1..=MAX_CAP_HEIGHT).contains(&cap_height) {
                return Err(CmdError::OutOfRange);
            }
            let text = core::str::from_utf8(&p[20..20 + text_len]).map_err(|_| CmdError::BadUtf8)?;
            let work = (text_len as u64) * ((cap_height as u64) * (cap_height as u64));
            Ok((Cmd::Text { x, y, cap_height, color, tracking, centered: align == 1, text }, len, work))
        }
        OP_BUTTON => {
            if p.len() < 24 {
                return Err(CmdError::BadLength);
            }
            let (x, y, w, h) = (i32_at(p, 0), i32_at(p, 4), i32_at(p, 8), i32_at(p, 12));
            let accent = u32_at(p, 16);
            let label_len = u16::from_le_bytes([p[20], p[21]]) as usize;
            let (style, hover) = (p[22], p[23]);
            if !coord_ok(x)
                || !coord_ok(y)
                || !(1..=MAX_PANEL_DIM).contains(&w)
                || !(1..=MAX_PANEL_DIM).contains(&h)
                || style > 1
                || hover > 1
                || label_len > MAX_LABEL_BYTES
            {
                return Err(CmdError::OutOfRange);
            }
            if p.len() != 24 + padded(label_len) {
                return Err(CmdError::BadLength);
            }
            if p[24 + label_len..].iter().any(|&b| b != 0) {
                return Err(CmdError::BadFlags);
            }
            let label = core::str::from_utf8(&p[24..24 + label_len]).map_err(|_| CmdError::BadUtf8)?;
            let work = (w as u64) * (h as u64);
            Ok((Cmd::Button { x, y, w, h, accent, style, hover: hover == 1, label }, len, work))
        }
        _ => Err(CmdError::UnknownOp),
    }
}

/// Tüm akışı çizmeden önce doğrular.
pub fn validate(bytes: &[u8]) -> Result<Summary, CmdError> {
    if bytes.len() > MAX_STREAM_BYTES {
        return Err(CmdError::StreamTooLarge);
    }
    let mut off = 0usize;
    let mut records = 0usize;
    let mut work = 0u64;
    while off < bytes.len() {
        let (_, len, cost) = parse_record(bytes, off)?;
        records += 1;
        if records > MAX_RECORDS {
            return Err(CmdError::TooManyRecords);
        }
        work = work.saturating_add(cost);
        if work > MAX_WORK_PIXELS {
            return Err(CmdError::WorkBudget);
        }
        off += len;
    }
    Ok(Summary { records, work_pixels: work })
}

/// Komutları sırayla verir. Hata görürse bir kez `Err` verir ve durur.
pub struct CmdReader<'a> {
    bytes: &'a [u8],
    off: usize,
}

impl<'a> CmdReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, off: 0 }
    }
}

impl<'a> Iterator for CmdReader<'a> {
    type Item = Result<Cmd<'a>, CmdError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.off >= self.bytes.len() {
            return None;
        }
        match parse_record(self.bytes, self.off) {
            Ok((cmd, len, _)) => {
                self.off += len;
                Some(Ok(cmd))
            }
            Err(error) => {
                self.off = self.bytes.len();
                Some(Err(error))
            }
        }
    }
}

/// Uygulama tarafı (SDK) yazıcısı.
pub struct CmdWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl<'a> CmdWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    fn begin(&mut self, op: u8, payload: usize) -> Result<usize, CmdError> {
        let total = HEADER_LEN + payload;
        if total > u16::MAX as usize || total % 4 != 0 || self.len + total > self.buf.len() {
            return Err(CmdError::BufferFull);
        }
        let start = self.len;
        self.buf[start] = op;
        self.buf[start + 1] = 0;
        self.buf[start + 2..start + 4].copy_from_slice(&(total as u16).to_le_bytes());
        self.len += total;
        Ok(start + HEADER_LEN)
    }

    fn put_i32(&mut self, off: usize, v: i32) {
        self.buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn put_u32(&mut self, off: usize, v: u32) {
        self.buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) -> Result<(), CmdError> {
        let p = self.begin(OP_FILL_RECT, 20)?;
        self.put_i32(p, x);
        self.put_i32(p + 4, y);
        self.put_i32(p + 8, w);
        self.put_i32(p + 12, h);
        self.put_u32(p + 16, color);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn panel(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        radius: i32,
        fill: u32,
        fill_alpha: u32,
        border: u32,
        border_alpha: u32,
    ) -> Result<(), CmdError> {
        let p = self.begin(OP_PANEL, 36)?;
        self.put_i32(p, x);
        self.put_i32(p + 4, y);
        self.put_i32(p + 8, w);
        self.put_i32(p + 12, h);
        self.put_i32(p + 16, radius);
        self.put_u32(p + 20, fill);
        self.put_u32(p + 24, fill_alpha);
        self.put_u32(p + 28, border);
        self.put_u32(p + 32, border_alpha);
        Ok(())
    }

    pub fn dot(&mut self, cx: i32, cy: i32, radius: i32, color: u32, strength: u32) -> Result<(), CmdError> {
        let p = self.begin(OP_DOT, 20)?;
        self.put_i32(p, cx);
        self.put_i32(p + 4, cy);
        self.put_i32(p + 8, radius);
        self.put_u32(p + 12, color);
        self.put_u32(p + 16, strength);
        Ok(())
    }

    pub fn text(&mut self, x: i32, y: i32, cap_height: i32, color: u32, text: &str) -> Result<(), CmdError> {
        self.text_styled(x, y, cap_height, color, 0, false, text)
    }

    /// `tracking`: harf aralığı yüzdesi (0 = varsayılan); `centered`: `x` metnin yatay ORTASI olur.
    #[allow(clippy::too_many_arguments)]
    pub fn text_styled(
        &mut self,
        x: i32,
        y: i32,
        cap_height: i32,
        color: u32,
        tracking: u8,
        centered: bool,
        text: &str,
    ) -> Result<(), CmdError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(CmdError::OutOfRange);
        }
        let p = self.begin(OP_TEXT, 20 + padded(text.len()))?;
        self.put_i32(p, x);
        self.put_i32(p + 4, y);
        self.put_i32(p + 8, cap_height);
        self.put_u32(p + 12, color);
        self.buf[p + 16..p + 18].copy_from_slice(&(text.len() as u16).to_le_bytes());
        self.buf[p + 18] = tracking;
        self.buf[p + 19] = centered as u8;
        self.put_label(p + 20, text.as_bytes());
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn button(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        accent: u32,
        style: u8,
        hover: bool,
        label: &str,
    ) -> Result<(), CmdError> {
        if label.len() > MAX_LABEL_BYTES {
            return Err(CmdError::OutOfRange);
        }
        let p = self.begin(OP_BUTTON, 24 + padded(label.len()))?;
        self.put_i32(p, x);
        self.put_i32(p + 4, y);
        self.put_i32(p + 8, w);
        self.put_i32(p + 12, h);
        self.put_u32(p + 16, accent);
        self.buf[p + 20..p + 22].copy_from_slice(&(label.len() as u16).to_le_bytes());
        self.buf[p + 22] = style;
        self.buf[p + 23] = hover as u8;
        self.put_label(p + 24, label.as_bytes());
        Ok(())
    }

    fn put_label(&mut self, at: usize, bytes: &[u8]) {
        self.buf[at..at + bytes.len()].copy_from_slice(bytes);
        for pad in at + bytes.len()..at + padded(bytes.len()) {
            self.buf[pad] = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_sample(buf: &mut [u8]) -> usize {
        let mut w = CmdWriter::new(buf);
        w.fill_rect(-8, -8, 200, 200, 0x0000_00FF).unwrap();
        w.fill_rect(10, 6, 40, 20, 0x00FF_0000).unwrap();
        w.panel(4, 4, 100, 50, 8, 0x00FFFFFF, 255, 0x00CCCCCC, 255).unwrap();
        w.dot(20, 20, 5, 0x0000FF00, 255).unwrap();
        w.text(12, 30, 16, 0x0010_2030, "Merhaba").unwrap();
        w.text(0, 0, 8, 0, "").unwrap();
        w.text_styled(60, 4, 20, 0x00AA_BBCC, 40, true, "Ortali").unwrap();
        w.button(4, 60, 90, 30, 0x0030_80F0, 1, true, "Başlat").unwrap();
        w.len()
    }

    #[test]
    fn writer_reader_roundtrip() {
        let mut buf = [0u8; 512];
        let n = build_sample(&mut buf);
        let summary = validate(&buf[..n]).unwrap();
        assert_eq!(summary.records, 8);
        let cmds: [Cmd; 8] = {
            let mut it = CmdReader::new(&buf[..n]);
            [
                it.next().unwrap().unwrap(), it.next().unwrap().unwrap(), it.next().unwrap().unwrap(),
                it.next().unwrap().unwrap(), it.next().unwrap().unwrap(), it.next().unwrap().unwrap(),
                it.next().unwrap().unwrap(), it.next().unwrap().unwrap(),
            ]
        };
        assert_eq!(cmds[0], Cmd::FillRect { x: -8, y: -8, w: 200, h: 200, color: 0xFF });
        assert_eq!(cmds[4], Cmd::Text { x: 12, y: 30, cap_height: 16, color: 0x0010_2030, tracking: 0, centered: false, text: "Merhaba" });
        assert_eq!(cmds[5], Cmd::Text { x: 0, y: 0, cap_height: 8, color: 0, tracking: 0, centered: false, text: "" });
        assert_eq!(
            cmds[6],
            Cmd::Text { x: 60, y: 4, cap_height: 20, color: 0x00AA_BBCC, tracking: 40, centered: true, text: "Ortali" }
        );
        assert_eq!(
            cmds[7],
            Cmd::Button { x: 4, y: 60, w: 90, h: 30, accent: 0x0030_80F0, style: 1, hover: true, label: "Başlat" }
        );
        assert!(CmdReader::new(&buf[..n]).count() == 8);
    }

    #[test]
    fn golden_bytes_are_stable() {
        // Bu baytlar sözleşmenin parçasıdır; değişirse ABI sürümü artmalıdır.
        let mut buf = [0u8; 64];
        let mut w = CmdWriter::new(&mut buf);
        w.fill_rect(1, 2, 3, 4, 0xAABBCCDD).unwrap();
        w.text(5, 6, 7, 0x11223344, "Hi").unwrap();
        let expected: [u8; 52] = [
            0x01, 0x00, 0x18, 0x00, 0x01, 0, 0, 0, 0x02, 0, 0, 0, 0x03, 0, 0, 0, 0x04, 0, 0, 0, 0xDD, 0xCC, 0xBB, 0xAA,
            0x04, 0x00, 0x1C, 0x00, 0x05, 0, 0, 0, 0x06, 0, 0, 0, 0x07, 0, 0, 0, 0x44, 0x33, 0x22, 0x11, 0x02, 0x00, 0x00, 0x00,
            b'H', b'i', 0, 0,
        ];
        assert_eq!(w.as_bytes(), &expected[..]);
    }

    #[test]
    fn rejects_every_prefix_truncation() {
        let mut buf = [0u8; 512];
        let n = build_sample(&mut buf);
        let mut off = 0;
        let mut boundaries = [false; 512];
        while off < n {
            boundaries[off] = true;
            off += u16::from_le_bytes([buf[off + 2], buf[off + 3]]) as usize;
        }
        boundaries[n] = true;
        for cut in 0..n {
            let result = validate(&buf[..cut]);
            if boundaries[cut] {
                assert!(result.is_ok(), "kayit sinirinda kesme gecerli olmali: {}", cut);
            } else {
                assert!(result.is_err(), "kesik akis kabul edildi: {}", cut);
            }
        }
    }

    #[test]
    fn rejects_specific_violations() {
        let mut buf = [0u8; 128];
        let mut w = CmdWriter::new(&mut buf);
        w.fill_rect(0, 0, 1, 1, 0).unwrap();
        let mut ok = [0u8; 24];
        ok.copy_from_slice(&w.as_bytes()[..24]);

        let mut b = ok;
        b[0] = 99;
        assert_eq!(validate(&b), Err(CmdError::UnknownOp));
        let mut b = ok;
        b[1] = 1;
        assert_eq!(validate(&b), Err(CmdError::BadFlags));
        let mut b = ok;
        b[2] = 23;
        assert_eq!(validate(&b), Err(CmdError::BadLength));
        let mut b = ok;
        b[2] = 0;
        assert_eq!(validate(&b), Err(CmdError::BadLength));
        let mut b = ok;
        b[2] = 28;
        assert_eq!(validate(&b), Err(CmdError::Truncated));
        let mut b = ok;
        b[4..8].copy_from_slice(&(COORD_LIMIT + 1).to_le_bytes());
        assert_eq!(validate(&b), Err(CmdError::OutOfRange));
        let mut b = ok;
        b[12..16].copy_from_slice(&(-1i32).to_le_bytes());
        assert_eq!(validate(&b), Err(CmdError::OutOfRange));
        let mut b = ok;
        b[4..8].copy_from_slice(&i32::MIN.to_le_bytes());
        assert_eq!(validate(&b), Err(CmdError::OutOfRange));

        assert_eq!(validate(&[0u8; MAX_STREAM_BYTES + 4]), Err(CmdError::StreamTooLarge));
    }

    #[test]
    fn text_is_strict() {
        let mut buf = [0u8; 400];
        let mut w = CmdWriter::new(&mut buf);
        assert_eq!(w.text(0, 0, 8, 0, core::str::from_utf8(&[b'a'; 257]).unwrap()), Err(CmdError::OutOfRange));
        w.text(0, 0, 16, 0, "abc").unwrap();
        let n = w.len();
        let good = buf;

        let mut b = good;
        b[4 + 20] = 0xFF;
        b[4 + 21] = 0xFE;
        assert_eq!(validate(&b[..n]), Err(CmdError::BadUtf8));
        let mut b = good;
        b[4 + 23] = 1;
        assert_eq!(validate(&b[..n]), Err(CmdError::BadFlags));
        let mut b = good;
        b[4 + 16] = 200;
        assert_eq!(validate(&b[..n]), Err(CmdError::BadLength));
        let mut b = good;
        b[4 + 8..4 + 12].copy_from_slice(&0i32.to_le_bytes());
        assert_eq!(validate(&b[..n]), Err(CmdError::OutOfRange));
        let mut b = good;
        b[4 + 8..4 + 12].copy_from_slice(&(MAX_CAP_HEIGHT + 1).to_le_bytes());
        assert_eq!(validate(&b[..n]), Err(CmdError::OutOfRange));
    }

    #[test]
    fn work_budget_and_record_limits() {
        // 2048x2048 panel = 4_194_304 piksel; 5 panel 16 Mpiksel bütçesini aşar.
        let mut buf = [0u8; MAX_STREAM_BYTES];
        let mut w = CmdWriter::new(&mut buf);
        for _ in 0..4 {
            w.panel(0, 0, 2048, 2048, 0, 0, 255, 0, 255).unwrap();
        }
        assert!(validate(w.as_bytes()).is_ok(), "tam sinirdaki is kabul edilmeli");
        w.panel(0, 0, 2048, 2048, 0, 0, 255, 0, 255).unwrap();
        assert_eq!(validate(w.as_bytes()), Err(CmdError::WorkBudget));

        // 513 küçük kayıt (513 * 24 = 12312 bayt) kayıt sınırını aşar.
        let mut buf = [0u8; MAX_STREAM_BYTES];
        let mut w = CmdWriter::new(&mut buf);
        for _ in 0..MAX_RECORDS {
            w.fill_rect(0, 0, 1, 1, 0).unwrap();
        }
        assert_eq!(validate(w.as_bytes()).unwrap().records, MAX_RECORDS);
        w.fill_rect(0, 0, 1, 1, 0).unwrap();
        assert_eq!(validate(w.as_bytes()), Err(CmdError::TooManyRecords));

        // Yarıçap, kısa kenarın yarısını aşamaz.
        let mut buf = [0u8; 64];
        let mut w = CmdWriter::new(&mut buf);
        w.panel(0, 0, 10, 10, 6, 0, 0, 0, 0).unwrap();
        assert_eq!(validate(w.as_bytes()), Err(CmdError::OutOfRange));
    }

    // Belirlenimli mutasyon fuzz'ı: hiçbir girdi panik etmemeli; doğrulayıcı kabul ettiyse okuyucu da
    // aynı sayıda, hatasız komut vermeli ve tüm değerler sınırlar içinde olmalı.
    #[test]
    fn mutation_fuzz_never_panics_and_agrees() {
        let mut base = [0u8; 512];
        let n = build_sample(&mut base);
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut accepted = 0u32;
        for _ in 0..40_000 {
            let mut tmp = base;
            let mut len = n;
            let flips = 1 + (next() % 4) as usize;
            for _ in 0..flips {
                let pos = (next() as usize) % n;
                tmp[pos] = (next() & 0xFF) as u8;
            }
            if next() % 4 == 0 {
                len = (next() as usize) % (n + 1);
            }
            let stream = &tmp[..len];
            match validate(stream) {
                Ok(summary) => {
                    accepted += 1;
                    let mut count = 0usize;
                    for item in CmdReader::new(stream) {
                        let cmd = item.expect("dogrulanmis akis okunabilmeli");
                        count += 1;
                        match cmd {
                            Cmd::FillRect { x, y, w, h, .. } => {
                                assert!(coord_ok(x) && coord_ok(y) && w >= 0 && h >= 0 && w <= COORD_LIMIT && h <= COORD_LIMIT);
                            }
                            Cmd::Panel { w, h, radius, fill_alpha, border_alpha, .. } => {
                                assert!((1..=MAX_PANEL_DIM).contains(&w) && (1..=MAX_PANEL_DIM).contains(&h));
                                assert!(radius >= 0 && radius * 2 <= w.min(h) && fill_alpha <= 255 && border_alpha <= 255);
                            }
                            Cmd::Dot { radius, strength, .. } => {
                                assert!((0..=MAX_DOT_RADIUS).contains(&radius) && strength <= MAX_STRENGTH);
                            }
                            Cmd::Text { cap_height, tracking, text, .. } => {
                                assert!((1..=MAX_CAP_HEIGHT).contains(&cap_height) && text.len() <= MAX_TEXT_BYTES);
                                assert!(tracking <= MAX_TRACKING);
                            }
                            Cmd::Button { w, h, style, label, .. } => {
                                assert!((1..=MAX_PANEL_DIM).contains(&w) && (1..=MAX_PANEL_DIM).contains(&h));
                                assert!(style <= 1 && label.len() <= MAX_LABEL_BYTES);
                            }
                        }
                    }
                    assert_eq!(count, summary.records);
                }
                Err(_) => {
                    // Okuyucu hata vermeli ya da (ilk kayıtlar geçerliyse) önce komut sonra hata.
                    let mut saw_error = false;
                    for item in CmdReader::new(stream) {
                        if item.is_err() {
                            saw_error = true;
                        }
                    }
                    let _ = saw_error;
                }
            }
        }
        assert!(accepted > 0, "fuzz hic gecerli girdi uretmedi (test gecersiz)");
    }

    #[test]
    fn text_style_and_button_are_strict() {
        let mut buf = [0u8; 128];
        let mut w = CmdWriter::new(&mut buf);
        w.text_styled(0, 0, 16, 0, 101, false, "a").unwrap();
        assert_eq!(validate(w.as_bytes()), Err(CmdError::OutOfRange), "tracking > 100");
        let mut buf = [0u8; 128];
        let mut w = CmdWriter::new(&mut buf);
        w.text_styled(0, 0, 16, 0, 0, false, "a").unwrap();
        let mut b = [0u8; 128];
        let n = w.len();
        b[..n].copy_from_slice(w.as_bytes());
        b[4 + 19] = 2;
        assert_eq!(validate(&b[..n]), Err(CmdError::OutOfRange), "align > 1");

        let mut buf = [0u8; 256];
        let mut w = CmdWriter::new(&mut buf);
        w.button(0, 0, 10, 10, 0, 2, false, "x").unwrap();
        assert_eq!(validate(w.as_bytes()), Err(CmdError::OutOfRange), "style > 1");
        let mut buf = [0u8; 256];
        let mut w = CmdWriter::new(&mut buf);
        w.button(0, 0, 0, 10, 0, 0, false, "x").unwrap();
        assert_eq!(validate(w.as_bytes()), Err(CmdError::OutOfRange), "w = 0");
        let mut buf = [0u8; 256];
        let mut w = CmdWriter::new(&mut buf);
        assert_eq!(
            w.button(0, 0, 10, 10, 0, 0, false, core::str::from_utf8(&[b'a'; 65]).unwrap()),
            Err(CmdError::OutOfRange)
        );
        w.button(0, 0, 10, 10, 0, 0, true, "abc").unwrap();
        let n = w.len();
        let mut b = [0u8; 256];
        b[..n].copy_from_slice(w.as_bytes());
        assert!(validate(&b[..n]).is_ok());
        b[4 + 24 + 3] = 7;
        assert_eq!(validate(&b[..n]), Err(CmdError::BadFlags), "dolgu sifir olmali");
        let mut b2 = [0u8; 256];
        b2[..n].copy_from_slice(w.as_bytes());
        b2[4 + 24] = 0xFF;
        assert_eq!(validate(&b2[..n]), Err(CmdError::BadUtf8));
        let mut b3 = [0u8; 256];
        b3[..n].copy_from_slice(w.as_bytes());
        b3[4 + 20] = 10;
        assert_eq!(validate(&b3[..n]), Err(CmdError::BadLength));
    }
}
