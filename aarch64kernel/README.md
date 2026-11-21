# aarch64kernel

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64kernel` |
| **Katman** | Çekirdek |
| **Bağımlılıklar** | `nc-abi`, `nc-arch`, `nc-caps`, `nc-device`, `nc-device-select`, `nc-elf`, `nc-font`, `nc-fs`, `nc-fs-policy`, `nc-gfx`, `nc-hal`, `nc-math`, `nc-media`, `nc-mem`, `nc-pkg`, `nc-ui` |
| **İsteğe bağlı bağımlılıklar** | `nc-app`, `nc-desktop`, `nc-mobile-assets`, `nc-splash` |
| **Kullananlar** | — |
| **Özellikler (features)** | `default`, `el0-selftest`, `netizen-home`, `ui-desktop`, `ui-mobile` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

AArch64 çekirdeği (`armv8a`, `arm64`, `armv9a`). Ortak katmanları (`nc-gfx`, `nc-ui`, `nc-splash`, `nc-mem`, `nc-abi`) kullanır;
mimariye özel her şey `nc-arch-aarch64` ve `aarch64crates/drivers/<cihaz>` içindedir.

| Dosya | Görev |
|---|---|
| `entry.rs` | Linux arm64 `Image` başlığı, EL2→EL1, FP/NEON, BSS sıfırlama, bayat önbellek geçersiz kılma |
| `dtb.rs` | Sınır denetimli FDT okuyucu: bellek, ayrılmış bölgeler, framebuffer, `timer` frekansı |
| `boot.rs` | Açılış akışı: UART → DTB → MMU (W^X) → heap + kare havuzu + arena → GIC/zamanlayıcı → (EL0 öz-sınama) → arayüz |
| `vm.rs` | Fiziksel kare yöneticisi (bitmap) ve kullanıcı adres uzayı (4 düzey, 48 bit) |
| `task.rs` | Görev tablosu, round-robin zamanlayıcı, uyku, kesme ile öncelik hakkı kullanma |
| `syscall.rs` | `svc #0` sistem çağrıları (şimdilik EXIT, YIELD, SLEEP, WRITE, CLOCK_GET) |
| `selftest.rs` | EL0'da çalışan gömülü test görevleri (özellik: `el0-selftest`) |
| `host.rs` | Compositor: uygulama olaylarını teslim eder (`OP_INIT/UPDATE/DRAW`), `OP_UPDATE`'e `input::snapshot` ile işaretçi/tuş alanlarını yazar (teslim başarılıysa `input::consume`), `SYS_GUI_DRAW` akışını `nc_ui::remote::replay` ile ekrana çizer |
| `input.rs` | virtio-mmio girdi sürücüsü (mutlak işaretçi/tablet + klavye; legacy v1 ve modern v2). `profile.drivers` içindeki `virtio,mmio` tabanından 32 aktarım yuvasını tarar. Kuyruk belleği çekirdek `.bss`'indedir (kimlik eşleme: VA = PA). Kesme yerine 100 Hz zamanlayıcı kesmesinde yoklar (`task::on_irq` → `input::poll`). Düğme/tuş değişince `host::tick` hemen `OP_UPDATE` teslim eder |
| `blk.rs` | virtio-mmio blok sürücüsü (legacy v1 + modern v2), eşzamanlı ve yoklamalı salt-okunur okuma (en çok 64 KiB'lık parçalar); `nc_hal::BlockDevice` uygular, yazma `Err` döner |
| `lp.rs` | Android LP (dinamik bölüm / `super.img`) üst veri okuyucusu: geometri + birincil metadata yuvası 0, adla bölüm bulur, yalnızca doğrusal tek-aygıt uzantıları; `PartitionDevice` uzantıları `BlockDevice` olarak sunar. Sağlama toplamları DOĞRULANMIYOR (`TODO(lp-checksum)`), tüm sınırlar denetlenir |
| `storage.rs` | `super.img` içindeki `system` (yoksa `system_a`) bölümünü FAT32 olarak bağlar (`nc-fs`, salt-okunur); `/etc/netizencore-release` ve `/packages/*.ntzc` listesini UART'a yazar; bağlı dosya sistemini `storage::system_fs()` ile verir; `data_usage()` / `system_usage()` dizin ağacını gezip `/app`, `/media`, `/appdata` ve diğer için gerçek dosya boyutu toplamını verir (en çok 20 000 girdi / 2 048 dizin, aşılırsa `complete=false`); önbellek her yazma/silme sonrası `sync_data()` içinde geçersiz kılınır. Küme payı ve meta veri sayılmaz: tam boş alan için `nc-fs` içinden ayırma bitmap'i okunmalıdır (`TODO(nc-fs-free)`) |
| `wallpaper.rs` | `/media` içindeki PNG/JPEG dosyalarını (en çok 6, 8 MiB'a kadar) listeler; seçilen resmi `nc-media` ile dosya imzasına göre çözüp ekran boyutunda önbelleğe alır ve `host.rs` her çizimde çalıştırmadan önce ekrana basar (duvar kağıdı değeri 4). Ayar `settings.cfg` içinde `wallpaper_image=<dosya adı>` olarak saklanır; çözülemezse hazır temaya dönülür. Uygulamalar yalnızca dosya adı gönderir (`MOBILE_SET_WALLPAPER_IMAGE`), yol çekirdekte kurulur |
| `home.rs` | Açılış ekranı (splash) ve NetizenHome yüklenemezse yer tutucu ekran |
| `embedded/home.elf` | Gömülü NetizenHome ELF'i (build.sh üretir) |

## Bellek düzeni

- **Çekirdek**: TTBR0 altında kimlik eşlemeli (L0[0] → paylaşılan L1, 16 GiB). RAM Normal-WB, framebuffer Normal-NC, geri kalan Device.
  Çekirdek imajı 4 KiB sayfalarla eşlenir: `.text` = RX, `.rodata` = RO+XN, `.data/.bss/yığın` = RW+XN. `SCTLR.WXN` açık.
- **Kullanıcı**: L0[1] (0x80_0000_0000 ve üstü) — `nc-abi::USER_SPACE_START` ile aynı; x86_64 ile aynı sabitler (`USER_IMAGE_MIN`,
  `USER_APP_STACK_TOP`, ...) kullanılır. Her süreç kendi L0 tablosuna sahiptir; `L0[0]` hepsinde aynı çekirdek L1'ini gösterir
  (EL0 erişimi AP bitleriyle kapalı). Süreç geçişinde `TTBR0` değişir ve `tlbi vmalle1` yapılır.
- Çekirdek kullanıcı belleğine **fiziksel kimlik eşlemesi üzerinden** erişir (yazılım sayfa gezintisi): `copy_from_user`/`copy_to_user`
  hata veremez, PAN açık kalabilir.

## Çalışma modeli ("trap-return")

Çekirdek tek yığınlıdır ve çekirdek içinde bloklamaz. EL0'dan her istisna (`svc`, kesme, hata) yığının tepesine bir `TrapFrame`
(272 bayt) kaydeder; işleyici çerçevenin içeriğini ya olduğu gibi bırakır ya da başka görevin çerçevesiyle değiştirir ve `eret` eder.
Görev yoksa işleyici `wfi` ile uyur (zamanlayıcı kesmesi 100 Hz). Tüm görevler bitince çerçeve çekirdek devam fonksiyonuna
(`ui_phase`) çevrilir.

## Olay-tabanlı uygulamalar (NetizenHome)

Uygulama her olayda `_start`'tan girer (x86_64 ile aynı model); durumu kendi `static` belleğinde kalır. `SYS_APP_DONE` görevi
"parked" duruma alır; `host::tick` saniyede bir `OP_UPDATE`, uygulama `RET_REDRAW` dönerse hemen `OP_DRAW` teslim eder. Çizim akışı
`SYS_GUI_DRAW` ile çekirdeğe gelir, doğrulanır ve ekrana işlenir (uygulama piksel görmez). `SYS_CLOCK_GET`: `x0=0` tikler, `x0=1` duvar saati.

## Sistem çağrısı ABI'si (aarch64)

`svc #0`; numara `x8`; argümanlar `x0..x5`; dönüş `x0` (negatif = hata, x86_64 ile aynı numaralar: `nc-abi::SYS_*`).
Hatalar: `-9` EBADF, `-14` EFAULT, `-22` EINVAL, `-38` ENOSYS.

## Beklenen UART çıktısı (QEMU, öz-sınama açık)

    gic: true, zamanlayici: true (100 Hz)
    EL0 oz-sinama basliyor
    [A] merhaba EL0 / [B] gorev iki canli (araya girerek)
    [gorev] test-hata hata verdi: ec=36 ... -> sonlandirildi
    [gorev] test-gecersiz-isaretci bitti, cikis kodu -14
    [gorev] test-mesgul-dongu bitti, cikis kodu 7
    pmm: bosta N kare (baslangic N)      <- iki sayi esit olmali (sizinti yok)

## Girdi (virtio-input)

- Cihaz türü yapılandırma alanından anlaşılır: `EV_ABS` içinde `ABS_X`/`ABS_Y` varsa mutlak işaretçi (aralık `ABS_INFO`'dan okunur, uydurulmaz), değilse `EV_KEY` içinde `KEY_A` varsa klavye. Göreli fare ve ikinci tablet atlanır (günlükte yazar).
- İşaretçi: `BTN_LEFT`/`BTN_TOUCH` basışı `PTR_CLICKED` (tek seferlik, x86_64 ile aynı: basış kenarı) + `PTR_DOWN` (basılı olduğu sürece); `BTN_RIGHT` basışı `PTR_RIGHT_CLICKED`. Koordinat, ham değerin ekran boyutuna doğrusal ölçeklenmesidir. Tıklama konumu basış anındaki konumdur. Düğme basılı değilken gelen hareket uygulamayı uyandırmaz; sonraki olayda son konum yazılır.
- Klavye: Linux tuş kodları, ABD QWERTY düzeni, Shift izlenir; Caps Lock, AltGr, sayısal tuş takımı ve Türkçe düzen yok (eşlenmeyen tuş `key::UNKNOWN`). Tuşlar 16'lık kuyrukta tutulur, her `OP_UPDATE`'te bir tane teslim edilir.
- Yalnızca `OP_UPDATE` girdi taşır; `OP_INIT`/`OP_DRAW` işaretçi alanlarını boş taşır.
- Sınırlar: kesme (GIC SPI) yok, yoklama; DMA önbellek bakımı yok (QEMU tutarlı bellek varsayımı); imleç çizimi yok; aynı iki teslim arasındaki birden fazla tıklama tek tıklamada birleşir.

Beklenen UART çıktısı (QEMU, `bash build.sh armv8a qemu`, `pan:` satırından sonra; adresler/sürüm QEMU sürümüne göre değişebilir):

    [girdi] 0x0a003e00: virtio-input mutlak isaretci (surum 1, x 0..32767, y 0..32767, kuyruk 64)
    [girdi] 0x0a003c00: virtio-input klavye (surum 1, kuyruk 64)
    virtio-input: 2 aygit

Pencereye tıklayınca: `[girdi] dokunma: x=… y=…`; tuşa basınca: `[girdi] tus: kod=0x…`.

## Depolama (virtio-blk + super.img)

- `bash build.sh armv8a qemu` artık `build/armv8a-qemu-virt/super.img` üretir ve QEMU'ya `-drive ...,readonly=on -device virtio-blk-device` ile verir. `system.img` FAT32'dir (`mkfs.vfat`, `mcopy`, `mmd` gerekir: `dosfstools mtools`); içine `stage_system_root` ağacı (`/etc/netizencore-release`, `/etc/device.conf`, `/assets`, `/packages/*.ntzc`) konur.
- `super.img` varsayılan olarak `aarch64crates/tools/mk-super.py` ile (tek bölümlü LP, yalnızca geliştirme akışı) üretilir. `NC_USE_LPMAKE=1` ile AOSP `lpmake` kullanılır (bu yol bu depoda DENENMEDİ). Çekirdek iki çıktıyı da aynı LP biçimi olarak okur.
- `nc-fs` artık `storage` özelliği (varsayılan açık) kapalıyken `nc-storage`'ı (x86 port G/Ç, PCI) çekmeden derlenir; aarch64 çekirdeği `default-features = false` kullanır. x86_64 çekirdeği varsayılan özelliklerle eskisi gibi derlenir.
- Yazma yok: `/data` (yazılabilir bölüm), paket kurma ve virtio-blk kesmesi sonraki adımlardır. Aynı virtio taşıma kodu `input.rs` ve `blk.rs` içinde kopyadır (`TODO`: ortak modül).
- Bağlama ve paket listesi boot sırasında `input::init` sonrasında, `blk::init` + `storage::mount_system` ile yapılır.

Beklenen UART çıktısı (`virtio-input: 2 aygit` satırından sonra; boş `/packages` ile):

    [depo] 0xa003e00: virtio-blk 86016 sektor (42 MiB, surum 1, kuyruk 8)
    virtio-blk: 1 aygit
    [depo] super.img: bolum 'system', 1 uzanti, 83968 sektor
    [depo] sistem bolumu FAT32 baglandi (salt-okunur)
    [depo] /etc/netizencore-release (66 bayt):
    [depo]   name=NetizenCore
    [depo]   arch=...
    [depo] /packages: 0 paket

(Adres, sektor sayısı ve sürüm farklı olabilir. `.ntzc` dosyası varsa her biri `[depo]   paket: ad (bayt)` satırıyla listelenir.)
