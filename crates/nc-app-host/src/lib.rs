//! Ring 3 ana-bilgisayar çalışma zamanı: mevcut `nc_app::App` uygulamalarını DEĞİŞTİRMEDEN Ring 3'te çalıştırır.
//!
//! Çekirdek her olayda uygulamayı baştan çağırır (olay-tabanlı model, bkz. `nc-abi`); durum bu crate'in
//! `static`'inde (`HOST`) ve uygulamanın heap'inde kalır. Çizim, pencere boyutunda bir piksel tamponuna yapılır:
//! `SYS_GUI_BACKDROP` altta kalan masaüstünü tampona kopyalar (alfa karışımı birebir aynı kalsın), uygulama
//! çizer, `SYS_GUI_PRESENT` tamponu pencereye kırparak basar. Uygulamaya pencere (0,0)-(w,h) olarak,
//! işaretçi pencereye göreli verilir (uygulamalar kaynak-bağımsızdır).
#![no_std]

extern crate alloc;

pub mod heap;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use nc_abi::ctx::{ContextBlock, CTX_OFFSET, CTX_SIZE, NO_CUSTOM_WALLPAPER, PLATFORM_MAX};
use nc_abi::event::{
    EventBlock, OP_CLOSE, OP_DRAW, OP_INIT, OP_UPDATE, PTR_CLICKED, PTR_DOWN, PTR_HAS, PTR_RIGHT_CLICKED,
    RET_CLOSE, RET_INSTALL, RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_app::{App, Context, Input, KeyCode, KeyboardLayout, Language, Pointer, Settings, Update};
use nc_gfx::{Canvas, Rect};

pub const MAX_SURFACE_DIM: i32 = 4096;
/// Piksel tamponunun asgari kapasitesi (1280x720). Yeniden boyutlandırmada yeniden ayırmayı önler.
const MIN_PIXEL_CAP: usize = 1280 * 720;

pub type AppFactory = fn(&mut Context) -> Box<dyn App>;

// ---- Dönüşümler (çekirdek de kullanır; ana makinede test edilir) -------------------------------

pub fn block_from_context(ctx: &Context) -> ContextBlock {
    let s = &ctx.settings;
    let mut block = ContextBlock {
        screen_w: ctx.screen_width,
        screen_h: ctx.screen_height,
        memory_mb: ctx.total_memory_mb,
        wallpaper: s.wallpaper as u32,
        accent: s.accent as u32,
        wallpaper_animated: s.wallpaper_animated,
        wallpaper_speed: s.wallpaper_speed,
        custom_wallpaper: s.custom_wallpaper.map(|v| v as u16).unwrap_or(NO_CUSTOM_WALLPAPER),
        glass_blur: s.glass_blur,
        dock_auto_hide: s.dock_auto_hide,
        launch_anim_speed: s.launch_anim_speed,
        wifi_enabled: s.wifi_enabled,
        bluetooth_enabled: s.bluetooth_enabled,
        language: s.language as u8,
        keyboard: s.keyboard as u8,
        dark_theme: s.dark_theme,
        platform_len: 0,
        platform: [0; PLATFORM_MAX],
    };
    block.set_platform(ctx.platform);
    block
}

pub fn settings_from_block(b: &ContextBlock) -> Settings {
    let language = Language::ALL.iter().copied().find(|l| *l as u8 == b.language).unwrap_or(Language::English);
    Settings {
        wallpaper: b.wallpaper as usize,
        accent: b.accent as usize,
        wallpaper_animated: b.wallpaper_animated,
        wallpaper_speed: b.wallpaper_speed,
        custom_wallpaper: if b.custom_wallpaper == NO_CUSTOM_WALLPAPER { None } else { Some(b.custom_wallpaper as u8) },
        glass_blur: b.glass_blur,
        dock_auto_hide: b.dock_auto_hide,
        launch_anim_speed: b.launch_anim_speed,
        wifi_enabled: b.wifi_enabled,
        bluetooth_enabled: b.bluetooth_enabled,
        language,
        keyboard: KeyboardLayout::from_u8(b.keyboard),
        dark_theme: b.dark_theme,
    }
}

pub fn encode_key(code: Option<KeyCode>) -> u32 {
    match code {
        None => key::NONE,
        Some(KeyCode::Enter) => key::ENTER,
        Some(KeyCode::Escape) => key::ESCAPE,
        Some(KeyCode::Space) => key::SPACE,
        Some(KeyCode::Tab) => key::TAB,
        Some(KeyCode::Backspace) => key::BACKSPACE,
        Some(KeyCode::Up) => key::UP,
        Some(KeyCode::Down) => key::DOWN,
        Some(KeyCode::Left) => key::LEFT,
        Some(KeyCode::Right) => key::RIGHT,
        Some(KeyCode::Char(c)) => key::from_char(c),
        Some(KeyCode::Unknown) => key::UNKNOWN,
    }
}

pub fn decode_key(code: u32) -> Option<KeyCode> {
    match code {
        key::NONE => None,
        key::ENTER => Some(KeyCode::Enter),
        key::ESCAPE => Some(KeyCode::Escape),
        key::SPACE => Some(KeyCode::Space),
        key::TAB => Some(KeyCode::Tab),
        key::BACKSPACE => Some(KeyCode::Backspace),
        key::UP => Some(KeyCode::Up),
        key::DOWN => Some(KeyCode::Down),
        key::LEFT => Some(KeyCode::Left),
        key::RIGHT => Some(KeyCode::Right),
        other => Some(key::as_char(other).map(KeyCode::Char).unwrap_or(KeyCode::Unknown)),
    }
}

pub fn input_from_event(ev: &EventBlock) -> Input {
    let pointer = if ev.pointer_flags & PTR_HAS != 0 {
        Some(Pointer {
            x: ev.pointer_x.wrapping_sub(ev.area_x),
            y: ev.pointer_y.wrapping_sub(ev.area_y),
            down: ev.pointer_flags & PTR_DOWN != 0,
            clicked: ev.pointer_flags & PTR_CLICKED != 0,
            right_clicked: ev.pointer_flags & PTR_RIGHT_CLICKED != 0,
        })
    } else {
        None
    };
    Input { area: Rect::new(0, 0, ev.area_w, ev.area_h), pointer, key: decode_key(ev.key), focused: true }
}

/// Uygulamanın kurmak istediği paketin `/data/app` yolu (Dosyalar'ın `persist_ntzc_to_data_app` ile AYNI kural).
pub fn install_path(app_id: &str) -> String {
    format!("/data/app/{}.ntzc", app_id.rsplit('.').next().unwrap_or(app_id))
}

// ---- Çekirdek G/Ç soyutlaması (gerçek: sistem çağrıları; test: sahte) -----------------------------

pub trait HostIo {
    fn backdrop(&mut self, surface: u64, pixels: &mut [u32], width: usize, height: usize);
    fn present(&mut self, surface: u64, pixels: &[u32], width: usize, height: usize);
    fn pkg_install(&mut self, path: &str) -> i64;
}

pub struct SysIo;

impl HostIo for SysIo {
    fn backdrop(&mut self, surface: u64, pixels: &mut [u32], width: usize, height: usize) {
        nc_app_sdk::sys::gui_backdrop(surface, pixels, width, height);
    }
    fn present(&mut self, surface: u64, pixels: &[u32], width: usize, height: usize) {
        nc_app_sdk::sys::gui_present(surface, pixels, width, height);
    }
    fn pkg_install(&mut self, path: &str) -> i64 {
        nc_app_sdk::sys::pkg_install(path)
    }
}

// ---- Ana-bilgisayar --------------------------------------------------------------------------

pub struct Host {
    app: Box<dyn App>,
    ctx: Context,
    pixels: Vec<u32>,
}

impl Host {
    pub fn new(factory: AppFactory, ev: &EventBlock, block: &ContextBlock) -> Host {
        let platform: &'static str = Box::leak(String::from(block.platform_str()).into_boxed_str());
        let mut ctx = Context {
            platform,
            screen_width: block.screen_w,
            screen_height: block.screen_h,
            unit: ev.unit,
            uptime_ms: ev.uptime_ms,
            settings: settings_from_block(block),
            total_memory_mb: block.memory_mb,
        };
        let app = factory(&mut ctx);
        Host { app, ctx, pixels: Vec::new() }
    }

    fn refresh(&mut self, ev: &EventBlock, block: &ContextBlock) {
        self.ctx.screen_width = block.screen_w;
        self.ctx.screen_height = block.screen_h;
        self.ctx.unit = ev.unit;
        self.ctx.uptime_ms = ev.uptime_ms;
        self.ctx.settings = settings_from_block(block);
        self.ctx.total_memory_mb = block.memory_mb;
    }

    pub fn handle(&mut self, ev: &EventBlock, block: &ContextBlock, io: &mut dyn HostIo) -> u32 {
        self.refresh(ev, block);
        match ev.op {
            OP_INIT => {
                self.app.opened(&mut self.ctx);
                RET_REDRAW
            }
            OP_UPDATE => {
                let input = input_from_event(ev);
                let update: Update = self.app.update(&mut self.ctx, &input);
                let mut ret = RET_NONE;
                if update.redraw {
                    ret |= RET_REDRAW;
                }
                if update.close {
                    ret |= RET_CLOSE;
                }
                if let Some(installed) = update.install_app {
                    let path = install_path(installed.info().id.as_str());
                    if io.pkg_install(&path) == 0 {
                        ret |= RET_INSTALL;
                    }
                }
                ret
            }
            OP_DRAW => {
                self.draw(ev, io);
                RET_NONE
            }
            OP_CLOSE => {
                self.app.closed(&mut self.ctx);
                RET_NONE
            }
            _ => RET_NONE,
        }
    }

    fn draw(&mut self, ev: &EventBlock, io: &mut dyn HostIo) {
        if ev.area_w <= 0 || ev.area_h <= 0 || ev.area_w > MAX_SURFACE_DIM || ev.area_h > MAX_SURFACE_DIM {
            return;
        }
        let (w, h) = (ev.area_w as usize, ev.area_h as usize);
        let need = w * h;
        if self.pixels.len() != need {
            if need > self.pixels.capacity() {
                // Büyüme: eski tamponu ÖNCE bırak (eski+yeni birlikte heap'te durmasın) ve en az
                // MIN_PIXEL_CAP kapasite ayır; böylece pencere boyutu değişirken heap parçalanmaz.
                self.pixels = Vec::new();
                let mut grown: Vec<u32> = Vec::new();
                if grown.try_reserve_exact(need.max(MIN_PIXEL_CAP)).is_err() {
                    // Bellek yok: uygulamayı panikle öldürmek yerine bu kareyi atla.
                    return;
                }
                self.pixels = grown;
            }
            // resize() sayfaları dokunur: çekirdek, dokunulmamış talep-sayfasına yazamaz.
            self.pixels.clear();
            self.pixels.resize(need, 0);
        }
        io.backdrop(ev.surface, &mut self.pixels, w, h);
        if let Some(mut canvas) = Canvas::new(&mut self.pixels, w, h) {
            self.app.draw(&mut canvas, Rect::new(0, 0, ev.area_w, ev.area_h), &self.ctx);
        }
        io.present(ev.surface, &self.pixels, w, h);
    }
}

static mut HOST: Option<Host> = None;
static mut HEAP_READY: bool = false;

fn read_block() -> Option<ContextBlock> {
    let bytes = unsafe {
        core::slice::from_raw_parts((nc_abi::USER_EVENT_PAGE as usize + CTX_OFFSET) as *const u8, CTX_SIZE)
    };
    ContextBlock::decode(bytes)
}

/// Uygulama ikilisinin `_start`'ı bunu çağırır. `heap`, ikilideki `#[global_allocator]`'dır.
pub fn run(heap: &heap::GlobalHeap, factory: AppFactory) -> ! {
    let Some(ev) = nc_app_sdk::read_event() else {
        nc_app_sdk::sys::app_done(0)
    };
    unsafe {
        if !*core::ptr::addr_of!(HEAP_READY) {
            heap.init(ev.heap_base as usize, ev.heap_size as usize);
            *core::ptr::addr_of_mut!(HEAP_READY) = true;
        }
    }
    let Some(block) = read_block() else {
        nc_app_sdk::sys::app_done(0)
    };
    let slot = unsafe { &mut *core::ptr::addr_of_mut!(HOST) };
    if slot.is_none() {
        *slot = Some(Host::new(factory, &ev, &block));
    }
    let ret = match slot.as_mut() {
        Some(host) => host.handle(&ev, &block, &mut SysIo),
        None => 0,
    };
    nc_app_sdk::sys::app_done(ret)
}

#[cfg(test)]
mod tests;
