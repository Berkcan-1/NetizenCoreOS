# Bağımlılık haritası

> Bu dosya `python3 tools/docs/sync_module_docs.py` ile üretilir; elle düzenlemeyin. Özetler `tools/docs/modules.toml` içindedir.

Kural: bir crate yalnızca **kendi katmanındaki veya altındaki** crate'lere bağlanır. `python3 tools/check/verify_layout.py` bunu denetler.

```
foundation ← hardware ← services ← graphics ← ui ← runtime ← apps ← kernel
```

## Temel (`crates/foundation`)

| Crate | Konum | Bağımlılık | Kullanan |
|---|---|---|---|
| `nc-abi` | `crates/foundation/nc-abi` | — | nc-app-host, nc-app-sdk, nc-browser-r3, nc-files-r3, nc-files-v8a, nc-home-r3, nc-kernel-aarch64, nc-pkg, nc-r3-stopwatch, nc-settings-r3, nc-settings-v8a, nc-stopwatch-r3, nc-sys-facade, nc-system-r3, nc-fs-facade*, nc-kernel*, nc-net-facade*, nc-ui* |
| `nc-crypto` | `crates/foundation/nc-crypto` | — | nc-app, nc-storage |
| `nc-efra` | `crates/foundation/nc-efra` | — | nc-app-settings, nc-driver, nc-sys-facade |
| `nc-hal` | `crates/foundation/nc-hal` | — | nc-app, nc-audio, nc-desktop, nc-device, nc-driver, nc-fs, nc-gfx, nc-input, nc-kernel, nc-kernel-aarch64, nc-media, nc-net, nc-platform-generic, nc-platform-qemu, nc-setup, nc-storage, nc-uefi |
| `nc-i18n` | `crates/foundation/nc-i18n` | — | nc-app |
| `nc-inflate` | `crates/foundation/nc-inflate` | — | nc-kernel* |
| `nc-math` | `crates/foundation/nc-math` | — | nc-desktop, nc-font, nc-kernel, nc-kernel-aarch64, nc-splash, nc-ui |

## Donanım (`crates/hardware`)

| Crate | Konum | Bağımlılık | Kullanan |
|---|---|---|---|
| `nc-arch` | `crates/hardware/nc-arch` | — | nc-apps, nc-driver, nc-input, nc-kernel, nc-kernel-aarch64, nc-net, nc-platform-generic, nc-platform-qemu, nc-storage, nc-sys-facade* |
| `nc-arch-aarch64` | `aarch64crates/hardware/nc-arch-aarch64` | — | — |
| `nc-arch-x86_64` | `crates/hardware/nc-arch-x86_64` | — | — |
| `nc-dev-qemu-virt` | `aarch64crates/drivers/qemu-virt` | nc-device | nc-device-select |
| `nc-dev-rpi4` | `aarch64crates/drivers/rpi4` | nc-device | — |
| `nc-dev-x6826c` | `aarch64crates/drivers/x6826c` | nc-device | — |
| `nc-device` | `aarch64crates/hardware/nc-device` | nc-hal | nc-dev-qemu-virt, nc-dev-rpi4, nc-dev-x6826c, nc-device-select, nc-kernel-aarch64 |
| `nc-device-select` | `aarch64crates/hardware/nc-device-select` | nc-dev-qemu-virt, nc-device | nc-kernel-aarch64 |
| `nc-driver` | `crates/hardware/nc-driver` | nc-arch, nc-efra, nc-hal | nc-audio, nc-input, nc-kernel, nc-net, nc-setup, nc-storage, nc-sys-facade* |
| `nc-input` | `crates/hardware/nc-input` | nc-arch, nc-driver, nc-hal | nc-kernel |
| `nc-platform-generic` | `crates/hardware/nc-platform-generic` | nc-arch, nc-hal | nc-kernel* |
| `nc-platform-qemu` | `crates/hardware/nc-platform-qemu` | nc-arch, nc-hal | nc-kernel* |
| `nc-storage` | `crates/hardware/nc-storage` | nc-arch, nc-crypto, nc-driver, nc-hal | nc-kernel, nc-fs* |

## Çekirdek hizmetleri (`crates/services`)

| Crate | Konum | Bağımlılık | Kullanan |
|---|---|---|---|
| `nc-fs` | `crates/services/nc-fs` | nc-hal, nc-storage* | nc-apps, nc-desktop, nc-kernel, nc-kernel-aarch64, nc-fs-facade* |
| `nc-mem` | `crates/services/nc-mem` | — | nc-kernel, nc-kernel-aarch64, nc-net, nc-splash |
| `nc-net` | `crates/services/nc-net` | nc-arch, nc-driver, nc-hal, nc-mem | nc-kernel, nc-net-facade*, nc-sys-facade* |

