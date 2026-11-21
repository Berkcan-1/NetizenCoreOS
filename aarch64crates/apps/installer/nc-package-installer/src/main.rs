//! NetizenCore Bagimsiz Ring 3 / EL0 NTZC Paket Kurucu (`nc.package-installer`).
//!
//! Dosyalar uygulamasi veya harici USB depolamadan secilen `.ntzc` paketlerini cekirdek
//! araciligiyla denetler, izinleri ve imza durumunu kullaniciya gosterir, acik onay alir
//! ve yetkili `SYS_PKG_INSTALL` cagrisiyla kurar.
#![no_std]
#![no_main]

use nc_abi::draw::ICON_GENERIC;
use nc_abi::event::{
    unpack_dark_theme, EventBlock, OP_DRAW, OP_INIT, OP_PAUSE, OP_RESUME, OP_UPDATE, RET_CLOSE,
    RET_NONE, RET_REDRAW,
};
use nc_abi::key;
use nc_app_sdk::{apps, dispatch, pkg, pointer, sys, Gfx, Line, RingApp};
use nc_brand::{
    draw_pill_btn, Dp, Palette, AMBER, BLUE, CORAL, GREEN, WHITE,
};

const MAX_PATH: usize = 200;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Empty,
    Review,
    Invalid,
    Installed,
    Failed,
}

struct InstallerApp {
    stage: Stage,
    path: [u8; MAX_PATH],
    path_len: usize,
    pkg_id: [u8; 40],
    pkg_id_len: usize,
    pkg_name: [u8; 40],
    pkg_name_len: usize,
    pkg_ver: [u8; 20],
    pkg_ver_len: usize,
    installed_ver: [u8; 20],
    installed_ver_len: usize,
    arch: [u8; 28],
    arch_len: usize,
    abi: [u8; 16],
    abi_len: usize,
    publisher: [u8; 40],
    publisher_len: usize,
    error_msg: [u8; 64],
    error_len: usize,
    icon_id: u32,
    verified: bool,
    runnable: bool,
    already_installed: bool,
    system_conflict: bool,
    requested_caps: u32,
    granted_caps: u32,
    denied_caps: u32,
    declared_perms: u32,
    unknown_perms: [u8; 64],
    unknown_perms_len: usize,
    size_bytes: u32,
    close_requested: bool,
}

static mut RESP_BUF: [u8; 2048] = [0; 2048];
static mut COMMANDS: [u8; 16384] = [0; 16384];

fn copy_str(dst: &mut [u8], s: &str) -> usize {
    let b = s.as_bytes();
    let n = b.len().min(dst.len());
    dst[..n].copy_from_slice(&b[..n]);
    n
}

fn s_of(buf: &[u8], len: usize) -> &str {
    core::str::from_utf8(&buf[..len]).unwrap_or("")
}

fn parse_u32(s: &str) -> u32 {
    s.trim().parse::<u32>().unwrap_or(0)
}

fn parse_hex(s: &str) -> u32 {
    u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).unwrap_or(0)
}

impl InstallerApp {
    const fn new() -> Self {
        Self {
            stage: Stage::Empty,
            path: [0; MAX_PATH],
            path_len: 0,
            pkg_id: [0; 40],
            pkg_id_len: 0,
            pkg_name: [0; 40],
            pkg_name_len: 0,
            pkg_ver: [0; 20],
            pkg_ver_len: 0,
            installed_ver: [0; 20],
            installed_ver_len: 0,
            arch: [0; 28],
            arch_len: 0,
            abi: [0; 16],
            abi_len: 0,
            publisher: [0; 40],
            publisher_len: 0,
            error_msg: [0; 64],
            error_len: 0,
            icon_id: ICON_GENERIC,
            verified: false,
            runnable: false,
            already_installed: false,
            system_conflict: false,
            requested_caps: 0,
            granted_caps: 0,
            denied_caps: 0,
            declared_perms: 0,
            unknown_perms: [0; 64],
            unknown_perms_len: 0,
            size_bytes: 0,
            close_requested: false,
        }
    }

    fn load_pending_intent(&mut self) {
        let mut buf = [0u8; MAX_PATH + 16];
        if let Some(p) = apps::take_intent(&mut buf) {
            self.path_len = copy_str(&mut self.path, p);
            self.inspect_current();
        }
    }

