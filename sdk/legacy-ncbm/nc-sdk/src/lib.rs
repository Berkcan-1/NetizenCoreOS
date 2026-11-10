#![no_std]

use core::panic::PanicInfo;

pub const NTZC_ABI_MAGIC: u32 = 0x4D42_434E; // "NCBM"

pub const OP_INIT: u32 = 0;
pub const OP_UPDATE: u32 = 1;
pub const OP_DRAW: u32 = 2;
pub const OP_CLOSE: u32 = 3;

pub const RET_NONE: u32 = 0;
pub const RET_REDRAW: u32 = 1;
pub const RET_CLOSE: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct NtzcInput {
    pub area_x: i32,
    pub area_y: i32,
    pub area_w: i32,
    pub area_h: i32,
    pub unit: i32,
    pub uptime_ms: u32,
    pub accent_color: u32,
    pub has_pointer: u8,
    pub pointer_down: u8,
    pub pointer_clicked: u8,
    pub pointer_right_clicked: u8,
    pub pointer_x: i32,
    pub pointer_y: i32,
    pub key_code: u32,
}

pub type c_void = core::ffi::c_void;

#[repr(C)]
pub struct NtzcHostApi {
    pub canvas_ptr: *mut c_void,
    pub fill_rect: extern "sysv64" fn(*mut c_void, i32, i32, i32, i32, u32),
    pub draw_panel: extern "sysv64" fn(*mut c_void, i32, i32, i32, i32, i32, u32, u32, u32, u32),
    pub draw_dot: extern "sysv64" fn(*mut c_void, i32, i32, i32, u32, u32),
    pub draw_text: extern "sysv64" fn(*mut c_void, *const u8, usize, i32, i32, i32, u32),
    pub text_width: extern "sysv64" fn(*const u8, usize, i32) -> i32,
}

pub struct Gui<'a> {
    api: &'a NtzcHostApi,
    pub input: &'a NtzcInput,
}

impl<'a> Gui<'a> {
    pub fn new(api: &'a NtzcHostApi, input: &'a NtzcInput) -> Self {
        Self { api, input }
    }

    pub fn fill_rect(&self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        (self.api.fill_rect)(self.api.canvas_ptr, x, y, w, h, color);
    }

    pub fn draw_panel(
        &self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        radius: i32,
        fill: u32,
        fill_alpha: u32,
        border: u32,
        border_alpha: u32,
    ) {
        (self.api.draw_panel)(
            self.api.canvas_ptr,
            x,
            y,
            w,
            h,
            radius,
            fill,
            fill_alpha,
            border,
            border_alpha,
        );
    }

    pub fn draw_dot(&self, cx: i32, cy: i32, radius: i32, color: u32, strength: u32) {
        (self.api.draw_dot)(self.api.canvas_ptr, cx, cy, radius, color, strength);
    }

    pub fn draw_text(&self, text: &str, x: i32, y: i32, cap_height: i32, color: u32) {
        (self.api.draw_text)(
            self.api.canvas_ptr,
            text.as_ptr(),
            text.len(),
            x,
            y,
            cap_height,
            color,
        );
    }

    pub fn text_width(&self, text: &str, cap_height: i32) -> i32 {
        (self.api.text_width)(text.as_ptr(), text.len(), cap_height)
    }
}

pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

#[macro_export]
macro_rules! declare_ntzc_app {
    ($state_ty:ty, $init_fn:ident, $update_fn:ident, $draw_fn:ident) => {
        core::arch::global_asm!(
            ".pushsection .text.entry,\"ax\",@progbits",
            ".global _ntzc_entry",
            "_ntzc_entry:",
            ".long 0x4D42434E",       // Offset 0: Magic "NCBM"
            ".long 1",                // Offset 4: ABI Version 1
            ".long {state_size}",     // Offset 8: State struct boyutu
            ".long 16",               // Offset 12: Giris kodu ofseti (her zaman 16. bayt!)
            "jmp __ntzc_dispatch",    // Offset 16: Dogrudan PIC goreli sicrama
            ".popsection",
            state_size = const core::mem::size_of::<$state_ty>(),
        );

        #[no_mangle]
        pub extern "sysv64" fn __ntzc_dispatch(
            op: u32,
            state_ptr: *mut u8,
            input_ptr: *const $crate::NtzcInput,
            api_ptr: *const $crate::NtzcHostApi,
        ) -> u32 {
            let state = unsafe { &mut *(state_ptr as *mut $state_ty) };
            let input = unsafe { &*input_ptr };
            match op {
                $crate::OP_INIT => {
                    $init_fn(state, input);
                    $crate::RET_REDRAW
                }
                $crate::OP_UPDATE => $update_fn(state, input),
                $crate::OP_DRAW => {
                    let api = unsafe { &*api_ptr };
                    let gui = $crate::Gui::new(api, input);
                    $draw_fn(state, &gui);
                    $crate::RET_NONE
                }
                _ => $crate::RET_NONE,
            }
        }
    };
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
