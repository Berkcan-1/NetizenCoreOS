# aarch64crates

NetizenCore'un yalnızca arm64 hedefine (`arm64`, `armv8a`, `armv9a`) ait her şeyi bu klasörde ve `../aarch64kernel/` içinde durur.
x86_64 tarafı yerinde kalır (`crates/`, `kernel/`, `apps/`, `bootloader/`); iki mimariyi birlikte taşıyan ortak crate'ler de `crates/` altındadır.

## Klasörler

| Klasör | İçerik |
|---|---|
| `hardware/` | `nc-arch-aarch64` (UART, zamanlayıcı, MMU, istisna vektörleri, PSCI, fw_cfg), `nc-device` (DeviceProfile sözleşmesi), `nc-device-select` (seçilen cihazı çekirdeğe bağlayan, `build.sh`'ın yeniden yazdığı crate) |
| `runtime/` | `nc-fs-policy` (dosya yolu erişim politikası; yalnızca aarch64 çekirdeği kullanır) ve `app-aarch64.ld` (EL0 uygulamalarının bağlayıcı betiği) |
| `drivers/` | Cihaz katmanı: `qemu-virt`, `x6826c`, `rpi4`, şablon `_template`; her biri `device.conf` + profil crate'i (ayrıntı: `drivers/README.md`) |
| `apps/` | EL0 ikilileri: `home/nc-home-r3` (NetizenHome), `files/nc-files-v8a`, `settings/nc-settings-v8a`. Ana workspace dışındadırlar; `build.sh` derler, Dosyalar ve Ayarlar `packages/mobile/` altına `.ntzc` olarak paketlenir |
| `tools/` | `aarch64.sh`, `qemu-aarch64.sh`, `android.sh` (`build.sh` bunları `source` eder), `mk-super.py`, `elf2bin.py`, `select_device.py`, `android/` (`mkbootimg.py`, `unpack_bootimg.py`) |

Çekirdek: `aarch64kernel/` (eski adıyla kernel altındaki aarch64 klasörü). Paket adı ve ikili adı değişmedi: `nc-kernel-aarch64` / `kernel-aarch64`.

## Yapay zekâya ne verilir

aarch64 işi için: `aarch64kernel/` + `aarch64crates/` + çekirdeğin bağlandığı ortak crate'lerin README dosyaları
(`nc-abi`, `nc-hal`, `nc-math`, `nc-arch`, `nc-mem`, `nc-fs`, `nc-gfx`, `nc-font`, `nc-ui`, `nc-splash`, `nc-caps`, `nc-elf`, `nc-pkg`, `nc-app-sdk`; hepsi `crates/` altında kalır).
Bu ortak crate'ler `cfg(target_arch)` ile iki mimariyi birlikte taşıdığı için bu klasöre alınmadı; hangilerine bağlandığını aşağıdaki tablo ve
`docs/architecture/DEPENDENCIES.md` gösterir.

## Kurallar

Katman kuralı aynıdır (`foundation ← hardware ← services ← graphics ← ui ← runtime ← apps ← kernel`): `aarch64crates/hardware` ve `drivers/` donanım katmanı,
`runtime/` çalışma zamanı katmanı, `apps/` uygulama katmanı, `aarch64kernel` çekirdek katmanıdır. `python3 tools/check/verify_layout.py` denetler.

| Eklenen | Konum |
|---|---|
| Yeni cihaz | `bash build.sh new-device <ad> [soc]` → `drivers/<ad>/` (kök `Cargo.toml` üyelerine kendiliğinden eklenir) |
| Yeni arm64 uygulaması | `apps/<ad>/nc-<ad>-v8a/` (kendi `[workspace]` bölümüyle, `app-aarch64.ld` bağlanır) + `tools/aarch64.sh` içine paketleme satırı |
| Yeni arm64 mimari kodu | `hardware/nc-arch-aarch64/` |
| Yeni arm64 çekirdek kodu | `../aarch64kernel/src/` |

Not: cihaz katmanı şimdilik yalnızca arm64 cihazlarını taşır; x86_64 cihaz sürücüleri henüz çekirdeğe bağlanmadığı için ayrı bir x86 cihaz klasörü yoktur.

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| Crate | Konum | Ne yapar | Bağımlılıkları | Kullananlar |
|---|---|---|---|---|
| [`nc-arch-aarch64`](hardware/nc-arch-aarch64/README.md) | `aarch64crates/hardware/nc-arch-aarch64` | AArch64 mimari katmanı: PL011/16550 UART, genel zamanlayıcı, istisna vektörleri, MMU, PSCI, fw_cfg ramfb | — | — |
| [`nc-dev-qemu-virt`](drivers/qemu-virt/README.md) | `aarch64crates/drivers/qemu-virt` | QEMU virt (aarch64) cihaz profili ve sürücü kayıtları | nc-device | nc-device-select |
| [`nc-dev-rpi4`](drivers/rpi4/README.md) | `aarch64crates/drivers/rpi4` | Raspberry Pi 4 (BCM2711) cihaz profili ve sürücü kayıtları | nc-device | — |
| [`nc-dev-x6826c`](drivers/x6826c/README.md) | `aarch64crates/drivers/x6826c` | Infinix Hot 20 (X6826C, MT6768) cihaz profili ve sürücü kayıtları | nc-device | — |
| [`nc-device`](hardware/nc-device/README.md) | `aarch64crates/hardware/nc-device` | Cihaz profili sözleşmesi (DeviceProfile, DriverEntry, DriverHost) | nc-hal | nc-dev-qemu-virt, nc-dev-rpi4, nc-dev-x6826c, nc-device-select, nc-kernel-aarch64 |
| [`nc-device-select`](hardware/nc-device-select/README.md) | `aarch64crates/hardware/nc-device-select` | Derleme sırasında seçilen cihazı çekirdeğe bağlayan üretilen crate | nc-dev-qemu-virt, nc-device | nc-kernel-aarch64 |
| [`nc-files-v8a`](apps/files/nc-files-v8a/README.md) | `aarch64crates/apps/files/nc-files-v8a` | Dosyalar (ARMv8-A EL0): /data (exFAT) ve /system (FAT32) VFS istemcisi, dosya inceleyici/düzenleyici ve paket kurucu | nc-abi, nc-app-sdk | — |
| [`nc-fs-policy`](runtime/nc-fs-policy/README.md) | `aarch64crates/runtime/nc-fs-policy` | Dosya yolu erişim politikası: uygulama kimliği + yetki bitleri + yol + işlem | nc-caps | nc-kernel-aarch64 |
| [`nc-home-r3`](apps/home/nc-home-r3/README.md) | `aarch64crates/apps/home/nc-home-r3` | NetizenHome: başlatıcı/kabuk, Ring 3 olay-tabanlı uygulama, açık tema | nc-abi, nc-app-sdk | — |
| [`nc-kernel-aarch64`](../aarch64kernel/README.md) | `aarch64kernel` | AArch64 çekirdeği: Image başlığı, EL2→EL1, DTB, MMU, EL0 uygulamaları, depolama, ana ekran | nc-abi, nc-arch, nc-caps, nc-device, nc-device-select, nc-elf, nc-font, nc-fs, nc-fs-policy, nc-gfx, nc-hal, nc-math, nc-mem, nc-pkg, nc-splash, nc-ui | — |
| [`nc-settings-v8a`](apps/settings/nc-settings-v8a/README.md) | `aarch64crates/apps/settings/nc-settings-v8a` | Ayarlar (ARMv8-A EL0): kalıcı görünüm/saat/dil ayarları (/data), depolama, yaşam döngüsü ve donanım bilgisi | nc-abi, nc-app-sdk | — |

`*` = isteğe bağlı (feature ile açılan) bağımlılık.
<!-- otomatik:bitti -->
