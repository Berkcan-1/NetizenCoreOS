//! Mature NetizenHome Widget Host & Built-in System Widget Providers.
//!
//! All widgets implement `nc_app_sdk::widget::WidgetProvider` (Widget SDK v1).
//! Data is always real (`WidgetContext` sourced from kernel `OP_SYSTEM_STATUS` and RTC);
//! when hardware or provider data is unavailable, widgets display a clean, honest state.

use nc_app_sdk::status::CONN_CONNECTED;
use nc_app_sdk::widget::{
    WidgetAction, WidgetContext, WidgetMeta, WidgetPreviewKind, WidgetProvider, WidgetSize,
    SIZE_LARGE, SIZE_MEDIUM, SIZE_SMALL,
};
use nc_app_sdk::{Gfx, Line};
use nc_mobile_ui::{
    draw_battery_icon, draw_dnd_glyph, draw_wifi_status_icon, GlassTheme, AMBER, CORAL, GREEN,
};

use crate::clock::{
    days_in_month, format_time, from_unix_tz, month_start_monday_index, DAYS, DAYS_EN,
    DAYS_SHORT_EN, DAYS_SHORT_TR, MONTHS, MONTHS_EN,
};

pub const PROVIDER_COUNT: usize = 6;

pub const PROVIDER_CLOCK: u8 = 0;
pub const PROVIDER_BATTERY: u8 = 1;
pub const PROVIDER_SYSINFO: u8 = 2;
pub const PROVIDER_CONNECTIVITY: u8 = 3;
pub const PROVIDER_CALENDAR: u8 = 4;
pub const PROVIDER_STORAGE: u8 = 5;

struct ClockWidget;
struct BatteryWidget;
struct SystemInfoWidget;
struct ConnectivityWidget;
struct CalendarWidget;
struct StorageWidget;

static W_CLOCK: ClockWidget = ClockWidget;
static W_BATTERY: BatteryWidget = BatteryWidget;
static W_SYSINFO: SystemInfoWidget = SystemInfoWidget;
static W_CONN: ConnectivityWidget = ConnectivityWidget;
static W_CAL: CalendarWidget = CalendarWidget;
static W_STORAGE: StorageWidget = StorageWidget;

pub fn provider_by_idx(idx: u8) -> Option<&'static dyn WidgetProvider> {
    match idx {
        PROVIDER_CLOCK => Some(&W_CLOCK),
        PROVIDER_BATTERY => Some(&W_BATTERY),
        PROVIDER_SYSINFO => Some(&W_SYSINFO),
        PROVIDER_CONNECTIVITY => Some(&W_CONN),
        PROVIDER_CALENDAR => Some(&W_CAL),
        PROVIDER_STORAGE => Some(&W_STORAGE),
        _ => None,
    }
}

pub fn provider_meta(idx: u8) -> Option<WidgetMeta> {
    provider_by_idx(idx).map(|p| p.meta())
}

pub fn provider_idx_by_id(id: &str) -> Option<u8> {
    for i in 0..PROVIDER_COUNT as u8 {
        if let Some(p) = provider_by_idx(i) {
            if p.meta().id == id {
                return Some(i);
            }
        }
    }
    None
}

fn card_surface(g: &mut Gfx<'_>, ctx: &WidgetContext) -> GlassTheme {
    let th = GlassTheme::for_dark(ctx.dark_theme);
    let r = ctx.rect;
    let rad = (r.h.min(r.w) / 5).clamp(14, 22);
    g.panel(r.x, r.y, r.w, r.h, rad, th.card_bg, th.card_alpha, th.rim, th.rim_alpha);
    th
}

/// Renders a fault-isolated fallback card when a widget provider index is invalid.
pub fn render_unavailable_provider(g: &mut Gfx<'_>, ctx: &WidgetContext) {
    let th = card_surface(g, ctx);
    let r = ctx.rect;
    g.text_centered(
        r.x + r.w / 2,
        r.y + r.h / 2 - 5,
        9,
        th.muted,
        1,
        if ctx.lang_en { "Widget unavailable" } else { "Araç takımı yok" },
    );
}

