use super::*;
use alloc::string::ToString;
use nc_abi::event::OP_UPDATE;
use nc_app::{AppInfo, Icon};

fn base_ctx(language: Language) -> Context {
    Context {
        platform: "QEMU / KVM Sanal Donanim",
        screen_width: 1280,
        screen_height: 720,
        unit: 16,
        uptime_ms: 5,
        settings: Settings { language, accent: 2, dark_theme: true, custom_wallpaper: Some(7), ..Settings::new() },
        total_memory_mb: 492,
    }
}

#[test]
fn context_roundtrips_through_the_block_in_every_language() {
    for language in Language::ALL {
        let ctx = base_ctx(language);
        let block = block_from_context(&ctx);
        let back = settings_from_block(&ContextBlock::decode(&block.encode()).unwrap());
        assert_eq!(back.language, ctx.settings.language);
        assert_eq!(back.accent, ctx.settings.accent);
        assert_eq!(back.custom_wallpaper, Some(7));
        assert_eq!(back.keyboard, ctx.settings.keyboard);
        assert!(back.dark_theme);
        assert_eq!(block.platform_str(), "QEMU / KVM Sanal Donanim");
        assert_eq!((block.screen_w, block.screen_h, block.memory_mb), (1280, 720, 492));
    }
    let mut none = base_ctx(Language::English);
    none.settings.custom_wallpaper = None;
    assert_eq!(settings_from_block(&block_from_context(&none)).custom_wallpaper, None);
}

#[test]
fn keys_roundtrip() {
    let keys = [
        None, Some(KeyCode::Enter), Some(KeyCode::Escape), Some(KeyCode::Space), Some(KeyCode::Tab),
        Some(KeyCode::Backspace), Some(KeyCode::Up), Some(KeyCode::Down), Some(KeyCode::Left), Some(KeyCode::Right),
        Some(KeyCode::Char('a')), Some(KeyCode::Char('ş')), Some(KeyCode::Unknown),
    ];
    for k in keys {
        let back = decode_key(encode_key(k));
        match (k, back) {
            (None, None) => {}
            (Some(a), Some(b)) => assert_eq!(core::mem::discriminant(&a), core::mem::discriminant(&b)),
            _ => panic!("tus gidis-donus bozuk"),
        }
        if let (Some(KeyCode::Char(a)), Some(KeyCode::Char(b))) = (k, back) {
            assert_eq!(a, b);
        }
    }
    assert!(matches!(decode_key(key::CHAR_FLAG | 0xD800), Some(KeyCode::Unknown)));
    assert!(matches!(decode_key(0x7FFF), Some(KeyCode::Unknown)));
}

#[test]
fn input_is_window_relative() {
    let ev = EventBlock {
        op: OP_UPDATE,
        area_x: 100,
        area_y: 50,
        area_w: 640,
        area_h: 480,
        pointer_flags: PTR_HAS | PTR_CLICKED | PTR_RIGHT_CLICKED,
        pointer_x: 130,
        pointer_y: 45,
        key: key::from_char('x'),
        ..Default::default()
    };
    let input = input_from_event(&ev);
    assert_eq!((input.area.x, input.area.y, input.area.width, input.area.height), (0, 0, 640, 480));
    let p = input.pointer.unwrap();
    assert_eq!((p.x, p.y, p.clicked, p.right_clicked, p.down), (30, -5, true, true, false));
    assert!(matches!(input.key, Some(KeyCode::Char('x'))));
    assert!(input_from_event(&EventBlock::default()).pointer.is_none());
}

#[test]
fn install_path_matches_files_persist_rule() {
    assert_eq!(install_path("nc.user.sayac"), "/data/app/sayac.ntzc");
    assert_eq!(install_path("plain"), "/data/app/plain.ntzc");
}

// ---- Sahte G/Ç ile uçtan uca ana-bilgisayar davranışı -------------------------------------------

struct MockIo {
    desktop: u32,
    presented: Vec<u32>,
    backdrop_calls: u32,
    installs: Vec<String>,
    install_result: i64,
}

impl HostIo for MockIo {
    fn backdrop(&mut self, _surface: u64, pixels: &mut [u32], _w: usize, _h: usize) {
        self.backdrop_calls += 1;
        pixels.fill(self.desktop);
    }
    fn present(&mut self, _surface: u64, pixels: &[u32], _w: usize, _h: usize) {
        self.presented = pixels.to_vec();
    }
    fn pkg_install(&mut self, path: &str) -> i64 {
        self.installs.push(path.to_string());
        self.install_result
    }
}

struct Probe {
    info: AppInfo,
    opened: u32,
    closed: u32,
    last_pointer: Option<(i32, i32)>,
    seen_backdrop: u32,
    want_install: bool,
}

