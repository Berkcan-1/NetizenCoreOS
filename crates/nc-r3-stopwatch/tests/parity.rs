//! Ring 3 Kronometre, Ring 0 Kronometre ile PİKSEL PİKSEL aynı görünmeli ve aynı davranmalı.

use nc_abi::event::{EventBlock, OP_DRAW, OP_INIT, OP_UPDATE, PTR_CLICKED, PTR_HAS, RET_REDRAW};
use nc_app::{tr, App, Context, Input, Language, Pointer, Settings, TextId};
use nc_app_sdk::{run_with, Gfx, RingApp};
use nc_app_stopwatch::StopwatchApp as Ring0;
use nc_gfx::{Canvas, Rect};
use nc_r3_stopwatch::{strings, StopwatchApp as Ring3};

const W: usize = 560;
const H: usize = 360;
const BG: u32 = 0x0020_2428;

struct Rig {
    r0: Ring0,
    r3: Ring3,
    ctx: Context,
    area: Rect,
    seq: u64,
}

impl Rig {
    fn new(language: Language, unit: i32) -> Rig {
        let ctx = Context {
            platform: "test",
            screen_width: 1280,
            screen_height: 720,
            unit,
            uptime_ms: 0,
            settings: Settings { language, ..Settings::new() },
            total_memory_mb: 512,
        };
        let area = Rect::new(40, 30, 26 * unit, 16 * unit);
        Rig { r0: Ring0::new(), r3: Ring3::new(), ctx, area, seq: 0 }
    }

    fn event(&mut self, op: u32, pointer: Option<(i32, i32, bool)>) -> EventBlock {
        self.seq += 1;
        let mut e = EventBlock {
            op,
            surface: 3,
            area_x: self.area.x,
            area_y: self.area.y,
            area_w: self.area.width,
            area_h: self.area.height,
            unit: self.ctx.unit,
            uptime_ms: self.ctx.uptime_ms,
            accent: self.ctx.settings.accent_color(),
            lang: self.ctx.settings.language as u32,
            seq: self.seq,
            ..Default::default()
        };
        if let Some((x, y, clicked)) = pointer {
            e.pointer_flags = PTR_HAS | if clicked { PTR_CLICKED } else { 0 };
            e.pointer_x = x;
            e.pointer_y = y;
        }
        e
    }

    fn step(&mut self, uptime_ms: u32, pointer: Option<(i32, i32, bool)>) {
        self.ctx.uptime_ms = uptime_ms;
        let input = Input {
            area: self.area,
            pointer: pointer.map(|(x, y, clicked)| Pointer { x, y, down: clicked, clicked, right_clicked: false }),
            key: None,
            focused: true,
        };
        let mut ctx = self.ctx;
        let u0 = self.r0.update(&mut ctx, &input);
        let event = self.event(OP_UPDATE, pointer);
        let mut buf = [0u8; 4096];
        let ret = run_with(&mut self.r3, &event, &mut buf, |_, _| panic!("UPDATE cizim gondermemeli"));
        assert_eq!(u0.redraw, ret & RET_REDRAW != 0, "redraw bayragi ayrisiyor (uptime {}, pointer {:?})", uptime_ms, pointer);
        assert!(!u0.close && !u0.install_app.is_some());
    }

    fn render_both(&mut self) -> (Vec<u32>, Vec<u32>) {
        let mut a = vec![BG; W * H];
        {
            let mut canvas = Canvas::new(&mut a, W, H).unwrap();
            self.r0.draw(&mut canvas, self.area, &self.ctx);
        }
        let event = self.event(OP_DRAW, None);
        let mut buf = [0u8; 16384];
        let mut stream: Vec<u8> = Vec::new();
        let ret = run_with(&mut self.r3, &event, &mut buf, |surface, bytes| {
            assert_eq!(surface, 3);
            stream.extend_from_slice(bytes);
        });
        assert_eq!(ret, 0);
        assert_eq!(nc_abi::draw::validate(&stream).unwrap().records, 4, "zaman, aciklama, iki dugme");
        let mut b = vec![BG; W * H];
        {
            let mut canvas = Canvas::new(&mut b, W, H).unwrap();
            nc_ui::remote::replay(&mut canvas, self.area, &stream).unwrap();
        }
        (a, b)
    }

    fn assert_same(&mut self, what: &str) {
        let (a, b) = self.render_both();
        assert!(a.iter().any(|&p| p != BG), "Ring 0 hic cizmedi");
        if a != b {
            let first = a.iter().zip(b.iter()).position(|(x, y)| x != y).unwrap();
            panic!("{}: piksel farki ({}, {}) Ring0={:#x} Ring3={:#x}", what, first % W, first / W, a[first], b[first]);
        }
    }

