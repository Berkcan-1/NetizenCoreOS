# NetizenCore Widget SDK v1

This document describes the versioned public widget interface (`nc_app_sdk::widget`, `WIDGET_API_VERSION = 1`) for NetizenHome (`nc-home-r3`).

## Design Principles

- **`#![no_std]` and zero heap allocation:** Widget providers run in Ring 3 under the standard NetizenCore application isolation model.
- **Real data only:** Widgets receive a `WidgetContext` populated from kernel `OP_SYSTEM_STATUS` and RTC syscalls. When hardware or provider data is unavailable, widgets must display a clean empty/unavailable state rather than fabricated values.
- **Event-driven updates:** NetizenHome suspends widget refreshes while covered by a foreground application and updates widgets on minute ticks, status changes, or user interaction.
- **Fault isolation:** Invalid provider indices or out-of-bounds layouts render a safe fallback card without crashing the launcher.

## Core Types (`crates/runtime/nc-app-sdk/src/widget.rs`)

### `WidgetSize`
- `WidgetSize::Small` (`2x1` grid cells)
- `WidgetSize::Medium` (`4x1` grid cells on 4-column grid, adapts to 3-column grid)
- `WidgetSize::Large` (`4x2` grid cells on 4-column grid, adapts to 3-column grid)

### `WidgetMeta`
Static metadata advertised by every widget provider:
- `id`: Unique reverse-DNS widget identifier (e.g. `"nc.widget.clock"`).
- `provider_app_id`: Owning application package ID (e.g. `"nc.home"`, `"nc.files"`).
- `name_en` / `name_tr`, `desc_en` / `desc_tr`: User-facing localized display names and descriptions.
- `supported_sizes`: Bitmask of `SIZE_SMALL | SIZE_MEDIUM | SIZE_LARGE`.
- `default_size`: Default placement size.
- `min_cols`, `min_rows`, `max_cols`, `max_rows`: Grid span constraints.
- `update_interval_ms`: Minimum refresh cadence when polling is unavoidable (`0` for purely event-driven).
- `configurable`: Whether the widget exposes user configuration flags (`config_flags: u16`).
- `preview`: `WidgetPreviewKind` for the Edit Mode Widget Picker.

### `WidgetProvider` Trait
```rust
pub trait WidgetProvider {
    fn meta(&self) -> WidgetMeta;
    fn render(&self, g: &mut Gfx<'_>, ctx: &WidgetContext);
    fn on_tap(&self, rel_x: i32, rel_y: i32, ctx: &WidgetContext) -> WidgetAction {
        let _ = (rel_x, rel_y, ctx);
        WidgetAction::None
    }
}
```

### `WidgetAction`
Returned from `on_tap` to trigger safe, host-mediated actions without direct launcher or kernel access:
- `WidgetAction::None`
- `WidgetAction::Redraw`
- `WidgetAction::OpenControlCenter`
- `WidgetAction::ToggleWifi`
- `WidgetAction::ToggleTheme`
- `WidgetAction::LaunchApp(&'static str)`
- `WidgetAction::ToggleConfigBit(u16)`

## Discovery & Persistence

- **Kernel Registry Opcode:** `nc_abi::sys::OP_WIDGET_REGISTRY` (`28`) advertises `api_version=1` and registry generation for dynamic package-based widget discovery.
- **Persistence:** Placed widget instances (`id`, `page`, `col`, `row`, `size`, `config_flags`) are persisted in `/data/appdata/nc.home/layout.cfg` and validated on load. Unknown or missing widget IDs are skipped safely on boot.

## Built-in System Widgets

1. `nc.widget.clock` — Small / Medium / Large, real RTC time & date, configurable 12h/24h toggle.
2. `nc.widget.battery` — Small / Medium, real PMIC/AUXADC battery percentage, charging state, voltage; displays an honest unavailable state when no battery sensor exists.
3. `nc.widget.sysinfo` — Small / Medium / Large, real RAM usage, uptime, active tasks, storage summary.
4. `nc.widget.connectivity` — Small / Medium, real Wi-Fi/Network & Do Not Disturb status with tap toggle.
5. `nc.widget.calendar` — Small / Medium / Large, real current date & mini-month view; displays honest `"No calendar provider"` state for events.
6. `nc.widget.storage` — Medium, real `/data` storage usage bar and USB mount status; taps launch Files.
