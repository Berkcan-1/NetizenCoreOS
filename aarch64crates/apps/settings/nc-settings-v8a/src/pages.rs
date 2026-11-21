use nc_abi::draw::ICON_SETTINGS;
use nc_abi::event::{unpack_dark_theme, EventBlock, RET_NONE, RET_REDRAW};
use nc_abi::sys as wire;
use nc_app_sdk::{apps, perm, security, settings, storage, Gfx, Line};
use nc_brand::{
    draw_badge, draw_chevron_left, draw_chevron_right, draw_pill_btn, draw_slider, draw_switch,
    Dp, Palette, UiRect, AMBER, BLUE, CORAL, GREEN, ORANGE, PURPLE, TEAL, WHITE,
};

use crate::{copy_str, put_size, s_of, Page, PinWizardMode, SettingsApp, ACCENTS};

const HOME_VISIBLE: usize = 6;
const APPS_VISIBLE: usize = 5;
/// Logical design height (dp) including consistent bottom safe margin.
/// Ensures all 14 Settings screens fit cleanly inside any viewport without clipping.
const SETTINGS_DESIGN_H: i32 = 476;

pub fn settings_dp(ev: &EventBlock) -> Dp {
    Dp::fit(ev.area_w, ev.area_h, SETTINGS_DESIGN_H)
}

/// Shared geometry for Settings interactive controls (used identically by draw & handle_click).
struct SettingsGeo {
    back_btn: UiRect,
    home_card: UiRect,
    home_row_y0: i32,
    home_row_h: i32,
    home_prev: UiRect,
    home_next: UiRect,
    apps_card: UiRect,
    apps_row_y0: i32,
    apps_row_h: i32,
    apps_prev: UiRect,
    apps_next: UiRect,
    net_wifi_sw: UiRect,
    dev_btn_left: UiRect,
    dev_btn_right: UiRect,
    disp_slider: UiRect,
    disp_dark_sw: UiRect,
    disp_timeout_row: UiRect,
    lock_pin_btn: UiRect,
    lock_pass_btn: UiRect,
    lock_off_btn: UiRect,
    lock_now_btn: UiRect,
    lock_boot_sw: UiRect,
    lock_info_sw: UiRect,
    lock_timeout_row: UiRect,
    sec_perms_btn: UiRect,
    detail_open_btn: UiRect,
    detail_stop_btn: UiRect,
    detail_clear_btn: UiRect,
    detail_uninst_btn: UiRect,
    storage_sync_btn: UiRect,
    dev_opt_sw: UiRect,
    dev_touch_sw: UiRect,
    dev_log_sw: UiRect,
    dev_anim_row: UiRect,
    sys_tr_btn: UiRect,
    sys_en_btn: UiRect,
    sys_tz_minus: UiRect,
    sys_tz_plus: UiRect,
    sys_build_row: UiRect,
}

impl SettingsGeo {
    fn new(dp: &Dp) -> Self {
        let usable_bottom = dp.usable_h(10);
        let nav_h = dp.d(34);
        let nav_y = dp.d(422).min(usable_bottom - nav_h);
        let home_card_y = dp.d(56);
        let home_card_h = (nav_y - dp.d(10) - home_card_y).max(dp.d(240));
        let home_row_y0 = home_card_y + dp.d(4);
        let home_row_h = ((home_card_h - dp.d(8)) / HOME_VISIBLE as i32).max(dp.d(40));

        let apps_nav_y = dp.d(412).min(usable_bottom - nav_h);
        let apps_card_y = dp.d(56);
        let apps_card_h = (apps_nav_y - dp.d(12) - apps_card_y).max(dp.d(240));
        let apps_row_y0 = apps_card_y + dp.d(6);
        let apps_row_h = ((apps_card_h - dp.d(12)) / APPS_VISIBLE as i32).max(dp.d(48));

        Self {
            back_btn: dp.rect(12, 10, 52, 38),
            home_card: UiRect::new(dp.x(16), home_card_y, dp.d(328), home_card_h),
            home_row_y0,
            home_row_h,
            home_prev: UiRect::new(dp.x(16), nav_y, dp.d(156), nav_h),
            home_next: UiRect::new(dp.x(188), nav_y, dp.d(156), nav_h),
            apps_card: UiRect::new(dp.x(16), apps_card_y, dp.d(328), apps_card_h),
            apps_row_y0,
            apps_row_h,
            apps_prev: UiRect::new(dp.x(16), apps_nav_y, dp.d(156), nav_h),
            apps_next: UiRect::new(dp.x(188), apps_nav_y, dp.d(156), nav_h),
            net_wifi_sw: dp.rect(284, 74, 46, 26),
            dev_btn_left: dp.rect(30, 172, 138, 30),
            dev_btn_right: dp.rect(192, 172, 138, 30),
            disp_slider: dp.rect(30, 98, 300, 22),
            disp_dark_sw: dp.rect(284, 158, 46, 26),
            disp_timeout_row: dp.rect(16, 196, 328, 36),
            lock_pin_btn: dp.rect(28, 124, 146, 34),
            lock_pass_btn: dp.rect(186, 124, 146, 34),
            lock_off_btn: dp.rect(28, 166, 146, 32),
            lock_now_btn: dp.rect(186, 166, 146, 32),
            lock_boot_sw: dp.rect(284, 230, 46, 26),
            lock_info_sw: dp.rect(284, 274, 46, 26),
            lock_timeout_row: dp.rect(16, 312, 328, 44),
            sec_perms_btn: dp.rect(30, 312, 300, 32),
            detail_open_btn: dp.rect(16, 324, 156, 36),
            detail_stop_btn: dp.rect(188, 324, 156, 36),
            detail_clear_btn: dp.rect(16, 372, 156, 36),
            detail_uninst_btn: dp.rect(188, 372, 156, 36),
            storage_sync_btn: dp.rect(30, 222, 300, 32),
            dev_opt_sw: dp.rect(284, 66, 46, 26),
            dev_touch_sw: dp.rect(284, 110, 46, 26),
            dev_log_sw: dp.rect(284, 154, 46, 26),
            dev_anim_row: dp.rect(16, 308, 328, 28),
            sys_tr_btn: dp.rect(186, 64, 66, 26),
            sys_en_btn: dp.rect(260, 64, 66, 26),
            sys_tz_minus: dp.rect(246, 108, 36, 26),
            sys_tz_plus: dp.rect(290, 108, 36, 26),
            sys_build_row: dp.rect(16, 336, 328, 50),
        }
    }

    fn home_row(&self, slot: usize) -> UiRect {
        UiRect::new(self.home_card.x, self.home_row_y0 + slot as i32 * self.home_row_h, self.home_card.w, self.home_row_h)
    }

    fn apps_row(&self, slot: usize) -> UiRect {
        UiRect::new(self.apps_card.x, self.apps_row_y0 + slot as i32 * self.apps_row_h, self.apps_card.w, self.apps_row_h)
    }

    fn pin_key(dp: &Dp, idx: usize) -> UiRect {
        let col = (idx % 3) as i32;
        let row = (idx / 3) as i32;
        UiRect::new(dp.x(44) + col * dp.d(94), dp.d(194) + row * dp.d(60), dp.d(84), dp.d(48))
    }

    fn wp_btn(dp: &Dp, idx: usize) -> UiRect {
        UiRect::new(dp.x(28) + idx as i32 * dp.d(62), dp.d(398), dp.d(54), dp.d(30))
    }
}

fn tr(en: bool, a: &'static str, b: &'static str) -> &'static str {
    if en { b } else { a }
}

fn card(gfx: &mut Gfx<'_>, pal: &Palette, x: i32, y: i32, w: i32, h: i32, r: i32) {
    gfx.panel(x, y, w, h, r, pal.card, 255, pal.divider, 255);
}

fn kv_row(gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, y: i32, label: &str, val: &str) {
    gfx.text(dp.x(30), dp.d(y), dp.d(9), pal.muted, 1, label);
    gfx.text(dp.x(172), dp.d(y), dp.d(9), pal.ink, 2, val);
}

impl SettingsApp {
    pub fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        let dp = settings_dp(ev);
        let en = ev.lang == 1;
        let pal = Palette::for_dark(unpack_dark_theme(ev.flags) || self.dark_theme);

        gfx.fill_rect(0, 0, ev.area_w, ev.area_h, pal.bg);

        // Ust baslik cubugu
        if self.page == Page::Home {
            gfx.icon(ICON_SETTINGS, dp.x(18), dp.d(14), dp.d(32), dp.d(32));
            gfx.text(dp.x(58), dp.d(19), dp.d(18), pal.ink, 3, tr(en, "Ayarlar", "Settings"));
        } else {
            let bs = dp.d(30);
            gfx.panel(dp.x(16), dp.d(14), bs, bs, bs / 2, pal.card, 255, pal.divider, 255);
            draw_chevron_left(gfx, dp.x(31), dp.d(29), &dp, pal.ink);
            gfx.text(dp.x(54), dp.d(21), dp.d(13), pal.ink, 2, self.page_title(en));
        }

        if self.toast_len > 0 {
            gfx.panel(dp.x(186), dp.d(18), dp.d(158), dp.d(24), dp.d(12), pal.card, 255, ev.accent, 180);
            gfx.text_centered(dp.x(265), dp.d(26), dp.d(7), ev.accent, 1, s_of(&self.toast, self.toast_len));
        }

