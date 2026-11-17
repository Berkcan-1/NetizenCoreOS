//! Ayarlar (ARMv8-A / Ring 3 EL0): paylaşılan `nc_app_sdk::ui` altyapısını kullanan,
//! Kontrol Merkezi ile çift yönlü bağlı (Wi-Fi, Bluetooth, Parlaklık, Tema, Duvar Kağıdı, Saat, Dil)
//! minimalist ticari mobil sistem ayarları uygulaması.
#![no_std]
#![no_main]

use nc_abi::event::{
    EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE, RET_CLOSE, RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_abi::sys as wire;
use nc_app_sdk::ui::{
    draw_app_tile, draw_bt_glyph, draw_card, draw_pill_btn, draw_slider, draw_sun_glyph,
    draw_switch, draw_wifi_glyph, Dp, Glyph, AMBER, CARD, CORAL, CORAL_SOFT, DIVIDER, GREEN,
    GREEN_SOFT, INK, MUTED, SUBTLE, SURFACE_BG, TRACK, WHITE,
};
use nc_app_sdk::{dispatch, pointer, sys, Gfx, Line, RingApp};

const ACCENTS: [(u32, &str, &str); 5] = [
    (0x000A_59F7, "Mavi", "Blue"),
    (0x0007_C160, "Yeşil", "Green"),
    (0x00FA_7D00, "Turuncu", "Orange"),
    (0x007A_5AF8, "Mor", "Purple"),
    (0x00E1_1D48, "Mercan", "Coral"),
];

const WALLPAPERS: [(u32, u32, u32, &str, &str); 4] = [
    (0x0010_8F9B, 0x0037_2E9E, 0x00E8_B04A, "Okyanus", "Ocean"),
    (0x000F_172A, 0x0031_1042, 0x00A8_55F7, "Gece", "Night"),
    (0x007C_2D12, 0x004C_1D95, 0x00FB_923C, "Gün Batımı", "Sunset"),
    (0x0006_4E3B, 0x001E_3A8A, 0x0034_D399, "Zümrüt", "Emerald"),
];

const MAX_APPS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Network = 0,
    Display = 1,
    Storage = 2,
    System = 3,
}

#[derive(Clone, Copy)]
struct AppRow {
    id: [u8; 28],
    id_len: u8,
    name: [u8; 28],
    name_len: u8,
    version: [u8; 16],
    version_len: u8,
    state: u8,
}

impl AppRow {
    const EMPTY: AppRow = AppRow {
        id: [0; 28],
        id_len: 0,
        name: [0; 28],
        name_len: 0,
        version: [0; 16],
        version_len: 0,
        state: 0,
    };

    fn id_str(&self) -> &str {
        core::str::from_utf8(&self.id[..self.id_len as usize]).unwrap_or("")
    }

    fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("")
    }

    fn version_str(&self) -> &str {
        core::str::from_utf8(&self.version[..self.version_len as usize]).unwrap_or("")
    }
}

struct SettingsApp {
    tab: Tab,
    last_flags: u32,
    device_name: [u8; 32],
    device_name_len: usize,
    screen: [u8; 24],
    screen_len: usize,
    net_mac: [u8; 20],
    net_mac_len: usize,
    ram_mib: u32,
    free_mib: u32,
    sys_mib: u32,
    data_mib: u32,
    pkg_count: u32,
    user_app_count: u32,
    media_count: u32,
    accent: u32,
    wallpaper: u8,
    tz_hours: i32,
    lang: u32,
    wifi: bool,
    bt: bool,
    brightness: u8,
    apps: [AppRow; MAX_APPS],
    app_count: usize,
}

static mut REQ_BUF: [u8; 128] = [0; 128];
static mut RESP_BUF: [u8; 4096] = [0; 4096];

fn copy_str(dst: &mut [u8], text: &str) -> usize {
    let bytes = text.as_bytes();
    let len = bytes.len().min(dst.len());
    dst[..len].copy_from_slice(&bytes[..len]);
    len
}

fn parse_u32(text: &str) -> u32 {
    text.trim().parse::<u32>().unwrap_or(0)
}

