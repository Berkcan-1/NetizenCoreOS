use crate::dtb::{self, DtbInfo};
use crate::{blk, home, host, input, selftest, storage, syscall, task, vm};
use nc_mem::LockedHeap;
use core::fmt::Write;
use core::panic::PanicInfo;
use nc_arch::mmu::{Layout, Range};
use nc_arch::time::Clock;
use nc_device::{DeviceProfile, FramebufferSource, PixelOrder, PsciConduit, UartKind};
use nc_device_select::PROFILE;
use nc_gfx::Canvas;
use nc_hal::FramebufferInfo;
use nc_mem::Arena;

extern "C" {
    static __image_base: u8;
    static __kernel_end: u8;
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static mut __stack_bottom: u64;
}

static HOME_ELF: &[u8] = include_bytes!("../embedded/home.elf");

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

const STACK_CANARY: u64 = 0x4E43_5354_4143_4B21;
const TWO_MIB: u64 = 2 << 20;
const FOUR_GIB: u64 = nc_arch::mmu::MAPPED_BYTES;
const MAX_ARENA: u64 = 128 << 20;
const MAX_HEAP: u64 = 64 << 20;
const TICK_HZ: u64 = task::TICK_HZ;
const MAX_SIDE: u32 = 8192;

static IN_PANIC: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static TOTAL_RAM_MIB: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(4096);

pub fn total_ram_mib() -> u32 {
    TOTAL_RAM_MIB.load(core::sync::atomic::Ordering::Relaxed)
}

fn arm_stack_canary() {
    unsafe {
        core::ptr::write_volatile(core::ptr::addr_of_mut!(__stack_bottom), STACK_CANARY);
    }
}

fn stack_healthy() -> bool {
    unsafe { core::ptr::read_volatile(core::ptr::addr_of!(__stack_bottom)) == STACK_CANARY }
}

fn align_down(value: u64, alignment: u64) -> u64 {
    value & !(alignment - 1)
}

fn largest_gap(start: u64, limit: u64, exclusions: &mut [(u64, u64)]) -> (u64, u64) {
    for index in 1..exclusions.len() {
        let mut position = index;
        while position > 0 && exclusions[position - 1].0 > exclusions[position].0 {
            exclusions.swap(position - 1, position);
            position -= 1;
        }
    }
    let mut best = (0u64, 0u64);
    let mut consider = |from: u64, to: u64| {
        let from = align_up(from, TWO_MIB);
        let to = align_down(to, TWO_MIB);
        if to > from && to - from > best.1 - best.0 {
            best = (from, to);
        }
    };
    let mut cursor = start;
    for &(begin, finish) in exclusions.iter() {
        if finish <= cursor {
            continue;
        }
        if begin > cursor {
            consider(cursor, begin.min(limit));
        }
        if finish > cursor {
            cursor = finish;
        }
        if cursor >= limit {
            break;
        }
    }
    if cursor < limit {
        consider(cursor, limit);
    }
    best
}

struct Uart;

impl Write for Uart {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        nc_arch::serial::write_str(text);
        Ok(())
    }
}

macro_rules! log {
    ($($argument:tt)*) => {{
        let _ = writeln!(Uart, $($argument)*);
    }};
}

fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

fn configure_uart(profile: &DeviceProfile) {
    let base = profile.uart_base as usize;
    match profile.uart {
        UartKind::None => {}
        UartKind::Pl011 => nc_arch::serial::configure(nc_arch::serial::KIND_PL011, base, 0),
        UartKind::Ns16550 { reg_shift } => {
            nc_arch::serial::configure(nc_arch::serial::KIND_NS16550, base, reg_shift)
        }
        UartKind::Ns16550Dw32 { reg_shift } => {
            nc_arch::serial::configure(nc_arch::serial::KIND_NS16550_32, base, reg_shift)
        }
    }
    nc_arch::serial::init();
}

fn configure_psci(profile: &DeviceProfile) {
    let conduit = match profile.psci {
        PsciConduit::None => nc_arch::psci::CONDUIT_NONE,
        PsciConduit::Hvc => nc_arch::psci::CONDUIT_HVC,
        PsciConduit::Smc => nc_arch::psci::CONDUIT_SMC,
    };
    nc_arch::psci::configure(conduit);
}

struct Screen {
    info: FramebufferInfo,
    base: u64,
    bytes: u64,
}

