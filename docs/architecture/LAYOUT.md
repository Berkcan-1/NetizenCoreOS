# Klasör düzeni ve katman kuralı

## Amaç

Bir iş üzerinde çalışırken (özellikle yapay zekâ araçlarıyla) yalnızca ilgili klasörü vermek yeterli olsun. Her klasör tek bir
sorumluluğu taşır; klasör adı, içindeki crate'in ne yaptığını söyler.

## Katmanlar

    foundation ← hardware ← services ← graphics ← ui ← runtime ← apps ← kernel

| Katman | Yol | Ne içerir |
|---|---|---|
| foundation | `crates/foundation/` | Sözleşmeler (`nc-hal`, `nc-abi`, `nc-efra`) ve bağımsız kütüphaneler (`nc-math`, `nc-crypto`, `nc-inflate`, `nc-i18n`) |
| hardware | `crates/hardware/` | `nc-arch`, `nc-driver`, `nc-input`, `nc-storage`, `nc-platform-*` |
| services | `crates/services/` | `nc-mem`, `nc-fs`, `nc-net` |
| graphics | `crates/graphics/` | `nc-gfx`, `nc-font`, `nc-media`, `nc-assets` |
| ui | `crates/ui/` | `nc-ui`, `nc-app`, `nc-splash`, `nc-setup`, `nc-desktop` |
| runtime | `crates/runtime/` | `nc-caps`, `nc-elf`, `nc-pkg`, `nc-app-sdk`, `nc-app-host`, `nc-*-facade` |
| apps | `apps/<ad>/` | Uygulama mantığı + Ring 3 giriş ikilisi; `apps/registry/nc-apps` yerleşik liste |
| kernel | `kernel/` | Hepsini birleştirir |
| bootloader | `bootloader/` | `bios/` (Assembly), `uefi/` (yalnızca foundation'a bağlanır) |

Kural: bir crate yalnızca **kendi katmanına ve altındakilere** bağlanır. `python3 tools/check/verify_layout.py` bunu denetler
ve ihlalde hata verir. Crate adları klasör adlarıyla aynıdır; yeniden düzenleme crate adlarını değiştirmedi.

## arm64 klasörleri

arm64'e ait her şey iki üst klasörde toplanır; x86_64 tarafı yukarıdaki düzende kalır.

| Yol | Katman | İçerik |
|---|---|---|
| `aarch64kernel/` | kernel | AArch64 çekirdeği (eski adıyla kernel altındaki aarch64 klasörü) |
| `aarch64crates/hardware/` | hardware | `nc-arch-aarch64`, `nc-device`, `nc-device-select` |
| `aarch64crates/drivers/` | hardware | cihaz klasörleri (`qemu-virt`, `x6826c`, `rpi4`, `_template`) |
| `aarch64crates/runtime/` | runtime | `nc-fs-policy`, `app-aarch64.ld` |
| `aarch64crates/apps/` | apps | EL0 ikilileri (`nc-home-r3`, `nc-files-v8a`, `nc-settings-v8a`) |
| `aarch64crates/tools/` | — | arm64 build betikleri, `select_device.py`, Android araçları |

Katman kuralı bu klasörlerde de geçerlidir ve `verify_layout.py` aynı denetimi yapar. `cfg(target_arch)` ile iki mimariyi taşıyan crate'ler (`nc-arch`, `nc-abi`, `nc-app-sdk`, ...) `crates/` altında kalır. Ayrıntı: `aarch64crates/README.md`.

## Yeni şey nereye konur

| Eklenen | Konum |
|---|---|
| Yeni uygulama | `apps/<ad>/nc-app-<ad>/` (+ Ring 3 için `apps/<ad>/nc-<ad>-r3/`), sonra `apps/registry/nc-apps`'e kayıt |
| Yeni sürücü / aygıt | `crates/hardware/` |
| Yeni platform | `crates/hardware/nc-platform-<ad>/` |
| Yeni çekirdek hizmeti | `crates/services/` |
| Yeni UI bileşeni | `crates/ui/nc-ui` (ortak) veya ilgili ekran crate'i |
| Yeni Ring 3 yeteneği | `crates/runtime/` (ve gerekirse tel biçimi için `crates/foundation/nc-abi`) |
| Çekirdek kodu | `kernel/src/boot` (açılış), `kernel/src/tasking` (süreç, syscall, IPC), `kernel/src/ring3` |

Yeni crate eklerken: klasörü oluştur, `Cargo.toml` yaz, kök `Cargo.toml` üye listesine ekle (glob kullanılmaz),
`tools/docs/modules.toml`'a özet yaz, `python3 tools/docs/sync_module_docs.py` ve `python3 tools/check/verify_layout.py` çalıştır.

## Çekirdeğin dokunma yüzeyi

Arayüz crate'lerini (`nc-desktop`, `nc-ui`, `nc-setup`, `nc-splash`) çekirdek yalnızca `kernel/src/main.rs` içindeki
`kernel_main`'de çağırır; `nc-ui` ayrıca `kernel/src/ring3/` içinde uzak çizim için kullanılır. Ayrıntılı tablo: `kernel/README.md`.
Bu crate'lerin genel (pub) API'sini değiştirmediğin sürece çekirdekte düzenleme gerekmez.

## Derleme dışı / üretilmiş şeyler

| Yol | Ne |
|---|---|
| `kernel/embedded/ring3-apps/` | `build.sh`'ın ürettiği Ring 3 ELF ve .gz dosyaları (çekirdeğe gömülür) |
| `kernel/embedded/ring3-selftest/` | Ring 3 kendini-sınama uygulaması (sabit test verisi; `gen.py` üretir) |
| `crates/hardware/nc-storage/mbr.bin` | `bootloader/bios/mbr.asm`'den `build.sh`'ın ürettiği MBR |
| `apps/browser/nc-app-browser/legacy/` | Arşiv; derlenmez |
| `sdk/legacy-ncbm/` | Eski .ncbm paket aracı; derleme dışı |
| `build/` | Derleme çıktısı (git'e girmez) |