    fn inspect_current(&mut self) {
        if self.path_len == 0 {
            self.stage = Stage::Empty;
            return;
        }
        let mut path_copy = [0u8; MAX_PATH];
        let plen = self.path_len;
        path_copy[..plen].copy_from_slice(&self.path[..plen]);
        let path_str = core::str::from_utf8(&path_copy[..plen]).unwrap_or("");

        let resp = unsafe { &mut *core::ptr::addr_of_mut!(RESP_BUF) };
        let Ok(text) = pkg::inspect(path_str, resp) else {
            self.stage = Stage::Invalid;
            self.error_len = copy_str(&mut self.error_msg, "Paket okunamadi / Package unreadable");
            return;
        };

        let mut valid = false;
        self.icon_id = ICON_GENERIC;
        self.verified = false;
        self.runnable = false;
        self.already_installed = false;
        self.system_conflict = false;
        self.requested_caps = 0;
        self.granted_caps = 0;
        self.denied_caps = 0;
        self.declared_perms = 0;
        self.unknown_perms_len = 0;
        self.size_bytes = 0;
        self.error_len = 0;
        self.installed_ver_len = 0;

        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "valid" => valid = parse_u32(v) != 0,
                "error" => self.error_len = copy_str(&mut self.error_msg, v),
                "runnable" => self.runnable = parse_u32(v) != 0,
                "id" => self.pkg_id_len = copy_str(&mut self.pkg_id, v),
                "name" => self.pkg_name_len = copy_str(&mut self.pkg_name, v),
                "version" => self.pkg_ver_len = copy_str(&mut self.pkg_ver, v),
                "installed_version" => self.installed_ver_len = copy_str(&mut self.installed_ver, v),
                "icon_id" => {
                    let id = parse_u32(v);
                    self.icon_id = if id == 0 { ICON_GENERIC } else { id };
                }
                "arch" => self.arch_len = copy_str(&mut self.arch, v),
                "abi" => self.abi_len = copy_str(&mut self.abi, v),
                "verified" => self.verified = parse_u32(v) != 0,
                "publisher" => self.publisher_len = copy_str(&mut self.publisher, v),
                "requested_caps" => self.requested_caps = parse_hex(v),
                "granted_caps" => self.granted_caps = parse_hex(v),
                "denied_caps" => self.denied_caps = parse_hex(v),
                "declared_perms" => self.declared_perms = parse_hex(v),
                "unknown_perms" => self.unknown_perms_len = copy_str(&mut self.unknown_perms, v),
                "size_bytes" => self.size_bytes = parse_u32(v),
                "already_installed" => self.already_installed = parse_u32(v) != 0,
                "system_conflict" => self.system_conflict = parse_u32(v) != 0,
                _ => {}
            }
        }

        if !valid {
            self.stage = Stage::Invalid;
        } else if !self.runnable {
            self.stage = Stage::Invalid;
            self.error_len = copy_str(&mut self.error_msg, "Uyumsuz mimari/ABI / Incompatible arch or ABI");
        } else if self.system_conflict {
            self.stage = Stage::Invalid;
            self.error_len = copy_str(&mut self.error_msg, "Sistem uygulamasi degistirilemez");
        } else {
            self.stage = Stage::Review;
        }
    }

    fn do_install(&mut self) {
        let mut path_copy = [0u8; MAX_PATH];
        let plen = self.path_len;
        path_copy[..plen].copy_from_slice(&self.path[..plen]);
        let path_str = core::str::from_utf8(&path_copy[..plen]).unwrap_or("");
        let rc = pkg::install(path_str);
        if rc == 0 {
            self.stage = Stage::Installed;
        } else {
            self.stage = Stage::Failed;
            self.error_len = copy_str(&mut self.error_msg, "Kurulum cekirdek tarafindan reddedildi");
        }
    }

    fn draw_caps(&self, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, y0: i32, en: bool) {
        let mut y = y0;
        let items: [(u32, &str, &str); 7] = [
            (nc_caps::SETTINGS_READ, "Sistem ayarlarini okuma", "Read system settings"),
            (nc_caps::FS_USER_READ, "Kullanici dosyalarini okuma", "Read user storage"),
            (nc_caps::FS_USER_WRITE, "Kullanici dosyalarina yazma", "Write user storage"),
            (nc_caps::FS_USER_DELETE, "Kullanici dosyalarini silme", "Delete user files"),
            (nc_caps::NETWORK_CLIENT, "Ag baglantisi", "Network access"),
            (nc_caps::AUDIO_PLAY, "Ses calma", "Audio playback"),
            (nc_caps::NOTIFY_POST, "Bildirim gonderme", "Post notifications"),
        ];
        let mut shown = 0usize;
        for (bit, tr_s, en_s) in items {
            if self.granted_caps & bit != 0 && shown < 4 {
                gfx.dot(dp.x(34), y + dp.d(4), dp.d(3).max(2), GREEN, 1024);
                gfx.text(dp.x(44), y, dp.d(8), pal.ink, 1, if en { en_s } else { tr_s });
                y += dp.d(17);
                shown += 1;
            }
        }
        if shown == 0 {
            gfx.text(
                dp.x(30),
                y,
                dp.d(8),
                pal.muted,
                1,
                if en { "No special permissions required (sandboxed)" } else { "Ozel izin gerekmiyor (korumali alan)" },
            );
            y += dp.d(17);
        }
        if self.denied_caps != 0 {
            gfx.text(
                dp.x(30),
                y + dp.d(2),
                dp.d(7),
                AMBER,
                1,
                if en {
                    "Privileged system capabilities were stripped by policy"
                } else {
                    "Ayricalikli sistem yetkileri politika geregi kisitlandi"
                },
            );
        }
    }

    fn draw_runtime_perms(&self, gfx: &mut Gfx<'_>, dp: &Dp, pal: &Palette, y0: i32, en: bool) {
        let mut y = y0;
        let mut shown = 0usize;
        for info in &nc_abi::perm::PERM_REGISTRY {
            if (self.declared_perms & info.bit) != 0 && shown < 4 {
                gfx.dot(dp.x(34), y + dp.d(4), dp.d(3).max(2), BLUE, 1024);
                gfx.text(
                    dp.x(44),
                    y,
                    dp.d(7),
                    pal.ink,
                    1,
                    if en { info.title_en } else { info.title_tr },
                );
                y += dp.d(14);
                shown += 1;
            }
        }
        if shown == 0 {
            gfx.text(
                dp.x(30),
                y,
                dp.d(7),
                pal.muted,
                1,
                if en {
                    "No sensitive runtime permissions requested"
                } else {
                    "Hassas calisma zamani izni istenmiyor"
                },
            );
            y += dp.d(14);
        }
        if self.unknown_perms_len > 0 {
            let mut ul = Line::new();
            ul.text(if en { "Unsupported/denied: " } else { "Desteklenmeyen: " })
                .text(s_of(&self.unknown_perms, self.unknown_perms_len));
            gfx.text(dp.x(30), y + dp.d(2), dp.d(7), AMBER, 1, ul.as_str());
        }
    }

    fn draw(&self, ev: &EventBlock, gfx: &mut Gfx<'_>) {
        let dp = Dp::new(ev.area_w, ev.area_h);
        let en = ev.lang == 1;
        let dark = unpack_dark_theme(ev.flags);
        let pal = Palette::for_dark(dark);

        gfx.fill_rect(0, 0, ev.area_w, ev.area_h, pal.bg);

        // Baslik
        let title = if en { "Package Installer" } else { "Paket Kurucu" };
        gfx.text(dp.x(20), dp.d(18), dp.d(18), pal.ink, 3, title);

        match self.stage {
            Stage::Empty => {
                gfx.panel(dp.x(16), dp.d(64), dp.d(328), dp.d(150), dp.d(20), pal.card, 255, pal.divider, 255);
                gfx.icon(ICON_GENERIC, dp.x(152), dp.d(84), dp.d(56), dp.d(56));
                gfx.text_centered(
                    dp.x(180),
                    dp.d(154),
                    dp.d(10),
                    pal.ink,
                    2,
                    if en { "No package selected" } else { "Secili paket yok" },
                );
                gfx.text_centered(
                    dp.x(180),
                    dp.d(174),
                    dp.d(8),
                    pal.muted,
                    1,
                    if en { "Select an .ntzc file in Files to install" } else { "Kurmak icin Dosyalar'dan bir .ntzc secin" },
                );
                draw_pill_btn(
                    gfx,
                    dp.x(110),
                    dp.d(232),
                    dp.d(140),
                    dp.d(38),
                    ev.accent,
                    WHITE,
                    dp.d(9),
                    if en { "Close" } else { "Kapat" },
                );
            }
            Stage::Invalid | Stage::Failed => {
                gfx.panel(dp.x(16), dp.d(64), dp.d(328), dp.d(164), dp.d(20), pal.card, 255, CORAL, 160);
                gfx.icon(ICON_GENERIC, dp.x(152), dp.d(82), dp.d(52), dp.d(52));
                gfx.text_centered(
                    dp.x(180),
                    dp.d(148),
                    dp.d(11),
                    CORAL,
                    2,
                    if self.stage == Stage::Invalid {
                        if en { "Invalid Package" } else { "Gecersiz Paket" }
                    } else {
                        if en { "Installation Failed" } else { "Kurulum Basarisiz" }
                    },
                );
                gfx.text_centered(
                    dp.x(180),
                    dp.d(172),
                    dp.d(8),
                    pal.muted,
                    1,
                    s_of(&self.error_msg, self.error_len),
                );
                draw_pill_btn(
                    gfx,
                    dp.x(110),
                    dp.d(244),
                    dp.d(140),
                    dp.d(38),
                    pal.card,
                    pal.ink,
                    dp.d(9),
                    if en { "Close" } else { "Kapat" },
                );
            }
            Stage::Installed => {
                gfx.panel(dp.x(16), dp.d(64), dp.d(328), dp.d(156), dp.d(20), pal.card, 255, GREEN, 160);
                gfx.icon(self.icon_id, dp.x(152), dp.d(82), dp.d(56), dp.d(56));
                gfx.text_centered(
                    dp.x(180),
                    dp.d(152),
                    dp.d(12),
                    GREEN,
                    2,
                    if en { "Application Installed" } else { "Uygulama Kuruldu" },
                );
                gfx.text_centered(
                    dp.x(180),
                    dp.d(174),
                    dp.d(9),
                    pal.ink,
                    2,
                    s_of(&self.pkg_name, self.pkg_name_len),
                );
                gfx.text_centered(
                    dp.x(180),
                    dp.d(194),
                    dp.d(7),
                    pal.muted,
                    1,
                    if en { "Added to Home app drawer" } else { "Ana ekran cekmecesine eklendi" },
                );
                draw_pill_btn(
                    gfx,
                    dp.x(24),
                    dp.d(238),
                    dp.d(148),
                    dp.d(40),
                    pal.card,
                    pal.ink,
                    dp.d(9),
                    if en { "Done" } else { "Bitti" },
                );
                draw_pill_btn(
                    gfx,
                    dp.x(188),
                    dp.d(238),
                    dp.d(148),
                    dp.d(40),
                    ev.accent,
                    WHITE,
                    dp.d(9),
                    if en { "Open" } else { "Ac" },
                );
            }
            Stage::Review => {
                // Paket ust karti
                gfx.panel(dp.x(16), dp.d(56), dp.d(328), dp.d(118), dp.d(20), pal.card, 255, pal.divider, 255);
                gfx.icon(self.icon_id, dp.x(30), dp.d(72), dp.d(52), dp.d(52));
                gfx.text(dp.x(94), dp.d(76), dp.d(12), pal.ink, 2, s_of(&self.pkg_name, self.pkg_name_len));
                gfx.text(dp.x(94), dp.d(96), dp.d(8), pal.muted, 1, s_of(&self.pkg_id, self.pkg_id_len));

                let mut ver_line = Line::new();
                ver_line.text("v").text(s_of(&self.pkg_ver, self.pkg_ver_len));
                ver_line.text("  |  ").number((self.size_bytes / 1024).max(1), 1).text(" KB");
                if self.already_installed {
                    ver_line.text(if en { "  (Update)" } else { "  (Guncelleme)" });
                }
                gfx.text(dp.x(94), dp.d(114), dp.d(7), pal.subtle, 1, ver_line.as_str());

                // Imza durumu rozeti
                let (sig_col, sig_txt) = if self.verified {
                    (GREEN, if en { "Verified Application" } else { "Doğrulanmış Uygulama" })
                } else {
                    (AMBER, if en { "Unverified App (Limited Permissions)" } else { "Doğrulanmamış Uygulama (Sınırlı İzinler)" })
                };
                gfx.panel(dp.x(28), dp.d(138), dp.d(304), dp.d(24), dp.d(12), sig_col, 32, sig_col, 140);
                gfx.text_centered(dp.x(180), dp.d(145), dp.d(7), sig_col, 2, sig_txt);

                // Statik yetkiler karti
                gfx.panel(dp.x(16), dp.d(180), dp.d(328), dp.d(92), dp.d(16), pal.card, 255, pal.divider, 255);
                gfx.text(
                    dp.x(30),
                    dp.d(190),
                    dp.d(8),
                    pal.ink,
                    2,
                    if en { "Static Capabilities (Clamped by Policy)" } else { "Statik Yetkiler (Politika Sinirli)" },
                );
                self.draw_caps(gfx, &dp, &pal, dp.d(208), en);

                // Calisma zamani izinleri karti (kurulumda verilmez; ilk kullanimda sorulur)
                gfx.panel(dp.x(16), dp.d(280), dp.d(328), dp.d(96), dp.d(16), pal.card, 255, pal.divider, 255);
                gfx.text(
                    dp.x(30),
                    dp.d(290),
                    dp.d(8),
                    pal.ink,
                    2,
                    if en {
                        "Requested Permissions (Asked on First Use)"
                    } else {
                        "Istenen Izinler (Ilk Kullanimda Sorulur)"
                    },
                );
                gfx.text(
                    dp.x(30),
                    dp.d(304),
                    dp.d(7),
                    pal.muted,
                    1,
                    if en {
                        "Not granted on install — requires your approval"
                    } else {
                        "Kurulumda verilmez — acik onayiniz gerekir"
                    },
                );
                self.draw_runtime_perms(gfx, &dp, &pal, dp.d(320), en);

                // Dugmeler
                let btn_y = (ev.area_h - dp.d(56)).max(dp.d(386));
                draw_pill_btn(
                    gfx,
                    dp.x(20),
                    btn_y,
                    dp.d(152),
                    dp.d(42),
                    pal.card,
                    pal.ink,
                    dp.d(10),
                    if en { "Cancel" } else { "Iptal" },
                );
                draw_pill_btn(
                    gfx,
                    dp.x(188),
                    btn_y,
                    dp.d(152),
                    dp.d(42),
                    if self.already_installed { BLUE } else { ev.accent },
                    WHITE,
                    dp.d(10),
                    if self.already_installed {
                        if en { "Update" } else { "Guncelle" }
                    } else {
                        if en { "Install" } else { "Yukle" }
                    },
                );
            }
        }
    }

    fn on_click(&mut self, ev: &EventBlock, x: i32, y: i32) -> bool {
        let dp = Dp::new(ev.area_w, ev.area_h);
        match self.stage {
            Stage::Empty | Stage::Invalid | Stage::Failed => {
                if y >= dp.d(224) && y <= dp.d(292) && x >= dp.x(90) && x <= dp.x(270) {
                    self.close_requested = true;
                    return true;
                }
            }
            Stage::Installed => {
                if y >= dp.d(230) && y <= dp.d(286) {
                    if x < dp.x(180) {
                        self.close_requested = true;
                        return true;
                    } else {
                        let mut id_copy = [0u8; 40];
                        let len = self.pkg_id_len;
                        id_copy[..len].copy_from_slice(&self.pkg_id[..len]);
                        let id_str = core::str::from_utf8(&id_copy[..len]).unwrap_or("");
                        if !id_str.is_empty() {
                            let _ = apps::launch(id_str);
                        }
                        return true;
                    }
                }
            }
            Stage::Review => {
                let btn_y = (ev.area_h - dp.d(56)).max(dp.d(386));
                if y >= btn_y - dp.d(4) && y <= btn_y + dp.d(46) {
                    if x < dp.x(180) {
                        self.close_requested = true;
                        return true;
                    } else {
                        self.do_install();
                        return true;
                    }
                }
            }
        }
        false
    }
}

impl RingApp for InstallerApp {
    fn handle(&mut self, ev: &EventBlock, gfx: &mut Gfx<'_>) -> u32 {
        match ev.op {
            OP_INIT | OP_RESUME => {
                self.close_requested = false;
                self.load_pending_intent();
                RET_REDRAW
            }
            OP_PAUSE => RET_NONE,
            OP_UPDATE => {
                if ev.key == key::ESCAPE {
                    return RET_CLOSE;
                }
                if let Some(p) = pointer(ev) {
                    if p.clicked {
                        let dirty = self.on_click(ev, p.x, p.y);
                        if self.close_requested {
                            return RET_CLOSE;
                        }
                        if dirty {
                            return RET_REDRAW;
                        }
                    }
                }
                RET_NONE
            }
            OP_DRAW => {
                self.draw(ev, gfx);
                RET_NONE
            }
            _ => RET_NONE,
        }
    }
}

static mut APP: InstallerApp = InstallerApp::new();

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