fn resolve_screen(profile: &DeviceProfile, dtb: &Option<DtbInfo>) -> Option<Screen> {
    let display = &profile.display;
    let mut width = display.width;
    let mut height = display.height;
    let mut stride = display.stride_bytes;
    let base = match display.source {
        FramebufferSource::None => return None,
        FramebufferSource::Fixed(address) => address,
        FramebufferSource::MtkVideoLfb => match dtb {
            Some(info) if info.lfb_base != 0 => info.lfb_base,
            _ => return None,
        },
        FramebufferSource::DtbSimpleFramebuffer => match dtb {
            Some(DtbInfo { simple_fb: Some(fb), .. }) => {
                width = fb.width;
                height = fb.height;
                stride = fb.stride;
                fb.base
            }
            _ => return None,
        },
    };
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    if stride < width * 4 {
        stride = width * 4;
    }
    let bytes = stride as u64 * height as u64;
    if stride & 3 != 0 || base & 7 != 0 || base == 0 || base.saturating_add(bytes) > FOUR_GIB {
        return None;
    }
    let (red, blue) = match display.order {
        PixelOrder::Bgrx => (16u8, 0u8),
        PixelOrder::Rgbx => (0u8, 16u8),
    };
    Some(Screen {
        info: FramebufferInfo {
            address: base,
            pitch: stride,
            width,
            height,
            bpp: 32,
            red_position: red,
            red_size: 8,
            green_position: 8,
            green_size: 8,
            blue_position: blue,
            blue_size: 8,
            reserved: 0,
        },
        base,
        bytes,
    })
}

struct Boot {
    profile: &'static DeviceProfile,
    screen: Option<Screen>,
    arena: Arena,
}

static mut BOOT: Option<Boot> = None;

fn address_of(symbol: &u8) -> u64 {
    symbol as *const u8 as u64
}

