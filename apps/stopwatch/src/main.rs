//! Kronometre: Ring 3 ELF girişi. Mantık: crates/nc-r3-stopwatch; çalışma döngüsü: crates/nc-app-sdk.
#![no_std]
#![no_main]

use nc_app_sdk::{dispatch, sys};
use nc_r3_stopwatch::StopwatchApp;

// Durum çağrılar arasında bu belleğte kalır (çekirdek her olayda `_start`'tan, boş yığınla girer).
static mut APP: StopwatchApp = StopwatchApp::new();
static mut COMMANDS: [u8; 16384] = [0; 16384];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe { dispatch(&mut *core::ptr::addr_of_mut!(APP), &mut *core::ptr::addr_of_mut!(COMMANDS)) }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sys::panic_exit(info)
}