fn sys_request(op: u8, arg0: u32, arg1: u32) -> Result<(i32, &'static [u8]), i32> {
    unsafe {
        let req = &mut *core::ptr::addr_of_mut!(REQ_BUF);
        let resp = &mut *core::ptr::addr_of_mut!(RESP_BUF);
        let Some(n_req) = wire::encode_request(req, op, arg0, arg1, 0, &[]) else {
            return Err(wire::ERR_BAD_REQUEST);
        };
        let n = sys::sys_call(&req[..n_req], resp);
        if n < 0 {
            return Err(wire::ERR_FAILED);
        }
        let n = (n as usize).min(resp.len());
        match wire::parse_response(&resp[..n]) {
            Ok((status, payload)) => Ok((status, payload)),
            Err(_) => Err(wire::ERR_FAILED),
        }
    }
}

impl SettingsApp {
    const fn new() -> Self {
        Self {
            tab: Tab::Display,
            last_flags: 0,
            device_name: [0; 32],
            device_name_len: 0,
            screen: [0; 24],
            screen_len: 0,
            net_mac: [0; 20],
            net_mac_len: 0,
            ram_mib: 4096,
            free_mib: 0,
            sys_mib: 0,
            data_mib: 0,
            pkg_count: 0,
            user_app_count: 0,
            media_count: 0,
            accent: 0x000A_59F7,
            wallpaper: 0,
            tz_hours: 3,
            lang: 0,
            wifi: true,
            bt: false,
            brightness: 100,
            apps: [AppRow::EMPTY; MAX_APPS],
            app_count: 0,
        }
    }

