//! Çizim komut tamponu (nc-abi::draw::CmdWriter üzerinde ince bir sarmalayıcı).
//! Tampon dolarsa komutlar SESSİZCE atlanır (`dropped()` ile sorgulanır); uygulama çökmez.
//! Sınır dışı değerler sıkıştırılır: tek bir kötü komut yüzünden akışın TAMAMI reddedilmesin.

use nc_abi::draw::{CmdWriter, COORD_LIMIT, MAX_CAP_HEIGHT, MAX_DOT_RADIUS, MAX_PANEL_DIM, MAX_RADIUS};

fn c(v: i32) -> i32 {
    v.clamp(-COORD_LIMIT, COORD_LIMIT)
}

fn dim(v: i32) -> i32 {
    v.clamp(1, MAX_PANEL_DIM)
}

fn cap(v: i32) -> i32 {
    v.clamp(1, MAX_CAP_HEIGHT)
}

pub struct Gfx<'a> {
    writer: CmdWriter<'a>,
    dropped: bool,
}

impl<'a> Gfx<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self { writer: CmdWriter::new(buffer), dropped: false }
    }

    pub fn is_empty(&self) -> bool {
        self.writer.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.writer.as_bytes()
    }

    pub fn dropped(&self) -> bool {
        self.dropped
    }

    fn note<T>(&mut self, result: Result<T, nc_abi::draw::CmdError>) {
        if result.is_err() {
            self.dropped = true;
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        let r = self.writer.fill_rect(c(x), c(y), w.clamp(0, COORD_LIMIT), h.clamp(0, COORD_LIMIT), color);
        self.note(r);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn panel(&mut self, x: i32, y: i32, w: i32, h: i32, radius: i32, fill: u32, fill_alpha: u32, border: u32, border_alpha: u32) {
        let (w, h) = (dim(w), dim(h));
        let radius = radius.clamp(0, MAX_RADIUS).min(w.min(h) / 2);
        let r = self.writer.panel(c(x), c(y), w, h, radius, fill, fill_alpha.min(255), border, border_alpha.min(255));
        self.note(r);
    }

    pub fn dot(&mut self, cx: i32, cy: i32, radius: i32, color: u32, strength: u32) {
        let r = self.writer.dot(c(cx), c(cy), radius.clamp(0, MAX_DOT_RADIUS), color, strength.min(nc_abi::draw::MAX_STRENGTH));
        self.note(r);
    }

    pub fn text(&mut self, x: i32, y: i32, cap_height: i32, color: u32, tracking: u8, text: &str) {
        let r = self.writer.text_styled(c(x), c(y), cap(cap_height), color, tracking.min(nc_abi::draw::MAX_TRACKING), false, text);
        self.note(r);
    }

    /// `center_x`: metnin yatay ortası (genişliği çekirdek ölçer).
    pub fn text_centered(&mut self, center_x: i32, y: i32, cap_height: i32, color: u32, tracking: u8, text: &str) {
        let r = self.writer.text_styled(c(center_x), c(y), cap(cap_height), color, tracking.min(nc_abi::draw::MAX_TRACKING), true, text);
        self.note(r);
    }

    /// Tema düğmesi (compositor çizer): `accent = Some(renk)` vurgulu, `None` normal.
    pub fn button(&mut self, x: i32, y: i32, w: i32, h: i32, accent: Option<u32>, hover: bool, label: &str) {
        let r = self.writer.button(c(x), c(y), dim(w), dim(h), accent.unwrap_or(0), accent.is_some() as u8, hover, label);
        self.note(r);
    }
}
