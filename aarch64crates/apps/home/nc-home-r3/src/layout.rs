use nc_abi::event::EventBlock;
use nc_app_sdk::widget::WidgetSize;
use nc_mobile_ui::Dp;

use crate::model::{HomePrefs, DOCK_SLOTS, MAX_ROWS};

pub const STATUS_DP: i32 = 24;
pub const NAV_DP: i32 = 42;

#[derive(Clone, Copy)]
pub struct R {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl R {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> R {
        R { x, y, w, h }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn cx(&self) -> i32 {
        self.x + self.w / 2
    }

    pub fn cy(&self) -> i32 {
        self.y + self.h / 2
    }

    pub fn grow(&self, d: i32) -> R {
        R::new(self.x - d, self.y - d, self.w + d * 2, self.h + d * 2)
    }
}

#[derive(Clone, Copy)]
pub struct Geo {
    pub dp: Dp,
    pub w: i32,
    pub h: i32,
    pub status_h: i32,
    pub nav_h: i32,
    pub cols: usize,
    pub rows: usize,
    pub icon: i32,
    pub grid_x: i32,
    pub grid_y: i32,
    pub cell_w: i32,
    pub cell_h: i32,
    pub dock: R,
    pub dock_icon: i32,
    pub dots_y: i32,
    pub app: R,
    pub cc: R,
    pub sheet: R,
    pub drawer_y: i32,
}

impl Geo {
    pub fn new(ev: &EventBlock, prefs: &HomePrefs) -> Geo {
        let dp = Dp::new(ev.area_w, ev.area_h);
        let (w, h) = (ev.area_w, ev.area_h);
        let status_h = dp.d(STATUS_DP);
        let nav_h = dp.d(NAV_DP);

        let cols = prefs.density.cols();
        let max_rows = prefs.density.max_rows();
        let margin = dp.d(12);
        let grid_x = dp.origin_x + margin;
        let grid_w = dp.width - margin * 2;
        let cell_w = grid_w / cols as i32;

        let base_icon = dp.d(prefs.icon_size.dp()).min(cell_w - dp.d(14));
        let grid_y = status_h + dp.d(12);

        let dock_h = dp.d(64);
        let dock = R::new(dp.x(12), h - nav_h - dp.d(8) - dock_h, dp.d(336), dock_h);
        let dots_y = dock.y - dp.d(12);

        let free = (dots_y - dp.d(8) - grid_y).max(dp.d(200));
        let desired_cell_h = match max_rows {
            6 => dp.d(70),
            4 => dp.d(92),
            _ => dp.d(78),
        };
        let rows = ((free / desired_cell_h).max(3) as usize).min(max_rows).min(MAX_ROWS);
        let cell_h = (free / rows as i32).min(dp.d(96)).max(dp.d(64));

        let slot_w = (dock.w - dp.d(16)) / DOCK_SLOTS as i32;
        let dock_icon = dp.d(44).min(slot_w - dp.d(12));

        let cc_h = dp.d(372).min(h - status_h - nav_h - dp.d(16));
        let cc = R::new(dp.x(12), status_h + dp.d(6), dp.d(336), cc_h);
        let sheet_h = (h * 64 / 100).min(h - status_h - nav_h - dp.d(20)).min(dp.d(430));
        let sheet = R::new(dp.x(12), status_h + dp.d(8), dp.d(336), sheet_h);

        Geo {
            dp,
            w,
            h,
            status_h,
            nav_h,
            cols,
            rows,
            icon: base_icon,
            grid_x,
            grid_y,
            cell_w,
            cell_h,
            dock,
            dock_icon,
            dots_y,
            app: R::new(0, status_h, w, (h - status_h - nav_h).max(160)),
            cc,
            sheet,
            drawer_y: status_h + dp.d(18),
        }
    }

    pub fn capacity(&self) -> usize {
        self.rows * self.cols
    }

    pub fn cell_origin(&self, idx: usize) -> (i32, i32) {
        self.cell_origin_ex(idx, 0, 0)
    }

    pub fn cell_origin_ex(&self, idx: usize, page_dx: i32, edit_dy: i32) -> (i32, i32) {
        let col = (idx % self.cols) as i32;
        let row = (idx / self.cols) as i32;
        (
            self.grid_x + page_dx + col * self.cell_w + (self.cell_w - self.icon) / 2,
            self.grid_y + edit_dy + row * self.cell_h + self.dp.d(4),
        )
    }

    pub fn cell_rect_ex(&self, idx: usize, page_dx: i32, edit_dy: i32) -> R {
        let (x, y) = self.cell_origin_ex(idx, page_dx, edit_dy);
        R::new(x - self.dp.d(6), y - self.dp.d(4), self.icon + self.dp.d(12), self.icon + self.dp.d(24))
    }

    pub fn cell_rect(&self, idx: usize) -> R {
        self.cell_rect_ex(idx, 0, 0)
    }