    fn refresh(&mut self) {
        let Ok((0, payload)) = sys_request(wire::OP_DEVICE_INFO, 0, 0) else {
            return;
        };
        let Ok(text) = core::str::from_utf8(payload) else {
            return;
        };
        self.app_count = 0;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "device_name" => self.device_name_len = copy_str(&mut self.device_name, v),
                "screen" => self.screen_len = copy_str(&mut self.screen, v),
                "net_mac" => self.net_mac_len = copy_str(&mut self.net_mac, v),
                "ram_mib" => self.ram_mib = parse_u32(v),
                "free_mib" => self.free_mib = parse_u32(v),
                "sys_mib" => self.sys_mib = parse_u32(v),
                "data_mib" => self.data_mib = parse_u32(v),
                "pkg_count" => self.pkg_count = parse_u32(v),
                "user_app_count" => self.user_app_count = parse_u32(v),
                "media_count" => self.media_count = parse_u32(v),
                "accent" => {
                    if let Ok(val) = u32::from_str_radix(v.trim().trim_start_matches("0x"), 16) {
                        self.accent = val;
                    }
                }
                "wallpaper" => self.wallpaper = (parse_u32(v) as u8).min(3),
                "tz_hours" => self.tz_hours = v.trim().parse::<i32>().unwrap_or(3).clamp(-12, 14),
                "lang" => self.lang = parse_u32(v).min(1),
                "wifi" => self.wifi = parse_u32(v) != 0,
                "bt" => self.bt = parse_u32(v) != 0,
                "brightness" => self.brightness = (parse_u32(v) as u8).clamp(20, 100),
                "app" => {
                    if self.app_count < MAX_APPS {
                        let mut parts = v.split('|');
                        let id = parts.next().unwrap_or("");
                        let name = parts.next().unwrap_or("");
                        let ver = parts.next().unwrap_or("");
                        let _tier = parts.next().unwrap_or("");
                        let st = parts.next().unwrap_or("");
                        let mut row = AppRow::EMPTY;
                        row.id_len = copy_str(&mut row.id, id) as u8;
                        row.name_len = copy_str(&mut row.name, name) as u8;
                        row.version_len = copy_str(&mut row.version, ver) as u8;
                        row.state = match st {
                            "fg" => 2,
                            "paused" => 1,
                            _ => 0,
                        };
                        self.apps[self.app_count] = row;
                        self.app_count += 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn apply_setting(&mut self, sub_op: u32, val: u32) {
        if sys::mobile_set(sub_op, val) {
            self.refresh();
        }
    }

    fn handle_click(&mut self, x: i32, y: i32, dp: &Dp) -> u32 {
        // Üst sekme hapları (y: 48..82 dp)
        if y >= dp.d(48) && y < dp.d(82) {
            if x >= dp.x(16) && x < dp.x(98) {
                self.tab = Tab::Network;
                self.refresh();
                return RET_REDRAW;
            }
            if x >= dp.x(98) && x < dp.x(180) {
                self.tab = Tab::Display;
                self.refresh();
                return RET_REDRAW;
            }
            if x >= dp.x(180) && x < dp.x(262) {
                self.tab = Tab::Storage;
                self.refresh();
                return RET_REDRAW;
            }
            if x >= dp.x(262) && x < dp.x(344) {
                self.tab = Tab::System;
                self.refresh();
                return RET_REDRAW;
            }
        }

        match self.tab {
            Tab::Network => {
                // Wi-Fi kartı (y: 92..182 dp)
                if y >= dp.d(92) && y < dp.d(182) && x >= dp.x(16) && x < dp.x(344) {
                    self.apply_setting(wire::MOBILE_SET_WIFI, (!self.wifi) as u32);
                    return RET_REDRAW;
                }
                // Bluetooth kartı (y: 194..284 dp)
                if y >= dp.d(194) && y < dp.d(284) && x >= dp.x(16) && x < dp.x(344) {
                    self.apply_setting(wire::MOBILE_SET_BT, (!self.bt) as u32);
                    return RET_REDRAW;
                }
            }
            Tab::Display => {
                // Parlaklık kaydırıcısı (y: 92..152 dp)
                if y >= dp.d(92) && y < dp.d(152) && x >= dp.x(28) && x < dp.x(332) {
                    let rel = (x - dp.x(64)).clamp(0, dp.d(252));
                    let pct = (20 + rel * 80 / dp.d(252).max(1)).clamp(20, 100) as u32;
                    let rounded = ((pct + 5) / 10) * 10;
                    self.apply_setting(wire::MOBILE_SET_BRIGHTNESS, rounded);
                    return RET_REDRAW;
                }
                // Tema renkleri (y: 160..236 dp)
                if y >= dp.d(160) && y < dp.d(236) {
                    for (idx, &(color, _, _)) in ACCENTS.iter().enumerate() {
                        let cx = dp.x(52 + (idx as i32) * 64);
                        if (x - cx).abs() <= dp.d(26) {
                            self.apply_setting(wire::MOBILE_SET_ACCENT, color);
                            return RET_REDRAW;
                        }
                    }
                }
                // Duvar kağıdı minyatürleri (y: 244..344 dp)
                if y >= dp.d(244) && y < dp.d(344) {
                    for idx in 0..4 {
                        let kx = dp.x(26 + (idx as i32) * 79);
                        if x >= kx && x < kx + dp.d(71) {
                            self.apply_setting(wire::MOBILE_SET_WALLPAPER, idx as u32);
                            return RET_REDRAW;
                        }
                    }
                }
                // Saat dilimi: - / + (y: 354..394 dp)
                if y >= dp.d(352) && y < dp.d(396) {
                    if x >= dp.x(250) && x < dp.x(288) {
                        let next = (self.tz_hours - 1).clamp(-12, 14);
                        self.apply_setting(wire::MOBILE_SET_TZ, next as u32);
                        return RET_REDRAW;
                    }
                    if x >= dp.x(292) && x < dp.x(334) {
                        let next = (self.tz_hours + 1).clamp(-12, 14);
                        self.apply_setting(wire::MOBILE_SET_TZ, next as u32);
                        return RET_REDRAW;
                    }
                }
                // Dil seçimi: Türkçe / English (y: 398..444 dp)
                if y >= dp.d(398) && y < dp.d(446) {
                    if x >= dp.x(186) && x < dp.x(258) {
                        self.apply_setting(wire::MOBILE_SET_LANG, 0);
                        return RET_REDRAW;
                    }
                    if x >= dp.x(260) && x < dp.x(334) {
                        self.apply_setting(wire::MOBILE_SET_LANG, 1);
                        return RET_REDRAW;
                    }
                }
            }
            Tab::Storage => {
                if y >= dp.d(248) && y < dp.d(296) && x >= dp.x(30) && x < dp.x(330) {
                    let _ = sys::app_launch("nc.files");
                    return RET_REDRAW;
                }
            }
            Tab::System => {
                let top = dp.d(94);
                let row_h = dp.d(72);
                for idx in 0..self.app_count {
                    let ry = top + (idx as i32) * row_h;
                    if y >= ry + dp.d(16) && y < ry + dp.d(54) {
                        if x >= dp.x(218) && x < dp.x(278) {
                            let mut id_copy = [0u8; 28];
                            let n = self.apps[idx].id_len as usize;
                            id_copy[..n].copy_from_slice(&self.apps[idx].id[..n]);
                            if let Ok(id_s) = core::str::from_utf8(&id_copy[..n]) {
                                let _ = sys::app_launch(id_s);
                            }
                            return RET_REDRAW;
                        }
                        if x >= dp.x(282) && x < dp.x(336) && self.apps[idx].state == 1 {
                            let mut id_copy = [0u8; 28];
                            let n = self.apps[idx].id_len as usize;
                            id_copy[..n].copy_from_slice(&self.apps[idx].id[..n]);
                            if let Ok(id_s) = core::str::from_utf8(&id_copy[..n]) {
                                if sys::app_stop(id_s) == 0 {
                                    self.refresh();
                                }
                            }
                            return RET_REDRAW;
                        }
                    }
                }
            }
        }
        RET_NONE
    }

    fn draw_network_tab(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        let en = self.lang == 1;
        let mac = core::str::from_utf8(&self.net_mac[..self.net_mac_len]).unwrap_or("52:54:00:12:34:56");

        // Wi-Fi Kartı
        draw_card(gfx, dp.x(16), dp.d(94), dp.d(328), dp.d(88), dp.d(20));
        let wf_col = if self.wifi { ev.accent } else { TRACK };
        gfx.panel(dp.x(28), dp.d(114), dp.d(46), dp.d(46), dp.d(14), wf_col, 255, 0, 0);
        draw_wifi_glyph(gfx, dp.x(51), dp.d(137), dp.d(9), if self.wifi { WHITE } else { MUTED });

        gfx.text(dp.x(86), dp.d(116), dp.d(10), INK, 2, "Wi-Fi");
        gfx.text(
            dp.x(86),
            dp.d(136),
            dp.d(7),
            if self.wifi { GREEN } else { MUTED },
            1,
            if self.wifi {
                if en { "Connected  •  NetizenCore-5G" } else { "Bağlı  •  NetizenCore-5G" }
            } else if en {
                "Disabled"
            } else {
                "Kapalı"
            },
        );
        let mut mac_line = Line::new();
        mac_line.text("MAC  ").text(mac);
        gfx.text(dp.x(86), dp.d(153), dp.d(7), SUBTLE, 1, mac_line.as_str());
        draw_switch(gfx, dp.x(280), dp.d(124), dp.d(48), dp.d(26), self.wifi, ev.accent);

        // Bluetooth Kartı
        draw_card(gfx, dp.x(16), dp.d(194), dp.d(328), dp.d(84), dp.d(20));
        let bt_col = if self.bt { ev.accent } else { TRACK };
        gfx.panel(dp.x(28), dp.d(213), dp.d(46), dp.d(46), dp.d(14), bt_col, 255, 0, 0);
        draw_bt_glyph(gfx, dp.x(51), dp.d(236), dp.d(9), if self.bt { WHITE } else { MUTED });

        gfx.text(dp.x(86), dp.d(218), dp.d(10), INK, 2, "Bluetooth");
        gfx.text(
            dp.x(86),
            dp.d(240),
            dp.d(7),
            if self.bt { ev.accent } else { MUTED },
            1,
            if self.bt {
                if en { "On  •  Discoverable" } else { "Açık  •  Keşfedilebilir" }
            } else if en {
                "Off"
            } else {
                "Kapalı"
            },
        );
        draw_switch(gfx, dp.x(280), dp.d(223), dp.d(48), dp.d(26), self.bt, ev.accent);
    }

    fn draw_display_tab(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        let en = self.lang == 1;

        // Kart 1: Parlaklık (Kontrol Merkezi ile senkronize)
        draw_card(gfx, dp.x(16), dp.d(92), dp.d(328), dp.d(58), dp.d(18));
        draw_sun_glyph(gfx, dp.x(42), dp.d(121), dp.d(8), AMBER);
        draw_slider(gfx, dp.x(64), dp.d(112), dp.d(252), dp.d(18), self.brightness, ev.accent);

        // Kart 2: Tema Rengi
        draw_card(gfx, dp.x(16), dp.d(158), dp.d(328), dp.d(78), dp.d(18));
        gfx.text(dp.x(28), dp.d(168), dp.d(7), MUTED, 2, if en { "Theme Color" } else { "Tema Rengi" });
        for (idx, &(color, tr, en_lbl)) in ACCENTS.iter().enumerate() {
            let cx = dp.x(52 + (idx as i32) * 64);
            let cy = dp.d(200);
            let active = (self.accent & 0x00FF_FFFF) == color;
            if active {
                gfx.dot(cx, cy, dp.d(15), color, 220);
                gfx.dot(cx, cy, dp.d(13), CARD, 1024);
            }
            gfx.dot(cx, cy, dp.d(10), color, 1024);
            if active {
                gfx.dot(cx, cy, dp.d(3).max(2), WHITE, 1024);
            }
            gfx.text_centered(cx, dp.d(220), dp.d(7), if active { INK } else { SUBTLE }, 1, if en { en_lbl } else { tr });
        }

        // Kart 3: Duvar Kağıdı
        draw_card(gfx, dp.x(16), dp.d(244), dp.d(328), dp.d(98), dp.d(18));
        gfx.text(dp.x(28), dp.d(254), dp.d(7), MUTED, 2, if en { "Wallpaper" } else { "Duvar Kağıdı" });
        for (idx, &(top_c, bot_c, glow_c, tr, en_lbl)) in WALLPAPERS.iter().enumerate() {
            let kx = dp.x(26 + (idx as i32) * 79);
            let ky = dp.d(270);
            let kw = dp.d(71);
            let kh = dp.d(44);
            let active = self.wallpaper as usize == idx;
            if active {
                gfx.panel(kx - dp.d(2), ky - dp.d(2), kw + dp.d(4), kh + dp.d(4), dp.d(12), ev.accent, 255, 0, 0);
            }
            gfx.panel(kx, ky, kw, kh, dp.d(10), bot_c, 255, 0, 0);
            gfx.panel(kx, ky, kw, kh * 3 / 5, dp.d(10), top_c, 210, 0, 0);
            gfx.dot(kx + kw * 2 / 3, ky + kh / 2, dp.d(12), glow_c, 95);
            gfx.text_centered(kx + kw / 2, dp.d(322), dp.d(7), if active { INK } else { MUTED }, 1, if en { en_lbl } else { tr });
        }

        // Kart 4: Saat Dilimi ve Dil
        draw_card(gfx, dp.x(16), dp.d(350), dp.d(328), dp.d(94), dp.d(18));
        gfx.text(dp.x(28), dp.d(368), dp.d(8), INK, 2, if en { "Time Zone" } else { "Saat Dilimi" });
        let mut tz_lbl = Line::new();
        tz_lbl.text("UTC");
        if self.tz_hours >= 0 {
            tz_lbl.text("+").number(self.tz_hours as u32, 1);
        } else {
            tz_lbl.text("-").number((-self.tz_hours) as u32, 1);
        }
        gfx.text(dp.x(186), dp.d(368), dp.d(8), MUTED, 2, tz_lbl.as_str());
        draw_pill_btn(gfx, dp.x(252), dp.d(360), dp.d(34), dp.d(26), TRACK, INK, dp.d(8), "-");
        draw_pill_btn(gfx, dp.x(294), dp.d(360), dp.d(34), dp.d(26), ev.accent, WHITE, dp.d(8), "+");

        gfx.fill_rect(dp.x(28), dp.d(394), dp.d(304), 1, DIVIDER);

        gfx.text(dp.x(28), dp.d(414), dp.d(8), INK, 2, if en { "Language" } else { "Dil" });
        gfx.panel(dp.x(186), dp.d(404), dp.d(144), dp.d(28), dp.d(14), TRACK, 255, 0, 0);
        let pill_x = if self.lang == 0 { dp.x(189) } else { dp.x(259) };
        gfx.panel(pill_x, dp.d(407), dp.d(68), dp.d(22), dp.d(11), CARD, 255, 0, 0);
        gfx.text_centered(dp.x(223), dp.d(414), dp.d(7), if self.lang == 0 { INK } else { MUTED }, 1, "Türkçe");
        gfx.text_centered(dp.x(293), dp.d(414), dp.d(7), if self.lang == 1 { INK } else { MUTED }, 1, "English");
    }

    fn draw_storage_tab(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        let en = self.lang == 1;
        draw_card(gfx, dp.x(16), dp.d(94), dp.d(328), dp.d(208), dp.d(22));
        gfx.text(dp.x(30), dp.d(112), dp.d(8), MUTED, 2, if en { "Device Storage" } else { "Cihaz Depolaması" });

        let mut cap_line = Line::new();
        cap_line.number(self.data_mib, 1).text(" MB");
        gfx.text(dp.x(30), dp.d(132), dp.d(18), INK, 2, cap_line.as_str());

        let bar_x = dp.x(30);
        let bar_y = dp.d(168);
        let bar_w = dp.d(300);
        let bar_h = dp.d(12);
        gfx.panel(bar_x, bar_y, bar_w, bar_h, bar_h / 2, TRACK, 255, 0, 0);
        let app_w = dp.d(48 + (self.user_app_count as i32) * 14).min(bar_w / 2);
        let doc_w = dp.d(26 + (self.media_count as i32) * 10).min(bar_w / 3);
        gfx.panel(bar_x, bar_y, app_w + doc_w, bar_h, bar_h / 2, AMBER, 255, 0, 0);
        gfx.panel(bar_x, bar_y, app_w, bar_h, bar_h / 2, ev.accent, 255, 0, 0);

        gfx.dot(dp.x(36), dp.d(202), dp.d(4).max(2), ev.accent, 1024);
        let mut a_line = Line::new();
        a_line
            .text(if en { "Apps (" } else { "Uygulamalar (" })
            .number(self.pkg_count + self.user_app_count, 1)
            .text(")");
        gfx.text(dp.x(46), dp.d(198), dp.d(8), INK, 1, a_line.as_str());

        gfx.dot(dp.x(184), dp.d(202), dp.d(4).max(2), AMBER, 1024);
        let mut m_line = Line::new();
        m_line
            .text(if en { "Documents (" } else { "Belgeler (" })
            .number(self.media_count, 1)
            .text(")");
        gfx.text(dp.x(194), dp.d(198), dp.d(8), INK, 1, m_line.as_str());

        draw_pill_btn(
            gfx,
            dp.x(30),
            dp.d(250),
            dp.d(300),
            dp.d(36),
            ev.accent,
            WHITE,
            dp.d(8),
            if en { "Manage Files" } else { "Dosyaları Yönet" },
        );

        draw_card(gfx, dp.x(16), dp.d(314), dp.d(328), dp.d(86), dp.d(20));
        gfx.text(dp.x(30), dp.d(334), dp.d(9), INK, 2, if en { "System Partition" } else { "Sistem Bölümü" });
        let mut sys_line = Line::new();
        sys_line
            .number(self.sys_mib, 1)
            .text(" MB  •  ")
            .number(self.pkg_count, 1)
            .text(if en { " system packages" } else { " sistem paketi" });
        gfx.text(dp.x(30), dp.d(358), dp.d(8), MUTED, 1, sys_line.as_str());
    }

    fn draw_system_tab(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp) {
        let en = self.lang == 1;
        let top = dp.d(94);
        let row_h = dp.d(72);
        for idx in 0..self.app_count {
            let app = &self.apps[idx];
            let ry = top + (idx as i32) * row_h;
            draw_card(gfx, dp.x(16), ry, dp.d(328), row_h - dp.d(8), dp.d(18));

            let icon_s = dp.d(42);
            let (glyph, col) = if app.id_str() == "nc.files" {
                (Glyph::Files, AMBER)
            } else {
                (Glyph::Settings, ev.accent)
            };
            draw_app_tile(gfx, glyph, dp.x(28), ry + dp.d(11), icon_s, col, false);

            let display_name = match app.id_str() {
                "nc.files" => if en { "Files" } else { "Dosyalar" },
                "nc.settings" => if en { "Settings" } else { "Ayarlar" },
                _ => app.name_str(),
            };
            gfx.text(dp.x(82), ry + dp.d(15), dp.d(9), INK, 2, display_name);

            let (st_text, st_col) = match app.state {
                2 => (if en { "Active" } else { "Açık" }, GREEN),
                1 => (if en { "In Background" } else { "Arka Planda" }, ev.accent),
                _ => (app.version_str(), MUTED),
            };
            gfx.text(dp.x(82), ry + dp.d(35), dp.d(7), st_col, 1, st_text);

            draw_pill_btn(
                gfx,
                dp.x(218),
                ry + dp.d(18),
                dp.d(58),
                dp.d(28),
                if app.state == 2 { GREEN_SOFT } else { ev.accent },
                if app.state == 2 { GREEN } else { WHITE },
                dp.d(8),
                if en { "Open" } else { "Aç" },
            );
            if app.state == 1 {
                draw_pill_btn(
                    gfx,
                    dp.x(282),
                    ry + dp.d(18),
                    dp.d(52),
                    dp.d(28),
                    CORAL_SOFT,
                    CORAL,
                    dp.d(8),
                    if en { "Close" } else { "Kapat" },
                );
            }
        }

        // Cihaz Özeti Kartı
        let about_y = top + (self.app_count as i32) * row_h + dp.d(6);
        let dev_name = core::str::from_utf8(&self.device_name[..self.device_name_len]).unwrap_or("NetizenCore");
        let screen = core::str::from_utf8(&self.screen[..self.screen_len]).unwrap_or("540x960");
        draw_card(gfx, dp.x(16), about_y, dp.d(328), dp.d(156), dp.d(20));

        gfx.text(dp.x(30), about_y + dp.d(16), dp.d(11), INK, 2, "NetizenCore ARMv8-A");
        gfx.fill_rect(dp.x(30), about_y + dp.d(36), dp.d(300), 1, DIVIDER);

        let mut mem_line = Line::new();
        if self.ram_mib >= 1024 {
            mem_line.number(self.ram_mib / 1024, 1).text(" GB (").number(self.free_mib, 1).text(" MB)");
        } else {
            mem_line.number(self.ram_mib, 1).text(" MB");
        }
        let rows: [(&str, &str); 3] = [
            (if en { "Model" } else { "Cihaz" }, dev_name),
            (if en { "Display" } else { "Ekran" }, screen),
            (if en { "Memory" } else { "Bellek" }, mem_line.as_str()),
        ];
        for (i, (label, val)) in rows.iter().enumerate() {
            let ry = about_y + dp.d(48 + (i as i32) * 34);
            gfx.text(dp.x(30), ry, dp.d(8), MUTED, 1, label);
            gfx.text(dp.x(170), ry, dp.d(8), INK, 2, val);
        }
    }

    fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        let dp = Dp::new(ev.area_w, ev.area_h);
        let en = self.lang == 1;
        gfx.fill_rect(0, 0, ev.area_w, ev.area_h, SURFACE_BG);

        gfx.text(
            dp.x(20),
            dp.d(16),
            dp.d(14),
            INK,
            2,
            if en { "Settings" } else { "Ayarlar" },
        );

        gfx.panel(dp.x(16), dp.d(48), dp.d(328), dp.d(34), dp.d(17), TRACK, 255, 0, 0);
        let idx = self.tab as i32;
        let seg_w = dp.d(79);
        let seg_x = dp.x(19 + idx * 81);
        gfx.panel(seg_x, dp.d(51), seg_w, dp.d(28), dp.d(14), CARD, 255, 0, 0);

        let labels = if en {
            ["Network", "Display", "Storage", "System"]
        } else {
            ["Bağlantı", "Görünüm", "Depolama", "Sistem"]
        };
        for (i, label) in labels.iter().enumerate() {
            let cx = dp.x(19 + (i as i32) * 81) + seg_w / 2;
            let col = if i == idx as usize { INK } else { MUTED };
            gfx.text_centered(cx, dp.d(61), dp.d(8), col, 2, label);
        }

        match self.tab {
            Tab::Network => self.draw_network_tab(ev, gfx, &dp),
            Tab::Display => self.draw_display_tab(ev, gfx, &dp),
            Tab::Storage => self.draw_storage_tab(ev, gfx, &dp),
            Tab::System => self.draw_system_tab(ev, gfx, &dp),
        }
    }
}

impl RingApp for SettingsApp {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.last_flags = ev.flags;
                self.refresh();
                RET_REDRAW
            }
            OP_PAUSE => RET_NONE,
            OP_UPDATE => {
                if ev.key == key::ESCAPE {
                    return RET_CLOSE;
                }
                let mut ret = RET_NONE;
                if ev.flags != self.last_flags {
                    self.last_flags = ev.flags;
                    self.refresh();
                    ret |= RET_REDRAW;
                }
                if let Some(p) = pointer(ev) {
                    if p.clicked {
                        let dp = Dp::new(ev.area_w, ev.area_h);
                        ret |= self.handle_click(p.x, p.y, &dp);
                    }
                }
                ret
            }
            OP_DRAW => {
                self.draw(ev, gfx);
                RET_NONE
            }
            _ => RET_NONE,
        }
    }
}

static mut APP: SettingsApp = SettingsApp::new();
static mut COMMANDS: [u8; 16384] = [0; 16384];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
