# Eski → yeni yol haritası

Klasör yeniden düzenlemesinde **crate adları, paket adları ve Rust kodu değişmedi**; yalnızca dosyaların konumu, `Cargo.toml` `path` değerleri,
gömülü dosya yolları ve betikler güncellendi. Kodda `use nc_desktop::...` gibi satırlar aynen geçerlidir.

## Crate'ler ve uygulamalar

| Eski | Yeni |
|---|---|
| `apps/browser` | `apps/browser/nc-browser-r3` |
| `apps/files` | `apps/files/nc-files-r3` |
| `apps/settings` | `apps/settings/nc-settings-r3` |
| `apps/stopwatch` | `apps/stopwatch/nc-stopwatch-r3` |
| `apps/system` | `apps/system/nc-system-r3` |
| `crates/nc-abi` | `crates/foundation/nc-abi` |
| `crates/nc-app` | `crates/ui/nc-app` |
| `crates/nc-app-browser` | `apps/browser/nc-app-browser` |
| `crates/nc-app-files` | `apps/files/nc-app-files` |
| `crates/nc-app-host` | `crates/runtime/nc-app-host` |
| `crates/nc-app-sdk` | `crates/runtime/nc-app-sdk` |
| `crates/nc-app-settings` | `apps/settings/nc-app-settings` |
| `crates/nc-app-stopwatch` | `apps/stopwatch/nc-app-stopwatch` |
| `crates/nc-app-system` | `apps/system/nc-app-system` |
| `crates/nc-apps` | `apps/registry/nc-apps` |
| `crates/nc-arch` | `crates/hardware/nc-arch` |
| `crates/nc-assets` | `crates/graphics/nc-assets` |
| `crates/nc-caps` | `crates/runtime/nc-caps` |
| `crates/nc-crypto` | `crates/foundation/nc-crypto` |
| `crates/nc-desktop` | `crates/ui/nc-desktop` |
| `crates/nc-driver` | `crates/hardware/nc-driver` |
| `crates/nc-efra` | `crates/foundation/nc-efra` |
| `crates/nc-elf` | `crates/runtime/nc-elf` |
| `crates/nc-font` | `crates/graphics/nc-font` |
| `crates/nc-fs` | `crates/services/nc-fs` |
| `crates/nc-fs-facade` | `crates/runtime/nc-fs-facade` |
| `crates/nc-gfx` | `crates/graphics/nc-gfx` |
| `crates/nc-hal` | `crates/foundation/nc-hal` |
| `crates/nc-i18n` | `crates/foundation/nc-i18n` |
| `crates/nc-inflate` | `crates/foundation/nc-inflate` |
| `crates/nc-input` | `crates/hardware/nc-input` |
| `crates/nc-js` | `apps/browser/nc-js` |
| `crates/nc-math` | `crates/foundation/nc-math` |
| `crates/nc-media` | `crates/graphics/nc-media` |
| `crates/nc-mem` | `crates/services/nc-mem` |
| `crates/nc-net` | `crates/services/nc-net` |
| `crates/nc-net-facade` | `crates/runtime/nc-net-facade` |
| `crates/nc-pkg` | `crates/runtime/nc-pkg` |
| `crates/nc-platform-generic` | `crates/hardware/nc-platform-generic` |
| `crates/nc-platform-qemu` | `crates/hardware/nc-platform-qemu` |
| `crates/nc-r3-stopwatch` | `apps/stopwatch/nc-r3-stopwatch` |
| `crates/nc-setup` | `crates/ui/nc-setup` |
| `crates/nc-splash` | `crates/ui/nc-splash` |
| `crates/nc-storage` | `crates/hardware/nc-storage` |
| `crates/nc-sys-facade` | `crates/runtime/nc-sys-facade` |
| `crates/nc-uefi` | `bootloader/uefi` |
| `crates/nc-ui` | `crates/ui/nc-ui` |