    fn toggle_center(&self) -> (i32, i32) {
        let u = self.ctx.unit;
        (self.area.x + self.area.width / 2 - u * 9 - u / 2 + u * 9 / 2, self.area.y + self.area.height - u * 5 + u * 3 / 2)
    }

    fn reset_center(&self) -> (i32, i32) {
        let u = self.ctx.unit;
        (self.area.x + self.area.width / 2 + u / 2 + u * 9 / 2, self.area.y + self.area.height - u * 5 + u * 3 / 2)
    }
}

fn scenario(language: Language, unit: i32) {
    let mut rig = Rig::new(language, unit);
    let mut buf = [0u8; 64];
    let init = rig.event(OP_INIT, None);
    assert_eq!(run_with(&mut rig.r3, &init, &mut buf, |_, _| {}), RET_REDRAW);
    rig.assert_same("baslangic");

    let (tx, ty) = rig.toggle_center();
    let (rx, ry) = rig.reset_center();
    rig.step(100, Some((tx, ty, false)));
    rig.assert_same("toggle uzerinde gezinme");
    rig.step(200, Some((tx, ty, true)));
    rig.assert_same("baslatildi");
    rig.step(1534, None);
    rig.assert_same("calisiyor 1.3 s");
    rig.step(61_234, Some((rx, ry, false)));
    rig.assert_same("reset uzerinde gezinme (1 dk+)");
    rig.step(65_000, Some((tx, ty, true)));
    rig.assert_same("durduruldu");
    rig.step(66_000, Some((rx, ry, true)));
    rig.assert_same("sifirlandi");
    rig.step(66_100, Some((rx + 5000, ty, false)));
    rig.assert_same("pencere disi isaretci");
}

#[test]
fn pixel_parity_in_every_language() {
    for language in [
        Language::Turkish, Language::English, Language::French, Language::German, Language::Spanish,
        Language::Portuguese, Language::Russian, Language::Chinese, Language::Japanese, Language::Korean,
        Language::Hindi,
    ] {
        scenario(language, 16);
    }
}

#[test]
fn pixel_parity_at_other_unit_sizes() {
    scenario(Language::English, 17);
    scenario(Language::Turkish, 20);
}

#[test]
fn string_table_matches_the_system_translations() {
    let languages = [
        Language::Turkish, Language::English, Language::French, Language::German, Language::Spanish,
        Language::Portuguese, Language::Russian, Language::Chinese, Language::Japanese, Language::Korean,
        Language::Hindi,
    ];
    assert_eq!(languages.len(), strings::LANGUAGES);
    for (index, language) in languages.iter().enumerate() {
        assert_eq!(*language as usize, index, "Language sirasi degisti: tools/gen_r3_strings.py guncellenmeli");
        let row = strings::for_language(index as u32);
        assert_eq!(row[strings::RUNNING], tr(*language, TextId::SwRunning));
        assert_eq!(row[strings::STOPPED], tr(*language, TextId::SwStopped));
        assert_eq!(row[strings::START], tr(*language, TextId::SwStart));
        assert_eq!(row[strings::STOP], tr(*language, TextId::SwStop));
        assert_eq!(row[strings::RESET], tr(*language, TextId::SwReset));
    }
    assert_eq!(strings::for_language(999), strings::for_language(1), "bilinmeyen dil Ingilizce'ye duser");
}

#[test]
fn theme_colors_match_nc_ui() {
    assert_eq!(nc_r3_stopwatch::TEXT_PRIMARY, nc_ui::TEXT_PRIMARY);
    assert_eq!(nc_r3_stopwatch::TEXT_DIM, nc_ui::TEXT_DIM);
}

#[test]
fn state_lives_in_the_app_between_events() {
    let mut rig = Rig::new(Language::English, 16);
    let (tx, ty) = rig.toggle_center();
    rig.step(1000, Some((tx, ty, true)));

    // Olaylar arasında yalnızca uygulama nesnesi taşınır: yeni Gfx, yeni tampon, yeni olay.
    rig.ctx.uptime_ms = 3500;
    let event = rig.event(OP_DRAW, None);
    let mut buf = [0u8; 8192];
    let mut gfx = Gfx::new(&mut buf);
    rig.r3.handle(&event, &mut gfx);
    let mut found = false;
    for item in nc_abi::draw::CmdReader::new(gfx.as_bytes()) {
        if let Ok(nc_abi::draw::Cmd::Text { text, .. }) = item {
            if text == "00:02.5" {
                found = true;
            }
        }
    }
    assert!(found, "1000 ms'de baslayip 3500 ms'de 2.5 s gostermeli");
}
