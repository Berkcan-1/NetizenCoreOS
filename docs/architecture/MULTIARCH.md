# Çok mimarili yapı (x86_64, arm64, armv8a, armv9a)

## Mimari adları

| Ad | Anlamı | Arayüz profili | Hedef |
|---|---|---|---|
| `x86_64` | Mevcut PC hedefi | masaüstü | BIOS/UEFI ISO |
| `arm64` | Standart ARM PC/kart (UEFI, Raspberry Pi) | masaüstü | `Image` / `kernel8.img` |
| `armv8a` | ARMv8-A telefon ve tabletler | mobil | `Image`, `boot.img`, `super.img` |
| `armv9a` | ARMv9-A (ertelenebilir) | mobil | aynı; yalnızca `+v9a` derleyici bayrağı |

Çekirdek aynıdır; fark (a) cihaz profili/sürücüleri, (b) arayüz profili (`ui-desktop` / `ui-mobile` özelliği), (c) paketlemedir.

## Katmanlar

    kernel/                                   x86_64 çekirdeği (mevcut)
    aarch64kernel/                            AArch64 çekirdeği (Image başlığı, DTB, ekran, açılış)
    crates/hardware/nc-arch                   mimari cephesi (cfg ile seçer)
    crates/hardware/nc-arch-x86_64            eski nc-arch
    aarch64crates/hardware/nc-arch-aarch64    UART, zamanlayıcı, MMU, istisna, PSCI, fw_cfg
    aarch64crates/hardware/nc-device          DeviceProfile sözleşmesi
    aarch64crates/hardware/nc-device-select   seçilen cihaz (build.sh yeniden yazar)
    aarch64crates/drivers/<cihaz>/                   cihaza özel profil + sürücüler + device.conf
    packages/<ui>/                     ayrı .ntzc sistem paketleri

## Derleme komutları

    bash build.sh devices
    bash build.sh x86_64 qemu run
    bash build.sh armv8a qemu run --mem 1024 --smp 2 [--gdb] [--headless] [-- ham qemu argümanları]
    bash build.sh armv8a android x6826c
    bash build.sh unpack-boot stok-boot.img
    bash build.sh new-device <ad> [soc]

## aarch64 açılış sözleşmesi

Çekirdek Linux arm64 `Image` başlığıyla başlar (64 bayt, imza `ARM\x64`, `text_offset = kernel_base & 0x1fffff`). Böylece QEMU `-kernel`,
U-Boot, Android LK ve Pi firmware'i aynı dosyayı yükler. Giriş: MMU kapalı, `x0` = DTB adresi, EL2 ya da EL1. `entry.rs` EL2'den
EL1'e iner, FP/NEON'u açar, BSS'yi sıfırlar. Bağlama adresi `device.conf: kernel_base` (örn. `0x40080000`; Pi'de `0x80000`).
Çekirdek `+strict-align` ile derlenir. RAM = DTB `/memory` (ya da profil `ram`), ekran = profil `display.source`.

## Aşamalar

**Aşama 1 (bu sürüm):** build sistemi, cihaz katmanı, boot.img/super.img, QEMU parametreleri, aarch64 açılış çekirdeği
(UART günlüğü, MMU, DTB, framebuffer, splash, ana ekran yer tutucusu).

**Aşama 2 — paylaşılan masaüstünü aarch64'e taşıma.** Engelleyiciler (hepsi x86'ya bağlı, `#[cfg(target_arch = "x86_64")]` ile
ayrılmalı, aarch64 karşılıkları cihaz sürücülerinden gelmeli):

| Crate | x86'ya bağlı kısım | aarch64 karşılığı |
|---|---|---|
| `nc-driver` | `pci`, `ps2`, `ac97`, `dma` | MMIO sürücü kayıt defteri (`nc-device`), SoC ses sürücüsü |
| `nc-storage` | ATA/AHCI/NVMe (port G/Ç, PCI) | eMMC/SD (MSDC, SDHCI), UFS, virtio-blk |
| `nc-input` | PS/2, xHCI | dokunmatik (I2C/SPI), tuşlar, virtio-input |
| `nc-net` | e1000 | virtio-net, Wi-Fi SoC sürücüsü |
| `nc-audio` | AC'97 | cihaz ses sürücüsü |
| `nc-mem` | `paging`, `kpt` (zaten `cfg(x86_64)` arkasında) | aarch64 sayfa tabloları + kullanıcı adres uzayı (Ring 3 için) |
| `nc-sys-facade`, `nc-app-sdk` | port G/Ç, `syscall` komutu | `svc #0` ABI'si |
| `nc-desktop` | `nc-audio` bağımlılığı | ses arka ucunu `nc-hal::AudioOutput` ile soyutla |

**Aşama 3:** mobil kabuk + dokunmatik olay modeli, Ayarlar ve kabuğun `.ntzc` paketlerine ayrılması, Ring 3 (EL0) çalışma zamanı,
kesme denetleyicisi (GICv2/v3) ve zamanlayıcı kesmeleri, SMP.

**Aşama 4:** gerçek cihaz sürücüleri (ekran paneli, dokunmatik IC, eMMC, güç), UEFI aarch64 önyükleyicisi (arm64 PC), armv9a özellikleri (PAC/BTI/MTE).

## Güncel durum

Aşama 1 tamamlandı; Aşama 3'ün çekirdek kısmı (EL0, svc, zamanlayıcı, GIC) başlatıldı. Ayrıntı ve açık işler: `docs/architecture/AARCH64_STATUS.md`, `aarch64kernel/README.md`.

## Doğrulama durumu

Bu sürümdeki Rust kodu yazıldığı ortamda **derlenmedi** (Rust/QEMU yoktu). Doğrulananlar: `build.sh` komut çözümleme, QEMU komut satırı
üretimi (`--dry-run`), boot.img v0–v4 üretici/açıcı gidiş-dönüşü, Android paketleme akışı (sahte araçlarla). İlk derlemede
derleyici hataları beklenmelidir.
