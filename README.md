# NetizenCore

<img width="1377" height="768" alt="cre" src="https://github.com/user-attachments/assets/1547d582-d169-433f-9cef-ddbcf3693264" />



**NetizenCore is an operating system built from scratch, without a Unix or Linux foundation.** Its bootloader is written in Assembly, while the kernel and all kernel modules are written entirely in Rust.

The project is designed to boot in QEMU and currently implements a BIOS-based boot sequence, 64-bit processor initialization, graphics setup, and an animated startup experience leading to an initial setup screen.



## Current Features

NetizenCore currently implements the following boot and startup sequence:

1. **BIOS boot:** The BIOS loads Stage 1 from the bootable ISO. Stage 1 loads Stage 2 and the kernel into memory.
2. **Processor initialization:** Stage 2 checks for 64-bit processor support, selects a graphics mode through VBE, configures paging for the first 4 GiB of address space, and switches the processor into 64-bit long mode.
3. **Kernel startup:** The kernel initializes the display, calibrates its clock, and plays the startup animation. The animation features a logo and the `NetizenCore` wordmark fading from blurred to sharp, a breathing line beneath them, a `PREPARING SETUP` message, and blinking dots.
4. **Initial setup screen:** Once the preparation sequence finishes, the startup animation fades away and the `Welcome` setup screen appears. Execution currently stops at this screen.

## Interface Design

NetizenCore uses a clean, predominantly white interface inspired by HarmonyOS and EMUI.

Frutiger Aero influences are reserved for subtle visual accents rather than the entire interface. These include azure and aqua glass spheres, gradient pill-shaped elements, and a soft highlight effect on cards.

For detailed interface specifications and implementation information, see [`crates/nc-ui/README.md`](crates/nc-ui/README.md).

## Build Profiles

| Profile | Target | Resolution | Debug Output |
|---|---|---|---|
| `qemu` | QEMU virtual hardware | 1280 × 720 | Serial port, debugcon, `isa-debug-exit` |
| `generic` | General-purpose PC and VirtualBox | 1024 × 768 | Serial port, when available |

## Building and Running

### Requirements

On Debian or Ubuntu, install the required system packages:

```bash
sudo apt install nasm xorriso binutils qemu-system-x86 zip
```

Install Rust through `rustup`. The build script automatically adds the `x86_64-unknown-none` compilation target when needed.

The first build downloads the required PNG-processing library and its dependencies from the internet.

### Build Commands

Run the following commands from the repository root:

| Command | Description |
|---|---|
| `bash build.sh` | Builds the QEMU ISO at `build/netizencore-qemu.iso`. |
| `bash build.sh generic` | Builds the generic ISO at `build/netizencore-generic.iso`. |
| `bash build.sh all` | Builds both ISO images. |
| `bash build.sh run` | Builds the QEMU ISO and launches it in QEMU. |
| `bash build.sh vbox` | Builds the generic ISO and launches it in VirtualBox. |
| `bash build.sh clean` | Removes the build directory. |

## Repository Structure

| Directory | Description |
|---|---|
| `assets/` | Files embedded into the kernel at build time. Currently contains the logo. |
| `bootloader/` | The Stage 1 and Stage 2 bootloader implementations, written in Assembly. |
| `crates/` | Reusable Rust modules, including hardware abstraction, architecture support, graphics, fonts, and interface components. |
| `docs/` | Architecture documentation, memory maps, and module information. |
| `kernel/` | Kernel entry point, platform selection, and startup sequence. |

Each directory and its source directories contain a `README.md` explaining the relevant components, their responsibilities, and their implementation details.

The overall architecture and module dependency graph are documented in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Debugging in QEMU

NetizenCore provides multiple debugging output channels to help diagnose boot and kernel issues.

- **Serial output:** Messages are available in the terminal.
- **Debug console:** Output is written to `build/debugcon.log`.
- **Kernel panics:** Panic messages are sent to both the serial output and debug console.

QEMU automatically uses KVM acceleration when `/dev/kvm` is accessible. Without KVM, the startup animation uses fewer frames to compensate for slower execution while keeping its overall duration unchanged.

## Project Status

NetizenCore is an actively developed, from-scratch operating system project. Its current implementation covers the initial boot process, basic kernel startup, graphics initialization, and the transition to the Welcome setup screen.

It is not yet a complete general-purpose operating system. The current build stops at the initial setup interface, and further system functionality remains to be implemented.