// ---------------------------------------------------------------------------
// 1. Clock Widget (Small 2x1, Medium 4x1, Large 4x2)
//    config_flags bit 0: 12-hour format
// ---------------------------------------------------------------------------
impl WidgetProvider for ClockWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "nc.widget.clock",
            provider_app_id: "nc.home",
            name_en: "Clock",
            name_tr: "Saat",
            desc_en: "Current time and date",
            desc_tr: "Güncel saat ve tarih",
            supported_sizes: SIZE_SMALL | SIZE_MEDIUM | SIZE_LARGE,
            default_size: WidgetSize::Medium,
            min_cols: 2,
            min_rows: 1,
            max_cols: 4,
            max_rows: 2,
            update_interval_ms: 60_000,
            configurable: true,
            preview: WidgetPreviewKind::Clock,
        }
    }

    fn render(&self, g: &mut Gfx<'_>, ctx: &WidgetContext) {
        let th = card_surface(g, ctx);
        let r = ctx.rect;
        let en = ctx.lang_en;
        let use_12h = (ctx.config_flags & 1) != 0;

        let Some(local) = from_unix_tz(ctx.wall_clock_unix, ctx.tz_hours) else {
            g.text_centered(
                r.x + r.w / 2,
                r.y + r.h / 2 - 6,
                10,
                th.muted,
                1,
                if en { "Time unavailable" } else { "Saat bilgisi yok" },
            );
            return;
        };

        let mut time_line = Line::new();
        if use_12h {
            let h12 = match local.hour % 12 {
                0 => 12,
                v => v,
            };
            time_line.number(h12 as u32, 1).text(":").number(local.minute as u32, 2);
            time_line.text(if local.hour >= 12 { " PM" } else { " AM" });
        } else {
            let mut s = *b"--:--";
            format_time(&local, &mut s);
            if let Ok(t) = core::str::from_utf8(&s) {
                time_line.text(t);
            }
        }

        let day = if en { DAYS_EN[local.weekday as usize % 7] } else { DAYS[local.weekday as usize % 7] };
        let mi = (local.month.saturating_sub(1) as usize).min(11);
        let mon = if en { MONTHS_EN[mi] } else { MONTHS[mi] };
        let mut date_l = Line::new();
        date_l.text(day).text(", ").number(local.day as u32, 1).text(" ").text(mon);

        match ctx.size {
            WidgetSize::Small => {
                let cap = (r.h * 34 / 100).clamp(16, 26);
                g.text(r.x + 14, r.y + r.h / 2 - cap / 2 - 7, cap, th.ink, 2, time_line.as_str());
                let mut short_d = Line::new();
                short_d.number(local.day as u32, 1).text(" ").text(mon);
                g.text(r.x + 14, r.y + r.h / 2 + cap / 2 - 2, 8, th.muted, 1, short_d.as_str());
            }
            WidgetSize::Medium => {
                let cap = (r.h * 44 / 100).clamp(22, 34);
                g.text(r.x + 18, r.y + (r.h - cap) / 2 - 2, cap, th.ink, 2, time_line.as_str());
                let rx = r.x + r.w - 18;
                let mut tz_l = Line::new();
                tz_l.text("UTC");
                if ctx.tz_hours >= 0 {
                    tz_l.text("+").number(ctx.tz_hours as u32, 1);
                } else {
                    tz_l.text("-").number((-ctx.tz_hours) as u32, 1);
                }
                g.text(r.x + r.w / 2 + 6, r.y + r.h / 2 - 12, 9, th.ink, 1, date_l.as_str());
                g.text(r.x + r.w / 2 + 6, r.y + r.h / 2 + 4, 8, th.muted, 1, tz_l.as_str());
                let _ = rx;
            }
            WidgetSize::Large => {
                let cap = (r.h * 28 / 100).clamp(28, 42);
                g.text(r.x + 20, r.y + 18, cap, th.ink, 2, time_line.as_str());
                g.text(r.x + 20, r.y + 24 + cap, 10, th.muted, 1, date_l.as_str());
                // Subtle horizontal divider
                g.panel(r.x + 20, r.y + r.h - 36, r.w - 40, 1, 0, th.rim, 40, 0, 0);
                let mut foot = Line::new();
                foot.text(if en { "Format: " } else { "Biçim: " })
                    .text(if use_12h { "12h" } else { "24h" })
                    .text("  UTC");
                if ctx.tz_hours >= 0 {
                    foot.text("+").number(ctx.tz_hours as u32, 1);
                } else {
                    foot.text("-").number((-ctx.tz_hours) as u32, 1);
                }
                g.text(r.x + 20, r.y + r.h - 24, 8, th.subtle, 1, foot.as_str());
            }
        }
    }

    fn on_tap(&self, _rel_x: i32, _rel_y: i32, ctx: &WidgetContext) -> WidgetAction {
        if ctx.edit_mode {
            WidgetAction::ToggleConfigBit(1)
        } else {
            WidgetAction::ToggleConfigBit(1)
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Battery & Power Widget (Small 2x1, Medium 4x1)
// ---------------------------------------------------------------------------
impl WidgetProvider for BatteryWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "nc.widget.battery",
            provider_app_id: "nc.home",
            name_en: "Battery",
            name_tr: "Pil",
            desc_en: "Real battery charge and power state",
            desc_tr: "Gerçek pil seviyesi ve şarj durumu",
            supported_sizes: SIZE_SMALL | SIZE_MEDIUM,
            default_size: WidgetSize::Small,
            min_cols: 2,
            min_rows: 1,
            max_cols: 4,
            max_rows: 1,
            update_interval_ms: 10_000,
            configurable: false,
            preview: WidgetPreviewKind::Battery,
        }
    }

    fn render(&self, g: &mut Gfx<'_>, ctx: &WidgetContext) {
        let th = card_surface(g, ctx);
        let r = ctx.rect;
        let en = ctx.lang_en;

        draw_battery_icon(
            g,
            r.x + 14,
            r.y + 14,
            28,
            14,
            th.ink,
            ctx.bat_percent,
            ctx.bat_charging,
            ctx.bat_available,
        );

        if !ctx.bat_available {
            g.text(
                r.x + 14,
                r.y + r.h - 24,
                9,
                th.muted,
                1,
                if en { "Unavailable" } else { "Sensör yok" },
            );
            if ctx.size == WidgetSize::Medium {
                g.text(
                    r.x + r.w / 2,
                    r.y + r.h / 2 - 5,
                    8,
                    th.subtle,
                    1,
                    if en { "No battery hardware" } else { "Pil donanımı algılanmadı" },
                );
            }
            return;
        }

        let mut pct = Line::new();
        pct.number(ctx.bat_percent as u32, 1).text("%");
        g.text(r.x + 14, r.y + r.h - 28, 16, th.ink, 2, pct.as_str());

        if ctx.size == WidgetSize::Medium {
            let state_str = if ctx.bat_full {
                if en { "Fully charged" } else { "Tam dolu" }
            } else if ctx.bat_charging {
                if en { "Charging" } else { "Şarj oluyor" }
            } else if ctx.bat_percent <= 15 {
                if en { "Critical battery" } else { "Kritik pil" }
            } else if ctx.bat_percent <= 25 {
                if en { "Low battery" } else { "Düşük pil" }
            } else {
                if en { "On battery" } else { "Pilde" }
            };
            g.text(r.x + r.w / 2 - 10, r.y + r.h / 2 - 12, 9, th.ink, 1, state_str);
            if ctx.bat_voltage_mv > 0 {
                let mut mv = Line::new();
                mv.number(ctx.bat_voltage_mv as u32, 1).text(" mV");
                g.text(r.x + r.w / 2 - 10, r.y + r.h / 2 + 4, 8, th.muted, 1, mv.as_str());
            }
            // Subtle progress bar along bottom right
            let bx = r.x + r.w / 2 - 10;
            let bw = (r.w / 2 - 8).max(40);
            let by = r.y + r.h - 14;
            g.panel(bx, by, bw, 4, 2, th.slider_track, 200, 0, 0);
            let fw = (bw * ctx.bat_percent as i32 / 100).max(4);
            let fc = if ctx.bat_charging { GREEN } else if ctx.bat_percent <= 15 { CORAL } else if ctx.bat_percent <= 25 { AMBER } else { th.ink };
            g.panel(bx, by, fw, 4, 2, fc, 255, 0, 0);
        }
    }
}