    pub fn widget_rect(&self, col: u8, row: u8, size: WidgetSize, page_dx: i32, edit_dy: i32) -> R {
        let (sc, sr) = size.grid_span(self.cols);
        let pad_x = self.dp.d(5);
        let pad_y = self.dp.d(5);
        let x = self.grid_x + page_dx + col as i32 * self.cell_w + pad_x;
        let y = self.grid_y + edit_dy + row as i32 * self.cell_h + pad_y;
        let w = (sc as i32 * self.cell_w - pad_x * 2).max(self.dp.d(64));
        let h = (sr as i32 * self.cell_h - pad_y * 2).max(self.dp.d(56));
        R::new(x, y, w, h)
    }

    pub fn widget_remove_badge(&self, wr: R) -> R {
        let s = self.dp.d(22);
        R::new(wr.x - self.dp.d(4), wr.y - self.dp.d(4), s, s)
    }

    pub fn widget_resize_badge(&self, wr: R) -> R {
        let w = self.dp.d(48);
        let h = self.dp.d(20);
        R::new(wr.right() - w - self.dp.d(4), wr.bottom() - h - self.dp.d(4), w, h)
    }

    pub fn edit_bar_add(&self) -> R {
        R::new(self.dp.x(14), self.status_h + self.dp.d(4), self.dp.d(98), self.dp.d(28))
    }

    pub fn edit_bar_customize(&self) -> R {
        R::new(self.dp.x(118), self.status_h + self.dp.d(4), self.dp.d(104), self.dp.d(28))
    }

    pub fn edit_bar_done(&self) -> R {
        R::new(self.dp.x(266), self.status_h + self.dp.d(4), self.dp.d(80), self.dp.d(28))
    }

    pub fn dock_origin(&self, slot: usize) -> (i32, i32, i32) {
        let pad = self.dp.d(8);
        let slot_w = (self.dock.w - pad * 2) / DOCK_SLOTS as i32;
        let cx = self.dock.x + pad + slot_w * slot as i32 + slot_w / 2;
        (cx - self.dock_icon / 2, self.dock.y + (self.dock.h - self.dock_icon) / 2, self.dock_icon)
    }

    pub fn dock_rect(&self, slot: usize) -> R {
        let pad = self.dp.d(8);
        let slot_w = (self.dock.w - pad * 2) / DOCK_SLOTS as i32;
        R::new(self.dock.x + pad + slot_w * slot as i32, self.dock.y, slot_w, self.dock.h)
    }

    pub fn nav_rect(&self) -> R {
        R::new(0, self.h - self.nav_h, self.w, self.nav_h)
    }

    pub fn nav_button(&self, x: i32) -> u8 {
        let third = self.dp.width / 3;
        let rel = x - self.dp.origin_x;
        if rel < third {
            0
        } else if rel < third * 2 {
            1
        } else {
            2
        }
    }

    pub fn drawer_search(&self) -> R {
        R::new(self.dp.x(16), self.drawer_y + self.dp.d(18), self.dp.d(328), self.dp.d(34))
    }

    pub fn drawer_cell(&self, idx: usize) -> (i32, i32) {
        let side = self.dp.d(14);
        let cw = (self.dp.width - side * 2) / 4;
        let col = (idx % 4) as i32;
        let row = (idx / 4) as i32;
        (
            self.dp.origin_x + side + col * cw + (cw - self.icon) / 2,
            self.drawer_y + self.dp.d(68) + row * self.dp.d(80),
        )
    }

    // --- Control Center geometry ---
    pub fn cc_lock_btn(&self) -> R {
        R::new(self.cc.right() - self.dp.d(74), self.cc.y + self.dp.d(10), self.dp.d(62), self.dp.d(24))
    }

    /// 2x2 Connectivity & quick toggle tiles:
    /// 0 = Wi-Fi, 1 = Bluetooth, 2 = Mobile Data, 3 = Do Not Disturb
    pub fn cc_tile(&self, idx: usize) -> R {
        let pad = self.dp.d(12);
        let gap = self.dp.d(8);
        let tw = (self.cc.w - pad * 2 - gap) / 2;
        let th = self.dp.d(56);
        let col = (idx % 2) as i32;
        let row = (idx / 2) as i32;
        R::new(
            self.cc.x + pad + col * (tw + gap),
            self.cc.y + self.dp.d(42) + row * (th + gap),
            tw,
            th,
        )
    }

    pub fn cc_bright_card(&self) -> R {
        let pad = self.dp.d(12);
        R::new(self.cc.x + pad, self.cc.y + self.dp.d(172), self.cc.w - pad * 2, self.dp.d(52))
    }

    pub fn cc_audio_card(&self) -> R {
        let pad = self.dp.d(12);
        R::new(self.cc.x + pad, self.cc.y + self.dp.d(232), self.cc.w - pad * 2, self.dp.d(44))
    }

    pub fn cc_theme_btn(&self) -> R {
        let pad = self.dp.d(12);
        let gap = self.dp.d(8);
        let bw = (self.cc.w - pad * 2 - gap) / 2;
        R::new(self.cc.x + pad, self.cc.y + self.dp.d(284), bw, self.dp.d(40))
    }