#[no_mangle]
pub extern "C" fn nc_main(dtb_address: usize) -> ! {
    let profile: &'static DeviceProfile = &PROFILE;
    configure_uart(profile);
    configure_psci(profile);
    for entry in profile.drivers.iter() {
        if entry.name == "pl031" {
            nc_arch::rtc::configure_pl031(entry.regs.base as usize);
        }
    }
    nc_arch::exceptions::install();
    log!("");
    log!("NetizenCore aarch64 cekirdegi basladi");
    log!("cihaz: {} ({}), soc: {}", profile.display_name, profile.id, profile.soc);
    log!("EL{}, dtb: {:#x}", nc_arch::current_el(), dtb_address);

    let dtb_info = dtb::parse(dtb_address);
    match &dtb_info {
        Some(info) => {
            log!("dtb gecerli: {} bellek bolgesi, {} ayrilmis bolge", info.ram_count, info.reserved_count);
            for index in 0..info.ram_count {
                log!("  ram[{}] = {:#x} + {:#x}", index, info.ram[index].0, info.ram[index].1);
            }
            if info.lfb_base != 0 {
                log!("  mtk videolfb: {:#x} ({} bayt)", info.lfb_base, info.lfb_size);
            }
            if info.timer_hz != 0 {
                nc_arch::time::set_frequency_hint(info.timer_hz as u64);
            }
        }
        None => log!("dtb yok veya gecersiz"),
    }

    let mut ram = [Range { start: 0, end: 0 }; 8];
    let mut ram_count = 0usize;
    if !profile.ram.is_empty() {
        for region in profile.ram.iter().take(8) {
            ram[ram_count] = Range { start: region.base, end: region.base.saturating_add(region.size) };
            ram_count += 1;
        }
    } else if let Some(info) = &dtb_info {
        for index in 0..info.ram_count.min(8) {
            ram[ram_count] = Range {
                start: info.ram[index].0,
                end: info.ram[index].0.saturating_add(info.ram[index].1),
            };
            ram_count += 1;
        }
    }
    if ram_count == 0 {
        log!("kritik hata: RAM haritasi yok (cihaz profili ve dtb bos)");
        nc_arch::halt_forever();
    }
    let mut total_ram_bytes = 0u64;
    for r in ram[..ram_count].iter() {
        total_ram_bytes = total_ram_bytes.saturating_add(r.end.saturating_sub(r.start));
    }
    TOTAL_RAM_MIB.store((total_ram_bytes / (1024 * 1024)) as u32, core::sync::atomic::Ordering::Relaxed);

    let screen = resolve_screen(profile, &dtb_info);
    if let Some(screen) = &screen {
        if profile.id == "qemu-virt" {
            let ok = nc_arch::fwcfg::probe(nc_arch::fwcfg::QEMU_VIRT_FWCFG_BASE)
                && nc_arch::fwcfg::setup_ramfb(
                    nc_arch::fwcfg::QEMU_VIRT_FWCFG_BASE,
                    screen.base,
                    screen.info.width,
                    screen.info.height,
                    screen.info.pitch,
                );
            log!("ramfb: {}", if ok { "hazir" } else { "kurulamadi (-device ramfb verin)" });
        }
        log!("ekran: {}x{} @ {:#x}", screen.info.width, screen.info.height, screen.base);
    } else {
        log!("ekran bulunamadi; yalnizca UART");
    }

    let uncached = match &screen {
        Some(screen) => [Range { start: screen.base, end: screen.base.saturating_add(screen.bytes) }],
        None => [Range { start: 0, end: 0 }],
    };
    let kernel_start = address_of(unsafe { &__image_base });
    let kernel_end = address_of(unsafe { &__kernel_end });
    let layout = Layout {
        ram: &ram[..ram_count],
        uncached: &uncached,
        image: Range { start: kernel_start, end: kernel_end },
        text: Range { start: address_of(unsafe { &__text_start }), end: address_of(unsafe { &__text_end }) },
        rodata: Range { start: address_of(unsafe { &__rodata_start }), end: address_of(unsafe { &__rodata_end }) },
    };
    if !unsafe { nc_arch::mmu::enable(&layout) } {
        log!("kritik hata: MMU kurulamadi (cekirdek imaji cok buyuk veya 16 GiB disinda)");
        nc_arch::halt_forever();
    }
    arm_stack_canary();
    log!("mmu acik: W^X, cekirdek {:#x}..{:#x}", kernel_start, kernel_end);

    let mut region_end = 0u64;
    for index in 0..ram_count {
        if ram[index].start <= kernel_end && ram[index].end > kernel_end {
            region_end = ram[index].end.min(FOUR_GIB);
        }
    }
    let arena_from = align_up(kernel_end, TWO_MIB);
    let mut exclusions = [(0u64, 0u64); dtb::MAX_RESERVED + 3];
    let mut exclusion_count = 0usize;
    if let Some(screen) = &screen {
        exclusions[exclusion_count] = (screen.base, screen.base.saturating_add(screen.bytes));
        exclusion_count += 1;
    }
    if let Some(info) = &dtb_info {
        exclusions[exclusion_count] = (info.address, info.address.saturating_add(info.size));
        exclusion_count += 1;
        for index in 0..info.reserved_count {
            if exclusion_count < exclusions.len() {
                exclusions[exclusion_count] =
                    (info.reserved[index].0, info.reserved[index].0.saturating_add(info.reserved[index].1));
                exclusion_count += 1;
            }
        }
    }
    let (gap_start, gap_end) = if region_end > arena_from {
        largest_gap(arena_from, region_end, &mut exclusions[..exclusion_count])
    } else {
        (0, 0)
    };
    if gap_end <= gap_start || gap_end - gap_start < (48 << 20) {
        log!("kritik hata: bos bellek yetersiz ({:#x}..{:#x}, en az 48 MiB gerekir)", gap_start, gap_end);
        nc_arch::halt_forever();
    }
    let total = gap_end - gap_start;
    let heap_bytes = MAX_HEAP.min(align_down(total / 4, TWO_MIB));
    let arena_bytes = MAX_ARENA.min(align_down(total / 4, TWO_MIB));
    let heap_start = gap_start;
    let arena_start = heap_start + heap_bytes;
    let pmm_start = arena_start + arena_bytes;
    log!(
        "bellek: heap {} MiB, arena {} MiB, kare havuzu {} MiB ({:#x}..{:#x})",
        heap_bytes >> 20,
        arena_bytes >> 20,
        (gap_end - pmm_start) >> 20,
        gap_start,
        gap_end
    );
    unsafe {
        HEAP.init(heap_start as usize, heap_bytes as usize);
        vm::pmm_init(pmm_start, gap_end - pmm_start);
    }
    heap_self_test();

    nc_arch::exceptions::register_handlers(nc_arch::exceptions::Handlers {
        syscall: syscall::dispatch,
        fault: task::on_fault,
        irq: task::on_irq,
    });
    let gic_ok = nc_arch::gic::init(profile.gic_dist as usize, profile.gic_redist as usize);
    let timer_ok = gic_ok && nc_arch::timer::start(TICK_HZ);
    log!("gic: {}, zamanlayici: {} ({} Hz)", gic_ok, timer_ok, TICK_HZ);
    let (_, pan) = nc_arch::enable_cpu_hardening();
    log!("pan: {}", pan);
    let input_devices = input::init(profile);
    log!("virtio-input: {} aygit", input_devices);
    let disks = blk::init(profile);
    log!("virtio-blk: {} aygit", disks);
    if disks > 0 {
        storage::mount_system();
        storage::mount_data();
    } else {
        log!("[depo] disk yok; sistem paketleri okunamaz");
    }

    let arena = unsafe { Arena::new(arena_start as usize, arena_bytes as usize) };
    unsafe {
        *core::ptr::addr_of_mut!(BOOT) = Some(Boot { profile, screen, arena });
    }

    if timer_ok && cfg!(feature = "el0-selftest") {
        selftest::run(ui_phase)
    } else {
        ui_phase()
    }
}