// ---------------------------------------------------------------------------
// 3. System Info Widget (Small 2x1, Medium 4x1, Large 4x2)
// ---------------------------------------------------------------------------
impl WidgetProvider for SystemInfoWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "nc.widget.sysinfo",
            provider_app_id: "nc.home",
            name_en: "System",
            name_tr: "Sistem",
            desc_en: "Live memory and uptime status",
            desc_tr: "Canlı bellek ve çalışma süresi",
            supported_sizes: SIZE_SMALL | SIZE_MEDIUM | SIZE_LARGE,
            default_size: WidgetSize::Small,
            min_cols: 2,
            min_rows: 1,
            max_cols: 4,
            max_rows: 2,
            update_interval_ms: 5_000,
            configurable: false,
            preview: WidgetPreviewKind::SystemInfo,
        }
    }

    fn render(&self, g: &mut Gfx<'_>, ctx: &WidgetContext) {
        let th = card_surface(g, ctx);
        let r = ctx.rect;
        let en = ctx.lang_en;

        let used_mib = ctx.ram_total_mib.saturating_sub(ctx.ram_free_mib);
        let ram_pct = if ctx.ram_total_mib > 0 {
            ((used_mib * 100) / ctx.ram_total_mib).min(100)
        } else {
            0
        };

        g.text(r.x + 14, r.y + 12, 8, th.muted, 1, if en { "MEMORY" } else { "BELLEK" });

        let mut mem_l = Line::new();
        if ctx.ram_total_mib > 0 {
            mem_l.number(ram_pct, 1).text("%");
        } else {
            mem_l.text("--");
        }
        g.text(r.x + 14, r.y + 28, 16, th.ink, 2, mem_l.as_str());

        // Memory bar
        let bar_w = if ctx.size == WidgetSize::Small { r.w - 28 } else { r.w / 2 - 24 };
        let bar_y = r.y + r.h - 16;
        g.panel(r.x + 14, bar_y, bar_w.max(24), 4, 2, th.slider_track, 200, 0, 0);
        let fw = (bar_w * ram_pct as i32 / 100).clamp(4, bar_w.max(4));
        g.panel(r.x + 14, bar_y, fw, 4, 2, th.ink, 230, 0, 0);

        if ctx.size != WidgetSize::Small {
            let rx = r.x + r.w / 2 + 4;
            let mins = ctx.uptime_s / 60;
            let hrs = mins / 60;
            let mut up = Line::new();
            up.text(if en { "Uptime " } else { "Süre " });
            if hrs > 0 {
                up.number(hrs, 1).text("h ");
            }
            up.number(mins % 60, 1).text("m");
            g.text(rx, r.y + 14, 9, th.ink, 1, up.as_str());

            let mut free_l = Line::new();
            free_l.number(ctx.ram_free_mib, 1).text(" MiB ").text(if en { "free" } else { "boş" });
            g.text(rx, r.y + 32, 8, th.muted, 1, free_l.as_str());

            if ctx.size == WidgetSize::Large {
                g.panel(r.x + 14, r.y + r.h / 2 + 8, r.w - 28, 1, 0, th.rim, 40, 0, 0);
                let mut tasks = Line::new();
                tasks.text(if en { "Active tasks: " } else { "Aktif görev: " }).number(ctx.live_tasks, 1);
                g.text(r.x + 14, r.y + r.h / 2 + 20, 9, th.ink, 1, tasks.as_str());
                let mut stor = Line::new();
                stor.text(if en { "Storage: " } else { "Depolama: " })
                    .number(ctx.data_used_kib / 1024, 1)
                    .text(" / ")
                    .number(ctx.data_total_mib, 1)
                    .text(" MiB");
                g.text(r.x + 14, r.y + r.h / 2 + 40, 8, th.muted, 1, stor.as_str());
            }
        }
    }

    fn on_tap(&self, _rel_x: i32, _rel_y: i32, ctx: &WidgetContext) -> WidgetAction {
        if ctx.edit_mode {
            WidgetAction::None
        } else {
            WidgetAction::LaunchApp("nc.settings")
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Connectivity Widget (Small 2x1, Medium 4x1)
// ---------------------------------------------------------------------------
impl WidgetProvider for ConnectivityWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "nc.widget.connectivity",
            provider_app_id: "nc.home",
            name_en: "Connectivity",
            name_tr: "Bağlantı",
            desc_en: "Network status and quick toggle",
            desc_tr: "Ağ durumu ve hızlı anahtar",
            supported_sizes: SIZE_SMALL | SIZE_MEDIUM,
            default_size: WidgetSize::Small,
            min_cols: 2,
            min_rows: 1,
            max_cols: 4,
            max_rows: 1,
            update_interval_ms: 5_000,
            configurable: false,
            preview: WidgetPreviewKind::Connectivity,
        }
    }

    fn render(&self, g: &mut Gfx<'_>, ctx: &WidgetContext) {
        let th = card_surface(g, ctx);
        let r = ctx.rect;
        let en = ctx.lang_en;

        let active = ctx.wifi_state == CONN_CONNECTED;
        let bx = r.x + 26;
        let by = r.y + r.h / 2;
        g.dot(bx, by, 14, if active { th.tile_on_bg } else { th.tile_off_bg }, 1024);
        draw_wifi_status_icon(g, bx, by, 6, if active { th.tile_on_fg } else { th.ink }, ctx.wifi_state, 0);

        let st_text = match ctx.wifi_state {
            4 => if en { "Connected" } else { "Bağlı" },
            3 => if en { "Connecting" } else { "Bağlanıyor" },
            2 => if en { "No link" } else { "Bağlı değil" },
            1 => if en { "Off" } else { "Kapalı" },
            _ => if en { "Unavailable" } else { "Yok" },
        };
        g.text(r.x + 48, r.y + r.h / 2 - 11, 9, th.ink, 2, "Wi-Fi");
        g.text(r.x + 48, r.y + r.h / 2 + 3, 8, th.muted, 1, st_text);

        if ctx.size == WidgetSize::Medium {
            let dx = r.x + r.w / 2 + 24;
            g.dot(dx, by, 14, if ctx.dnd { th.tile_on_bg } else { th.tile_off_bg }, 1024);
            draw_dnd_glyph(
                g,
                dx,
                by,
                6,
                if ctx.dnd { th.tile_on_fg } else { th.ink },
                if ctx.dnd { th.tile_on_bg } else { th.tile_off_bg },
            );
            g.text(dx + 22, r.y + r.h / 2 - 11, 9, th.ink, 2, if en { "DND" } else { "Rahatsız Etme" });
            g.text(
                dx + 22,
                r.y + r.h / 2 + 3,
                8,
                th.muted,
                1,
                if ctx.dnd { if en { "Active" } else { "Açık" } } else { if en { "Off" } else { "Kapalı" } },
            );
        }
    }

    fn on_tap(&self, rel_x: i32, _rel_y: i32, ctx: &WidgetContext) -> WidgetAction {
        if ctx.edit_mode {
            return WidgetAction::None;
        }
        if ctx.size == WidgetSize::Medium && rel_x >= ctx.rect.w / 2 {
            WidgetAction::OpenControlCenter
        } else if ctx.wifi_hw {
            WidgetAction::ToggleWifi
        } else {
            WidgetAction::OpenControlCenter
        }
    }
}