        match self.page {
            Page::Home => self.draw_home(ev, gfx, &dp, &pal, en),
            Page::Network => self.draw_network(ev, gfx, &dp, &pal, en),
            Page::Devices => self.draw_devices(ev, gfx, &dp, &pal, en),
            Page::Display => self.draw_display(ev, gfx, &dp, &pal, en),
            Page::Sound => self.draw_sound(gfx, &dp, &pal, en),
            Page::LockScreen => self.draw_lock_screen(ev, gfx, &dp, &pal, en),
            Page::LockPinWizard => self.draw_pin_wizard(ev, gfx, &dp, &pal, en),
            Page::Security => self.draw_security(ev, gfx, &dp, &pal, en),
            Page::Permissions => self.draw_permissions(ev, gfx, &dp, &pal, en),
            Page::AppPermissions => self.draw_app_permissions(ev, gfx, &dp, &pal, en),
            Page::Location => self.draw_location(gfx, &dp, &pal, en),
            Page::Apps => self.draw_apps(ev, gfx, &dp, &pal, en),
            Page::AppDetail => self.draw_app_detail(ev, gfx, &dp, &pal, en),
            Page::Storage => self.draw_storage(ev, gfx, &dp, &pal, en),
            Page::Developer => self.draw_developer(ev, gfx, &dp, &pal, en),
            Page::System => self.draw_system(ev, gfx, &dp, &pal, en),
        }
    }

    fn page_title(&self, en: bool) -> &'static str {
        match self.page {
            Page::Home => tr(en, "Ayarlar", "Settings"),
            Page::Network => tr(en, "Ağ ve Bağlantı", "Network & Connectivity"),
            Page::Devices => tr(en, "Bağlı Cihazlar", "Connected Devices"),
            Page::Display => tr(en, "Ekran", "Display"),
            Page::Sound => tr(en, "Ses ve Titreşim", "Sound & Vibration"),
            Page::LockScreen => tr(en, "Kilit Ekranı", "Lock Screen"),
            Page::LockPinWizard => tr(en, "Kilit Kimliği", "Lock Credential"),
            Page::Security => tr(en, "Güvenlik ve Gizlilik", "Security & Privacy"),
            Page::Permissions => tr(en, "Uygulama İzinleri", "App Permissions"),
            Page::AppPermissions => tr(en, "İzin Yönetimi", "Manage Permissions"),
            Page::Location => tr(en, "Konum", "Location"),
            Page::Apps => tr(en, "Uygulamalar", "Applications"),
            Page::AppDetail => tr(en, "Uygulama Bilgisi", "App Info"),
            Page::Storage => tr(en, "Depolama", "Storage"),
            Page::Developer => tr(en, "Geliştirici Seçenekleri", "Developer Options"),
            Page::System => tr(en, "Sistem ve Hakkında", "System & About"),
        }
    }

    fn draw_home(&self, _ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        let geo = SettingsGeo::new(dp);
        let total = self.home_category_count();
        let hc = geo.home_card;
        card(gfx, pal, hc.x, hc.y, hc.w, hc.h, dp.d(20));
        for slot in 0..HOME_VISIBLE {
            let idx = self.main_scroll + slot;
            if idx >= total {
                break;
            }
            let row = geo.home_row(slot);
            if slot > 0 {
                gfx.fill_rect(dp.x(72), row.y, dp.d(256), 1, pal.divider);
            }
            let (badge, title, sub_line) = self.category_meta(idx, en);
            draw_badge(gfx, dp.x(44), row.cy(), dp.d(16), badge);
            gfx.text(dp.x(72), row.y + (row.h * 22 / 100), dp.d(10), pal.ink, 2, title);
            gfx.text(dp.x(72), row.y + (row.h * 56 / 100), dp.d(7), pal.muted, 1, sub_line.as_str());
            draw_chevron_right(gfx, dp.x(326), row.cy(), dp, pal.subtle);
        }

        // Sayfalama dugmeleri (Yukari / Asagi) — viewport-clamped shared geometry
        let prev_ok = self.main_scroll > 0;
        let next_ok = self.main_scroll + HOME_VISIBLE < total;
        let pb = geo.home_prev;
        let nb = geo.home_next;
        draw_pill_btn(
            gfx,
            pb.x,
            pb.y,
            pb.w,
            pb.h,
            pal.card,
            if prev_ok { pal.ink } else { pal.subtle },
            dp.d(8),
            tr(en, "▲ Yukarı", "▲ Up"),
        );
        draw_pill_btn(
            gfx,
            nb.x,
            nb.y,
            nb.w,
            nb.h,
            pal.card,
            if next_ok { pal.ink } else { pal.subtle },
            dp.d(8),
            tr(en, "▼ Aşağı", "▼ Down"),
        );
    }

    fn category_meta(&self, idx: usize, en: bool) -> (u32, &'static str, Line) {
        let mut sub = Line::new();
        match idx {
            0 => {
                if self.wifi_hw {
                    sub.text(if self.wifi {
                        tr(en, "Wi-Fi açık • Bağlantı hazır", "Wi-Fi on • Ready")
                    } else {
                        tr(en, "Wi-Fi kapalı", "Wi-Fi off")
                    });
                } else {
                    sub.text(tr(en, "Kablosuz bağlantı yok", "No wireless connection"));
                }
                (BLUE, tr(en, "Ağ ve Bağlantı", "Network & Connectivity"), sub)
            }
            1 => {
                if self.usb.mounted {
                    sub.text(tr(en, "USB takılı: ", "USB connected: ")).text(self.usb.label_str());
                } else {
                    sub.text(tr(en, "USB bellek, klavye ve ekran", "USB drives, keyboard & display"));
                }
                (TEAL, tr(en, "Bağlı Cihazlar", "Connected Devices"), sub)
            }
            2 => {
                sub.text(tr(en, "Parlaklık %", "Brightness "))
                    .number(self.brightness as u32, 1)
                    .text(if en { "% • " } else { " • " })
                    .text(if self.dark_theme { tr(en, "Koyu tema", "Dark theme") } else { tr(en, "Açık tema", "Light theme") });
                (AMBER, tr(en, "Ekran", "Display"), sub)
            }
            3 => {
                sub.text(if self.audio_hw {
                    tr(en, "Ses ve titreşim ayarları", "Volume and vibration controls")
                } else {
                    tr(en, "Bu cihazda desteklenmiyor", "Not available on this device")
                });
                (ORANGE, tr(en, "Ses ve Titreşim", "Sound & Vibration"), sub)
            }
            4 => {
                if self.lock.enabled {
                    sub.text(if self.lock.kind == 2 {
                        tr(en, "Parola ile korunuyor", "Protected with Password")
                    } else {
                        tr(en, "PIN ile korunuyor", "Protected with PIN")
                    });
                } else {
                    sub.text(tr(en, "Kilit kapalı • PIN veya parola ekleyin", "Off • Add a PIN or password"));
                }
                (PURPLE, tr(en, "Kilit Ekranı", "Lock Screen"), sub)
            }
            5 => {
                sub.text(tr(en, "Uygulama izinleri ve cihaz koruması", "App permissions and device safety"));
                (GREEN, tr(en, "Güvenlik ve Gizlilik", "Security & Privacy"), sub)
            }
            6 => {
                sub.text(if self.gps_hw {
                    tr(en, "Konum erişimi", "Location access")
                } else {
                    tr(en, "Bu cihazda konum servisi yok", "Not available on this device")
                });
                (TEAL, tr(en, "Konum", "Location"), sub)
            }
            7 => {
                sub.number(self.app_count as u32, 1).text(tr(en, " uygulama • İzinler ve yönetim", " apps • Permissions & storage"));
                (BLUE, tr(en, "Uygulamalar", "Applications"), sub)
            }
            8 => {
                put_size(&mut sub, self.used_kib, !en);
                sub.text(" / ").number(self.data_mib / 1024, 1).text(tr(en, " GB kullanılıyor", " GB used"));
                (GREEN, tr(en, "Depolama", "Storage"), sub)
            }
            9 => {
                sub.text(tr(en, "Dil, saat ve cihaz bilgileri", "Language, time & device info"));
                (0x0047_5569, tr(en, "Sistem ve Hakkında", "System & About"), sub)
            }
            _ => {
                sub.text(tr(en, "Gelişmiş hata ayıklama araçları", "Advanced debugging tools"));
                (CORAL, tr(en, "Geliştirici Seçenekleri", "Developer Options"), sub)
            }
        }
    }

    // A. Network & Connectivity
    fn draw_network(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        card(gfx, pal, dp.x(16), dp.d(58), dp.d(328), dp.d(124), dp.d(18));
        gfx.text(dp.x(30), dp.d(74), dp.d(10), pal.ink, 2, "Wi-Fi");
        gfx.text(
            dp.x(30),
            dp.d(92),
            dp.d(7),
            pal.muted,
            1,
            if self.wifi_hw {
                if self.wifi { tr(en, "Kablosuz ağ bağlantısı açık", "Wireless network is turned on") } else { tr(en, "Kablosuz ağ bağlantısı kapalı", "Wireless network is turned off") }
            } else {
                tr(en, "Ağ bağlantısı bulunamadı", "No network hardware found")
            },
        );
        if self.wifi_hw {
            draw_switch(gfx, dp.x(284), dp.d(74), dp.d(46), dp.d(26), self.wifi, ev.accent);
        }
        gfx.fill_rect(dp.x(28), dp.d(118), dp.d(304), 1, pal.divider);
        kv_row(gfx, dp, pal, 132, tr(en, "Ağ Adresi", "MAC Address"), s_of(&self.net_mac, self.net_mac_len));
        kv_row(gfx, dp, pal, 156, tr(en, "Bağlantı", "Status"), if self.wifi_hw && self.wifi { tr(en, "Etkin", "Active") } else { tr(en, "Kapalı", "Off") });

        // Bluetooth
        card(gfx, pal, dp.x(16), dp.d(194), dp.d(328), dp.d(78), dp.d(18));
        gfx.text(dp.x(30), dp.d(210), dp.d(10), pal.ink, 2, "Bluetooth");
        gfx.text(
            dp.x(30),
            dp.d(232),
            dp.d(8),
            AMBER,
            1,
            tr(en, "Bu cihazda Bluetooth desteği yok", "Bluetooth is not available on this device"),
        );
        gfx.text(
            dp.x(30),
            dp.d(250),
            dp.d(7),
            pal.muted,
            1,
            tr(en, "Bluetooth donanımı algılandığında açılabilir", "Available when Bluetooth hardware is present"),
        );

        // Ag Bilgisi (gercek QEMU virtio-net DHCP + Wi-Fi tarama durumu)
        card(gfx, pal, dp.x(16), dp.d(284), dp.d(328), dp.d(146), dp.d(18));
        gfx.text(dp.x(30), dp.d(298), dp.d(10), pal.ink, 2, tr(en, "Bağlantı & Wi-Fi Tarama", "Network & Wi-Fi Scan"));
        kv_row(
            gfx, dp, pal, 320,
            tr(en, "IPv4 (DHCP)", "IPv4 (DHCP)"),
            if self.net_dhcp { s_of(&self.net_ip, self.net_ip_len) } else { tr(en, "Atanmadı", "Unassigned") },
        );
        kv_row(
            gfx, dp, pal, 342,
            tr(en, "Ağ Geçidi", "Gateway"),
            if self.net_dhcp { s_of(&self.net_gw, self.net_gw_len) } else { "-" },
        );
        kv_row(
            gfx, dp, pal, 364,
            "DNS",
            if self.net_dhcp { s_of(&self.net_dns, self.net_dns_len) } else { "-" },
        );
        kv_row(
            gfx, dp, pal, 386,
            tr(en, "Wi-Fi Radyo", "Wi-Fi Radio"),
            if self.wifi_radio {
                tr(en, "Algılandı (802.11)", "Detected (802.11)")
            } else {
                tr(en, "Yok (QEMU virtio-net)", "Unavailable (QEMU virtio-net)")
            },
        );
        kv_row(
            gfx, dp, pal, 408,
            tr(en, "Kullanılabilir Ağlar", "Available Networks"),
            if self.wifi_radio {
                if self.wifi_scan_count > 0 { tr(en, "Tarama hazır", "Networks found") } else { tr(en, "Ağ bulunamadı", "None found") }
            } else {
                tr(en, "Wi-Fi donanımı gerekli", "Requires Wi-Fi hardware")
            },
        );
    }

    // B. Connected Devices
    fn draw_devices(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        card(gfx, pal, dp.x(16), dp.d(58), dp.d(328), dp.d(156), dp.d(18));
        gfx.text(dp.x(30), dp.d(74), dp.d(10), pal.ink, 2, tr(en, "USB Bellek", "USB Storage"));
        if self.usb.mounted {
            kv_row(gfx, dp, pal, 100, tr(en, "Adı", "Name"), self.usb.label_str());
            kv_row(gfx, dp, pal, 122, tr(en, "Biçim", "Format"), self.usb.fs_type_str());
            let mut cap = Line::new();
            put_size(&mut cap, self.usb.total_kib as u32, !en);
            cap.text(if self.usb.read_only { tr(en, " (Yalnızca okuma)", " (Read-only)") } else { tr(en, " (Okuma/Yazma)", " (Read/Write)") });
            kv_row(gfx, dp, pal, 144, tr(en, "Boyut", "Capacity"), cap.as_str());
            draw_pill_btn(gfx, dp.x(30), dp.d(172), dp.d(138), dp.d(30), pal.bg, pal.ink, dp.d(8), tr(en, "Yeniden Tara", "Rescan"));
            draw_pill_btn(gfx, dp.x(192), dp.d(172), dp.d(138), dp.d(30), CORAL, WHITE, dp.d(8), tr(en, "Güvenle Çıkar", "Safe Eject"));
        } else {
            gfx.text(dp.x(30), dp.d(106), dp.d(8), pal.muted, 1, tr(en, "Takılı USB bellek bulunamadı", "No USB drive is connected"));
            gfx.text(dp.x(30), dp.d(126), dp.d(7), pal.subtle, 1, tr(en, "Dosyalarına göz atmak için bir USB bellek takın", "Connect a USB drive to browse its files"));
            draw_pill_btn(gfx, dp.x(30), dp.d(166), dp.d(160), dp.d(32), ev.accent, WHITE, dp.d(8), tr(en, "USB Tara", "Scan for USB"));
        }

        card(gfx, pal, dp.x(16), dp.d(226), dp.d(328), dp.d(130), dp.d(18));
        gfx.text(dp.x(30), dp.d(242), dp.d(10), pal.ink, 2, tr(en, "Bağlı Donanımlar", "Connected Hardware"));
        kv_row(gfx, dp, pal, 268, tr(en, "Klavye", "Keyboard"), tr(en, "Bağlı", "Connected"));
        kv_row(gfx, dp, pal, 290, tr(en, "Dokunmatik", "Touchscreen"), tr(en, "Bağlı", "Connected"));
        let mut dline = Line::new();
        dline.number(self.disks, 1).text(tr(en, " sürücü bağlı", " drives attached"));
        kv_row(gfx, dp, pal, 312, tr(en, "Depolama", "Storage"), dline.as_str());
        kv_row(gfx, dp, pal, 334, tr(en, "Ekran", "Display"), tr(en, "Dahili ekran", "Built-in display"));
    }

    // C. Display
    fn draw_display(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        // Parlaklik
        card(gfx, pal, dp.x(16), dp.d(56), dp.d(328), dp.d(82), dp.d(18));
        let mut bl = Line::new();
        bl.text(tr(en, "Parlaklık  %", "Brightness  ")).number(self.brightness as u32, 1);
        gfx.text(dp.x(30), dp.d(70), dp.d(9), pal.ink, 2, bl.as_str());
        draw_slider(gfx, dp.x(30), dp.d(98), dp.d(300), dp.d(22), self.brightness, ev.accent);

        // Karanlik Tema + Zaman Asimi + Olcek
        card(gfx, pal, dp.x(16), dp.d(148), dp.d(328), dp.d(124), dp.d(18));
        gfx.text(dp.x(30), dp.d(164), dp.d(9), pal.ink, 2, tr(en, "Karanlık Tema", "Dark Theme"));
        draw_switch(gfx, dp.x(284), dp.d(158), dp.d(46), dp.d(26), self.dark_theme, ev.accent);
        gfx.fill_rect(dp.x(28), dp.d(194), dp.d(304), 1, pal.divider);

        let mut tl = Line::new();
        if self.screen_timeout_s == 0 {
            tl.text(tr(en, "Asla", "Never"));
        } else {
            tl.number(self.screen_timeout_s, 1).text("s");
        }
        kv_row(gfx, dp, pal, 208, tr(en, "Ekran Süresi", "Timeout"), tl.as_str());
        gfx.text(dp.x(266), dp.d(208), dp.d(8), ev.accent, 2, tr(en, "Değiştir", "Cycle"));
        gfx.fill_rect(dp.x(28), dp.d(232), dp.d(304), 1, pal.divider);
        kv_row(gfx, dp, pal, 246, tr(en, "Çözünürlük", "Resolution"), s_of(&self.screen, self.screen_len));

        // Vurgu Rengi + Duvar Kagidi
        card(gfx, pal, dp.x(16), dp.d(282), dp.d(328), dp.d(160), dp.d(18));
        gfx.text(dp.x(30), dp.d(296), dp.d(9), pal.ink, 2, tr(en, "Vurgu Rengi", "Accent Color"));
        for (i, (col, _, _)) in ACCENTS.iter().enumerate() {
            let cx = dp.x(48) + i as i32 * dp.d(64);
            let cy = dp.d(332);
            if *col == self.accent {
                gfx.dot(cx, cy, dp.d(18), pal.ink, 1024);
            }
            gfx.dot(cx, cy, dp.d(14), *col, 1024);
        }
        gfx.fill_rect(dp.x(28), dp.d(360), dp.d(304), 1, pal.divider);
        gfx.text(dp.x(30), dp.d(372), dp.d(9), pal.ink, 2, tr(en, "Duvar Kağıdı", "Wallpaper"));
        for i in 0..5 {
            let bx = dp.x(28) + i as i32 * dp.d(62);
            let sel = self.wallpaper == i as u8;
            let mut lbl = Line::new();
            lbl.text("0").number(i as u32 + 1, 1);
            draw_pill_btn(
                gfx,
                bx,
                dp.d(398),
                dp.d(54),
                dp.d(30),
                if sel { ev.accent } else { pal.bg },
                if sel { WHITE } else { pal.ink },
                dp.d(8),
                lbl.as_str(),
            );
        }
    }

    // D. Sound & Vibration
    fn draw_sound(&self, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        card(gfx, pal, dp.x(16), dp.d(58), dp.d(328), dp.d(146), dp.d(18));
        gfx.text(dp.x(30), dp.d(76), dp.d(11), pal.ink, 2, tr(en, "Ses ve Titreşim", "Sound & Vibration"));
        kv_row(gfx, dp, pal, 108, tr(en, "Hoparlör", "Speaker"), if self.audio_hw { tr(en, "Hazır", "Ready") } else { tr(en, "Kullanılamıyor", "Not available") });
        kv_row(gfx, dp, pal, 132, tr(en, "Titreşim", "Vibration"), if self.vibrator_hw { tr(en, "Hazır", "Ready") } else { tr(en, "Kullanılamıyor", "Not available") });
        gfx.text(
            dp.x(30),
            dp.d(164),
            dp.d(8),
            AMBER,
            1,
            tr(en, "Bu cihazda ses ve titreşim donanımı bulunmuyor.", "Sound and vibration are not available on this device."),
        );
    }

    // E. Lock Screen
    fn draw_lock_screen(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        card(gfx, pal, dp.x(16), dp.d(58), dp.d(328), dp.d(150), dp.d(18));
        let st_txt = if self.lock.enabled {
            if self.lock.kind == 2 { tr(en, "Parola ile korumalı", "Protected with Password") } else { tr(en, "PIN ile korumalı", "Protected with PIN") }
        } else {
            tr(en, "Kapalı", "Off")
        };
        kv_row(gfx, dp, pal, 76, tr(en, "Ekran Kilidi", "Screen Lock"), st_txt);
        gfx.text(
            dp.x(30),
            dp.d(100),
            dp.d(7),
            pal.muted,
            1,
            tr(en, "Cihazınızı PIN veya parola ile koruyun", "Protect your device with a PIN or password"),
        );

        draw_pill_btn(
            gfx,
            dp.x(28),
            dp.d(124),
            dp.d(146),
            dp.d(34),
            ev.accent,
            WHITE,
            dp.d(8),
            if self.lock.enabled { tr(en, "PIN Değiştir", "Change PIN") } else { tr(en, "PIN Ayarla", "Set PIN") },
        );
        draw_pill_btn(
            gfx,
            dp.x(186),
            dp.d(124),
            dp.d(146),
            dp.d(34),
            pal.bg,
            pal.ink,
            dp.d(8),
            tr(en, "Parola Ayarla", "Set Password"),
        );
        if self.lock.enabled {
            draw_pill_btn(gfx, dp.x(28), dp.d(166), dp.d(146), dp.d(32), CORAL, WHITE, dp.d(8), tr(en, "Kilidi Kapat", "Turn Off Lock"));
            draw_pill_btn(gfx, dp.x(186), dp.d(166), dp.d(146), dp.d(32), GREEN, WHITE, dp.d(8), tr(en, "Şimdi Kilitle", "Lock Now"));
        }

        // Kilit ayarlari
        card(gfx, pal, dp.x(16), dp.d(220), dp.d(328), dp.d(146), dp.d(18));
        gfx.text(dp.x(30), dp.d(236), dp.d(9), pal.ink, 2, tr(en, "Yeniden Başlatmada Kilitle", "Lock After Restart"));
        draw_switch(gfx, dp.x(284), dp.d(230), dp.d(46), dp.d(26), self.lock.lock_on_boot, ev.accent);
        gfx.fill_rect(dp.x(28), dp.d(266), dp.d(304), 1, pal.divider);

        gfx.text(dp.x(30), dp.d(280), dp.d(9), pal.ink, 2, tr(en, "Saat ve Tarihi Göster", "Show Clock & Date"));
        draw_switch(gfx, dp.x(284), dp.d(274), dp.d(46), dp.d(26), self.lock.show_info, ev.accent);
        gfx.fill_rect(dp.x(28), dp.d(310), dp.d(304), 1, pal.divider);

        let mut auto_l = Line::new();
        if self.lock.timeout_s == 0 {
            auto_l.text(tr(en, "Kapalı", "Off"));
        } else {
            auto_l.number(self.lock.timeout_s, 1).text(tr(en, " sn", "s"));
        }
        kv_row(gfx, dp, pal, 326, tr(en, "Otomatik Kilit", "Auto-Lock"), auto_l.as_str());
        gfx.text(dp.x(266), dp.d(326), dp.d(8), ev.accent, 2, tr(en, "Değiştir", "Change"));
    }

    fn draw_pin_wizard(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        card(gfx, pal, dp.x(16), dp.d(58), dp.d(328), dp.d(122), dp.d(18));
        let heading = if self.wizard_step == 0 {
            tr(en, "Mevcut PIN/Parolayı Doğrulayın", "Confirm Current PIN/Password")
        } else if self.wizard_mode == PinWizardMode::SetPassword {
            tr(en, "Yeni Parola (6-32 karakter, klavye)", "New Password (6-32 chars, keyboard)")
        } else {
            tr(en, "Yeni 4 Haneli PIN Girin", "Enter New 4-Digit PIN")
        };
        gfx.text_centered(dp.x(180), dp.d(78), dp.d(10), pal.ink, 2, heading);

        let len = if self.wizard_step == 0 { self.cur_len } else { self.new_len };
        let slots = if self.wizard_mode == PinWizardMode::SetPassword { len.max(6).min(12) } else { 4 };
        let sp = dp.d(22);
        let sx = dp.x(180) - ((slots as i32 - 1) * sp) / 2;
        for i in 0..slots {
            gfx.dot(sx + i as i32 * sp, dp.d(124), dp.d(6).max(4), if i < len { ev.accent } else { pal.track }, 1024);
        }
        gfx.text_centered(
            dp.x(180),
            dp.d(154),
            dp.d(7),
            pal.muted,
            1,
            tr(en, "Tuş takımını veya klavyeyi kullanın", "Use keypad or keyboard"),
        );

        const LABELS: [&str; 12] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "<", "0", "OK"];
        for idx in 0..12 {
            let col = (idx % 3) as i32;
            let row = (idx / 3) as i32;
            let bx = dp.x(44) + col * dp.d(94);
            let by = dp.d(194) + row * dp.d(60);
            let is_ok = idx == 11;
            draw_pill_btn(
                gfx,
                bx,
                by,
                dp.d(84),
                dp.d(48),
                if is_ok { ev.accent } else { pal.card },
                if is_ok { WHITE } else { pal.ink },
                dp.d(12),
                LABELS[idx],
            );
        }
    }

    // F. Security & Privacy
    fn draw_security(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        card(gfx, pal, dp.x(16), dp.d(58), dp.d(328), dp.d(180), dp.d(18));
        gfx.text(dp.x(30), dp.d(74), dp.d(10), pal.ink, 2, tr(en, "Cihaz Güvenliği", "Device Security"));
        kv_row(gfx, dp, pal, 102, tr(en, "Ekran Kilidi", "Screen Lock"), if self.lock.enabled { tr(en, "Açık", "On") } else { tr(en, "Kapalı", "Off") });
        kv_row(gfx, dp, pal, 126, tr(en, "Uygulamalar", "Apps"), tr(en, "Birbirinden yalıtılmış", "Isolated from each other"));
        kv_row(gfx, dp, pal, 150, tr(en, "Kurulum", "Installs"), tr(en, "Kurulum öncesi onay istenir", "Asks before installing"));
        kv_row(gfx, dp, pal, 174, tr(en, "Veri Koruması", "App Data"), tr(en, "Her uygulama kendi alanında", "Each app has private space"));
        kv_row(gfx, dp, pal, 198, tr(en, "Ayarlar", "Settings"), tr(en, "Güvenlik ayarları korumalı", "Protected from apps"));

        card(gfx, pal, dp.x(16), dp.d(250), dp.d(328), dp.d(106), dp.d(18));
        gfx.text(dp.x(30), dp.d(266), dp.d(10), pal.ink, 2, tr(en, "Uygulama İzinleri", "App Permissions"));
        gfx.text(
            dp.x(30),
            dp.d(288),
            dp.d(8),
            pal.muted,
            1,
            tr(en, "Hangi uygulamanın neye eriştiğini görün", "See what each app is allowed to access"),
        );
        draw_pill_btn(gfx, dp.x(30), dp.d(312), dp.d(300), dp.d(32), ev.accent, WHITE, dp.d(8), tr(en, "İzinleri Görüntüle", "View App Permissions"));
    }

    // F2. App Permissions Dashboard (Runtime Permissions)
    fn draw_permissions(&self, _ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        let geo = SettingsGeo::new(dp);
        let ac = geo.apps_card;
        card(gfx, pal, ac.x, ac.y, ac.w, ac.h, dp.d(18));
        for slot in 0..APPS_VISIBLE {
            let idx = self.perm_scroll + slot;
            if idx >= self.app_count {
                break;
            }
            let app = &self.apps[idx];
            let row = geo.apps_row(slot);
            if slot > 0 {
                gfx.fill_rect(dp.x(76), row.y, dp.d(252), 1, pal.divider);
            }
            let icon_s = dp.d(36).min(row.h - dp.d(12)).max(dp.d(24));
            gfx.icon(app.icon_id, dp.x(28), row.y + (row.h - icon_s) / 2, icon_s, icon_s);
            gfx.text(dp.x(74), row.y + (row.h * 18 / 100), dp.d(10), pal.ink, 2, app.name_str());

            let summary = self
                .app_perms[..self.perm_count]
                .iter()
                .find(|p| p.id_str() == app.id_str());
            let mut sub = Line::new();
            let mut sub_col = pal.muted;
            if let Some(s) = summary {
                let total = s.declared_mask.count_ones();
                let granted = (s.declared_mask & s.granted_mask).count_ones();
                let denied = (s.declared_mask & s.denied_mask).count_ones();
                let ask = total.saturating_sub(granted + denied);
                if total == 0 {
                    sub.text(tr(en, "Hassas izin gerektirmez", "No sensitive permissions"));
                } else {
                    sub.number(granted, 1)
                        .text(tr(en, " izinli", " allowed"));
                    if denied > 0 {
                        sub.text(" • ").number(denied, 1).text(tr(en, " reddedildi", " denied"));
                        sub_col = CORAL;
                    } else if ask > 0 {
                        sub.text(" • ").number(ask, 1).text(tr(en, " sorulmadı", " not requested"));
                    } else {
                        sub_col = GREEN;
                    }
                }
            } else {
                sub.text(tr(en, "Hassas izin gerektirmez", "No sensitive permissions"));
            }
            gfx.text(dp.x(74), row.y + (row.h * 56 / 100), dp.d(7), sub_col, 1, sub.as_str());
            draw_chevron_right(gfx, dp.x(326), row.cy(), dp, pal.subtle);
        }

        if self.app_count > APPS_VISIBLE {
            let pb = geo.apps_prev;
            let nb = geo.apps_next;
            draw_pill_btn(gfx, pb.x, pb.y, pb.w, pb.h, pal.card, pal.ink, dp.d(8), tr(en, "▲ Önceki", "▲ Prev"));
            draw_pill_btn(gfx, nb.x, nb.y, nb.w, nb.h, pal.card, pal.ink, dp.d(8), tr(en, "▼ Sonraki", "▼ Next"));
        }
    }

    fn draw_app_permissions(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        let Some(app) = self.apps.get(self.selected_perm_app) else { return };
        card(gfx, pal, dp.x(16), dp.d(56), dp.d(328), dp.d(58), dp.d(16));
        gfx.icon(app.icon_id, dp.x(28), dp.d(65), dp.d(38), dp.d(38));
        gfx.text(dp.x(76), dp.d(67), dp.d(11), pal.ink, 2, app.name_str());
        let mut idl = Line::new();
        idl.text(app.id_str()).text(" • ").text(if app.is_system { tr(en, "Sistem", "System") } else { tr(en, "Kullanıcı", "User") });
        gfx.text(dp.x(76), dp.d(86), dp.d(7), pal.muted, 1, idl.as_str());

        let summary = self
            .app_perms[..self.perm_count]
            .iter()
            .find(|p| p.id_str() == app.id_str());
        let (declared, granted, denied, unsup) = match summary {
            Some(s) => (s.declared_mask, s.granted_mask, s.denied_mask, s.unsupported_mask),
            None => (0, 0, 0, 0),
        };

        let mut decl_indices = [0usize; 9];
        let mut decl_count = 0usize;
        for (i, info) in perm::PERM_REGISTRY.iter().enumerate() {
            if (declared & info.bit) != 0 && decl_count < decl_indices.len() {
                decl_indices[decl_count] = i;
                decl_count += 1;
            }
        }

        if decl_count == 0 {
            card(gfx, pal, dp.x(16), dp.d(124), dp.d(328), dp.d(94), dp.d(16));
            gfx.text(dp.x(30), dp.d(144), dp.d(10), pal.ink, 2, tr(en, "Hassas izin bildirimi yok", "No runtime permissions"));
            gfx.text(
                dp.x(30),
                dp.d(168),
                dp.d(8),
                pal.muted,
                1,
                tr(en, "Bu uygulama hassas donanım veya veri izni istemiyor.", "This app does not request sensitive permissions."),
            );
            return;
        }

        const PERMS_PER_PAGE: usize = 3;
        let scroll = self.app_perm_scroll.min(decl_count.saturating_sub(1));
        for slot in 0..PERMS_PER_PAGE {
            let pos = scroll + slot;
            if pos >= decl_count {
                break;
            }
            let info = &perm::PERM_REGISTRY[decl_indices[pos]];
            let ry = dp.d(124 + slot as i32 * 96);
            card(gfx, pal, dp.x(16), ry, dp.d(328), dp.d(88), dp.d(16));
            gfx.text(dp.x(28), ry + dp.d(10), dp.d(9), pal.ink, 2, if en { info.title_en } else { info.title_tr });

            let is_mic = info.bit == perm::PERM_MICROPHONE_RECORD;
            let hw_unavail = (unsup & info.bit) != 0;
            let is_granted = (granted & info.bit) != 0;
            let is_denied = (denied & info.bit) != 0;

            let (st_col, st_lbl) = if is_mic {
                (AMBER, tr(en, "Desteklenmiyor", "Unsupported"))
            } else if is_granted {
                (GREEN, tr(en, "İzinli", "Allowed"))
            } else if is_denied {
                (CORAL, tr(en, "Reddedildi", "Denied"))
            } else {
                (ev.accent, tr(en, "Sorulmadı", "Not requested"))
            };
            let bw = dp.d(94);
            let bh_pill = dp.d(18);
            gfx.panel(dp.x(236), ry + dp.d(8), bw, bh_pill, bh_pill / 2, st_col, 32, st_col, 140);
            gfx.text_centered(dp.x(236) + bw / 2, ry + dp.d(12), dp.d(7), st_col, 2, st_lbl);

            gfx.text(dp.x(28), ry + dp.d(28), dp.d(7), pal.muted, 1, if en { info.desc_en } else { info.desc_tr });

            if hw_unavail && !is_mic {
                gfx.text(
                    dp.x(28),
                    ry + dp.d(42),
                    dp.d(6),
                    AMBER,
                    1,
                    tr(en, "Bu cihazda donanım desteklenmiyor (izin ayrı saklanır)", "Unsupported on this device (choice still saved)"),
                );
            }

            if is_mic {
                gfx.text(
                    dp.x(28),
                    ry + dp.d(56),
                    dp.d(7),
                    AMBER,
                    1,
                    tr(en, "Bu cihazda desteklenmiyor", "Unsupported on this device"),
                );
            } else {
                let by = ry + dp.d(54);
                let bh = dp.d(26);
                draw_pill_btn(
                    gfx,
                    dp.x(28),
                    by,
                    dp.d(94),
                    bh,
                    if is_granted { ev.accent } else { pal.bg },
                    if is_granted { WHITE } else { pal.ink },
                    dp.d(7),
                    tr(en, "İzin Ver", "Allow"),
                );
                draw_pill_btn(
                    gfx,
                    dp.x(132),
                    by,
                    dp.d(94),
                    bh,
                    if is_denied { CORAL } else { pal.bg },
                    if is_denied { WHITE } else { pal.ink },
                    dp.d(7),
                    tr(en, "Reddet", "Deny"),
                );
                draw_pill_btn(
                    gfx,
                    dp.x(236),
                    by,
                    dp.d(94),
                    bh,
                    if !is_granted && !is_denied { pal.track } else { pal.bg },
                    pal.ink,
                    dp.d(7),
                    tr(en, "Her Seferinde", "Ask"),
                );
            }
        }

        if decl_count > PERMS_PER_PAGE {
            let geo = SettingsGeo::new(dp);
            let pb = geo.apps_prev;
            let nb = geo.apps_next;
            draw_pill_btn(
                gfx,
                pb.x,
                pb.y,
                pb.w,
                pb.h,
                pal.card,
                if scroll > 0 { pal.ink } else { pal.subtle },
                dp.d(8),
                tr(en, "▲ Önceki", "▲ Prev"),
            );
            draw_pill_btn(
                gfx,
                nb.x,
                nb.y,
                nb.w,
                nb.h,
                pal.card,
                if scroll + PERMS_PER_PAGE < decl_count { pal.ink } else { pal.subtle },
                dp.d(8),
                tr(en, "▼ Sonraki", "▼ Next"),
            );
        }
    }

    // G. Location
    fn draw_location(&self, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        card(gfx, pal, dp.x(16), dp.d(58), dp.d(328), dp.d(140), dp.d(18));
        gfx.text(dp.x(30), dp.d(76), dp.d(11), pal.ink, 2, tr(en, "Konum", "Location"));
        kv_row(gfx, dp, pal, 108, tr(en, "Konum Servisi", "Location"), if self.gps_hw { tr(en, "Hazır", "Ready") } else { tr(en, "Kullanılamıyor", "Not available") });
        kv_row(gfx, dp, pal, 132, tr(en, "İzinli Uyg.", "Allowed Apps"), tr(en, "Yok", "None"));
        gfx.text(
            dp.x(30),
            dp.d(162),
            dp.d(8),
            AMBER,
            1,
            tr(en, "Bu cihazda konum (GPS) donanımı bulunmuyor.", "This device does not have location (GPS) hardware."),
        );
    }

    // H. Applications (Dynamic Registry!)
    fn draw_apps(&self, _ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        let geo = SettingsGeo::new(dp);
        let ac = geo.apps_card;
        card(gfx, pal, ac.x, ac.y, ac.w, ac.h, dp.d(18));
        for slot in 0..APPS_VISIBLE {
            let idx = self.app_scroll + slot;
            if idx >= self.app_count {
                break;
            }
            let app = &self.apps[idx];
            let row = geo.apps_row(slot);
            if slot > 0 {
                gfx.fill_rect(dp.x(76), row.y, dp.d(252), 1, pal.divider);
            }
            let icon_s = dp.d(40).min(row.h - dp.d(12)).max(dp.d(24));
            gfx.icon(app.icon_id, dp.x(28), row.y + (row.h - icon_s) / 2, icon_s, icon_s);
            gfx.text(dp.x(78), row.y + (row.h * 20 / 100), dp.d(10), pal.ink, 2, app.name_str());

            let mut sub = Line::new();
            sub.text("v").text(app.version_str()).text(" • ");
            sub.text(if app.is_system { tr(en, "Sistem", "System") } else { tr(en, "Kullanıcı", "User") });
            sub.text(" • ");
            sub.text(match app.state {
                apps::STATE_FG => tr(en, "Açık", "Running"),
                apps::STATE_PAUSED => tr(en, "Arka Plan", "Paused"),
                _ => tr(en, "Durdu", "Stopped"),
            });
            gfx.text(dp.x(78), row.y + (row.h * 54 / 100), dp.d(7), pal.muted, 1, sub.as_str());
            draw_chevron_right(gfx, dp.x(326), row.cy(), dp, pal.subtle);
        }

        if self.app_count > APPS_VISIBLE {
            let pb = geo.apps_prev;
            let nb = geo.apps_next;
            draw_pill_btn(gfx, pb.x, pb.y, pb.w, pb.h, pal.card, pal.ink, dp.d(8), tr(en, "▲ Önceki", "▲ Prev"));
            draw_pill_btn(gfx, nb.x, nb.y, nb.w, nb.h, pal.card, pal.ink, dp.d(8), tr(en, "▼ Sonraki", "▼ Next"));
        }
    }

    fn draw_app_detail(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        let Some(app) = self.apps.get(self.selected_app) else { return };
        card(gfx, pal, dp.x(16), dp.d(56), dp.d(328), dp.d(142), dp.d(18));
        gfx.icon(app.icon_id, dp.x(30), dp.d(70), dp.d(48), dp.d(48));
        gfx.text(dp.x(88), dp.d(74), dp.d(12), pal.ink, 2, app.name_str());
        gfx.text(dp.x(88), dp.d(94), dp.d(8), pal.muted, 1, app.id_str());

        let mut meta = Line::new();
        meta.text("v").text(app.version_str()).text(" • ");
        meta.text(if app.is_system { tr(en, "Sistem", "System") } else { tr(en, "Kullanıcı", "User") });
        meta.text(" • ").number(app.size_bytes / 1024, 1).text(" KB");
        gfx.text(dp.x(88), dp.d(112), dp.d(7), pal.subtle, 1, meta.as_str());

        let mut st_l = Line::new();
        st_l.text(match app.state {
            apps::STATE_FG => tr(en, "Çalışıyor", "Running"),
            apps::STATE_PAUSED => tr(en, "Duraklatıldı", "Paused"),
            _ => tr(en, "Durduruldu", "Stopped"),
        });
        if app.pages_kib > 0 {
            st_l.text(" (").number(app.pages_kib, 1).text(" KB RAM)");
        }
        kv_row(gfx, dp, pal, 140, tr(en, "Durum", "Status"), st_l.as_str());
        kv_row(gfx, dp, pal, 164, tr(en, "Doğrulama", "Verification"), if app.verified { tr(en, "Doğrulanmış", "Verified") } else { tr(en, "Standart uygulama", "Standard app") });

        // Izinler karti (anlasilir son kullanici dili)
        card(gfx, pal, dp.x(16), dp.d(208), dp.d(328), dp.d(104), dp.d(18));
        gfx.text(dp.x(30), dp.d(222), dp.d(9), pal.ink, 2, tr(en, "Uygulama İzinleri", "App Permissions"));
        let mut cap_y = 242;
        let cap_names: [(u32, &str, &str); 8] = [
            (nc_caps::SETTINGS_READ, "Sistem ayarlarını görüntüleme", "View system settings"),
            (nc_caps::FS_USER_READ, "Dosyalarınızı okuma", "Read your files"),
            (nc_caps::FS_USER_WRITE, "Dosya oluşturma ve düzenleme", "Create and edit files"),
            (nc_caps::FS_ALL_READ, "Cihaz depolamasını görüntüleme", "Browse device storage"),
            (nc_caps::PACKAGE_INSTALL, "Uygulama yükleme", "Install applications"),
            (nc_caps::PACKAGE_REMOVE, "Uygulama kaldırma", "Remove applications"),
            (nc_caps::SECURITY_MANAGE, "Kilit ve güvenlik ayarlarını yönetme", "Manage lock & security"),
            (nc_caps::PROCESS_LAUNCH, "Diğer uygulamaları açma", "Open other apps"),
        ];
        let mut shown = 0;
        for (bit, tr_s, en_s) in cap_names {
            if app.granted_caps & bit != 0 && shown < 4 {
                gfx.text(dp.x(32), dp.d(cap_y), dp.d(7), GREEN, 1, tr(en, tr_s, en_s));
                cap_y += 15;
                shown += 1;
            }
        }
        if shown == 0 {
            gfx.text(dp.x(32), dp.d(246), dp.d(8), pal.muted, 1, tr(en, "Özel izin gerektirmiyor", "No special permissions required"));
        }

        // Eylem dugmeleri
        draw_pill_btn(gfx, dp.x(16), dp.d(324), dp.d(156), dp.d(36), ev.accent, WHITE, dp.d(9), tr(en, "Aç", "Open"));
        draw_pill_btn(
            gfx,
            dp.x(188),
            dp.d(324),
            dp.d(156),
            dp.d(36),
            pal.card,
            if app.state != apps::STATE_STOPPED { CORAL } else { pal.subtle },
            dp.d(9),
            tr(en, "Zorla Durdur", "Force Stop"),
        );

        let clear_lbl = if self.confirm_clear { tr(en, "Onayla: Veriyi Sil", "Confirm Clear") } else { tr(en, "Veriyi Temizle", "Clear Data") };
        draw_pill_btn(gfx, dp.x(16), dp.d(372), dp.d(156), dp.d(36), if self.confirm_clear { AMBER } else { pal.card }, if self.confirm_clear { WHITE } else { pal.ink }, dp.d(8), clear_lbl);

        if app.is_system {
            draw_pill_btn(gfx, dp.x(188), dp.d(372), dp.d(156), dp.d(36), pal.card, pal.subtle, dp.d(8), tr(en, "Sistem (Korumalı)", "System (Protected)"));
        } else {
            let un_lbl = if self.confirm_uninstall { tr(en, "Onayla: Kaldır", "Confirm Remove") } else { tr(en, "Kaldır", "Uninstall") };
            draw_pill_btn(gfx, dp.x(188), dp.d(372), dp.d(156), dp.d(36), CORAL, WHITE, dp.d(9), un_lbl);
        }
    }

    // I. Storage
    fn draw_storage(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        card(gfx, pal, dp.x(16), dp.d(56), dp.d(328), dp.d(210), dp.d(18));
        gfx.text(dp.x(30), dp.d(72), dp.d(10), pal.ink, 2, tr(en, "Cihaz Depolaması", "Device Storage"));
        let mut tot = Line::new();
        put_size(&mut tot, self.used_kib, !en);
        tot.text(" / ").number(self.data_mib / 1024, 1).text(" GB");
        kv_row(gfx, dp, pal, 98, tr(en, "Kullanılan", "Used"), tot.as_str());

        let mut a = Line::new(); put_size(&mut a, self.apps_kib, !en);
        kv_row(gfx, dp, pal, 122, tr(en, "Uygulamalar", "Apps"), a.as_str());
        let mut m = Line::new(); put_size(&mut m, self.media_kib, !en);
        kv_row(gfx, dp, pal, 146, tr(en, "Belgeler ve Medya", "Docs & Media"), m.as_str());
        let mut d = Line::new(); put_size(&mut d, self.appdata_kib, !en);
        kv_row(gfx, dp, pal, 170, tr(en, "Uyg. Verisi", "App Data"), d.as_str());
        let mut s = Line::new(); put_size(&mut s, self.sys_used_kib, !en);
        s.text(" / ").number(self.sys_mib, 1).text(" MB");
        kv_row(gfx, dp, pal, 194, tr(en, "Sistem", "System"), s.as_str());
        draw_pill_btn(gfx, dp.x(30), dp.d(222), dp.d(300), dp.d(32), ev.accent, WHITE, dp.d(8), tr(en, "Değişiklikleri Kaydet", "Save Changes to Disk"));

        card(gfx, pal, dp.x(16), dp.d(278), dp.d(328), dp.d(106), dp.d(18));
        gfx.text(dp.x(30), dp.d(294), dp.d(10), pal.ink, 2, tr(en, "USB Bellek", "USB Storage"));
        if self.usb.mounted {
            let mut u = Line::new();
            u.text(self.usb.label_str()).text(" • ").text(self.usb.fs_type_str()).text(" • ");
            put_size(&mut u, self.usb.total_kib as u32, !en);
            gfx.text(dp.x(30), dp.d(322), dp.d(8), GREEN, 1, u.as_str());
            kv_row(gfx, dp, pal, 346, tr(en, "Koruma", "Access"), if self.usb.read_only { tr(en, "Yalnızca okuma (güvenli)", "Read-only (safe)") } else { tr(en, "Okuma ve yazma", "Read & write") });
        } else {
            gfx.text(dp.x(30), dp.d(326), dp.d(8), pal.muted, 1, tr(en, "Takılı USB bellek yok", "No USB drive connected"));
        }
    }

    // J. Developer Options
    fn draw_developer(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        card(gfx, pal, dp.x(16), dp.d(56), dp.d(328), dp.d(164), dp.d(18));
        gfx.text(dp.x(30), dp.d(72), dp.d(10), pal.ink, 2, tr(en, "Geliştirici Seçenekleri", "Developer Options"));
        draw_switch(gfx, dp.x(284), dp.d(66), dp.d(46), dp.d(26), self.dev_options, ev.accent);
        gfx.fill_rect(dp.x(28), dp.d(102), dp.d(304), 1, pal.divider);

        gfx.text(dp.x(30), dp.d(116), dp.d(9), pal.ink, 2, tr(en, "Dokunmaları Göster", "Show Touch Input"));
        draw_switch(gfx, dp.x(284), dp.d(110), dp.d(46), dp.d(26), self.show_touch, ev.accent);
        gfx.fill_rect(dp.x(28), dp.d(146), dp.d(304), 1, pal.divider);

        gfx.text(dp.x(30), dp.d(160), dp.d(9), pal.ink, 2, tr(en, "Ayrıntılı Çekirdek Günlüğü", "Verbose Kernel Logs"));
        draw_switch(gfx, dp.x(284), dp.d(154), dp.d(46), dp.d(26), self.verbose_log, ev.accent);

        gfx.text(
            dp.x(30),
            dp.d(194),
            dp.d(7),
            AMBER,
            1,
            tr(en, "Not: Kök erişimi vermez, yetki sınırlarını kaldırmaz.", "Note: Never grants root or bypasses capabilities."),
        );

        card(gfx, pal, dp.x(16), dp.d(232), dp.d(328), dp.d(152), dp.d(18));
        gfx.text(dp.x(30), dp.d(246), dp.d(10), pal.ink, 2, tr(en, "Sistem ve Süreç Tanılama", "System & Process Diagnostics"));
        let mut ram_l = Line::new();
        ram_l.number(self.free_mib, 1).text(" / ").number(self.ram_mib, 1).text(" MiB");
        kv_row(gfx, dp, pal, 272, tr(en, "Boş Bellek", "Free RAM"), ram_l.as_str());
        let mut task_l = Line::new();
        task_l.number(self.live_tasks, 1).text(" EL0");
        kv_row(gfx, dp, pal, 294, tr(en, "Canlı Görev", "Live Tasks"), task_l.as_str());
        let mut anim_l = Line::new();
        anim_l.number(self.anim_scale as u32, 1).text("%");
        kv_row(gfx, dp, pal, 316, tr(en, "Anim. Ölçeği", "Anim Scale"), anim_l.as_str());
        gfx.text(dp.x(266), dp.d(316), dp.d(8), ev.accent, 2, tr(en, "Değiştir", "Cycle"));
        kv_row(gfx, dp, pal, 338, "USB Debug", tr(en, "Aktarım yok (devre dışı)", "No transport (off)"));
        kv_row(gfx, dp, pal, 360, "SDK ABI", s_of(&self.sdk_ver, self.sdk_ver_len));
    }

    // K. System & About Device
    fn draw_system(&self, ev: &EventBlock, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, en: bool) {
        // Dil ve Saat Dilimi
        card(gfx, pal, dp.x(16), dp.d(56), dp.d(328), dp.d(94), dp.d(18));
        gfx.text(dp.x(30), dp.d(70), dp.d(9), pal.ink, 2, tr(en, "Dil / Language", "Language / Dil"));
        draw_pill_btn(gfx, dp.x(186), dp.d(64), dp.d(66), dp.d(26), if self.lang == 0 { ev.accent } else { pal.bg }, if self.lang == 0 { WHITE } else { pal.ink }, dp.d(8), "TR");
        draw_pill_btn(gfx, dp.x(260), dp.d(64), dp.d(66), dp.d(26), if self.lang == 1 { ev.accent } else { pal.bg }, if self.lang == 1 { WHITE } else { pal.ink }, dp.d(8), "EN");
        gfx.fill_rect(dp.x(28), dp.d(100), dp.d(304), 1, pal.divider);

        let mut tz = Line::new();
        tz.text("UTC");
        if self.tz_hours >= 0 { tz.text("+").number(self.tz_hours as u32, 1); } else { tz.text("-").number((-self.tz_hours) as u32, 1); }
        kv_row(gfx, dp, pal, 114, tr(en, "Saat Dilimi", "Timezone"), tz.as_str());
        draw_pill_btn(gfx, dp.x(246), dp.d(108), dp.d(36), dp.d(26), pal.bg, pal.ink, dp.d(9), "-");
        draw_pill_btn(gfx, dp.x(290), dp.d(108), dp.d(36), dp.d(26), pal.bg, pal.ink, dp.d(9), "+");

        // Cihaz Hakkinda (Derleme Numarasina 7 kez dokun -> Gelistirici Secenekleri!)
        card(gfx, pal, dp.x(16), dp.d(160), dp.d(328), dp.d(256), dp.d(18));
        gfx.text(dp.x(30), dp.d(174), dp.d(10), pal.ink, 2, tr(en, "Cihaz Hakkında", "About Device"));
        kv_row(gfx, dp, pal, 200, tr(en, "Cihaz", "Device"), s_of(&self.device_model, self.device_model_len));
        kv_row(gfx, dp, pal, 222, "SoC / CPU", s_of(&self.soc, self.soc_len));
        kv_row(gfx, dp, pal, 244, tr(en, "Mimari", "Arch"), "ARMv8-A (AArch64)");
        kv_row(gfx, dp, pal, 266, tr(en, "Çekirdek", "Kernel"), s_of(&self.kernel_ver, self.kernel_ver_len));
        let mut mem_l = Line::new();
        mem_l.number(self.ram_mib, 1).text(" MiB RAM");
        kv_row(gfx, dp, pal, 288, tr(en, "Bellek", "Memory"), mem_l.as_str());
        kv_row(gfx, dp, pal, 310, "SDK", s_of(&self.sdk_ver, self.sdk_ver_len));
        gfx.fill_rect(dp.x(28), dp.d(334), dp.d(304), 1, pal.divider);
        kv_row(gfx, dp, pal, 348, tr(en, "Derleme No", "Build No"), s_of(&self.build_number, self.build_number_len));
        gfx.text(
            dp.x(30),
            dp.d(372),
            dp.d(7),
            pal.subtle,
            1,
            if self.dev_options {
                tr(en, "Geliştirici seçenekleri etkin", "Developer options enabled")
            } else {
                tr(en, "Geliştirici seçenekleri için Derleme No'ya 7 kez dokunun", "Tap Build No 7 times for Developer Options")
            },
        );
        gfx.text(dp.x(30), dp.d(394), dp.d(7), pal.muted, 1, "NetizenCore OS • Independent ARMv8-A Mobile OS");
    }

    pub fn handle_click(&mut self, ev: &EventBlock, dp: &Dp, x: i32, y: i32) -> u32 {
        let en = ev.lang == 1;
        let geo = SettingsGeo::new(dp);
        let pad = dp.d(6);

        // Ust sol geri dugmesi
        if self.page != Page::Home && geo.back_btn.contains(x, y) {
            self.toast_len = 0;
            self.confirm_clear = false;
            self.confirm_uninstall = false;
            self.page = match self.page {
                Page::AppDetail => Page::Apps,
                Page::AppPermissions => Page::Permissions,
                Page::Permissions => Page::Security,
                Page::LockPinWizard => Page::LockScreen,
                _ => Page::Home,
            };
            return RET_REDRAW;
        }

        match self.page {
            Page::Home => {
                for slot in 0..HOME_VISIBLE {
                    if geo.home_row(slot).contains(x, y) {
                        let idx = self.main_scroll + slot;
                        if idx < self.home_category_count() {
                            self.toast_len = 0;
                            self.page = self.home_category_page(idx);
                            self.refresh();
                            return RET_REDRAW;
                        }
                    }
                }
                if geo.home_prev.expand(pad).contains(x, y) && self.main_scroll > 0 {
                    self.main_scroll = self.main_scroll.saturating_sub(3);
                    return RET_REDRAW;
                }
                if geo.home_next.expand(pad).contains(x, y)
                    && self.main_scroll + HOME_VISIBLE < self.home_category_count()
                {
                    self.main_scroll = (self.main_scroll + 3)
                        .min(self.home_category_count().saturating_sub(HOME_VISIBLE));
                    return RET_REDRAW;
                }
            }
            Page::Network => {
                if self.wifi_hw && geo.net_wifi_sw.expand(dp.d(12)).contains(x, y) {
                    self.wifi = !self.wifi;
                    let _ = settings::set_u32(wire::MOBILE_SET_WIFI, self.wifi as u32);
                    return RET_REDRAW;
                }
            }
            Page::Devices => {
                if geo.dev_btn_left.expand(pad).contains(x, y)
                    || (!self.usb.mounted && dp.rect(30, 166, 160, 32).expand(pad).contains(x, y))
                {
                    let _ = storage::usb_rescan();
                    self.refresh();
                    self.set_toast(if self.usb.mounted { tr(en, "USB bulundu", "USB found") } else { tr(en, "USB yok", "No USB") });
                    return RET_REDRAW;
                } else if self.usb.mounted && geo.dev_btn_right.expand(pad).contains(x, y) {
                    let _ = storage::usb_eject();
                    self.refresh();
                    self.set_toast(tr(en, "USB çıkarıldı", "USB ejected"));
                    return RET_REDRAW;
                }
            }
            Page::Display => {
                if geo.disp_slider.expand(dp.d(8)).contains(x, y) {
                    let rel = (x - geo.disp_slider.x).clamp(0, geo.disp_slider.w);
                    let pct = (20 + rel * 80 / geo.disp_slider.w.max(1)).clamp(20, 100);
                    let rounded = (((pct + 5) / 10) * 10).clamp(20, 100) as u8;
                    self.brightness = rounded;
                    let _ = settings::set_u32(wire::MOBILE_SET_BRIGHTNESS, rounded as u32);
                    return RET_REDRAW;
                }
                if geo.disp_dark_sw.expand(dp.d(12)).contains(x, y) {
                    self.dark_theme = !self.dark_theme;
                    let _ = settings::set_u32(wire::MOBILE_SET_DARK_THEME, self.dark_theme as u32);
                    return RET_REDRAW;
                }
                if geo.disp_timeout_row.contains(x, y) {
                    self.screen_timeout_s = match self.screen_timeout_s {
                        30 => 60,
                        60 => 120,
                        120 => 300,
                        300 => 0,
                        _ => 30,
                    };
                    let _ = settings::set_u32(wire::MOBILE_SET_SCREEN_TIMEOUT, self.screen_timeout_s);
                    return RET_REDRAW;
                }
                if y >= dp.d(312) && y <= dp.d(354) {
                    for (i, (col, _, _)) in ACCENTS.iter().enumerate() {
                        let cx = dp.x(48) + i as i32 * dp.d(64);
                        if (x - cx).abs() <= dp.d(24) {
                            self.accent = *col;
                            let _ = settings::set_u32(wire::MOBILE_SET_ACCENT, *col);
                            return RET_REDRAW;
                        }
                    }
                }
                for i in 0..5 {
                    if SettingsGeo::wp_btn(dp, i).expand(pad).contains(x, y) {
                        self.wallpaper = i as u8;
                        let _ = settings::set_u32(wire::MOBILE_SET_WALLPAPER, i as u32);
                        return RET_REDRAW;
                    }
                }
            }
            Page::LockScreen => {
                if geo.lock_pin_btn.expand(pad).contains(x, y) {
                    self.start_pin_wizard(PinWizardMode::SetPin);
                    return RET_REDRAW;
                }
                if geo.lock_pass_btn.expand(pad).contains(x, y) {
                    self.start_pin_wizard(PinWizardMode::SetPassword);
                    return RET_REDRAW;
                }
                if self.lock.enabled && geo.lock_off_btn.expand(pad).contains(x, y) {
                    self.start_pin_wizard(PinWizardMode::Disable);
                    return RET_REDRAW;
                }
                if self.lock.enabled && geo.lock_now_btn.expand(pad).contains(x, y) {
                    let _ = security::lock_now();
                    return RET_REDRAW;
                }
                if geo.lock_boot_sw.expand(dp.d(12)).contains(x, y) {
                    let next = !self.lock.lock_on_boot;
                    self.lock.lock_on_boot = next;
                    let _ = settings::set_u32(wire::MOBILE_SET_LOCK_ON_BOOT, next as u32);
                    return RET_REDRAW;
                }
                if geo.lock_info_sw.expand(dp.d(12)).contains(x, y) {
                    let next = !self.lock.show_info;
                    self.lock.show_info = next;
                    let _ = settings::set_u32(wire::MOBILE_SET_LOCK_SHOW_INFO, next as u32);
                    return RET_REDRAW;
                }
                if geo.lock_timeout_row.contains(x, y) {
                    let next = match self.lock.timeout_s {
                        0 => 30,
                        30 => 60,
                        60 => 120,
                        120 => 300,
                        _ => 0,
                    };
                    self.lock.timeout_s = next;
                    let _ = settings::set_u32(wire::MOBILE_SET_LOCK_TIMEOUT, next);
                    return RET_REDRAW;
                }
            }
            Page::LockPinWizard => {
                for idx in 0..12 {
                    if SettingsGeo::pin_key(dp, idx).contains(x, y) {
                        match idx {
                            0..=8 => self.wizard_push(b'1' + idx as u8, en),
                            9 => self.wizard_pop(),
                            10 => self.wizard_push(b'0', en),
                            11 => self.wizard_submit(en),
                            _ => {}
                        }
                        return RET_REDRAW;
                    }
                }
            }
            Page::Security => {
                if geo.sec_perms_btn.expand(pad).contains(x, y) {
                    self.page = Page::Permissions;
                    self.perm_scroll = 0;
                    self.refresh();
                    return RET_REDRAW;
                }
            }
            Page::Permissions => {
                for slot in 0..APPS_VISIBLE {
                    if geo.apps_row(slot).contains(x, y) {
                        let idx = self.perm_scroll + slot;
                        if idx < self.app_count {
                            self.selected_perm_app = idx;
                            self.app_perm_scroll = 0;
                            self.page = Page::AppPermissions;
                            return RET_REDRAW;
                        }
                    }
                }
                if self.app_count > APPS_VISIBLE {
                    if geo.apps_prev.expand(pad).contains(x, y) && self.perm_scroll > 0 {
                        self.perm_scroll = self.perm_scroll.saturating_sub(APPS_VISIBLE);
                        return RET_REDRAW;
                    }
                    if geo.apps_next.expand(pad).contains(x, y)
                        && self.perm_scroll + APPS_VISIBLE < self.app_count
                    {
                        self.perm_scroll += APPS_VISIBLE;
                        return RET_REDRAW;
                    }
                }
            }
            Page::AppPermissions => {
                if let Some(app) = self.apps.get(self.selected_perm_app).copied() {
                    let mut id_buf = [0u8; 36];
                    let id_len = copy_str(&mut id_buf, app.id_str());
                    let id_str = core::str::from_utf8(&id_buf[..id_len]).unwrap_or("");
                    let declared = self
                        .app_perms[..self.perm_count]
                        .iter()
                        .find(|p| p.id_str() == id_str)
                        .map(|p| p.declared_mask)
                        .unwrap_or(0);
                    let mut decl_indices = [0usize; 9];
                    let mut decl_count = 0usize;
                    for (i, info) in perm::PERM_REGISTRY.iter().enumerate() {
                        if (declared & info.bit) != 0 && decl_count < decl_indices.len() {
                            decl_indices[decl_count] = i;
                            decl_count += 1;
                        }
                    }
                    const PERMS_PER_PAGE: usize = 3;
                    let scroll = self.app_perm_scroll.min(decl_count.saturating_sub(1));
                    for slot in 0..PERMS_PER_PAGE {
                        let pos = scroll + slot;
                        if pos >= decl_count {
                            break;
                        }
                        let info = &perm::PERM_REGISTRY[decl_indices[pos]];
                        if info.bit != perm::PERM_MICROPHONE_RECORD {
                            let ry = dp.d(124 + slot as i32 * 96);
                            let by = ry + dp.d(54);
                            let bh = dp.d(26);
                            let r_allow = UiRect::new(dp.x(28), by, dp.d(94), bh).expand(pad);
                            let r_deny = UiRect::new(dp.x(132), by, dp.d(94), bh).expand(pad);
                            let r_ask = UiRect::new(dp.x(236), by, dp.d(94), bh).expand(pad);
                            let next = if r_allow.contains(x, y) {
                                Some(perm::PermState::Granted)
                            } else if r_deny.contains(x, y) {
                                Some(perm::PermState::Denied)
                            } else if r_ask.contains(x, y) {
                                Some(perm::PermState::Ask)
                            } else {
                                None
                            };
                            if let Some(st) = next {
                                if perm::set_app_perm(id_str, info.bit, st).is_ok() {
                                    self.refresh();
                                    self.set_toast(tr(en, "İzin güncellendi", "Permission updated"));
                                } else {
                                    self.set_toast(tr(en, "İzin güncellenemedi", "Update failed"));
                                }
                                return RET_REDRAW;
                            }
                        }
                    }
                    if decl_count > PERMS_PER_PAGE {
                        if geo.apps_prev.expand(pad).contains(x, y) && scroll > 0 {
                            self.app_perm_scroll = scroll.saturating_sub(PERMS_PER_PAGE);
                            return RET_REDRAW;
                        }
                        if geo.apps_next.expand(pad).contains(x, y) && scroll + PERMS_PER_PAGE < decl_count {
                            self.app_perm_scroll = scroll + PERMS_PER_PAGE;
                            return RET_REDRAW;
                        }
                    }
                }
            }
            Page::Apps => {
                for slot in 0..APPS_VISIBLE {
                    if geo.apps_row(slot).contains(x, y) {
                        let idx = self.app_scroll + slot;
                        if idx < self.app_count {
                            self.selected_app = idx;
                            self.confirm_clear = false;
                            self.confirm_uninstall = false;
                            self.page = Page::AppDetail;
                            return RET_REDRAW;
                        }
                    }
                }
                if self.app_count > APPS_VISIBLE {
                    if geo.apps_prev.expand(pad).contains(x, y) && self.app_scroll > 0 {
                        self.app_scroll = self.app_scroll.saturating_sub(APPS_VISIBLE);
                        return RET_REDRAW;
                    }
                    if geo.apps_next.expand(pad).contains(x, y)
                        && self.app_scroll + APPS_VISIBLE < self.app_count
                    {
                        self.app_scroll += APPS_VISIBLE;
                        return RET_REDRAW;
                    }
                }
            }
            Page::AppDetail => {
                if let Some(app) = self.apps.get(self.selected_app).copied() {
                    if dp.rect(16, 208, 328, 104).contains(x, y) {
                        self.selected_perm_app = self.selected_app;
                        self.app_perm_scroll = 0;
                        self.page = Page::AppPermissions;
                        return RET_REDRAW;
                    }
                    let mut id_buf = [0u8; 36];
                    let id_len = copy_str(&mut id_buf, app.id_str());
                    let id_str = core::str::from_utf8(&id_buf[..id_len]).unwrap_or("");
                    if geo.detail_open_btn.expand(pad).contains(x, y) {
                        let _ = apps::launch(id_str);
                        return RET_REDRAW;
                    }
                    if geo.detail_stop_btn.expand(pad).contains(x, y) {
                        if app.state != apps::STATE_STOPPED {
                            let _ = apps::stop(id_str);
                            self.refresh();
                            self.set_toast(tr(en, "Durduruldu", "Stopped"));
                        }
                        return RET_REDRAW;
                    }
                    if geo.detail_clear_btn.expand(pad).contains(x, y) {
                        if self.confirm_clear {
                            self.confirm_clear = false;
                            if apps::clear_data(id_str).is_ok() {
                                self.set_toast(tr(en, "Veri temizlendi", "Data cleared"));
                            }
                        } else {
                            self.confirm_clear = true;
                        }
                        return RET_REDRAW;
                    }
                    if !app.is_system && geo.detail_uninst_btn.expand(pad).contains(x, y) {
                        if self.confirm_uninstall {
                            self.confirm_uninstall = false;
                            if apps::uninstall(id_str, true).is_ok() {
                                self.refresh();
                                self.page = Page::Apps;
                                self.set_toast(tr(en, "Kaldırıldı", "Uninstalled"));
                            }
                        } else {
                            self.confirm_uninstall = true;
                        }
                        return RET_REDRAW;
                    }
                }
            }
            Page::Storage => {
                if geo.storage_sync_btn.expand(pad).contains(x, y) {
                    let ok = settings::sync_data().is_ok();
                    self.refresh();
                    self.set_toast(if ok { tr(en, "Disk eşitlendi", "Storage synced") } else { tr(en, "Eşitleme başarısız", "Sync failed") });
                    return RET_REDRAW;
                }
            }
            Page::Developer => {
                if geo.dev_opt_sw.expand(dp.d(12)).contains(x, y) {
                    self.dev_options = !self.dev_options;
                    let _ = settings::set_u32(wire::MOBILE_SET_DEV_OPTIONS, self.dev_options as u32);
                    if !self.dev_options {
                        self.page = Page::Home;
                        self.main_scroll = 0;
                    }
                    return RET_REDRAW;
                }
                if geo.dev_touch_sw.expand(dp.d(12)).contains(x, y) {
                    self.show_touch = !self.show_touch;
                    let _ = settings::set_u32(wire::MOBILE_SET_SHOW_TOUCH, self.show_touch as u32);
                    return RET_REDRAW;
                }
                if geo.dev_log_sw.expand(dp.d(12)).contains(x, y) {
                    self.verbose_log = !self.verbose_log;
                    let _ = settings::set_u32(wire::MOBILE_SET_VERBOSE_LOG, self.verbose_log as u32);
                    return RET_REDRAW;
                }
                if geo.dev_anim_row.contains(x, y) {
                    self.anim_scale = match self.anim_scale {
                        0 => 50,
                        50 => 100,
                        100 => 150,
                        _ => 0,
                    };
                    let _ = settings::set_u32(wire::MOBILE_SET_ANIM_SCALE, self.anim_scale as u32);
                    return RET_REDRAW;
                }
            }
            Page::System => {
                if geo.sys_tr_btn.expand(pad).contains(x, y) {
                    self.lang = 0;
                    let _ = settings::set_u32(wire::MOBILE_SET_LANG, 0);
                    return RET_REDRAW;
                }
                if geo.sys_en_btn.expand(pad).contains(x, y) {
                    self.lang = 1;
                    let _ = settings::set_u32(wire::MOBILE_SET_LANG, 1);
                    return RET_REDRAW;
                }
                if geo.sys_tz_minus.expand(pad).contains(x, y) {
                    self.tz_hours = (self.tz_hours - 1).clamp(-12, 14);
                    let _ = settings::set_u32(wire::MOBILE_SET_TZ, self.tz_hours as u32);
                    return RET_REDRAW;
                }
                if geo.sys_tz_plus.expand(pad).contains(x, y) {
                    self.tz_hours = (self.tz_hours + 1).clamp(-12, 14);
                    let _ = settings::set_u32(wire::MOBILE_SET_TZ, self.tz_hours as u32);
                    return RET_REDRAW;
                }
                if geo.sys_build_row.contains(x, y) && !self.dev_options {
                    self.build_taps = self.build_taps.saturating_add(1);
                    if self.build_taps >= 7 {
                        self.dev_options = true;
                        let _ = settings::set_u32(wire::MOBILE_SET_DEV_OPTIONS, 1);
                        self.set_toast(tr(en, "Geliştirici açıldı!", "Developer enabled!"));
                    } else {
                        let rem = 7 - self.build_taps;
                        let mut tl = Line::new();
                        tl.number(rem as u32, 1).text(tr(en, " dokunuş kaldı", " taps left"));
                        self.set_toast(tl.as_str());
                    }
                    return RET_REDRAW;
                }
            }
            _ => {}
        }
        RET_NONE
    }
}