## Grafik (`crates/graphics`)

| Crate | Konum | Bağımlılık | Kullanan |
|---|---|---|---|
| `nc-assets` | `crates/graphics/nc-assets` | — | nc-audio, nc-desktop, nc-splash |
| `nc-audio` | `crates/graphics/nc-audio` | nc-assets, nc-driver, nc-hal | nc-desktop, nc-kernel, nc-setup |
| `nc-font` | `crates/graphics/nc-font` | nc-math | nc-app, nc-app-browser, nc-app-files, nc-app-settings, nc-app-stopwatch, nc-app-system, nc-desktop, nc-kernel-aarch64, nc-setup, nc-splash, nc-ui |
| `nc-gfx` | `crates/graphics/nc-gfx` | nc-hal | nc-app, nc-app-browser, nc-app-files, nc-app-host, nc-app-settings, nc-app-stopwatch, nc-app-system, nc-apps, nc-desktop, nc-kernel, nc-kernel-aarch64, nc-settings-r3, nc-setup, nc-splash, nc-ui |
| `nc-media` | `crates/graphics/nc-media` | nc-hal | nc-app-files, nc-desktop |

## Arayüz (`crates/ui`)

| Crate | Konum | Bağımlılık | Kullanan |
|---|---|---|---|
| `nc-app` | `crates/ui/nc-app` | nc-crypto, nc-font, nc-gfx, nc-hal, nc-i18n, nc-ui | nc-app-browser, nc-app-files, nc-app-host, nc-app-settings, nc-app-stopwatch, nc-app-system, nc-apps, nc-browser-r3, nc-desktop, nc-files-r3, nc-pkg, nc-settings-r3, nc-system-r3, nc-kernel*, nc-sys-facade* |
| `nc-desktop` | `crates/ui/nc-desktop` | nc-app, nc-assets, nc-audio, nc-font, nc-fs, nc-gfx, nc-hal, nc-math, nc-media, nc-ui | nc-kernel |
| `nc-setup` | `crates/ui/nc-setup` | nc-audio, nc-driver, nc-font, nc-gfx, nc-hal, nc-ui | nc-kernel |
| `nc-splash` | `crates/ui/nc-splash` | nc-assets, nc-font, nc-gfx, nc-math, nc-mem, nc-ui | nc-kernel, nc-kernel-aarch64 |
| `nc-ui` | `crates/ui/nc-ui` | nc-font, nc-gfx, nc-math, nc-abi* | nc-app, nc-app-browser, nc-app-files, nc-app-settings, nc-app-stopwatch, nc-app-system, nc-desktop, nc-kernel, nc-kernel-aarch64, nc-setup, nc-splash |

## Ring 3 çalışma zamanı (`crates/runtime`)

| Crate | Konum | Bağımlılık | Kullanan |
|---|---|---|---|
| `nc-app-host` | `crates/runtime/nc-app-host` | nc-abi, nc-app, nc-app-sdk, nc-gfx | nc-browser-r3, nc-files-r3, nc-settings-r3, nc-system-r3, nc-kernel* |
| `nc-app-sdk` | `crates/runtime/nc-app-sdk` | nc-abi | nc-app-host, nc-browser-r3, nc-files-r3, nc-files-v8a, nc-home-r3, nc-r3-stopwatch, nc-settings-r3, nc-settings-v8a, nc-stopwatch-r3, nc-system-r3, nc-fs-facade*, nc-net-facade*, nc-sys-facade* |
| `nc-caps` | `crates/runtime/nc-caps` | — | nc-fs-policy, nc-kernel, nc-kernel-aarch64, nc-pkg |
| `nc-elf` | `crates/runtime/nc-elf` | — | nc-kernel-aarch64, nc-pkg, nc-kernel* |
| `nc-fs-facade` | `crates/runtime/nc-fs-facade` | nc-abi*, nc-app-sdk*, nc-fs* | nc-app-files, nc-app-settings, nc-files-r3, nc-kernel, nc-settings-r3 |
| `nc-fs-policy` | `aarch64crates/runtime/nc-fs-policy` | nc-caps | nc-kernel-aarch64 |
| `nc-net-facade` | `crates/runtime/nc-net-facade` | nc-abi*, nc-app-sdk*, nc-net* | nc-app-browser, nc-browser-r3 |
| `nc-pkg` | `crates/runtime/nc-pkg` | nc-abi, nc-app, nc-caps, nc-elf | nc-app-files, nc-apps, nc-kernel-aarch64, nc-kernel* |
| `nc-sys-facade` | `crates/runtime/nc-sys-facade` | nc-abi, nc-efra, nc-app*, nc-app-sdk*, nc-arch*, nc-driver*, nc-net* | nc-app-settings, nc-settings-r3, nc-kernel* |

