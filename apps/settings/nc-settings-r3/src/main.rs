#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use nc_app::{App, AppInfo, Context, Input, Update};
use nc_gfx::{Canvas, Rect};
use nc_app_host::heap::GlobalHeap;

#[global_allocator]
static HEAP: GlobalHeap = GlobalHeap::empty();

struct SettingsR3 {
    inner: nc_app_settings::SettingsApp,
}

impl App for SettingsR3 {
    fn info(&self) -> &AppInfo {
        self.inner.info()
    }

    fn opened(&mut self, ctx: &mut Context) {
        let before = nc_app_host::block_from_context(ctx).encode();
        self.inner.opened(ctx);
        push_if_changed(ctx, &before);
    }

    fn closed(&mut self, ctx: &mut Context) {
        self.inner.closed(ctx);
    }

    fn update(&mut self, ctx: &mut Context, input: &Input) -> Update {
        let before = nc_app_host::block_from_context(ctx).encode();
        let update = self.inner.update(ctx, input);
        push_if_changed(ctx, &before);
        update
    }

    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        self.inner.draw(canvas, area, ctx);
    }
}

fn push_if_changed(ctx: &Context, before: &[u8]) {
    let after = nc_app_host::block_from_context(ctx).encode();
    if after[..] != before[..] {
        let _ = nc_sys_facade::push_settings(&after);
    }
}

fn factory(_ctx: &mut Context) -> Box<dyn App> {
    Box::new(SettingsR3 { inner: nc_app_settings::SettingsApp::new() })
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    nc_app_host::run(&HEAP, factory)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    nc_app_sdk::sys::panic_exit(info)
}