impl App for Probe {
    fn info(&self) -> &AppInfo {
        &self.info
    }
    fn opened(&mut self, _: &mut Context) {
        self.opened += 1;
    }
    fn closed(&mut self, _: &mut Context) {
        self.closed += 1;
    }
    fn update(&mut self, _: &mut Context, input: &Input) -> Update {
        self.last_pointer = input.pointer.map(|p| (p.x, p.y));
        if self.want_install {
            self.want_install = false;
            return Update::install(Box::new(Probe {
                info: AppInfo::new("nc.user.yeni", "Yeni", "1", Icon::Generic, 10, 10),
                opened: 0,
                closed: 0,
                last_pointer: None,
                seen_backdrop: 0,
                want_install: false,
            }));
        }
        Update { redraw: input.pointer.is_some(), close: false, install_app: None }
    }
    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, _: &Context) {
        // Altta kalan masaüstü arka planı görünmeli (alfa karışımı için şart).
        let under = canvas.pixels()[0];
        canvas.fill_rect(area.x + 1, area.y + 1, 2, 2, under ^ 0x00FF_FFFF);
    }
}

fn probe(_: &mut Context) -> Box<dyn App> {
    Box::new(Probe {
        info: AppInfo::new("nc.test", "Test", "1", Icon::Generic, 10, 10),
        opened: 0,
        closed: 0,
        last_pointer: None,
        seen_backdrop: 0,
        want_install: false,
    })
}

fn rig() -> (Host, ContextBlock, MockIo) {
    let ctx = base_ctx(Language::Turkish);
    let block = block_from_context(&ctx);
    let ev = EventBlock { op: OP_INIT, unit: 16, ..Default::default() };
    let host = Host::new(probe, &ev, &block);
    (host, block, MockIo { desktop: 0x0011_2233, presented: Vec::new(), backdrop_calls: 0, installs: Vec::new(), install_result: 0 })
}

#[test]
fn draw_sequence_is_backdrop_then_app_then_present() {
    let (mut host, block, mut io) = rig();
    let ev = EventBlock { op: OP_DRAW, area_w: 8, area_h: 4, surface: 3, unit: 16, ..Default::default() };
    assert_eq!(host.handle(&ev, &block, &mut io), RET_NONE);
    assert_eq!(io.backdrop_calls, 1);
    assert_eq!(io.presented.len(), 32);
    assert_eq!(io.presented[0], 0x0011_2233, "kosesi cizilmemis piksel masaustu olmali");
    assert_eq!(io.presented[1 * 8 + 1], 0x0011_2233 ^ 0x00FF_FFFF, "uygulama arka plani gorup ustune cizdi");
    // ikinci çizim aynı tamponu yeniden kullanır
    host.handle(&ev, &block, &mut io);
    assert_eq!(io.backdrop_calls, 2);
}

#[test]
fn invalid_or_huge_surfaces_are_not_drawn() {
    let (mut host, block, mut io) = rig();
    for (w, h) in [(0, 10), (10, 0), (-5, 10), (MAX_SURFACE_DIM + 1, 10), (10, MAX_SURFACE_DIM + 1)] {
        let ev = EventBlock { op: OP_DRAW, area_w: w, area_h: h, ..Default::default() };
        host.handle(&ev, &block, &mut io);
    }
    assert_eq!(io.backdrop_calls, 0);
    assert!(io.presented.is_empty());
}

#[test]
fn lifecycle_and_update_mapping() {
    let (mut host, block, mut io) = rig();
    assert_eq!(host.handle(&EventBlock { op: OP_INIT, ..Default::default() }, &block, &mut io), RET_REDRAW);
    let ev = EventBlock {
        op: OP_UPDATE,
        area_x: 10,
        area_y: 20,
        area_w: 100,
        area_h: 100,
        pointer_flags: PTR_HAS,
        pointer_x: 15,
        pointer_y: 26,
        ..Default::default()
    };
    assert_eq!(host.handle(&ev, &block, &mut io) & RET_REDRAW, RET_REDRAW);
    assert_eq!(host.handle(&EventBlock { op: OP_UPDATE, ..Default::default() }, &block, &mut io), RET_NONE);
    host.handle(&EventBlock { op: OP_CLOSE, ..Default::default() }, &block, &mut io);
    assert_eq!(host.handle(&EventBlock { op: 99, ..Default::default() }, &block, &mut io), RET_NONE);
}

#[test]
fn install_request_is_forwarded_to_the_kernel_not_the_app() {
    let (mut host, block, mut io) = rig();
    // Probe::update ilk çağrıda install döndürür: erişilebilir bir alan olmadığından kurulum bayrağını elle tetikle.
    let ev = EventBlock { op: OP_UPDATE, ..Default::default() };
    // want_install'ı true yapmak için yeni bir ana-bilgisayar: factory kendi bayrağını açar.
    fn installer(_: &mut Context) -> Box<dyn App> {
        Box::new(Probe {
            info: AppInfo::new("nc.test", "Test", "1", Icon::Generic, 10, 10),
            opened: 0,
            closed: 0,
            last_pointer: None,
            seen_backdrop: 0,
            want_install: true,
        })
    }
    let _ = &mut host;
    let ctx = base_ctx(Language::English);
    let mut h2 = Host::new(installer, &ev, &block_from_context(&ctx));
    let ret = h2.handle(&ev, &block, &mut io);
    assert_eq!(ret & RET_INSTALL, RET_INSTALL);
    assert_eq!(io.installs, ["/data/app/yeni.ntzc"]);
    io.install_result = -13;
    h2 = Host::new(installer, &ev, &block_from_context(&ctx));
    let ret = h2.handle(&ev, &block, &mut io);
    assert_eq!(ret & RET_INSTALL, 0, "cekirdek reddederse bayrak YOK");
}
