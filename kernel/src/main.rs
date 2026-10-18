#![no_std]
#![no_main]

extern crate alloc;

mod devices;
mod entry;
mod platform;
mod prepare;
mod process;

use alloc::boxed::Box;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, Ordering};
use devices::DeviceManager;
use nc_arch::halt_forever;
use nc_arch::time::Clock;
use nc_desktop::{DesktopAction, DesktopState};
use nc_gfx::compose::lerp_canvases;
use nc_gfx::{present, present_region, Canvas, Rect};
use nc_hal::{log, BootInfo, Platform};
use nc_math::{ratio256, smoothstep256};
use nc_mem::Arena;
use nc_setup::{SetupAction, SetupState};
use nc_splash::{Phase, Splash, SplashOptions};
use process::ProcessManager;

extern "C" {
    static __kernel_bin_start: u8;
    static __kernel_bin_end: u8;
}

#[global_allocator]
static ALLOCATOR: nc_mem::LockedHeap = nc_mem::LockedHeap::empty();
static PANICKING: AtomicBool = AtomicBool::new(false);

const ARENA_BASE: usize = 0x0080_0000;
const FRAME_MS: u32 = 16;
const TRANSITION_MS: u32 = 600;

// PAIR-028: Checked arithmetic ile guvenli Canvas olusturma
fn new_canvas(arena: &mut Arena, width: usize, height: usize) -> Option<Canvas<'static>> {
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return None;
    }
    let count = width.checked_mul(height)?;
    let pixels = arena.alloc_pixels(count)?;
    Canvas::new(pixels, width, height)
}

// PAIR-008 & PAIR-034: Sahte RAM tahminini kaldir, gercek kullanilabilir RAM'i dogrula
fn detect_ram_bounds(boot: &BootInfo) -> Option<(usize, usize, usize)> {
    let mut managed_usable: u64 = 0;
    let mut best_base: u64 = ARENA_BASE as u64;
    let mut best_end: u64 = 0;

    for region in boot.memory_regions() {
        if region.is_usable() {
            let start = region.base.max(ARENA_BASE as u64);
            let end = region.base.saturating_add(region.length).min(0xC000_0000);
            if end > start {
                let len = end - start;
                managed_usable = managed_usable.saturating_add(len);
                if len > best_end.saturating_sub(best_base) {
                    best_base = start;
                    best_end = end;
                }
            }
        }
    }

    // En az 16 MB kesintisiz kullanilabilir bolge sart
    if best_end <= best_base || (best_end - best_base) < 16 * 1024 * 1024 {
        return None;
    }

    let total_mb = (managed_usable / (1024 * 1024)).max(16) as usize;
    Some((total_mb, best_base as usize, best_end as usize))
}

fn detect_is_uefi(boot: &BootInfo) -> bool {
    // 1. nc-uefi tarafindan set edilen BootInfo bayragi
    if boot.framebuffer.reserved == 1 {
        return true;
    }

    // 2. Donanimsal Dogrulama: stage2.asm CR3 sayfa tablosunu daima 0x1000 yapar.
    // UEFI firmware ise sayfa tablolarini dinamik yuksek bellege acar.
    let cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    }
    cr3 != 0x1000
}

// PAIR-035: Timeout korumali COM2 gonderimi
unsafe fn send_com2_bytes_bounded(bytes: &[u8]) {
    if nc_arch::inb(0x2F8 + 5) == 0xFF {
        return;
    }
    for &b in bytes {
        let mut spins = 0u32;
        while (nc_arch::inb(0x2F8 + 5) & 0x20) == 0 {
            spins += 1;
            if spins > 50_000 {
                return;
            }
            core::hint::spin_loop();
        }
        nc_arch::outb(0x2F8, b);
    }
}

