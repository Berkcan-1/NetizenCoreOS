
<img width="2400" height="1350" alt="NetizenCore × Melvin (3)" src="https://github.com/user-attachments/assets/907bbed1-d36f-413f-8ef8-90f8a34d4e9b" />

# NetizenCore

**An operating system written from scratch. No Unix, no Linux underneath.**

The bootloaders are Assembly. The kernel and every kernel module are Rust. Applications run in user mode as separate processes (Ring 3 on x86_64, EL0 on AArch64) on top of a documented syscall ABI.

![License](https://img.shields.io/badge/license-AGPL--3.0-blue)
![Language](https://img.shields.io/badge/kernel-Rust-orange)
![Bootloader](https://img.shields.io/badge/bootloader-Assembly-lightgrey)
![Platforms](https://img.shields.io/badge/platforms-x86__64%20%7C%20AArch64-green)


NetizenCore is an experimental project. It boots, draws its own interface and runs its own applications, but it is not a finished general-purpose operating system. The [Status](#status) section says exactly what works and what does not.

---

## Table of contents

- [Highlights](#highlights)
- [Status](#status)
- [Quick start](#quick-start)
- [How it boots](#how-it-boots)
- [Repository layout](#repository-layout)
- [Platforms and build profiles](#platforms-and-build-profiles)
- [Userland and security model](#userland-and-security-model)
- [Hardware port: Infinix Hot 20 (X6826C)](#hardware-port-infinix-hot-20-x6826c)
- [Debugging](#debugging)
- [Documentation](#documentation)
- [Contributing](#contributing)
- [License](#license)

---

## Highlights

- **From scratch.** Own bootloaders, own kernel, own drivers, own graphics stack, own UI toolkit. Nothing is derived from Unix or Linux.
- **Two architectures.** x86_64 (BIOS and UEFI) and AArch64, with the same desktop on standard ARM PCs and a separate mobile shell for phones and tablets.
- **User mode by design.** Everything except drivers runs outside the kernel. Applications are separate ELF binaries behind a syscall ABI (`kernel/SYSCALL_ABI.md`).
- **Own application format.** `.ntzc` packages, an SDK for third-party applications and a signing key flow (`sdk/`).
- **Own storage stack.** A VFS layer with FAT32, exFAT, EROFS and in-memory filesystems, plus real USB mass storage detection.
- **Device profiles.** Hardware ports live in one folder per device, so a new phone or board is a new folder, not a change to the kernel.
- **Documented by directory.** Source files carry no inline comments. Each directory has a `README.md` that explains what the component does, what it depends on and who uses it.

## Status

| Area | State |
| --- | --- |
| x86_64 BIOS boot: stage 1, stage 2, long mode, VBE graphics | Working in QEMU and VirtualBox |
| Startup animation, setup wizard, desktop with windows and system apps | Working in QEMU |
| x86_64 UEFI loader | In progress |
| Real USB mass storage detection and mounting (QEMU passthrough) | Working |
| Networking (e1000, DHCP, DNS, TCP, HTTP) and the web app | In progress. HTTPS is not available yet |
| AArch64 boot on QEMU: UART log, framebuffer, virtio block, input | Working |
| AArch64 writable `/data` (exFAT) in QEMU | Working |
| AArch64 EL0 applications (Files, Settings, package installer) | In progress |
| Infinix Hot 20 (X6826C) drivers | Written, **not yet validated on hardware** |
| Wi-Fi, Bluetooth, audio, camera, cellular, GPU acceleration | Not implemented |

## Quick start

### Run a prebuilt image

Download the ISO attached to the [latest release](../../releases/latest) and start it in QEMU:

```sh
qemu-system-x86_64 -m 1G -vga std -cdrom netizencore-qemu.iso
```

### Build from source

On Debian or Ubuntu:

```sh
sudo apt install nasm xorriso binutils qemu-system-x86 zip
```

Install Rust through [`rustup`](https://rustup.rs). The build script adds the `x86_64-unknown-none` target automatically. The first build downloads a PNG-processing library and its dependencies, so it needs network access.

| Command | Result |
| --- | --- |
| `bash build.sh` | QEMU ISO at `build/netizencore-qemu.iso` |
| `bash build.sh generic` | Generic ISO at `build/netizencore-generic.iso` |
| `bash build.sh all` | Both ISOs |
| `bash build.sh run` | Build the QEMU ISO and start QEMU |
| `bash build.sh vbox` | Build the generic ISO and start VirtualBox |
| `bash build.sh clean` | Remove `build/` |

QEMU uses KVM automatically when `/dev/kvm` is accessible. Without KVM the startup animation uses fewer frames but keeps its total duration.

The build modules are documented in [`tools/build/`](tools/build).

## How it boots

On x86_64 with BIOS:

1. The BIOS loads **stage 1** from the ISO. Stage 1 loads **stage 2** and the kernel into memory. The UEFI path is handled by the loader in `bootloader/uefi`.
2. **Stage 2** checks for 64-bit support, reads the E820 memory map, selects a VBE graphics mode, builds page tables for the first 4 GiB and switches the CPU into long mode.
3. The **kernel** brings up the display, calibrates its clock and plays the startup animation.
4. If the system is not installed, the **setup wizard** opens. Otherwise the **desktop** opens.
5. The desktop provides a wallpaper, icons, a task bar, a start menu and draggable windows. Applications (Files, Settings, System, Stopwatch, Browser, Terminal and others) share one contract (`crates/ui/nc-app`) and can run as separate Ring 3 ELF processes.

On AArch64 the kernel in `aarch64kernel/` is started by the platform firmware (or by the Android bootloader on phones), reads the device tree and then follows the same path to the shell.

## Repository layout

Each top-level directory has one responsibility. Each one has its own `README.md`.

| Directory | Contents |
| --- | --- |
| `bootloader/` | `bios/` (stage 1, stage 2, MBR in Assembly) and `uefi/` (UEFI loader crate) |
| `kernel/` | x86_64 kernel: `src/boot`, `src/tasking`, `src/ring3`, embedded drivers and `SYSCALL_ABI.md` |
| `aarch64kernel/` | AArch64 kernel for arm64, armv8a and armv9a |
| `crates/foundation/` | Contracts and dependency-free libraries: HAL, ABI, math, crypto, inflate, i18n |
| `crates/hardware/` | x86 architecture support, drivers, input, storage, platforms |
| `crates/services/` | Kernel services: memory, filesystem, network |
| `crates/graphics/` | Canvas, font rendering, media decoders, logo source |
| `crates/ui/` | UI toolkit, application contract, splash, setup wizard, desktop |
| `crates/runtime/` | Ring 3 runtime: permissions, ELF loading, packages, SDK host, backends |
| `apps/` | Applications. Each app has its own folder with a logic crate and a Ring 3 entry binary. `registry/` lists them |
| `packages/` | Packaged applications |
| `sdk/` | Tools for third-party developers: `.ntzc` packaging, signing key, examples |
| `aarch64crates/` | Everything specific to AArch64: `hardware/` (architecture and device abstraction), `runtime/`, `drivers/` (one folder per device), `apps/` (EL0 apps), `tools/` (build helpers) |
| `tools/` | `build/` (build.sh modules), `check/` (layout checks), `docs/` (documentation generator), `gen/`, `model/` |
| `assets/` | Files embedded at build time (logo, sounds) |
| `docs/` | `architecture/`, `ring3/`, `security/`, `changelog/`, `migration/` |

The module dependency graph is described in [`docs/architecture/`](docs/architecture). The layout is deliberately modular: working on one component only requires that component's folder and the READMEs of its dependencies.

## Platforms and build profiles

### x86_64

| Profile | Target | Resolution | Debug output |
| --- | --- | --- | --- |
| `qemu` | QEMU virtual hardware | 1280 × 720 | Serial port, debugcon, `isa-debug-exit` |
| `generic` | General-purpose PCs and VirtualBox | 1024 × 768 | Serial port, when available |

### AArch64

| Target | Meaning | UI |
| --- | --- | --- |
| `arm64` | Standard ARM PCs | Same desktop as x86_64 |
| `armv8a` | Phones and tablets | Mobile shell, same kernel |
| `armv9a` | Newer ARM cores | Postponed |

On QEMU the AArch64 drivers are generic (virtio). Real devices are added as profiles under `aarch64crates/drivers/<device>`.

The build command for a phone image takes the architecture, the packaging target and the device:

```sh
bash build.sh armv8a android x6826c
```

With the `android` target the kernel is packaged as `boot.img` (Android boot image v2) and the system as `super.img`, including the drivers of the named device.

## Userland and security model

- Everything except drivers runs in user mode: Ring 3 on x86_64, EL0 on AArch64.
- Each application is its own sandboxed process, started with a normal tap or click.
- **System applications** (Files, Settings, the package installer) hold privileged permissions. Applications installed later receive normal user permissions.
- Applications are distributed as `.ntzc` packages. The SDK in `sdk/` provides packaging, signing and examples.
- On the mobile shell, navigation animations (back and leaving an app) belong to the shell, not to the applications.

The security model and the syscall ABI are documented in `docs/security/` and `kernel/SYSCALL_ABI.md`. To report a vulnerability, see [SECURITY.md](SECURITY.md).

## Hardware port: Infinix Hot 20 (X6826C)

The first real-hardware target is the Infinix Hot 20 (MediaTek Helio G85 / MT6768, ARMv8-A). The device profile lives in `aarch64crates/drivers/x6826c` and contains a device tree, a driver table and modular `no_std` Rust drivers.

| Driver | Block | Scope |
| --- | --- | --- |
| `wdt` | MediaTek TOPRGU watchdog | Disable at boot, warm reset |
| `display` | OVL0 | Uses the framebuffer prepared by the bootloader (720 × 1640, BGRX); does not initialize the panel |
| `pwrap` | PMIC wrapper (MT6358) | Register read and write, voltage readout |
| `cpufreq` | APMIXED PLLs | Clock readout and big-cluster frequency control |
| `emmc` | MSDC0 | Read-only PIO block driver, GPT parsing, exposes `super` and `userdata` |
| `touch` | SPI2, TD4160 | TCM touch controller with firmware loader |
| `keys` | Keypad and PMIC power key | Volume and power keys with debouncing |
| `scp` | Sensor hub | Status and wake handling |
| `mali` | Mali-G52 | Power domains and core bring-up only |
| `connsys` | MT6631 | Power domain and supply control only |

**Status: written, not yet tested on a device.** The drivers rely on the bootloader having initialized clocks, the eMMC controller and the display. Proprietary vendor firmware is not part of this repository and must be extracted from your own device.

> **Warning.** Do not flash experimental images onto a device you cannot recover. Prefer booting from RAM and keep a backup of the stock firmware.

## Debugging

On QEMU, NetizenCore exposes several output channels:

- **Serial output** in the terminal
- **Debug console** written to `build/debugcon.log`
- **Kernel panics** sent to both serial and debugcon

On AArch64 the kernel logs to the UART and to the framebuffer.

## Documentation

- Every directory has a `README.md` with its responsibilities, dependencies and users.
- Architecture, memory maps and module information: `docs/architecture/`.
- Ring 3 runtime and syscall interface: `docs/ring3/` and `kernel/SYSCALL_ABI.md`.
- Security notes: `docs/security/`.
- Release notes: `docs/changelog/`. Migration notes: `docs/migration/`.
- User interface design: `crates/ui/README.md`.

Source files intentionally contain no inline comments. The READMEs are the documentation.

## Contributing

Issues and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md) first. Bug reports should use the issue template and include the build profile, the emulator or device, and the serial or debugcon log.

## License

NetizenCore is licensed under the **GNU Affero General Public License v3.0**. See [LICENSE](LICENSE).
