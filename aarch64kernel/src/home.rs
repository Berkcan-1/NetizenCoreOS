use nc_arch::time::Clock;
use nc_device::DeviceProfile;
use nc_gfx::{present, present_region, rgb, Canvas, Rect};
use nc_hal::FramebufferInfo;
use nc_mem::Arena;
use nc_splash::{Phase, Splash, SplashOptions};
use nc_ui::{draw_text_color, text_style};

const SPLASH_HOLD_MS: u32 = 1800;
const SPLASH_MAX_MS: u32 = 6000;
const HEARTBEAT_MS: u32 = 1000;
const TOP: u32 = rgb(15, 23, 42);
const BOTTOM: u32 = rgb(30, 64, 175);

pub fn pause() {
    if nc_arch::gic::ready() {
        nc_arch::irq_enable();
        nc_arch::wait_for_interrupt();
        nc_arch::irq_disable();
    } else {
        core::hint::spin_loop();
    }
}

fn reached(now: u32, deadline: u32) -> bool {
    now.wrapping_sub(deadline) < 0x8000_0000
}

fn draw_static(canvas: &mut Canvas<'_>, profile: &DeviceProfile) {
    let width = canvas.width() as i32;
    let height = canvas.height() as i32;
    canvas.vertical_gradient(TOP, BOTTOM);

    let title = text_style((width / 14).max(18), 4);
    draw_text_color(canvas, "NetizenCore", width / 10, height / 4, &title, rgb(255, 255, 255));

    let sub = text_style((width / 30).max(10), 2);
    draw_text_color(canvas, profile.display_name, width / 10, height / 4 + width / 6, &sub, rgb(191, 219, 254));
    draw_text_color(canvas, profile.soc, width / 10, height / 4 + width / 6 + width / 14, &sub, rgb(147, 197, 253));
}

pub fn splash(
    profile: &DeviceProfile,
    canvas: &mut Canvas<'_>,
    arena: &mut Arena,
    framebuffer: &FramebufferInfo,
    clock: &mut Clock,
) {
    canvas.vertical_gradient(TOP, BOTTOM);
    unsafe {
        present(canvas, framebuffer);
    }
    let mark = arena.mark();
    if let Some(mut splash) = Splash::new(canvas, arena, SplashOptions { subtitle: Some(profile.display_name) }) {
        let mut requested = false;
        let start = clock.now_ms();
        loop {
            let now = clock.now_ms();
            let elapsed = now.wrapping_sub(start);
            if !requested && elapsed >= SPLASH_HOLD_MS {
                splash.request_finish();
                requested = true;
            }
            let update = splash.frame(canvas, now);
            for index in 0..update.count.min(update.rects.len()) {
                unsafe {
                    present_region(canvas, framebuffer, update.rects[index]);
                }
            }
            if splash.phase() == Phase::Done || elapsed >= SPLASH_MAX_MS {
                break;
            }
            let deadline = now.wrapping_add(16);
            while !reached(clock.now_ms(), deadline) {
                pause();
            }
        }
    }
    unsafe {
        arena.release_to(mark);
    }
}

pub fn placeholder(
    profile: &DeviceProfile,
    canvas: &mut Canvas<'_>,
    framebuffer: &FramebufferInfo,
    clock: &mut Clock,
    health: fn() -> bool,
) -> ! {
    draw_static(canvas, profile);
    unsafe {
        present(canvas, framebuffer);
    }
    let dot = ((canvas.width() as i32) / 40).max(8);
    let area = Rect::new(canvas.width() as i32 - dot * 2, dot, dot, dot);
    let mut beat = false;
    let mut next = clock.now_ms().wrapping_add(HEARTBEAT_MS);
    loop {
        let now = clock.now_ms();
        if reached(now, next) {
            next = now.wrapping_add(HEARTBEAT_MS);
            if !health() {
                nc_arch::serial::write_str("*** saglik denetimi basarisiz (yigin tasmasi?) ***\n");
                nc_arch::halt_forever();
            }
            beat = !beat;
            let color = if beat { rgb(34, 197, 94) } else { TOP };
            canvas.fill_rect(area.x, area.y, area.width, area.height, color);
            unsafe {
                present_region(canvas, framebuffer, area);
            }
        }
        pause();
    }
}