`apps/<ad>` (eski Ring 3 giriş crate'leri) artık `apps/<ad>/nc-<ad>-r3` altındadır; mantık crate'leri aynı `apps/<ad>/` klasörüne taşındı.

## Klasörler

| Eski | Yeni |
|---|---|
| `kernel/faz6` | `kernel/embedded/ring3-selftest` |
| `kernel/ring3_apps` | `kernel/embedded/ring3-apps` |

## Tek dosyalar

| Eski | Yeni |
|---|---|
| `bootloader/mbr.asm` | `bootloader/bios/mbr.asm` |
| `bootloader/stage1.asm` | `bootloader/bios/stage1.asm` |
| `bootloader/stage2.asm` | `bootloader/bios/stage2.asm` |
| `docs/ARCHITECTURE.md` | `docs/architecture/ARCHITECTURE.md` |
| `docs/ASAMA3_NOTLARI.md` | `docs/changelog/ASAMA3_NOTLARI.md` |
| `docs/GUNCELLEME_NOTLARI.md` | `docs/changelog/GUNCELLEME_NOTLARI.md` |
| `docs/GUVENLIK_DUZELTMELERI.md` | `docs/security/GUVENLIK_DUZELTMELERI.md` |
| `docs/MODULES.md` | `docs/architecture/MODULES.md` |
| `docs/RING3_YOL_HARITASI.md` | `docs/ring3/RING3_YOL_HARITASI.md` |
| `docs/yazi-tipi-onizleme.png` | `docs/assets/yazi-tipi-onizleme.png` |
| `kernel/src/devices.rs` | `kernel/src/boot/devices.rs` |
| `kernel/src/entry.rs` | `kernel/src/boot/entry.rs` |
| `kernel/src/ipc.rs` | `kernel/src/tasking/ipc.rs` |
| `kernel/src/platform.rs` | `kernel/src/boot/platform.rs` |
| `kernel/src/prepare.rs` | `kernel/src/boot/prepare.rs` |
| `kernel/src/process.rs` | `kernel/src/tasking/process.rs` |
| `kernel/src/ring3.rs` | `kernel/src/ring3/runtime.rs` |
| `kernel/src/ring3_app.rs` | `kernel/src/ring3/apps.rs` |
| `kernel/src/ring3_net.rs` | `kernel/src/ring3/net.rs` |
| `kernel/src/syscall.rs` | `kernel/src/tasking/syscall.rs` |
| `tools/gen_r3_strings.py` | `tools/gen/gen_r3_strings.py` |
| `tools/ring3-cargo.sh` | `tools/build/ring3-cargo.sh` |

## Değişmeyenler

`kernel/Cargo.toml`, `kernel/build.rs`, `kernel/linker.ld`, `kernel/SYSCALL_ABI.md`, `assets/`, `sdk/` (iç yapısı), `host_usb/`,
`build.sh` komutları (`bash build.sh`, `run`, `vbox`, `all`, `generic`, `clean`, `drivers`) ve tüm crate/paket adları.

## Yapısal değişiklikler (taşımadan başka)

- `build.sh` işlevleri `tools/build/*.sh` modüllerine bölündü (işlev gövdeleri aynı; yalnızca yollar güncellendi).
- `kernel/src/main.rs` içindeki `mod` bildirimleri `#[path = "..."]` ile alt klasörlere bağlandı; modül adları aynı kaldı.
- Kök `Cargo.toml` üye listesi katman başlıklarıyla düzenlendi; `nc-crypto` ve `nc-i18n` (önceden yalnızca örtük üye) açıkça eklendi.
- `nc-font/tools/build_outline.py`, `tools/gen/gen_r3_strings.py` ve `tools/model/elf_abi_model.py` içindeki göreli yollar güncellendi.
- Tarihsel kayıtlar (`docs/changelog/`) olduğu gibi bırakıldı; içindeki yollar eski düzene aittir.

## arm64 klasörlerinin ayrılması

arm64'e ait her şey `aarch64kernel/` ve `aarch64crates/` altına taşındı; x86_64 tarafı yerinde kaldı. Crate adları, paket adları ve Rust kodu değişmedi.

| Eski | Yeni |
|---|---|
| `kernel/aarch64` | `aarch64kernel` |
| `crates/hardware/nc-arch-aarch64` | `aarch64crates/hardware/nc-arch-aarch64` |
| `crates/hardware/nc-device` | `aarch64crates/hardware/nc-device` |
| `crates/hardware/nc-device-select` | `aarch64crates/hardware/nc-device-select` |
| `crates/runtime/nc-fs-policy` | `aarch64crates/runtime/nc-fs-policy` |
| `crates/runtime/nc-app-sdk/app-aarch64.ld` | `aarch64crates/runtime/app-aarch64.ld` |
| `drivers/` | `aarch64crates/drivers/` |
| `apps/home` | `aarch64crates/apps/home` |
| `apps/files/nc-files-v8a` | `aarch64crates/apps/files/nc-files-v8a` |
| `apps/settings/nc-settings-v8a` | `aarch64crates/apps/settings/nc-settings-v8a` |
| `tools/build/aarch64.sh`, `qemu-aarch64.sh`, `android.sh`, `mk-super.py`, `elf2bin.py` | `aarch64crates/tools/` |
| `tools/gen/select_device.py` | `aarch64crates/tools/select_device.py` |
| `tools/android/` | `aarch64crates/tools/android/` |

Diğer değişiklikler: `build.sh` arm64 modüllerini `aarch64crates/tools/` altından `source` eder; `tools/check/verify_layout.py` ve `tools/docs/sync_module_docs.py` yeni klasörleri aynı katman kuralıyla tanır;
x86_64 `bash build.sh drivers` komutunun ürettiği `generic-display.efra.src` artık `build/drivers/` altına yazılır (eskiden kökteki `drivers/` klasörüne yazılıyordu).