    pub fn cc_airplane_btn(&self) -> R {
        let pad = self.dp.d(12);
        let gap = self.dp.d(8);
        let bw = (self.cc.w - pad * 2 - gap) / 2;
        R::new(self.cc.x + pad + bw + gap, self.cc.y + self.dp.d(284), bw, self.dp.d(40))
    }

    pub fn cc_settings(&self) -> R {
        let pad = self.dp.d(12);
        R::new(self.cc.x + pad, self.cc.y + self.dp.d(330), self.cc.w - pad * 2, self.dp.d(30))
    }

    // --- Widget Picker Sheet ---
    pub fn wpick_sheet(&self) -> R {
        let sh = (self.h * 68 / 100).min(self.h - self.status_h - self.nav_h - self.dp.d(16)).min(self.dp.d(440));
        R::new(self.dp.x(12), self.h - self.nav_h - sh - self.dp.d(8), self.dp.d(336), sh)
    }

    pub fn wpick_size_chip(&self, idx: usize) -> R {
        let s = self.wpick_sheet();
        let cw = self.dp.d(96);
        let gap = self.dp.d(8);
        let total = cw * 3 + gap * 2;
        let x0 = s.cx() - total / 2;
        R::new(x0 + idx as i32 * (cw + gap), s.y + self.dp.d(44), cw, self.dp.d(28))
    }

    pub fn wpick_row(&self, idx: usize) -> R {
        let s = self.wpick_sheet();
        let rh = self.dp.d(56);
        let gap = self.dp.d(8);
        R::new(s.x + self.dp.d(14), s.y + self.dp.d(82) + idx as i32 * (rh + gap), s.w - self.dp.d(28), rh)
    }

    // --- Home Customization Sheet ---
    pub fn cust_sheet(&self) -> R {
        let sh = (self.h * 68 / 100).min(self.h - self.status_h - self.nav_h - self.dp.d(16)).min(self.dp.d(430));
        R::new(self.dp.x(12), self.h - self.nav_h - sh - self.dp.d(8), self.dp.d(336), sh)
    }

    pub fn cust_row(&self, idx: usize) -> R {
        let s = self.cust_sheet();
        let rh = self.dp.d(44);
        let gap = self.dp.d(8);
        R::new(s.x + self.dp.d(14), s.y + self.dp.d(48) + idx as i32 * (rh + gap), s.w - self.dp.d(28), rh)
    }

    pub fn folder_card(&self) -> R {
        let side = self.dp.d(232);
        R::new(self.w / 2 - side / 2, self.h * 42 / 100 - side / 2, side, side + self.dp.d(26))
    }

    pub fn folder_slot(&self, k: usize) -> (i32, i32) {
        let c = self.folder_card();
        let pad = self.dp.d(24);
        let cw = (c.w - pad * 2) / 2;
        let ch = self.dp.d(92);
        let col = (k % 2) as i32;
        let row = (k / 2) as i32;
        (
            c.x + pad + col * cw + cw / 2,
            c.y + self.dp.d(44) + row * ch + self.icon / 2,
        )
    }

    pub fn folder_ungroup(&self) -> R {
        let c = self.folder_card();
        R::new(self.w / 2 - self.dp.d(64), c.bottom() + self.dp.d(14), self.dp.d(128), self.dp.d(34))
    }

    pub fn recent_card(&self, k: usize, n: usize) -> R {
        let cw = self.dp.d(158);
        let gap = self.dp.d(14);
        let total = n as i32 * cw + (n as i32 - 1).max(0) * gap;
        let pw = cw - self.dp.d(12);
        let ph = pw * self.app.h / self.app.w.max(1);
        R::new((self.w - total) / 2 + k as i32 * (cw + gap), self.status_h + self.dp.d(66), cw, self.dp.d(30) + ph + self.dp.d(10))
    }

    pub fn recent_preview(&self, card: R) -> R {
        let pw = card.w - self.dp.d(12);
        R::new(card.x + self.dp.d(6), card.y + self.dp.d(30), pw, pw * self.app.h / self.app.w.max(1))
    }

    pub fn recents_clear(&self) -> R {
        let card = self.recent_card(0, 1);
        let y = (card.bottom() + self.dp.d(18)).min(self.h - self.nav_h - self.dp.d(54));
        R::new(self.w / 2 - self.dp.d(80), y, self.dp.d(160), self.dp.d(36))
    }

    pub fn search_field(&self) -> R {
        R::new(self.sheet.x + self.dp.d(14), self.sheet.y + self.dp.d(14), self.sheet.w - self.dp.d(28), self.dp.d(42))
    }

    pub fn search_row(&self, i: usize) -> R {
        R::new(self.sheet.x + self.dp.d(10), self.sheet.y + self.dp.d(68) + i as i32 * self.dp.d(54), self.sheet.w - self.dp.d(20), self.dp.d(50))
    }

    pub fn search_suggest(&self, i: usize) -> (i32, i32) {
        let cw = (self.sheet.w - self.dp.d(20)) / 4;
        (
            self.sheet.x + self.dp.d(10) + (i % 4) as i32 * cw + (cw - self.icon * 9 / 10) / 2,
            self.sheet.y + self.dp.d(88) + (i / 4) as i32 * self.dp.d(84),
        )
    }
}
