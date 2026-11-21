#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use nc_app::{App, Context};
use nc_app_host::heap::GlobalHeap;

#[global_allocator]
static HEAP: GlobalHeap = GlobalHeap::empty();

fn factory(_ctx: &mut Context) -> Box<dyn App> {
    Box::new(nc_app_browser::BrowserApp::new())
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    nc_app_host::run(&HEAP, factory)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    nc_app_sdk::sys::panic_exit(info)
}