// ---------------------------------------------------------------------------
// 5. Calendar / Today Widget (Small 2x1, Medium 4x1, Large 4x2)
// ---------------------------------------------------------------------------
impl WidgetProvider for CalendarWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "nc.widget.calendar",
            provider_app_id: "nc.home",
            name_en: "Calendar",
            name_tr: "Takvim",
            desc_en: "Today's date and month view",
            desc_tr: "Bugünün tarihi ve ay görünümü",
            supported_sizes: SIZE_SMALL | SIZE_MEDIUM | SIZE_LARGE,
            default_size: WidgetSize::Medium,
            min_cols: 2,
            min_rows: 1,
            max_cols: 4,
            max_rows: 2,
            update_interval_ms: 60_000,
            configurable: false,
            preview: WidgetPreviewKind::Calendar,
        }
    }

    fn render(&self, g: &mut Gfx<'_>, ctx: &WidgetContext) {
        let th = card_surface(g, ctx);
        let r = ctx.rect;
        let en = ctx.lang_en;

        let Some(local) = from_unix_tz(ctx.wall_clock_unix, ctx.tz_hours) else {
            g.text_centered(
                r.x + r.w / 2,
                r.y + r.h / 2 - 5,
                9,
                th.muted,
                1,
                if en { "Date unavailable" } else { "Tarih bilgisi yok" },
            );
            return;
        };

        let day_name = if en { DAYS_EN[local.weekday as usize % 7] } else { DAYS[local.weekday as usize % 7] };
        let mi = (local.month.saturating_sub(1) as usize).min(11);
        let mon_name = if en { MONTHS_EN[mi] } else { MONTHS[mi] };
        let mut day_num = Line::new();
        day_num.number(local.day as u32, 1);

        match ctx.size {
            WidgetSize::Small => {
                g.text(r.x + 14, r.y + 12, 8, CORAL, 2, day_name);
                g.text(r.x + 14, r.y + 26, 22, th.ink, 2, day_num.as_str());
                g.text(r.x + 14, r.y + r.h - 18, 8, th.muted, 1, mon_name);
            }
            WidgetSize::Medium => {
                g.text(r.x + 16, r.y + 12, 8, CORAL, 2, day_name);
                g.text(r.x + 16, r.y + 28, 24, th.ink, 2, day_num.as_str());
                g.panel(r.x + r.w / 3, r.y + 14, 1, r.h - 28, 0, th.rim, 40, 0, 0);
                let mut hdr = Line::new();
                hdr.text(mon_name).text(" ").number(local.year as u32, 1);
                g.text(r.x + r.w / 3 + 14, r.y + 18, 9, th.ink, 2, hdr.as_str());
                // Honest empty state — no fabricated events!
                g.text(
                    r.x + r.w / 3 + 14,
                    r.y + 38,
                    8,
                    th.muted,
                    1,
                    if en { "No calendar provider" } else { "Takvim sağlayıcısı yok" },
                );
            }
            WidgetSize::Large => {
                let mut hdr = Line::new();
                hdr.text(mon_name).text(" ").number(local.year as u32, 1);
                g.text(r.x + 16, r.y + 12, 10, th.ink, 2, hdr.as_str());
                let cols_w = (r.w - 32) / 7;
                let short_days = if en { &DAYS_SHORT_EN } else { &DAYS_SHORT_TR };
                for c in 0..7usize {
                    let cx = r.x + 16 + c as i32 * cols_w + cols_w / 2;
                    g.text_centered(cx, r.y + 30, 7, th.muted, 1, short_days[c]);
                }
                let start = month_start_monday_index(&local) as usize;
                let total = days_in_month(local.month, local.year) as usize;
                let row_h = ((r.h - 52) / 6).clamp(14, 22);
                for d_idx in 1..=total {
                    let slot = start + d_idx - 1;
                    let col = (slot % 7) as i32;
                    let row = (slot / 7) as i32;
                    if row >= 6 {
                        break;
                    }
                    let cx = r.x + 16 + col * cols_w + cols_w / 2;
                    let cy = r.y + 46 + row * row_h;
                    let is_today = d_idx as u8 == local.day;
                    if is_today {
                        g.dot(cx, cy + 4, (row_h / 2).min(9), th.tile_on_bg, 1024);
                    }
                    let mut dl = Line::new();
                    dl.number(d_idx as u32, 1);
                    g.text_centered(cx, cy, 8, if is_today { th.tile_on_fg } else { th.ink }, 1, dl.as_str());
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Storage Widget (Medium 4x1)
// ---------------------------------------------------------------------------
impl WidgetProvider for StorageWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "nc.widget.storage",
            provider_app_id: "nc.files",
            name_en: "Storage",
            name_tr: "Depolama",
            desc_en: "Internal storage and USB status",
            desc_tr: "Dahili depolama ve USB durumu",
            supported_sizes: SIZE_MEDIUM,
            default_size: WidgetSize::Medium,
            min_cols: 4,
            min_rows: 1,
            max_cols: 4,
            max_rows: 1,
            update_interval_ms: 15_000,
            configurable: false,
            preview: WidgetPreviewKind::Storage,
        }
    }

    fn render(&self, g: &mut Gfx<'_>, ctx: &WidgetContext) {
        let th = card_surface(g, ctx);
        let r = ctx.rect;
        let en = ctx.lang_en;

        g.text(r.x + 16, r.y + 12, 9, th.ink, 2, if en { "Storage" } else { "Depolama" });
        if ctx.usb_mounted {
            g.panel(r.x + r.w - 64, r.y + 10, 48, 16, 8, GREEN, 44, 0, 0);
            g.text_centered(r.x + r.w - 40, r.y + 14, 7, GREEN, 1, "USB");
        }

        let used_mib = ctx.data_used_kib / 1024;
        let mut info = Line::new();
        if ctx.data_total_mib > 0 {
            info.number(used_mib, 1)
                .text(" MiB / ")
                .number(ctx.data_total_mib, 1)
                .text(" MiB");
        } else {
            info.text(if en { "Unavailable" } else { "Erişilemiyor" });
        }
        g.text(r.x + 16, r.y + 30, 8, th.muted, 1, info.as_str());

        let bar_w = r.w - 32;
        let bar_y = r.y + r.h - 16;
        g.panel(r.x + 16, bar_y, bar_w, 5, 2, th.slider_track, 210, 0, 0);
        if ctx.data_total_mib > 0 {
            let pct = ((used_mib as u64 * 100) / ctx.data_total_mib as u64).min(100) as i32;
            let fw = (bar_w * pct / 100).clamp(4, bar_w);
            g.panel(r.x + 16, bar_y, fw, 5, 2, th.ink, 235, 0, 0);
        }
    }

    fn on_tap(&self, _rel_x: i32, _rel_y: i32, ctx: &WidgetContext) -> WidgetAction {
        if ctx.edit_mode {
            WidgetAction::None
        } else {
            WidgetAction::LaunchApp("nc.files")
        }
    }
}
