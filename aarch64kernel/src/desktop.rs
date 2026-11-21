//! ARM64 bilgisayarlar (`ui-desktop`) için standart `nc-desktop` masaüstü yürütücüsü.
//! ARMv8-A telefon (`ui-mobile`) derlemelerine hiç dahil edilmez.

use alloc::vec::Vec;
use nc_abi::event::{PTR_CLICKED, PTR_DOWN, PTR_HAS, PTR_RIGHT_CLICKED};
use nc_abi::key as abi_key;
use nc_arch::time::Clock;
use nc_desktop::{DesktopAction, DesktopState};
use nc_device::DeviceProfile;
use nc_gfx::{present, present_region, Canvas, Rect};
use nc_hal::{FramebufferInfo, KeyCode, MouseState};
use nc_mem::Arena;

use crate::{boot, home, input};

const FRAME_MS: u32 = 16;

fn decode_key(code: u32) -> Option<KeyCode> {
    match code {
        abi_key::NONE => None,
        abi_key::BACKSPACE => Some(KeyCode::Backspace),
        abi_key::ENTER => Some(KeyCode::Enter),
        abi_key::ESCAPE => Some(KeyCode::Escape),
        abi_key::TAB => Some(KeyCode::Tab),
        abi_key::UP => Some(KeyCode::Up),
        abi_key::DOWN => Some(KeyCode::Down),
        abi_key::LEFT => Some(KeyCode::Left),
        abi_key::RIGHT => Some(KeyCode::Right),
        c if (0x20..=0x7E).contains(&c) => Some(KeyCode::Char(c as u8 as char)),
        _ => None,
    }
}

fn reached(now: u32, deadline: u32) -> bool {
    now.wrapping_sub(deadline) < 0x8000_0000
}

pub fn run(
    profile: &DeviceProfile,
    screen: &mut Canvas<'_>,
    arena: &mut Arena,
    framebuffer: &FramebufferInfo,
    clock: &mut Clock,
) -> ! {
    let width = screen.width();
    let height = screen.height();

    let Some(wp_pixels) = arena.alloc_pixels(width * height) else {
        nc_arch::serial::write_str("[desktop] duvar kagidi bellegi ayrilamadi\n");
        nc_arch::halt_forever();
    };
    let Some(work_pixels) = arena.alloc_pixels(width * height) else {
        nc_arch::serial::write_str("[desktop] calisma ekrani bellegi ayrilamadi\n");
        nc_arch::halt_forever();
    };
    let Some(mut wallpaper) = Canvas::new(wp_pixels, width, height) else {
        nc_arch::halt_forever();
    };
    let Some(mut work_screen) = Canvas::new(work_pixels, width, height) else {
        nc_arch::halt_forever();
    };

    input::set_screen(width as i32, height as i32);

    let mut desktop_state = DesktopState::new(profile.display_name, boot::total_ram_mib(), Vec::new());
    nc_desktop::draw_wallpaper(&mut wallpaper, desktop_state.wallpaper_index());
    desktop_state.take_wallpaper_dirty();
    nc_desktop::draw(&mut work_screen, &wallpaper, &desktop_state);
    desktop_state.take_dirty();

    let cursor_w = nc_ui::CURSOR_WIDTH as i32;
    let cursor_h = nc_ui::CURSOR_HEIGHT as i32;
    let mut mouse = MouseState {
        x: (width as i32) / 2,
        y: (height as i32) / 2,
        left_down: false,
        left_clicked: false,
        right_down: false,
        right_clicked: false,
    };

    screen.copy_rect_from(&work_screen, 0, 0, 0, 0, width as i32, height as i32);
    nc_ui::draw_cursor(screen, mouse.x, mouse.y);
    unsafe {
        present(screen, framebuffer);
    }

    let start_ms = clock.now_ms();
    let mut last_cursor = (mouse.x, mouse.y);

    loop {
        let now = clock.now_ms();
        input::poll();
        let snap = input::snapshot();
        let key = decode_key(snap.key);
        if snap.pointer_flags & PTR_HAS != 0 {
            mouse.x = snap.x.clamp(0, width as i32 - 1);
            mouse.y = snap.y.clamp(0, height as i32 - 1);
            mouse.left_down = snap.pointer_flags & PTR_DOWN != 0;
            mouse.left_clicked = snap.pointer_flags & PTR_CLICKED != 0;
            mouse.right_clicked = snap.pointer_flags & PTR_RIGHT_CLICKED != 0;
        } else {
            mouse.left_clicked = false;
            mouse.right_clicked = false;
        }
        if input::pending() {
            input::consume();
        }

        let uptime_ms = now.wrapping_sub(start_ms);
        let action = desktop_state.update(&mouse, key, width as i32, height as i32, uptime_ms);
        match action {
            DesktopAction::Shutdown => nc_arch::psci::system_off(),
            DesktopAction::Reboot => nc_arch::psci::system_reset(),
            DesktopAction::None => {}
        }

        if desktop_state.take_wallpaper_dirty() {
            nc_desktop::draw_wallpaper(&mut wallpaper, desktop_state.wallpaper_index());
        }

        if desktop_state.take_dirty() {
            nc_desktop::draw(&mut work_screen, &wallpaper, &desktop_state);
            screen.copy_rect_from(&work_screen, 0, 0, 0, 0, width as i32, height as i32);
            nc_ui::draw_cursor(screen, mouse.x, mouse.y);
            unsafe {
                present(screen, framebuffer);
            }
            last_cursor = (mouse.x, mouse.y);
        } else if (mouse.x, mouse.y) != last_cursor {
            screen.copy_rect_from(&work_screen, last_cursor.0, last_cursor.1, last_cursor.0, last_cursor.1, cursor_w, cursor_h);
            nc_ui::draw_cursor(screen, mouse.x, mouse.y);
            unsafe {
                present_region(screen, framebuffer, Rect::new(last_cursor.0, last_cursor.1, cursor_w, cursor_h));
                present_region(screen, framebuffer, Rect::new(mouse.x, mouse.y, cursor_w, cursor_h));
            }
            last_cursor = (mouse.x, mouse.y);
        }

        let deadline = now.wrapping_add(FRAME_MS);
        while !reached(clock.now_ms(), deadline) {
            home::pause();
        }
    }
}
