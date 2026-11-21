//! NetizenCore Public Widget SDK v1 (`nc_app_sdk::widget`).
//!
//! Versioned, `#![no_std]`, zero-allocation interface for hosting home screen widgets
//! inside `nc-home-r3` without tightly coupling widget implementations to launcher internals.
//!
//! Both built-in system widgets and future third-party Ring 3 widget providers implement
//! `WidgetProvider` and register `WidgetMeta` descriptors.

use crate::Gfx;

/// Public Widget SDK ABI version.
pub const WIDGET_API_VERSION: u32 = 1;

/// Supported widget size classes on the NetizenHome grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WidgetSize {
    /// Compact 2x1 cells (e.g. mini clock, battery pill, quick status).
    Small = 1,
    /// Wide 4x1 cells (e.g. full clock + date, storage bar, connectivity row).
    Medium = 2,
    /// Expanded 4x2 cells (e.g. full calendar month, detailed system overview).
    Large = 3,
}

impl WidgetSize {
    pub const fn bit(self) -> u8 {
        match self {
            WidgetSize::Small => 1 << 0,
            WidgetSize::Medium => 1 << 1,
            WidgetSize::Large => 1 << 2,
        }
    }

    pub const fn grid_span(self, cols: usize) -> (u8, u8) {
        let max_c = if cols < 2 { 2 } else { cols as u8 };
        match self {
            WidgetSize::Small => (2, 1),
            WidgetSize::Medium => (max_c, 1),
            WidgetSize::Large => (max_c, 2),
        }
    }

    pub const fn from_u8(v: u8) -> Option<WidgetSize> {
        match v {
            1 => Some(WidgetSize::Small),
            2 => Some(WidgetSize::Medium),
            3 => Some(WidgetSize::Large),
            _ => None,
        }
    }

    pub const fn label(self, en: bool) -> &'static str {
        match (self, en) {
            (WidgetSize::Small, true) => "Small",
            (WidgetSize::Small, false) => "Küçük",
            (WidgetSize::Medium, true) => "Medium",
            (WidgetSize::Medium, false) => "Orta",
            (WidgetSize::Large, true) => "Large",
            (WidgetSize::Large, false) => "Büyük",
        }
    }
}

pub const SIZE_SMALL: u8 = WidgetSize::Small.bit();
pub const SIZE_MEDIUM: u8 = WidgetSize::Medium.bit();
pub const SIZE_LARGE: u8 = WidgetSize::Large.bit();

/// Visual preview category used by the Widget Picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WidgetPreviewKind {
    Clock = 1,
    Battery = 2,
    SystemInfo = 3,
    Connectivity = 4,
    Calendar = 5,
    Storage = 6,
    Generic = 7,
}

/// Static descriptor advertised by every widget provider.
#[derive(Clone, Copy)]
pub struct WidgetMeta {
    pub id: &'static str,
    pub provider_app_id: &'static str,
    pub name_en: &'static str,
    pub name_tr: &'static str,
    pub desc_en: &'static str,
    pub desc_tr: &'static str,
    pub supported_sizes: u8,
    pub default_size: WidgetSize,
    pub min_cols: u8,
    pub min_rows: u8,
    pub max_cols: u8,
    pub max_rows: u8,
    /// Minimum polling cadence in ms when event-driven updates are not sufficient (0 = event-driven only).
    pub update_interval_ms: u32,
    pub configurable: bool,
    pub preview: WidgetPreviewKind,
}

impl WidgetMeta {
    pub const fn supports(&self, size: WidgetSize) -> bool {
        (self.supported_sizes & size.bit()) != 0
    }

    pub const fn next_size(&self, current: WidgetSize) -> WidgetSize {
        let order = match current {
            WidgetSize::Small => [WidgetSize::Medium, WidgetSize::Large, WidgetSize::Small],
            WidgetSize::Medium => [WidgetSize::Large, WidgetSize::Small, WidgetSize::Medium],
            WidgetSize::Large => [WidgetSize::Small, WidgetSize::Medium, WidgetSize::Large],
        };
        let mut i = 0;
        while i < 3 {
            if self.supports(order[i]) {
                return order[i];
            }
            i += 1;
        }
        current
    }

    pub const fn display_name(&self, en: bool) -> &'static str {
        if en { self.name_en } else { self.name_tr }
    }

    pub const fn description(&self, en: bool) -> &'static str {
        if en { self.desc_en } else { self.desc_tr }
    }
}

/// Persistent widget instance placed on a NetizenHome page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetInstance {
    pub instance_id: u16,
    pub provider_idx: u8,
    pub page: u8,
    pub col: u8,
    pub row: u8,
    pub size: WidgetSize,
    /// Provider-specific configuration bitfield (persisted with layout).
    pub config_flags: u16,
    pub active: bool,
}

impl WidgetInstance {
    pub const EMPTY: WidgetInstance = WidgetInstance {
        instance_id: 0,
        provider_idx: 0,
        page: 0,
        col: 0,
        row: 0,
        size: WidgetSize::Medium,
        config_flags: 0,
        active: false,
    };
}

/// Action returned by a widget when tapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetAction {
    None,
    Redraw,
    OpenControlCenter,
    ToggleWifi,
    ToggleTheme,
    LaunchApp(&'static str),
    ToggleConfigBit(u16),
}

/// Pixel bounds passed to `WidgetProvider::render`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Real system data snapshot supplied to widgets during rendering (never fabricated).
#[derive(Clone, Copy)]
pub struct WidgetContext {
    pub rect: WidgetRect,
    pub size: WidgetSize,
    pub config_flags: u16,
    pub dark_theme: bool,
    pub accent: u32,
    pub lang_en: bool,
    pub wall_clock_unix: i64,
    pub tz_hours: i32,
    pub uptime_s: u32,
    pub bat_available: bool,
    pub bat_percent: u8,
    pub bat_charging: bool,
    pub bat_full: bool,
    pub bat_voltage_mv: u16,
    pub wifi_hw: bool,
    pub wifi_enabled: bool,
    pub wifi_state: u8,
    pub bt_hw: bool,
    pub bt_enabled: bool,
    pub cell_hw: bool,
    pub dnd: bool,
    pub ram_total_mib: u32,
    pub ram_free_mib: u32,
    pub data_total_mib: u32,
    pub data_used_kib: u32,
    pub live_tasks: u32,
    pub usb_mounted: bool,
    pub edit_mode: bool,
}

/// Trait implemented by every NetizenCore widget provider.
pub trait WidgetProvider {
    fn meta(&self) -> WidgetMeta;
    fn render(&self, g: &mut Gfx<'_>, ctx: &WidgetContext);
    fn on_tap(&self, rel_x: i32, rel_y: i32, ctx: &WidgetContext) -> WidgetAction {
        let _ = (rel_x, rel_y, ctx);
        WidgetAction::None
    }
}
