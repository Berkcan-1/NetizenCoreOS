// kernel/src/ring3_app.rs
//
// `RemoteApp`: Ring 3'te çalışan bir olay-tabanlı ELF uygulamasını masaüstüne `App` olarak bağlar.
// Masaüstü onu diğer uygulamalardan ayırt etmez; her `update/draw` bir Ring 3 olayına dönüşür.
// Uygulama çökerse / watchdog tetiklenirse pencere açık kalır ve hata gösterilir; çekirdek etkilenmez.
#![allow(dead_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;

use nc_abi::event::{
    EventBlock, OP_CLOSE, OP_DRAW, OP_INIT, OP_UPDATE, PTR_CLICKED, PTR_DOWN, PTR_HAS,
    PTR_RIGHT_CLICKED, RET_CLOSE, RET_INSTALL, RET_REDRAW,
};
use nc_abi::key;
use nc_app_host::{block_from_context, encode_key};
use nc_app::text::centered;
use nc_app::{App, AppInfo, Context, Input, Update};
use nc_gfx::{Canvas, Rect};
use nc_ui::{text_style, TEXT_DIM, TEXT_PRIMARY};

use crate::process::{ProcessManager, SecurityIdentity, TrustLevel};

static STOPWATCH_ELF: &[u8] = include_bytes!("../ring3_apps/stopwatch.elf");
static SYSTEM_ELF: &[u8] = include_bytes!("../ring3_apps/system.elf");
static FILES_ELF: &[u8] = include_bytes!("../ring3_apps/files.elf");

/// ELF imajının sahibi: gömülü (statik) ya da paketten gelen (sahipli).
pub enum Image {
    Static(&'static [u8]),
    Owned(Vec<u8>),
}

impl Image {
    fn bytes(&self) -> &[u8] {
        match self {
            Image::Static(bytes) => bytes,
            Image::Owned(bytes) => bytes.as_slice(),
        }
    }
}

pub struct RemoteApp {
    info: AppInfo,
    pm: *mut ProcessManager,
    image: Image,
    identity: SecurityIdentity,
    pid: Cell<Option<usize>>,
    seq: Cell<u64>,
    failure: Cell<Option<&'static str>>,
}

impl RemoteApp {
    /// `pm`, çekirdek ana işlevinin ömrü boyunca yaşayan `ProcessManager`'ı göstermelidir.
    pub fn new(info: AppInfo, pm: *mut ProcessManager, image: Image, identity: SecurityIdentity) -> Self {
        Self { info, pm, image, identity, pid: Cell::new(None), seq: Cell::new(0), failure: Cell::new(None) }
    }

    fn fail(&self, reason: &'static str) {
        if self.failure.get().is_none() {
            nc_arch::serial::write_str("[Ring 3] ");
            nc_arch::serial::write_str(self.info.id.as_str());
            nc_arch::serial::write_str(" durduruldu: ");
            nc_arch::serial::write_str(reason);
            nc_arch::serial::write_str("\n");
        }
        self.failure.set(Some(reason));
    }

    fn event(&self, op: u32, ctx: &Context, area: Rect, input: Option<&Input>) -> EventBlock {
        let seq = self.seq.get().wrapping_add(1);
        self.seq.set(seq);
        let mut flags = 0u32;
        let (mut px, mut py) = (0, 0);
        let mut key_code = key::NONE;
        if let Some(input) = input {
            key_code = encode_key(input.key);
            if let Some(pointer) = input.pointer {
                flags |= PTR_HAS;
                px = pointer.x;
                py = pointer.y;
                if pointer.down {
                    flags |= PTR_DOWN;
                }
                if pointer.clicked {
                    flags |= PTR_CLICKED;
                }
                if pointer.right_clicked {
                    flags |= PTR_RIGHT_CLICKED;
                }
            }
        }
        EventBlock {
            op,
            flags: 0,
            surface: 3,
            area_x: area.x,
            area_y: area.y,
            area_w: area.width,
            area_h: area.height,
            unit: ctx.unit,
            uptime_ms: ctx.uptime_ms,
            accent: ctx.settings.accent_color(),
            pointer_flags: flags,
            pointer_x: px,
            pointer_y: py,
            key: key_code,
            lang: ctx.settings.language as u32,
            heap_base: nc_abi::USER_HEAP_BASE,
            heap_size: nc_abi::APP_HEAP_BYTES,
            seq,
        }
    }

    fn manager(&self) -> &mut ProcessManager {
        unsafe { &mut *self.pm }
    }

