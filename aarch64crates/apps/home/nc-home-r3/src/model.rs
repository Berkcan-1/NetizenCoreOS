use nc_abi::draw::{
    ICON_BROWSER, ICON_CAMERA, ICON_CONTACTS, ICON_FILES, ICON_GALLERY, ICON_GENERIC,
    ICON_KEYBOARD, ICON_MESSAGES, ICON_PHONE, ICON_SETTINGS, ICON_STOPWATCH, ICON_SYSTEM,
};
use nc_app_sdk::apps::AppEntry;
use nc_app_sdk::widget::WidgetInstance;

pub const MAX_APPS: usize = 24;
pub const MAX_PAGES: usize = 4;
pub const MAX_COLS: usize = 4;
pub const MAX_ROWS: usize = 6;
pub const MAX_CELLS: usize = MAX_COLS * MAX_ROWS;
pub const DOCK_SLOTS: usize = 4;
pub const FOLDER_SLOTS: usize = 4;
pub const MAX_WIDGETS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    Empty,
    App(u8),
    Folder([Option<u8>; FOLDER_SLOTS]),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Grid(u8, usize),
    Dock(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GridDensity {
    /// 4 columns x 5 rows (balanced default)
    Balanced = 0,
    /// 4 columns x 6 rows (compact)
    Compact = 1,
    /// 3 columns x 4 rows (spacious)
    Large = 2,
}

impl GridDensity {
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Compact,
            2 => Self::Large,
            _ => Self::Balanced,
        }
    }

    pub const fn cols(self) -> usize {
        match self {
            Self::Large => 3,
            Self::Balanced | Self::Compact => 4,
        }
    }

    pub const fn max_rows(self) -> usize {
        match self {
            Self::Compact => 6,
            Self::Balanced => 5,
            Self::Large => 4,
        }
    }

    pub const fn label(self, en: bool) -> &'static str {
        match (self, en) {
            (Self::Balanced, true) => "4x5",
            (Self::Balanced, false) => "4x5",
            (Self::Compact, true) => "4x6",
            (Self::Compact, false) => "4x6",
            (Self::Large, true) => "3x4",
            (Self::Large, false) => "3x4",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IconSizePref {
    Small = 0,
    Medium = 1,
    Large = 2,
}

impl IconSizePref {
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Small,
            2 => Self::Large,
            _ => Self::Medium,
        }
    }

    pub const fn dp(self) -> i32 {
        match self {
            Self::Small => 44,
            Self::Medium => 48,
            Self::Large => 54,
        }
    }

    pub const fn label(self, en: bool) -> &'static str {
        match (self, en) {
            (Self::Small, true) => "Small",
            (Self::Small, false) => "Küçük",
            (Self::Medium, true) => "Medium",
            (Self::Medium, false) => "Orta",
            (Self::Large, true) => "Large",
            (Self::Large, false) => "Büyük",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TransitionStyle {
    Slide = 0,
    Fade = 1,
    Depth = 2,
}

impl TransitionStyle {
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Fade,
            2 => Self::Depth,
            _ => Self::Slide,
        }
    }

    pub const fn label(self, en: bool) -> &'static str {
        match (self, en) {
            (Self::Slide, true) => "Slide",
            (Self::Slide, false) => "Kaydır",
            (Self::Fade, true) => "Fade",
            (Self::Fade, false) => "Solma",
            (Self::Depth, true) => "Depth",
            (Self::Depth, false) => "Derinlik",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct HomePrefs {
    pub density: GridDensity,
    pub icon_size: IconSizePref,
    pub show_labels: bool,
    pub pill_indicator: bool,
    pub transition: TransitionStyle,
}

impl HomePrefs {
    pub const DEFAULT: HomePrefs = HomePrefs {
        density: GridDensity::Balanced,
        icon_size: IconSizePref::Medium,
        show_labels: true,
        pill_indicator: true,
        transition: TransitionStyle::Slide,
    };
}

pub fn is_launcher_visible(app: &AppEntry) -> bool {
    app.id_str() != "nc.package-installer" && app.id_str() != "nc.keyboard"
}

pub fn app_name<'a>(app: &'a AppEntry, en: bool) -> &'a str {
    match app.id_str() {
        "nc.files" => if en { "Files" } else { "Dosyalar" },
        "nc.settings" => if en { "Settings" } else { "Ayarlar" },
        "nc.package-installer" => if en { "Installer" } else { "Kurucu" },
        "nc.keyboard" => if en { "Keyboard" } else { "Klavye" },
        "nc.gallery" => if en { "Gallery" } else { "Galeri" },
        "nc.messages" => if en { "Messages" } else { "Mesajlar" },
        "nc.phone" => if en { "Phone" } else { "Telefon" },
        "nc.contacts" => if en { "Contacts" } else { "Kişiler" },
        "nc.camera" => if en { "Camera" } else { "Kamera" },
        _ => app.name_str(),
    }
}

