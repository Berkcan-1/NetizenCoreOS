# Modüller

> Bu dosya `python3 tools/docs/sync_module_docs.py` ile üretilir; elle düzenlemeyin. Özetler `tools/docs/modules.toml` içindedir.

## Temel

| Modül | Konum | Görev |
|---|---|---|
| `nc-abi` | `crates/foundation/nc-abi` | Ring 3 ↔ çekirdek tel biçimleri: bağlam/olay blokları, çizim komutları, VFS/ağ/sistem istek-yanıt kodlaması |
| `nc-crypto` | `crates/foundation/nc-crypto` | SHA-256, HMAC, PBKDF2, sabit zamanlı karşılaştırma ve disk kilidi kayıtları |
| `nc-efra` | `crates/foundation/nc-efra` | EFRA sürücü paketi biçimi (manifest, sağlama toplamı, ayrıştırma/serileştirme) |
| `nc-hal` | `crates/foundation/nc-hal` | Önyükleme sözleşmesi (BootInfo), Platform ve aygıt arayüzleri, log! makrosu |
| `nc-i18n` | `crates/foundation/nc-i18n` | 11 dilli çeviri tabloları (langs/*.lang) ve derleme zamanı üretimi |
| `nc-inflate` | `crates/foundation/nc-inflate` | DEFLATE/gzip açıcı (gömülü .elf.gz yüklerini açar) |
| `nc-math` | `crates/foundation/nc-math` | Tam sayı matematiği ve yumuşatma eğrileri |

## Donanım

| Modül | Konum | Görev |
|---|---|---|
| `nc-arch` | `crates/hardware/nc-arch` | Mimari cephesi: hedefe göre nc-arch-x86_64 veya nc-arch-aarch64'ü yeniden dışa aktarır |
| `nc-arch-aarch64` | `aarch64crates/hardware/nc-arch-aarch64` | AArch64 mimari katmanı: PL011/16550 UART, genel zamanlayıcı, istisna vektörleri, MMU, PSCI, fw_cfg ramfb |
| `nc-arch-x86_64` | `crates/hardware/nc-arch-x86_64` | x86_64 mimari katmanı: portlar, IDT, APIC, RTC, seri port, zaman, rastgele sayı |
| `nc-dev-qemu-virt` | `aarch64crates/drivers/qemu-virt` | QEMU virt (aarch64) cihaz profili ve sürücü kayıtları |
| `nc-dev-rpi4` | `aarch64crates/drivers/rpi4` | Raspberry Pi 4 (BCM2711) cihaz profili ve sürücü kayıtları |
| `nc-dev-x6826c` | `aarch64crates/drivers/x6826c` | Infinix Hot 20 (X6826C, MT6768) cihaz profili ve sürücü kayıtları |
| `nc-device` | `aarch64crates/hardware/nc-device` | Cihaz profili sözleşmesi (DeviceProfile, DriverEntry, DriverHost) |
| `nc-device-select` | `aarch64crates/hardware/nc-device-select` | Derleme sırasında seçilen cihazı çekirdeğe bağlayan üretilen crate |
| `nc-driver` | `crates/hardware/nc-driver` | PCI taraması, DMA, PS/2, AC'97 ve EFRA tabanlı sürücü kayıt defteri |
| `nc-input` | `crates/hardware/nc-input` | Klavye/fare girişi: PS/2 ve xHCI USB HID olay kuyruğu |
| `nc-platform-generic` | `crates/hardware/nc-platform-generic` | Genel PC (VirtualBox ve çoğu gerçek makine) için asgari Platform uygulaması |
| `nc-platform-qemu` | `crates/hardware/nc-platform-qemu` | QEMU sanal donanımına özel Platform uygulaması |
| `nc-storage` | `crates/hardware/nc-storage` | Blok aygıtları (ATA/AHCI/NVMe), bölüm tabloları, kurulum yazıcısı ve disk kilidi |

## Çekirdek hizmetleri

| Modül | Konum | Görev |
|---|---|---|
| `nc-fs` | `crates/services/nc-fs` | Sanal dosya sistemi (VFS): FAT32, exFAT, EROFS, bellek içi FS ve USB bölümleri |
| `nc-mem` | `crates/services/nc-mem` | Bellek yönetimi: arena/PMM, heap, sayfalama ve çekirdek sayfa tabloları (kpt) |
| `nc-net` | `crates/services/nc-net` | Ağ yığını: NIC sürücüsü, DHCP, DNS, TCP, TLS 1.3, HTTP istemcisi, Wi-Fi/Bluetooth durumu |

## Grafik

| Modül | Konum | Görev |
|---|---|---|
| `nc-assets` | `crates/graphics/nc-assets` | Logo PNG hattı ve açılış sesi (assets → derleme zamanı gömme, ses IMA-ADPCM'e sıkıştırılır) |
| `nc-audio` | `crates/graphics/nc-audio` | Ses motoru: kesmesiz AC'97 halka akışı, 12 sesli mikser, sentezli ses teması ve açılış müziği |
| `nc-font` | `crates/graphics/nc-font` | Vektör yazı tipi (kontur verisi + üretim araçları) |
| `nc-gfx` | `crates/graphics/nc-gfx` | Canvas, bulanıklaştırma, karıştırma ve ekrana aktarım |
| `nc-media` | `crates/graphics/nc-media` | PNG/JPEG çözücü ve MP3/WAV PCM16 akışı |

## Arayüz

| Modül | Konum | Görev |
|---|---|---|
| `nc-app` | `crates/ui/nc-app` | Uygulama sözleşmesi: App trait, Context, giriş olayları, ikonlar ve pencere çerçevesi |
| `nc-desktop` | `crates/ui/nc-desktop` | Masaüstü: duvar kâğıdı, simgeler, görev çubuğu, başlat menüsü ve pencere yönetimi |
| `nc-setup` | `crates/ui/nc-setup` | Kurulum sihirbazı ekranları |
| `nc-splash` | `crates/ui/nc-splash` | Açılış animasyonu |
| `nc-ui` | `crates/ui/nc-ui` | Tema, ortak çizim yardımcıları ve Ring 3 uzak çizim komut katmanı (remote) |

## Ring 3 çalışma zamanı

| Modül | Konum | Görev |
|---|---|---|
| `nc-app-host` | `crates/runtime/nc-app-host` | Mevcut nc_app::App uygulamalarını değiştirmeden Ring 3'te çalıştıran ana-bilgisayar çalışma zamanı |
| `nc-app-sdk` | `crates/runtime/nc-app-sdk` | Ring 3 uygulama SDK'sı: syscall sarmalayıcıları, Gfx, RingApp, bağlayıcı betik (app.ld) |
| `nc-caps` | `crates/runtime/nc-caps` | Yetki (capability) bitleri için tek doğruluk kaynağı |
| `nc-elf` | `crates/runtime/nc-elf` | ELF64 doğrulayıcı ve yükleme planı üretici (Ring 3 imajları) |
| `nc-fs-facade` | `crates/runtime/nc-fs-facade` | nc-fs cephesi: kernel özelliği gerçek VFS'i, client özelliği Ring 3 VFS istemcisini verir |
| `nc-fs-policy` | `aarch64crates/runtime/nc-fs-policy` | Dosya yolu erişim politikası: uygulama kimliği + yetki bitleri + yol + işlem |
| `nc-net-facade` | `crates/runtime/nc-net-facade` | nc-net cephesi: kernel özelliği gerçek ağ yığınını, client özelliği Ring 3 istemcisini verir |
| `nc-pkg` | `crates/runtime/nc-pkg` | NTZC paket biçimi, imza doğrulama, manifest ve DysonSphere yetki motoru |
| `nc-sys-facade` | `crates/runtime/nc-sys-facade` | Sistem/ayar hizmeti cephesi: Wi-Fi/Bluetooth yansıması ve çekirdek hizmet tarafı |

## Uygulamalar

| Modül | Konum | Görev |
|---|---|---|
| `nc-app-browser` | `apps/browser/nc-app-browser` | Tarayıcı uygulaması: HTML/CSS düzeni ve nc-js ile betik çalıştırma |
| `nc-app-files` | `apps/files/nc-app-files` | Dosyalar uygulaması (id nc.files): gezinme, ses çalma, .ntzc paket kurma |
| `nc-app-settings` | `apps/settings/nc-app-settings` | Ayarlar uygulaması (id nc.settings) |
| `nc-app-stopwatch` | `apps/stopwatch/nc-app-stopwatch` | Kronometre uygulaması (id nc.stopwatch) |
| `nc-app-system` | `apps/system/nc-app-system` | Sistem bilgisi uygulaması (id nc.system) |
| `nc-apps` | `apps/registry/nc-apps` | Çekirdeğe gömülü yerleşik uygulamaların kaydı (kernel yalnızca buna bağlanır) |
| `nc-browser-r3` | `apps/browser/nc-browser-r3` | Tarayıcının Ring 3 giriş ikilisi (ELF) |
| `nc-files-r3` | `apps/files/nc-files-r3` | Dosyalar uygulamasının Ring 3 giriş ikilisi (ELF) |
| `nc-files-v8a` | `aarch64crates/apps/files/nc-files-v8a` | Dosyalar (ARMv8-A EL0): /data (exFAT) ve /system (FAT32) VFS istemcisi, dosya inceleyici/düzenleyici ve paket kurucu |
| `nc-home-r3` | `aarch64crates/apps/home/nc-home-r3` | NetizenHome: başlatıcı/kabuk, Ring 3 olay-tabanlı uygulama, açık tema |
| `nc-js` | `apps/browser/nc-js` | no_std + alloc JavaScript yorumlayıcısı (ağaç yürüyen) |
| `nc-r3-stopwatch` | `apps/stopwatch/nc-r3-stopwatch` | Kronometrenin bağımsız Ring 3 sürümü (nc-app-host kullanmaz) ve i18n eşlik testi |
| `nc-settings-r3` | `apps/settings/nc-settings-r3` | Ayarlar uygulamasının Ring 3 giriş ikilisi (ELF) |
| `nc-settings-v8a` | `aarch64crates/apps/settings/nc-settings-v8a` | Ayarlar (ARMv8-A EL0): kalıcı görünüm/saat/dil ayarları (/data), depolama, yaşam döngüsü ve donanım bilgisi |
| `nc-stopwatch-r3` | `apps/stopwatch/nc-stopwatch-r3` | Kronometrenin Ring 3 giriş ikilisi (ELF) |
| `nc-system-r3` | `apps/system/nc-system-r3` | Sistem uygulamasının Ring 3 giriş ikilisi (ELF) |

## Önyükleyici

| Modül | Konum | Görev |
|---|---|---|
| `nc-uefi` | `bootloader/uefi` | UEFI önyükleyici (GOP + bellek haritası → BootInfo → çekirdek) |

## Çekirdek

| Modül | Konum | Görev |
|---|---|---|
| `nc-kernel` | `kernel` | Çekirdek: açılış, süreçler, syscall, IPC ve Ring 3 çalışma zamanı |
| `nc-kernel-aarch64` | `aarch64kernel` | AArch64 çekirdeği: Image başlığı, EL2→EL1, DTB, MMU, EL0 uygulamaları, depolama, ana ekran |

Her modülün klasöründe ayrıntılı README.md, `src/` klasöründe dosya dosya açıklama vardır.