    fn ensure_started(&self, ctx: &Context) -> bool {
        if self.failure.get().is_some() {
            return false;
        }
        if self.pid.get().is_some() {
            return true;
        }
        let pid = match self.manager().spawn_user_elf(self.info.id.as_str(), self.identity.clone(), self.image.bytes()) {
            Ok(pid) => pid,
            Err(reason) => {
                self.fail(reason);
                return false;
            }
        };
        self.pid.set(Some(pid));
        self.push_context(pid, ctx);
        let init = self.event(OP_INIT, ctx, Rect::new(0, 0, 0, 0), None);
        if let Err(reason) = self.manager().run_app_event(pid, &init, None) {
            self.fail(reason);
            return false;
        }
        true
    }

    fn push_context(&self, pid: usize, ctx: &Context) {
        crate::ring3::set_context(pid, block_from_context(ctx).encode());
    }

    fn draw_failure(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        let Some(reason) = self.failure.get() else {
            return;
        };
        let unit = ctx.unit;
        let title = text_style(unit * 6 / 10 + 8, 20);
        let body = text_style(unit * 45 / 100 + 6, 30);
        let cx = area.x + area.width / 2;
        let cy = area.y + area.height / 2;
        centered(canvas, "Uygulama durduruldu", cx, cy - unit * 2, &title, TEXT_PRIMARY);
        centered(canvas, reason, cx, cy, &body, TEXT_DIM);
    }
}

impl App for RemoteApp {
    fn info(&self) -> &AppInfo {
        &self.info
    }

    fn opened(&mut self, ctx: &mut Context) {
        self.failure.set(None);
        self.ensure_started(ctx);
    }

    fn closed(&mut self, ctx: &mut Context) {
        if let Some(pid) = self.pid.get() {
            self.push_context(pid, ctx);
            let close = self.event(OP_CLOSE, ctx, Rect::new(0, 0, 0, 0), None);
            let _ = self.manager().run_app_event(pid, &close, None);
            self.manager().kill_app(pid);
        }
        self.pid.set(None);
        self.failure.set(None);
    }

    fn update(&mut self, ctx: &mut Context, input: &Input) -> Update {
        if !self.ensure_started(ctx) {
            return Update::NONE;
        }
        let Some(pid) = self.pid.get() else {
            return Update::NONE;
        };
        self.push_context(pid, ctx);
        let event = self.event(OP_UPDATE, ctx, input.area, Some(input));
        match self.manager().run_app_event(pid, &event, None) {
            Ok(ret) => {
                let mut update = Update {
                    redraw: ret & RET_REDRAW != 0 || ret & RET_CLOSE != 0,
                    close: ret & RET_CLOSE != 0,
                    install_app: None,
                };
                if ret & RET_INSTALL != 0 {
                    // Çekirdek paketi SYS_PKG_INSTALL sırasında doğrulamıştı; burada yeniden ayrıştırıp bağlar.
                    if let Some(bytes) = crate::ring3::take_pending_package(pid) {
                        if let Some(pkg) = nc_pkg::NtzcPackage::parse(&bytes) {
                            if let Some(app) = nc_pkg::load_packaged_app(&pkg) {
                                update = Update::install(app);
                            }
                        }
                    }
                }
                update
            }
            Err(reason) => {
                self.fail(reason);
                Update::REDRAW
            }
        }
    }

    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        let ready = self.failure.get().is_none() && self.pid.get().is_some();
        if ready {
            let pid = self.pid.get().unwrap_or(0);
            self.push_context(pid, ctx);
            let event = self.event(OP_DRAW, ctx, area, None);
            if let Err(reason) = self.manager().run_app_event(pid, &event, Some((&mut *canvas, area))) {
                self.fail(reason);
            }
        }
        if self.failure.get().is_some() {
            self.draw_failure(canvas, area, ctx);
        }
    }
}

static mut PM_PTR: *mut ProcessManager = core::ptr::null_mut();