fn heap_self_test() {
    let mut values: alloc::vec::Vec<u32> = alloc::vec::Vec::new();
    for index in 0..50_000u32 {
        values.push(index.wrapping_mul(2_654_435_761));
    }
    let mut sum = 0u64;
    for value in values.iter() {
        sum = sum.wrapping_add(*value as u64);
    }
    let invalid = HEAP.invalid_dealloc_count();
    drop(values);
    log!("heap sinamasi tamam (toplam {}, gecersiz serbest birakma {})", sum, invalid);
}

fn ui_phase() -> ! {
    selftest::report();
    let boot = match unsafe { (*core::ptr::addr_of_mut!(BOOT)).take() } {
        Some(boot) => boot,
        None => nc_arch::halt_forever(),
    };
    let profile = boot.profile;
    let mut arena = boot.arena;

    let screen = match boot.screen {
        Some(screen) => screen,
        None => {
            log!("ekransiz mod: dongu");
            let mut clock = Clock::calibrate();
            let mut next = 1000u32;
            loop {
                let now = clock.now_ms();
                if now.wrapping_sub(next) < 0x8000_0000 {
                    log!("canli: {} ms (tik {})", now, task::ticks());
                    next = next.wrapping_add(1000);
                    if !stack_healthy() {
                        log!("yigin tasmasi!");
                        nc_arch::halt_forever();
                    }
                }
                home::pause();
            }
        }
    };

    let width = screen.info.width as usize;
    let height = screen.info.height as usize;
    let pixels = match arena.alloc_pixels(width * height) {
        Some(pixels) => pixels,
        None => {
            log!("kritik hata: canvas icin bellek yok");
            nc_arch::halt_forever();
        }
    };
    let mut canvas = match Canvas::new(pixels, width, height) {
        Some(canvas) => canvas,
        None => nc_arch::halt_forever(),
    };

    let mut clock = Clock::calibrate();
    log!("saat: {} tik/ms, hassas: {}", clock.ticks_per_ms(), clock.is_precise());

    home::splash(profile, &mut canvas, &mut arena, &screen.info, &mut clock);

    if cfg!(feature = "netizen-home") {
        task::init(host::recovery);
        match task::spawn_elf("NetizenHome", HOME_ELF) {
            Ok(slot) => {
                log!("NetizenHome yuklendi (EL0, gorev {})", slot);
                host::attach(canvas, screen.info, slot);
                task::start()
            }
            Err(reason) => log!("NetizenHome yuklenemedi: {} (yer tutucu ekran)", reason),
        }
    }
    home::placeholder(profile, &mut canvas, &screen.info, &mut clock, stack_healthy)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if IN_PANIC.load(core::sync::atomic::Ordering::Relaxed) {
        nc_arch::halt_forever();
    }
    IN_PANIC.store(true, core::sync::atomic::Ordering::Relaxed);
    let _ = writeln!(Uart, "\n*** CEKIRDEK PANIGI: {}", info);
    nc_arch::halt_forever()
}
