#![no_std]

extern crate alloc;

pub mod icon;
pub mod text;
pub mod theme;
pub mod locale;
pub mod security;
pub mod wallpaper;


use alloc::boxed::Box;
use alloc::string::String;
use nc_gfx::{Canvas, Rect};

pub use icon::{draw_icon, Icon};
pub use nc_hal::KeyCode;
pub use locale::{tr, trf, KeyboardLayout, Language, TextId};

pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub icon: Icon,
    pub width_units: i32,
    pub height_units: i32,
    pub title: Option<TextId>,
}

impl AppInfo {
    pub fn localized(
        id: &str,
        fallback_name: &str,
        title: TextId,
        version: &str,
        icon: Icon,
        width_units: i32,
        height_units: i32,
    ) -> AppInfo {
        let mut info = AppInfo::new(id, fallback_name, version, icon, width_units, height_units);
        info.title = Some(title);
        info
    }

    pub fn display_name(&self, lang: Language) -> &str {
        match self.title {
            Some(id) => tr(lang, id),
            None => &self.name,
        }
    }

    pub fn new(
        id: &str,
        name: &str,
        version: &str,
        icon: Icon,
        width_units: i32,
        height_units: i32,
    ) -> AppInfo {
        AppInfo {
            id: String::from(id),
            name: String::from(name),
            version: String::from(version),
            icon,
            width_units,
            height_units,
            title: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub wallpaper: usize,
    pub accent: usize,
    pub wallpaper_animated: bool,
    pub wallpaper_speed: u8,
    pub custom_wallpaper: Option<u8>,
    pub glass_blur: u8,
    pub dock_auto_hide: bool,
    pub launch_anim_speed: u8,
    pub wifi_enabled: bool,
    pub bluetooth_enabled: bool,
    pub language: Language,
    pub keyboard: KeyboardLayout,
    pub dark_theme: bool,
}

impl Settings {
    pub const fn new() -> Settings {
        Settings {
            wallpaper: 0,
            accent: 0,
            wallpaper_animated: false, // Boşta sıfır CPU/GPU yükü (Ayarlar'dan açılabilir)
            wallpaper_speed: 2,
            custom_wallpaper: None,
            glass_blur: 1,             // Hafif ve akıcı cam geçişi
            dock_auto_hide: false,     // Sabit ve sade dock (fare hareketinde sürekli animasyon tetiklemez)
            launch_anim_speed: 3,      // Anında pencere açılışı
            wifi_enabled: true,
            bluetooth_enabled: true,
            language: Language::Turkish,
            keyboard: KeyboardLayout::TurkishQ,
            dark_theme: false,
        }
    }

    pub fn accent_color(&self) -> u32 {
        theme::ACCENTS[self.accent % theme::ACCENT_COUNT]
    }
}

#[derive(Clone, Copy)]
pub struct Context {
    pub platform: &'static str,
    pub screen_width: i32,
    pub screen_height: i32,
    pub unit: i32,
    pub uptime_ms: u32,
    pub settings: Settings,
    pub total_memory_mb: u32,
}

#[derive(Clone, Copy)]
pub struct Pointer {
    pub x: i32,
    pub y: i32,
    pub down: bool,
    pub clicked: bool,
    pub right_clicked: bool,
}

#[derive(Clone, Copy)]
pub struct Input {
    pub area: Rect,
    pub pointer: Option<Pointer>,
    pub key: Option<KeyCode>,
    pub focused: bool,
}

pub struct Update {
    pub redraw: bool,
    pub close: bool,
    pub install_app: Option<Box<dyn App>>,
}

impl Update {
    pub const NONE: Update = Update {
        redraw: false,
        close: false,
        install_app: None,
    };
    pub const REDRAW: Update = Update {
        redraw: true,
        close: false,
        install_app: None,
    };
    pub const CLOSE: Update = Update {
        redraw: true,
        close: true,
        install_app: None,
    };

    pub fn install(app: Box<dyn App>) -> Update {
        Update {
            redraw: true,
            close: false,
            install_app: Some(app),
        }
    }
}

pub trait App {
    fn info(&self) -> &AppInfo;
    fn opened(&mut self, _ctx: &mut Context) {}
    fn closed(&mut self, _ctx: &mut Context) {}
    fn update(&mut self, ctx: &mut Context, input: &Input) -> Update;
    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context);
}
