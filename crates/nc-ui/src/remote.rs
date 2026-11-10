//! Compositor tarafı: uygulamadan gelen çizim komut akışını (nc-abi::draw) bir pencerenin
//! `Canvas`'ına çizer.
//!
//! Güvenlik özellikleri:
//!   - akış ÖNCE tamamen doğrulanır; bozuk akış tek piksel bile çizmez
//!   - tüm çizim, pencere alanı ile canvas'ın mevcut klibinin KESİŞİMİne kırpılır
//!   - koordinatlar pencere içeriğine göredir (uygulama pencere dışına yazamaz)
//!   - görünür alanla kesişmeyen panel/nokta atlanır (iş bütçesi zaten nc-abi'de sınırlı)

use nc_abi::draw::{self, Cmd, CmdError, CmdReader};
use nc_font::text_width;
use nc_gfx::{Canvas, Rect};

use crate::{draw_accent_button, draw_button, draw_dot, draw_panel, draw_text_color, text_style};

fn intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    Rect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
}

fn overlaps(a: Rect, b: Rect) -> bool {
    let i = intersect(a, b);
    i.width > 0 && i.height > 0
}

/// Metin genişliği (uygulamanın ortalama/hizalama yapabilmesi için `SYS_TEXT_WIDTH` bunu kullanır).
pub fn measure_text(text: &str, cap_height: i32) -> i32 {
    let style = text_style(cap_height.clamp(8, draw::MAX_CAP_HEIGHT), 15);
    text_width(text, &style)
}