// Gecis animasyonu sirasinda bloklamadan calisan PIT melodisi
unsafe fn update_async_chime(elapsed_ms: u32) {
    const NOTES: [(u32, u32, u32); 4] = [
        (0, 90, 523),
        (90, 180, 659),
        (180, 290, 784),
        (290, 510, 1046),
    ];
    let mut active_freq = 0u32;
    for &(start, end, freq) in &NOTES {
        if elapsed_ms >= start && elapsed_ms < end {
            active_freq = freq;
            break;
        }
    }
    if active_freq > 0 {
        let div = 1_193_182 / active_freq;
        nc_arch::outb(0x43, 0xB6);
        nc_arch::outb(0x42, (div & 0xFF) as u8);
        nc_arch::outb(0x42, ((div >> 8) & 0xFF) as u8);
        let tmp = nc_arch::inb(0x61);
        if (tmp & 3) != 3 {
            nc_arch::outb(0x61, tmp | 3);
        }
    } else {
        let tmp = nc_arch::inb(0x61);
        if (tmp & 3) != 0 {
            nc_arch::outb(0x61, tmp & 0xFC);
        }
    }
}

#[no_mangle]
pub extern "C" fn kernel_main() -> ! {
    unsafe {
        core::arch::asm!("cli");
        nc_arch::disable_pic();
    }

    let mut active = platform::create();
    active.init();
    log!(&active, "NetizenCore cekirdegi basladi, platform: {}", active.name());

    let boot = match unsafe { BootInfo::read_from_boot_address() } {
        Some(boot) => boot,
        None => {
            log!(&active, "boot bilgisi gecersiz");
            halt_forever()
        }
    };

    let framebuffer = boot.framebuffer;
    let mut clock = Clock::calibrate();
    let width = framebuffer.width as usize;
    let height = framebuffer.height as usize;

    let (total_ram_mb, usable_base, usable_limit) = match detect_ram_bounds(&boot) {
        Some(bounds) => bounds,
        None => {
            log!(&active, "Kritik hata: Gecerli E820/UEFI bellek haritasi bulunamadi");
            halt_forever()
        }
    };

    let total_avail = usable_limit.saturating_sub(usable_base);
    let canvas_bytes = match width.checked_mul(height).and_then(|px| px.checked_mul(4)) {
        Some(b) if b > 0 => b,
        _ => halt_forever(),
    };
    let arena_target = canvas_bytes.saturating_mul(8).max(24 * 1024 * 1024);
    let arena_size = arena_target.min(total_avail / 2);
    let heap_size = total_avail.saturating_sub(arena_size);
    let heap_base = usable_base + arena_size;

    let mut arena = unsafe { Arena::new(usable_base, arena_size) };
    unsafe {
        ALLOCATOR.init(heap_base, heap_size);
        for region in boot.memory_regions() {
            if region.is_usable() {
                let r_start = (region.base as usize).max(ARENA_BASE);
                let r_end = (region.base.saturating_add(region.length) as usize).min(0xC000_0000);
                if r_end > r_start && (r_end <= usable_base || r_start >= usable_limit) {
                    ALLOCATOR.add_region(r_start, r_end - r_start);
                }
            }
        }
    }

    nc_arch::idt::init();
    let drv_reg = nc_driver::init();
    log!(&active, "{}", drv_reg.status_log);

    // Eksik olan satır:
    let is_installed = nc_storage::find_installed_disk().is_some();

    // 4 yerine 3 tampon: wallpaper hem gecis snapshot'i hem duvar kagidi olarak kullanilir
    let mut screen = match new_canvas(&mut arena, width, height) {
        Some(canvas) => canvas,
        None => halt_forever(),
    };
    let mut work_screen = match new_canvas(&mut arena, width, height) {
        Some(canvas) => canvas,
        None => halt_forever(),
    };
    let mut wallpaper = match new_canvas(&mut arena, width, height) {
        Some(canvas) => canvas,
        None => halt_forever(),
    };

    nc_ui::draw_backdrop(&mut screen);
    unsafe {
        present(&screen, &framebuffer);
    }

    let splash_opts = if is_installed {
        SplashOptions { subtitle: None }
    } else {
        SplashOptions { subtitle: Some("PREPARING SETUP") }
    };

    let mut splash = match Splash::new(&screen, &mut arena, splash_opts) {
        Some(splash) => splash,
        None => halt_forever(),
    };

    let mut preparation = prepare::Preparation::new(clock.now_ms());
    loop {
        let now = clock.now_ms();
        if preparation.poll(now) {
            splash.request_finish();
        }
        let update = splash.frame(&mut screen, now);
        for index in 0..update.count {
            unsafe {
                present_region(&screen, &framebuffer, update.rects[index]);
            }
        }
        if splash.phase() == Phase::Done {
            break;
        }
        clock.sleep_until(now.wrapping_add(FRAME_MS));
    }

    let mut device_manager = DeviceManager::new();
    for dev in preparation.take_disks() {
        device_manager.register_block_device(dev);
    }

    nc_input::init();
    let ps2 = nc_input::Ps2Input::new(width as i32 / 2, height as i32 / 2);
    device_manager.register_input_device(Box::new(ps2));

    if !is_installed {
        let mut setup_state = SetupState::new(device_manager.block_infos());
        wallpaper.copy_rect_from(&screen, 0, 0, 0, 0, width as i32, height as i32);
        let (mouse, _) = device_manager.poll_input(width as i32, height as i32);
        nc_setup::draw(&mut work_screen, active.name(), &mut setup_state, &mouse);

        let transition_start = clock.now_ms();
        loop {
            let now = clock.now_ms();
            let progress = ratio256(now.wrapping_sub(transition_start), TRANSITION_MS);
            lerp_canvases(&mut screen, &wallpaper, &work_screen, smoothstep256(progress));
            unsafe {
                present(&screen, &framebuffer);
            }
            if progress >= 256 {
                break;
            }
            clock.sleep_until(now.wrapping_add(FRAME_MS));
        }

        let mut written_partitions = false;
        let mut install_attempts = 0u8;
        let mut last_setup_cursor = (mouse.x, mouse.y);
        screen.copy_rect_from(&work_screen, 0, 0, 0, 0, width as i32, height as i32);
        nc_ui::draw_cursor(&mut screen, mouse.x, mouse.y);
        unsafe {
            present(&screen, &framebuffer);
        }

        loop {
            let now = clock.now_ms();
            let (mouse, key) = device_manager.poll_input(width as i32, height as i32);
            let action = setup_state.update(&mouse, key, width as i32, height as i32);

            // PAIR-029: Kurulum yazma sonucunu dogrula, basarisizsa en fazla 3 kez dene
            if setup_state.step < 2 && !written_partitions {
                install_attempts = 0;
            }

            // C-01, H-01 & 5.3: Gercek donanim kanalini dogrula, UEFI imajini zorunlu kil ve sonucu UI'ya bildir
            if setup_state.step == 2 && !written_partitions && install_attempts < 3 {
                install_attempts += 1;
                let is_uefi_system = detect_is_uefi(&boot);

                let target_channel = match setup_state.selected_disk_channel() {
                    Some(ch) => ch,
                    None => {
                        setup_state.fail_install(
                            "Secilen disk kurulum icin uygun degil veya kapasitesi 3.4 GB altinda.",
                        );
                        continue;
                    }
                };

                let uefi_slice = unsafe {
                    let ptr = 0x0040_0000 as *const u8;
                    let magic = core::ptr::read_unaligned(ptr as *const u32);
                    let size = core::ptr::read_unaligned(ptr.add(4) as *const u32) as usize;
                    if magic == 0x5545_4649 && size > 0 && size <= 3 * 1024 * 1024 {
                        Some(core::slice::from_raw_parts(ptr.add(8), size))
                    } else {
                        None
                    }
                };

                // Bolum 5.3: UEFI sistemde BOOTX64.EFI yoksa kurulumu yarim birakma, hata ver
                if is_uefi_system && uefi_slice.is_none() {
                    log!(&active, "[HATA] UEFI ortami algilandi fakat gecerli BOOTX64.EFI bulunamadi");
                    setup_state.fail_install(
                        "UEFI sistemde BOOTX64.EFI imaji bulunamadi; kurulum guvenlik icin durduruldu.",
                    );
                    continue;
                }

                if let Some(mut target_disk) = nc_storage::find_install_target_disk(Some(target_channel)) {
                    let stage2_slice = unsafe { core::slice::from_raw_parts(0x8800 as *const u8, 4096) };
                    let kernel_bytes = unsafe {
                        (&__kernel_bin_end as *const u8 as usize)
                            .saturating_sub(&__kernel_bin_start as *const u8 as usize)
                    };

                    let install_success = if kernel_bytes > 0 && kernel_bytes <= 2 * 1024 * 1024 {
                        let kernel_slice = unsafe { core::slice::from_raw_parts(0x100000 as *const u8, kernel_bytes) };
                        if is_uefi_system {
                            log!(&active, "[BOOT] Kanal {} uzerine UEFI + ESP kurulumu basliyor...", target_channel);
                        } else {
                            log!(&active, "[BOOT] Kanal {} uzerine Legacy BIOS kurulumu basliyor...", target_channel);
                        }
                        nc_storage::install_to_disk(&mut target_disk, stage2_slice, kernel_slice, uefi_slice).is_ok()
                    } else {
                        false
                    };

                    if install_success {
                        nc_storage::write_initial_locale(
                            &mut target_disk,
                            setup_state.selected_language_code(),
                            setup_state.selected_keyboard_code(),
                        );
                        written_partitions = true;
                        setup_state.complete_install();
                        log!(
                            &active,
                            "Kurulum Kanal {} uzerinde basariyla tamamlandi ve dogrulandi ({})",
                            target_channel,
                            if is_uefi_system { "UEFI" } else { "Legacy" }
                        );
                    } else {
                        log!(&active, "Disk kurulum hatasi (Kanal {}, deneme {}/3)", target_channel, install_attempts);
                        if install_attempts >= 3 {
                            setup_state.fail_install(
                                "Disk yazma veya MBR geri-okuma dogrulamasi 3 denemede de basarisiz oldu.",
                            );
                        }
                    }
                } else {
                    log!(&active, "Hedef disk (Kanal {}) erisilemez durumda", target_channel);
                    if install_attempts >= 3 {
                        setup_state.fail_install(
                            "Secilen fiziksel disk kanali yanit vermiyor veya kapasite sinirini karsilamiyor.",
                        );
                    }
                }
            }

            if nc_setup::draw(&mut work_screen, active.name(), &mut setup_state, &mouse) {
                screen.copy_rect_from(&work_screen, 0, 0, 0, 0, width as i32, height as i32);
                nc_ui::draw_cursor(&mut screen, mouse.x, mouse.y);
                unsafe {
                    present(&screen, &framebuffer);
                }
                last_setup_cursor = (mouse.x, mouse.y);
            } else if (mouse.x, mouse.y) != last_setup_cursor {
                let cw = nc_ui::CURSOR_WIDTH as i32;
                let ch = nc_ui::CURSOR_HEIGHT as i32;
                screen.copy_rect_from(&work_screen, last_setup_cursor.0, last_setup_cursor.1, last_setup_cursor.0, last_setup_cursor.1, cw, ch);
                nc_ui::draw_cursor(&mut screen, mouse.x, mouse.y);
                unsafe {
                    present_region(&screen, &framebuffer, Rect::new(last_setup_cursor.0, last_setup_cursor.1, cw, ch));
                    present_region(&screen, &framebuffer, Rect::new(mouse.x, mouse.y, cw, ch));
                }
                last_setup_cursor = (mouse.x, mouse.y);
            }

            if action == SetupAction::Reboot {
                active.reboot();
            }
            clock.sleep_until(now.wrapping_add(FRAME_MS));
        }
    }

    let mut desktop_state = DesktopState::new(active.name(), total_ram_mb as u32, nc_apps::builtin());
    nc_desktop::draw_wallpaper(&mut wallpaper, desktop_state.wallpaper_index());
    desktop_state.take_wallpaper_dirty();
    nc_desktop::draw(&mut work_screen, &wallpaper, &desktop_state);
    desktop_state.take_dirty();

    // Gecis animasyonu icin mevcut ekrani wallpaper uzerinde gecici tut
    wallpaper.copy_rect_from(&screen, 0, 0, 0, 0, width as i32, height as i32);

    unsafe {
        send_com2_bytes_bounded(b"CMD:STARTUP_SOUND\n");
    }
    nc_driver::play_hardware_startup_chime();

    // Masaustu gecis animasyonu ve PIT melodisi eszamanli calisir (510 ms donma yok)
    let desktop_trans_start = clock.now_ms();
    loop {
        let now = clock.now_ms();
        let elapsed = now.wrapping_sub(desktop_trans_start);
        unsafe {
            update_async_chime(elapsed);
        }
        let progress = ratio256(elapsed, TRANSITION_MS);
        lerp_canvases(&mut screen, &wallpaper, &work_screen, smoothstep256(progress));
        unsafe {
            present(&screen, &framebuffer);
        }
        if progress >= 256 {
            unsafe {
                update_async_chime(1000); // Hoparloru kapat
            }
            break;
        }
        clock.sleep_until(now.wrapping_add(FRAME_MS));
    }

    // Duvar kagidini asil haliyle geri ciz
    nc_desktop::draw_wallpaper(&mut wallpaper, desktop_state.wallpaper_index());

    let cursor_width = nc_ui::CURSOR_WIDTH as i32;
    let cursor_height = nc_ui::CURSOR_HEIGHT as i32;
    let desktop_start_time = clock.now_ms();
    let (first_mouse, _) = device_manager.poll_input(width as i32, height as i32);

    screen.copy_rect_from(&work_screen, 0, 0, 0, 0, width as i32, height as i32);
    nc_ui::draw_cursor(&mut screen, first_mouse.x, first_mouse.y);
    unsafe {
        present(&screen, &framebuffer);
    }

    let mut process_manager = ProcessManager::new();
    let mut last_cursor = (first_mouse.x, first_mouse.y);

    loop {
        let now = clock.now_ms();
        process_manager.poll_all();

        let (mouse, key) = device_manager.poll_input(width as i32, height as i32);
        let uptime_ms = now.wrapping_sub(desktop_start_time);
        let action = desktop_state.update(&mouse, key, width as i32, height as i32, uptime_ms);

        if desktop_state.take_wallpaper_dirty() {
            nc_desktop::draw_wallpaper(&mut wallpaper, desktop_state.wallpaper_index());
        }

        if desktop_state.take_dirty() {
            nc_desktop::draw(&mut work_screen, &wallpaper, &desktop_state);
            screen.copy_rect_from(&work_screen, 0, 0, 0, 0, width as i32, height as i32);
            nc_ui::draw_cursor(&mut screen, mouse.x, mouse.y);
            unsafe {
                present(&screen, &framebuffer);
            }
            last_cursor = (mouse.x, mouse.y);
        } else if (mouse.x, mouse.y) != last_cursor {
            screen.copy_rect_from(&work_screen, last_cursor.0, last_cursor.1, last_cursor.0, last_cursor.1, cursor_width, cursor_height);
            nc_ui::draw_cursor(&mut screen, mouse.x, mouse.y);
            unsafe {
                present_region(&screen, &framebuffer, Rect::new(last_cursor.0, last_cursor.1, cursor_width, cursor_height));
                present_region(&screen, &framebuffer, Rect::new(mouse.x, mouse.y, cursor_width, cursor_height));
            }
            last_cursor = (mouse.x, mouse.y);
        }

        match action {
            DesktopAction::Shutdown => active.shutdown(0),
            DesktopAction::Reboot => active.reboot(),
            DesktopAction::None => {}
        }

        clock.sleep_until(now.wrapping_add(FRAME_MS));
    }
}

struct EmergencySerial;

impl Write for EmergencySerial {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        nc_arch::serial::write_str(s);
        Ok(())
    }
}

// PAIR-036: Tahsis yapmayan ve yeniden girise dayanikli acil durum panic handler
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
    }
    if !PANICKING.swap(true, Ordering::SeqCst) {
        let mut out = EmergencySerial;
        let _ = writeln!(out, "\n[KERNEL PANIC] {}", info);
    }
    halt_forever()
}
