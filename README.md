

<img width="256" height="256" alt="netizencore-crab-c" src="https://github.com/user-attachments/assets/79860eb6-02eb-4187-840f-be337bc37714" />

# NetizenCore

An operating system written from scratch, not based on Unix/Linux. The bootloader is written in Assembly, while the kernel and all modules are written entirely in Rust.
There are no comments in the code; explanations are provided in the README.md file in each directory.

## What It Currently Does

1. The BIOS loads stage1 from the ISO. stage1 reads stage2 and the kernel into memory.
2. stage2 checks for a 64-bit CPU, selects a graphics mode using VBE, sets up 4 GB paging, and switches to long mode.
3. The kernel initializes the display, calibrates the clock, and plays the boot animation: a logo that starts blurred and gradually becomes sharp, the
   NetizenCore text, a breathing line below it, the PREPARING SETUP text, and blinking dots.
4. Once preparation is complete, the animation fades out with a blur effect. If the system is not installed, the setup wizard opens; otherwise, the desktop loads.
5. Desktop: colorful wallpaper, icons, taskbar, Start menu, and draggable windows. Applications
   (Settings, System, Stopwatch) are separate crates and run through a shared contract (crates/nc-app/README.md).

## Appearance

A clean, white HarmonyOS/EMUI-style interface; Frutiger Aero is used only for small accents (azure/aqua glass spheres, gradient pills, subtle
highlights on cards). Details are available in crates/nc-ui/README.md.

## Profiles

| Profile | Target                 | Resolution | Log Output                         |
|---------|------------------------|------------|------------------------------------|
| qemu    | QEMU virtual hardware  | 1280x720   | serial port, debugcon, isa-debug-exit |
| generic | General PC, VirtualBox | 1024x768   | serial port (if available)         |

## Commands

    bash build.sh            produces the qemu ISO: build/netizencore-qemu.iso
    bash build.sh generic    produces the generic ISO: build/netizencore-generic.iso
    bash build.sh all        produces both
    bash build.sh run        produces the qemu ISO and launches it in QEMU
    bash build.sh vbox       produces the generic ISO and launches it in VirtualBox
    bash build.sh clean      removes the build directory

Requirements (Debian/Ubuntu):

    sudo apt install nasm xorriso binutils qemu-system-x86 zip
    rustup (build.sh adds the x86_64-unknown-none target automatically)

The PNG library is downloaded from the internet during the first build.

## Directories

| Directory     | Contents                                                               |
|---------------|------------------------------------------------------------------------|
| assets/       | Files embedded into the kernel at build time (currently, only the logo) |
| bootloader/   | stage1.asm and stage2.asm (Assembly)                                   |
| crates/       | Reusable Rust modules (hal, arch, gfx, font, ...)                      |
| docs/         | Architecture, memory map, module list                                  |
| kernel/       | Entry point, platform selection, boot flow                             |

Each directory and each src directory contains a README.md that explains that directory in detail.

## Debugging (QEMU)

Serial output is displayed in the terminal, while debugcon output is written to build/debugcon.log. If the kernel panics, the message
is written to both. QEMU is automatically accelerated when /dev/kvm is accessible; without KVM, the animation plays with fewer frames,
but its duration remains the same.

## Working with AI Tools

You do not need to provide the entire project. Provide the relevant module's directory (README.md + src) and the README.md files of its
dependencies. The dependency graph is available in docs/ARCHITECTURE.md.

======
# NetizenCore Architecture

## Boot Flow

    BIOS -> El Torito (no-emulation) -> stage1 -> stage2 -> kernel (_start asm, kernel_main Rust)

Execution order of kernel_main:

1. Initialize the platform (serial port, debugcon, etc.) and read BootInfo.
2. Calibrate the clock using PIT and TSC (nc-arch::time::Clock).
3. Allocate the display, transition, and setup buffers from the memory pool (Arena) at 0x400000.
4. Draw the backdrop and present it on the screen; then prepare the Splash (layers and blur levels).
5. Animation loop: run Intro and Idle until Preparation is ready; once ready, run Outro. Only the rectangles that have changed are
   written to the framebuffer in each frame.
6. If the system is not installed on the disk, run the setup wizard; otherwise, go directly to the desktop.
7. Desktop: DesktopState is created using nc_apps::builtin(). Each frame, the loop reads input and calls state.update; the scene
   is redrawn only when it is "dirty", and if only the cursor has moved, only the cursor's old and new rectangles are updated.

## Memory Map

    0x0600   VBE controller information (temporary)
    0x0800   VBE mode information (temporary)
    0x0900   BootInfo
    0x1000   page tables (up to 0x6FFF)
    0x7C00   stage1
    0x8800   stage2
    0x9800   kernel loaded image (temporary)
    0x100000 kernel (.text .rodata .data .bss, 64 KB stack)
    0x400000 memory pool (Arena), 60 MB, up to 0x4000000