## Uygulamalar (`apps`)

| Crate | Konum | Bağımlılık | Kullanan |
|---|---|---|---|
| `nc-app-browser` | `apps/browser/nc-app-browser` | nc-app, nc-font, nc-gfx, nc-js, nc-net-facade, nc-ui | nc-browser-r3, nc-apps* |
| `nc-app-files` | `apps/files/nc-app-files` | nc-app, nc-font, nc-fs-facade, nc-gfx, nc-media, nc-pkg, nc-ui, nc-app-settings*, nc-app-stopwatch*, nc-app-system* | nc-apps, nc-files-r3 |
| `nc-app-settings` | `apps/settings/nc-app-settings` | nc-app, nc-efra, nc-font, nc-fs-facade, nc-gfx, nc-sys-facade, nc-ui | nc-settings-r3, nc-app-files*, nc-apps* |
| `nc-app-stopwatch` | `apps/stopwatch/nc-app-stopwatch` | nc-app, nc-font, nc-gfx, nc-ui | nc-app-files*, nc-apps* |
| `nc-app-system` | `apps/system/nc-app-system` | nc-app, nc-font, nc-gfx, nc-ui | nc-system-r3, nc-app-files*, nc-apps* |
| `nc-apps` | `apps/registry/nc-apps` | nc-app, nc-app-files, nc-arch, nc-fs, nc-gfx, nc-pkg, nc-app-browser*, nc-app-settings*, nc-app-stopwatch*, nc-app-system* | nc-kernel |
| `nc-browser-r3` | `apps/browser/nc-browser-r3` | nc-abi, nc-app, nc-app-browser, nc-app-host, nc-app-sdk, nc-net-facade | — |
| `nc-files-r3` | `apps/files/nc-files-r3` | nc-abi, nc-app, nc-app-files, nc-app-host, nc-app-sdk, nc-fs-facade | — |
| `nc-files-v8a` | `aarch64crates/apps/files/nc-files-v8a` | nc-abi, nc-app-sdk | — |
| `nc-home-r3` | `aarch64crates/apps/home/nc-home-r3` | nc-abi, nc-app-sdk | — |
| `nc-js` | `apps/browser/nc-js` | — | nc-app-browser |
| `nc-r3-stopwatch` | `apps/stopwatch/nc-r3-stopwatch` | nc-abi, nc-app-sdk | nc-stopwatch-r3 |
| `nc-settings-r3` | `apps/settings/nc-settings-r3` | nc-abi, nc-app, nc-app-host, nc-app-sdk, nc-app-settings, nc-fs-facade, nc-gfx, nc-sys-facade | — |
| `nc-settings-v8a` | `aarch64crates/apps/settings/nc-settings-v8a` | nc-abi, nc-app-sdk | — |
| `nc-stopwatch-r3` | `apps/stopwatch/nc-stopwatch-r3` | nc-abi, nc-app-sdk, nc-r3-stopwatch | — |
| `nc-system-r3` | `apps/system/nc-system-r3` | nc-abi, nc-app, nc-app-host, nc-app-sdk, nc-app-system | — |

## Önyükleyici (`bootloader`)

| Crate | Konum | Bağımlılık | Kullanan |
|---|---|---|---|
| `nc-uefi` | `bootloader/uefi` | nc-hal | — |

## Çekirdek (`kernel`)

| Crate | Konum | Bağımlılık | Kullanan |
|---|---|---|---|
| `nc-kernel` | `kernel` | nc-apps, nc-arch, nc-audio, nc-caps, nc-desktop, nc-driver, nc-fs, nc-fs-facade, nc-gfx, nc-hal, nc-input, nc-math, nc-mem, nc-net, nc-setup, nc-splash, nc-storage, nc-ui, nc-abi*, nc-app*, nc-app-host*, nc-elf*, nc-inflate*, nc-pkg*, nc-platform-generic*, nc-platform-qemu*, nc-sys-facade* | — |
| `nc-kernel-aarch64` | `aarch64kernel` | nc-abi, nc-arch, nc-caps, nc-device, nc-device-select, nc-elf, nc-font, nc-fs, nc-fs-policy, nc-gfx, nc-hal, nc-math, nc-mem, nc-pkg, nc-splash, nc-ui | — |

`*` = isteğe bağlı (feature ile açılan) bağımlılık. Yalnızca test bağımlılıkları bu tabloya yazılmaz.