pub fn app_icon_id(app: &AppEntry) -> u32 {
    match app.icon_id {
        ICON_FILES
        | ICON_SETTINGS
        | ICON_BROWSER
        | ICON_SYSTEM
        | ICON_STOPWATCH
        | ICON_GENERIC
        | ICON_KEYBOARD
        | ICON_GALLERY
        | ICON_MESSAGES
        | ICON_PHONE
        | ICON_CONTACTS
        | ICON_CAMERA => app.icon_id,
        _ => ICON_GENERIC,
    }
}

pub fn app_color(app: &AppEntry, accent: u32) -> u32 {
    match app.icon_id {
        ICON_FILES => 0x002B_6CB0,
        ICON_SETTINGS => 0x0047_5569,
        ICON_BROWSER => 0x000E_A5E9,
        ICON_SYSTEM => 0x0033_4155,
        ICON_STOPWATCH => 0x00F5_9E0B,
        ICON_GALLERY => 0x007A_5AF8,
        ICON_MESSAGES => 0x000A_59F7,
        ICON_PHONE => 0x0007_C160,
        ICON_CONTACTS => 0x000E_A5E9,
        ICON_CAMERA => 0x0033_4155,
        _ => accent,
    }
}

pub fn folder_count(slots: &[Option<u8>; FOLDER_SLOTS]) -> usize {
    slots.iter().filter(|s| s.is_some()).count()
}

pub fn widget_covers(w: &WidgetInstance, page: u8, col: usize, row: usize, cols: usize) -> bool {
    if !w.active || w.page != page {
        return false;
    }
    let (sc, sr) = w.size.grid_span(cols);
    let c0 = w.col as usize;
    let r0 = w.row as usize;
    col >= c0 && col < c0 + sc as usize && row >= r0 && row < r0 + sr as usize
}

pub fn cell_covered_by_widgets(widgets: &[WidgetInstance; MAX_WIDGETS], page: u8, idx: usize, cols: usize) -> bool {
    let col = idx % cols;
    let row = idx / cols;
    widgets.iter().any(|w| widget_covers(w, page, col, row, cols))
}

pub fn lower_contains(haystack: &str, needle: &str) -> bool {
    let mut hay = [' '; 36];
    let mut hn = 0;
    for c in haystack.chars().flat_map(|c| c.to_lowercase()) {
        if hn < hay.len() {
            hay[hn] = c;
            hn += 1;
        }
    }
    let mut nd = [' '; 36];
    let mut nn = 0;
    for c in needle.chars().flat_map(|c| c.to_lowercase()) {
        if nn < nd.len() {
            nd[nn] = c;
            nn += 1;
        }
    }
    if nn == 0 {
        return true;
    }
    if nn > hn {
        return false;
    }
    (0..=hn - nn).any(|i| hay[i..i + nn] == nd[..nn])
}