The memory pool is currently used as follows (approximately 29 MB at 1280x720; plus an 8 MB dynamic heap, approximately 37 MB in total; applications use this heap through Box):

    display buffer       3.7 MB   (u32 pixels, 0x00RRGGBB)
    transition buffer    3.7 MB
    setup buffer         3.7 MB
    wallpaper            3.7 MB   (desktop wallpaper cache)
    splash layers        approx. 14 MB (backdrop copy + 4 blur levels + temporary buffer)

Paging: 0-3 GB cached, 3-4 GB uncached, 2 MB pages. The kernel expects at least 64 MB of RAM
(until E820 is read). QEMU and VirtualBox are allocated 256 MB.

## Dependency Graph

    nc-hal                 (none)
    nc-arch                (none)
    nc-math                (none)
    nc-mem                 (none)
    nc-assets              (none; build.rs: png)
    nc-gfx                 -> nc-hal
    nc-font                -> nc-math
    nc-ui                  -> nc-gfx, nc-font, nc-math
    nc-splash              -> nc-assets, nc-gfx, nc-font, nc-math, nc-mem, nc-ui
    nc-setup               -> nc-hal, nc-gfx, nc-font, nc-ui
    nc-input               -> nc-arch, nc-hal
    nc-storage             -> nc-arch, nc-hal
    nc-app                 -> nc-hal, nc-gfx, nc-font, nc-ui
    nc-app-settings        -> nc-app, nc-gfx, nc-font, nc-ui
    nc-app-system          -> nc-app, nc-gfx, nc-font, nc-ui
    nc-app-stopwatch       -> nc-app, nc-gfx, nc-font, nc-ui
    nc-apps                -> nc-app, nc-app-settings, nc-app-system, nc-app-stopwatch
    nc-desktop             -> nc-hal, nc-gfx, nc-font, nc-ui, nc-app   (does not know applications by name)
    nc-platform-qemu       -> nc-hal, nc-arch
    nc-platform-generic    -> nc-hal, nc-arch
    kernel (nc-kernel)     -> all (one selected according to platform feature flags)

## Profiles

A profile is a Cargo feature of the kernel crate: qemu or generic. If both or neither are selected, the build fails
(kernel/src/platform.rs). To add a new platform:

1. Create crates/nc-platform-<name> and implement nc-hal::Platform.
2. Add it to the workspace member list in the root Cargo.toml.
3. Add the feature and optional dependency to kernel/Cargo.toml.
4. Add a cfg line to kernel/src/platform.rs.
5. Add the resolution setting to configure_profile in build.sh.

## Rules

- Modules may only use one another according to the arrows in the dependency graph.
- Hardware-specific code exists only in nc-arch and nc-platform-*.

=====
# Modules

| Module              | Responsibility                                                               |
|---------------------|-------------------------------------------------------------------------------|
| nc-hal              | Bootloader contract (BootInfo), Platform interface, log! macro               |
| nc-arch             | x86_64 port I/O, halt, 16550 serial port, PIT/TSC clock                       |
| nc-math             | Integer trigonometry, square root, smoothing, and ratio functions             |
| nc-mem              | Simple memory pool (Arena)                                                    |
| nc-assets           | PNG logo pipeline (build.rs) and logo access                                 |
| nc-gfx              | Canvas, drawing primitives, blur, blending, framebuffer transfer             |
| nc-font             | Custom vector-stroke typeface (A-Z, 0-9, punctuation)                         |
| nc-ui               | Theme colors, backdrop, text, spheres, pills, shadows, card helpers           |
| nc-splash           | Animated boot screen                                                          |
| nc-input            | PS/2 keyboard and mouse (InputDevice implementation)                          |
| nc-storage          | ATA PIO disk detection, read/write, installation (MBR, stage2, kernel writing) |
| nc-app              | Application contract (App trait, Context, Input, theme, icons)                |
| nc-app-settings     | Settings application (wallpaper, accent color, resolution information)       |
| nc-app-system       | System information application                                                |
| nc-app-stopwatch    | Stopwatch application                                                          |
| nc-apps             | List of built-in applications (builtin)                                       |
| nc-desktop          | Desktop shell: windows, taskbar, Start menu                                   |
| nc-setup            | Setup screen (Welcome)                                                         |
| nc-platform-qemu    | QEMU-specific Platform implementation                                         |
| nc-platform-generic | General PC Platform implementation                                            |
| kernel              | Entry point, boot flow, panic handler                                         |

Each module's directory contains a detailed README.md, and its src directory contains file-by-file explanations.