fn package_loader(pkg: &nc_pkg::NtzcPackage) -> Option<Box<dyn App>> {
    let pm = unsafe { *core::ptr::addr_of!(PM_PTR) };
    if pm.is_null() || !pkg.runnable() {
        return None;
    }
    let manifest = &pkg.manifest;
    let info = AppInfo::new(
        &manifest.id,
        &manifest.name,
        &manifest.version,
        manifest.icon,
        manifest.width_units.clamp(12, 60),
        manifest.height_units.clamp(8, 40),
    );
    // Çekirdeğin mevcut politikasıyla AYNI: doğrulanmış paketler yalnızca bu dört yetkiyi alabilir.
    const PACKAGE_GRANTABLE: u32 = nc_caps::SETTINGS_READ
        | nc_caps::FS_USER_READ
        | nc_caps::FS_USER_WRITE
        | nc_caps::NETWORK_CLIENT;
    let identity = SecurityIdentity {
        app_id: manifest.id.clone(),
        uid: pkg.uid,
        trust: TrustLevel::VerifiedApp,
        capabilities: pkg.capability_bits() & PACKAGE_GRANTABLE,
    };
    nc_arch::serial::write_str("[Ring 3] paket uygulamasi yuklendi: ");
    nc_arch::serial::write_str(&manifest.id);
    nc_arch::serial::write_str("\n");
    Some(Box::new(RemoteApp::new(info, pm, Image::Owned(pkg.payload.clone()), identity)))
}

/// Faz 6 geçtiyse, paketlerden gelen uygulamalar için Ring 3 yükleyicisini kaydeder. `nc_apps::builtin()`
/// ÇAĞRILMADAN ÖNCE çağrılmalı (paket taraması yükleyiciyi kullanır). Geçmediyse hiçbir paket çalışmaz.
pub fn install_package_loader(ready: bool, pm: *mut ProcessManager) {
    unsafe {
        *core::ptr::addr_of_mut!(PM_PTR) = pm;
    }
    if ready {
        nc_pkg::set_package_loader(package_loader);
        nc_arch::serial::write_str("[Ring 3] paket yukleyicisi aktif: yuklenen uygulamalar YALNIZCA Ring 3'te calisir.\n");
    } else {
        nc_arch::serial::write_str("[Ring 3] Faz 6 gecmedi: paket uygulamalari CALISTIRILMAYACAK (yalniz bilgi karti).\n");
    }
}

/// Gömülü bir uygulamayı Ring 3 ELF'iyle değiştirir. ELF boşsa/bozuksa Ring 0 sürümü kalır.
fn replace_builtin(apps: &mut Vec<Box<dyn App>>, id: &str, elf: &'static [u8], pm: *mut ProcessManager) {
    use nc_arch::serial::write_str;
    if elf.is_empty() {
        write_str("[Ring 3] ");
        write_str(id);
        write_str(": ELF yok (build.sh derlemedi) -> Ring 0 surumuyle kaliyor.\n");
        return;
    }
    let policy = nc_elf::Policy {
        min_vaddr: nc_abi::USER_IMAGE_MIN,
        max_vaddr: nc_abi::USER_IMAGE_MAX,
        max_pages: nc_abi::ELF_MAX_PAGES,
    };
    if let Err(error) = nc_elf::parse(elf, &policy) {
        write_str("[Ring 3] ");
        write_str(id);
        write_str(": ELF reddedildi: ");
        write_str(error.as_str());
        write_str(" -> Ring 0 surumuyle kaliyor.\n");
        return;
    }
    let Some(position) = apps.iter().position(|app| app.info().id == id) else {
        return;
    };
    let old = apps[position].info();
    let info = AppInfo {
        id: old.id.clone(),
        name: old.name.clone(),
        version: old.version.clone(),
        icon: old.icon,
        width_units: old.width_units,
        height_units: old.height_units,
        title: old.title,
    };
    let identity = SecurityIdentity::from_builtin_app_id(id);
    apps[position] = Box::new(RemoteApp::new(info, pm, Image::Static(elf), identity));
    write_str("[Ring 3] ");
    write_str(id);
    write_str(" artik Ring 3'te calisiyor (ELF, olay-tabanli, watchdog'lu).\n");
}

/// Gömülü uygulamaları Ring 3 sürümleriyle değiştirir (Faz 6 geçtiyse).
pub fn with_ring3_apps(mut apps: Vec<Box<dyn App>>, ready: bool, pm: *mut ProcessManager) -> Vec<Box<dyn App>> {
    if !ready {
        nc_arch::serial::write_str("[Ring 3] Faz 6 gecmedi: yerlesik uygulamalar Ring 0 surumleriyle kaliyor.\n");
        return apps;
    }
    replace_builtin(&mut apps, "nc.stopwatch", STOPWATCH_ELF, pm);
    replace_builtin(&mut apps, "nc.system", SYSTEM_ELF, pm);
    replace_builtin(&mut apps, "nc.files", FILES_ELF, pm);
    apps
}