/// Akışı `area` (canvas koordinatlarında pencere içeriği) içine çizer. Döndürür: işlenen kayıt sayısı.
pub fn replay(canvas: &mut Canvas<'_>, area: Rect, bytes: &[u8]) -> Result<usize, CmdError> {
    let summary = draw::validate(bytes)?;
    let saved = canvas.clip_rect();
    let visible = intersect(saved, area);
    canvas.set_clip(visible);

    for item in CmdReader::new(bytes) {
        let Ok(cmd) = item else { break };
        match cmd {
            Cmd::FillRect { x, y, w, h, color } => {
                canvas.fill_rect(area.x + x, area.y + y, w, h, color);
            }
            Cmd::Panel { x, y, w, h, radius, fill, fill_alpha, border, border_alpha } => {
                let rect = Rect::new(area.x + x, area.y + y, w, h);
                if overlaps(rect, visible) {
                    draw_panel(canvas, rect, radius, fill, fill_alpha, border, border_alpha);
                }
            }
            Cmd::Dot { cx, cy, radius, color, strength } => {
                let reach = radius + 2;
                let bbox = Rect::new(area.x + cx - reach, area.y + cy - reach, reach * 2 + 1, reach * 2 + 1);
                if overlaps(bbox, visible) {
                    draw_dot(canvas, area.x + cx, area.y + cy, radius, color, strength);
                }
            }
            Cmd::Text { x, y, cap_height, color, tracking, centered, text } => {
                let percent = if tracking == 0 { 15 } else { tracking as i32 };
                let style = text_style(cap_height.max(8), percent);
                let left = if centered { x - text_width(text, &style) / 2 } else { x };
                draw_text_color(canvas, text, area.x + left, area.y + y, &style, color);
            }
            Cmd::Button { x, y, w, h, accent, style, hover, label } => {
                let rect = Rect::new(area.x + x, area.y + y, w, h);
                if overlaps(rect, visible) {
                    if style == 1 {
                        draw_accent_button(canvas, rect, label, hover, accent);
                    } else {
                        draw_button(canvas, rect, label, hover, false);
                    }
                }
            }
        }
    }

    canvas.set_clip(saved);
    Ok(summary.records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nc_abi::draw::CmdWriter;
    use std::vec;

    const BG: u32 = 0x0011_2233;
    const BLUE: u32 = 0x0000_00FF;
    const RED: u32 = 0x00FF_0000;
    const WHITE: u32 = 0x00FF_FFFF;
    const W: usize = 128;
    const H: usize = 64;

    fn area() -> Rect {
        Rect::new(16, 16, 64, 32)
    }

    fn pix(canvas: &Canvas<'_>, x: usize, y: usize) -> u32 {
        canvas.pixels()[y * W + x]
    }

    #[test]
    fn origin_offset_and_window_clip() {
        let mut pixels = vec![BG; W * H];
        let mut canvas = Canvas::new(&mut pixels, W, H).unwrap();
        let mut buf = [0u8; 256];
        let mut w = CmdWriter::new(&mut buf);
        w.fill_rect(-8, -8, 200, 200, BLUE).unwrap();
        w.fill_rect(10, 6, 40, 20, RED).unwrap();
        assert_eq!(replay(&mut canvas, area(), w.as_bytes()), Ok(2));

        assert_eq!(pix(&canvas, 0, 0), BG);
        assert_eq!(pix(&canvas, 15, 16), BG, "pencerenin solu disina tasmamali");
        assert_eq!(pix(&canvas, 16, 15), BG, "pencerenin ustu disina tasmamali");
        assert_eq!(pix(&canvas, 16, 16), BLUE);
        assert_eq!(pix(&canvas, 79, 47), BLUE);
        assert_eq!(pix(&canvas, 80, 47), BG, "pencerenin sagi disina tasmamali");
        assert_eq!(pix(&canvas, 79, 48), BG, "pencerenin alti disina tasmamali");
        assert_eq!(pix(&canvas, 16 + 10, 16 + 6), RED);
        assert_eq!(pix(&canvas, 16 + 9, 16 + 6), BLUE);
        assert_eq!(pix(&canvas, 16 + 49, 16 + 25), RED);
        assert_eq!(pix(&canvas, 16 + 50, 16 + 25), BLUE);
        assert_eq!(canvas.clip_rect(), Rect::new(0, 0, W as i32, H as i32), "klip geri yuklenmeli");
    }

    #[test]
    fn invalid_stream_draws_nothing() {
        let mut pixels = vec![BG; W * H];
        let mut canvas = Canvas::new(&mut pixels, W, H).unwrap();
        let mut buf = [0u8; 64];
        let mut w = CmdWriter::new(&mut buf);
        w.fill_rect(0, 0, 50, 50, BLUE).unwrap();
        let mut bytes = [0u8; 28];
        bytes[..24].copy_from_slice(w.as_bytes());
        bytes[24..28].copy_from_slice(&[99, 0, 4, 0]);
        assert_eq!(replay(&mut canvas, area(), &bytes), Err(CmdError::UnknownOp));
        assert!(canvas.pixels().iter().all(|&p| p == BG), "gecersiz akis kismen bile cizilmemeli");
        assert_eq!(canvas.clip_rect(), Rect::new(0, 0, W as i32, H as i32));
    }

    #[test]
    fn respects_existing_canvas_clip() {
        let mut pixels = vec![BG; W * H];
        let mut canvas = Canvas::new(&mut pixels, W, H).unwrap();
        canvas.set_clip(Rect::new(20, 20, 10, 10));
        let mut buf = [0u8; 64];
        let mut w = CmdWriter::new(&mut buf);
        w.fill_rect(-100, -100, 1000, 1000, BLUE).unwrap();
        replay(&mut canvas, area(), w.as_bytes()).unwrap();
        assert_eq!(canvas.clip_rect(), Rect::new(20, 20, 10, 10));
        canvas.reset_clip();
        for y in 0..H {
            for x in 0..W {
                let inside = (20..30).contains(&x) && (20..30).contains(&y);
                assert_eq!(pix(&canvas, x, y), if inside { BLUE } else { BG }, "({},{})", x, y);
            }
        }
    }

    #[test]
    fn panel_dot_and_text_render() {
        let mut pixels = vec![BG; W * H];
        let mut canvas = Canvas::new(&mut pixels, W, H).unwrap();
        let mut buf = [0u8; 256];
        let mut w = CmdWriter::new(&mut buf);
        w.fill_rect(-8, -8, 200, 200, BLUE).unwrap();
        w.panel(0, 2, 30, 20, 4, WHITE, 255, 0x00CCCCCC, 255).unwrap();
        w.dot(50, 22, 5, 0x0000FF00, 255).unwrap();
        w.text(36, 2, 12, WHITE, "Hi").unwrap();
        assert_eq!(replay(&mut canvas, area(), w.as_bytes()), Ok(4));

        assert_eq!(pix(&canvas, 16 + 15, 16 + 12), WHITE, "panel ici beyaz olmali");
        assert_ne!(pix(&canvas, 16 + 50, 16 + 22), BLUE, "nokta merkezi degismeli");
        let mut text_pixels = 0;
        for y in 16 + 2..16 + 16 {
            for x in 16 + 36..16 + 60 {
                if pix(&canvas, x, y) != BLUE {
                    text_pixels += 1;
                }
            }
        }
        assert!(text_pixels > 5, "metin cizilmeli (degisen piksel: {})", text_pixels);
    }

    #[test]
    fn offscreen_commands_are_cheap_and_harmless() {
        let mut pixels = vec![BG; W * H];
        let mut canvas = Canvas::new(&mut pixels, W, H).unwrap();
        let mut buf = [0u8; 256];
        let mut w = CmdWriter::new(&mut buf);
        w.panel(16000, 16000, 2048, 2048, 8, WHITE, 255, WHITE, 255).unwrap();
        w.dot(-16000, -16000, 512, WHITE, 255).unwrap();
        w.fill_rect(5000, 5000, 100, 100, WHITE).unwrap();
        assert_eq!(replay(&mut canvas, area(), w.as_bytes()), Ok(3));
        assert!(canvas.pixels().iter().all(|&p| p == BG));
    }

    #[test]
    fn measure_text_is_monotonic() {
        let a = measure_text("Hello", 16);
        let b = measure_text("Hello, world", 16);
        assert!(a > 0 && b > a);
        assert!(measure_text("Hello", 32) > a);
        assert_eq!(measure_text("", 16), 0);
        assert_eq!(measure_text("Hello", 1), measure_text("Hello", 8), "cap_height en az 8'e yukseltilir");
    }

    #[test]
    fn centered_text_matches_manual_centering() {
        let style = text_style(16, 20);
        let width = text_width("Merhaba", &style);

        let mut a = vec![BG; W * H];
        {
            let mut canvas = Canvas::new(&mut a, W, H).unwrap();
            let mut buf = [0u8; 128];
            let mut w = CmdWriter::new(&mut buf);
            w.text_styled(40, 8, 16, WHITE, 20, true, "Merhaba").unwrap();
            replay(&mut canvas, area(), w.as_bytes()).unwrap();
        }
        let mut b = vec![BG; W * H];
        {
            let mut canvas = Canvas::new(&mut b, W, H).unwrap();
            canvas.set_clip(area());
            draw_text_color(&mut canvas, "Merhaba", area().x + 40 - width / 2, area().y + 8, &style, WHITE);
        }
        assert!(a.iter().any(|&p| p != BG), "metin cizilmeli");
        assert_eq!(a, b, "ortali metin, elle ortalanmis metinle piksel piksel ayni olmali");
    }

    #[test]
    fn button_matches_direct_widget_call() {
        for (style, hover) in [(0u8, false), (0, true), (1, false), (1, true)] {
            let rect = Rect::new(area().x + 4, area().y + 6, 40, 14);
            let mut a = vec![BG; W * H];
            {
                let mut canvas = Canvas::new(&mut a, W, H).unwrap();
                let mut buf = [0u8; 128];
                let mut w = CmdWriter::new(&mut buf);
                w.button(4, 6, 40, 14, 0x0030_80F0, style, hover, "Baslat").unwrap();
                assert_eq!(replay(&mut canvas, area(), w.as_bytes()), Ok(1));
            }
            let mut b = vec![BG; W * H];
            {
                let mut canvas = Canvas::new(&mut b, W, H).unwrap();
                canvas.set_clip(area());
                if style == 1 {
                    draw_accent_button(&mut canvas, rect, "Baslat", hover, 0x0030_80F0);
                } else {
                    draw_button(&mut canvas, rect, "Baslat", hover, false);
                }
            }
            assert!(a.iter().any(|&p| p != BG), "dugme cizilmeli");
            assert_eq!(a, b, "dugme akisi dogrudan widget cagrisiyla ayni olmali (style {}, hover {})", style, hover);
        }
    }
}
