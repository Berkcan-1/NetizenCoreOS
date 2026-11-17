# NetizenCore

Sıfırdan yazılan, Unix/Linux tabanlı olmayan işletim sistemi. Bootloader Assembly, çekirdek ve modüllerin tamamı Rust.
Kodlarda yorum satırı yoktur; açıklamalar her klasördeki README.md dosyalarındadır.

## Şu an ne yapıyor

1. BIOS, ISO'dan stage1'i yükler. stage1, stage2 ve çekirdeği belleğe okur (UEFI için `bootloader/uefi`).
2. stage2 64 bit işlemciyi kontrol eder, E820 bellek haritasını okur, VBE ile grafik modunu seçer, 4 GB sayfalamayı kurar ve long mode'a geçer.
3. Çekirdek ekranı açar, saati kalibre eder ve açılış animasyonunu oynatır.
4. Hazırlık bitince sistem kurulu değilse kurulum sihirbazı, kuruluysa masaüstü açılır.
5. Masaüstü: duvar kâğıdı, simgeler, görev çubuğu, başlat menüsü ve sürüklenebilir pencereler. Uygulamalar (Dosyalar, Ayarlar,
   Sistem, Kronometre, Tarayıcı) ortak bir sözleşme üzerinden çalışır (`crates/ui/nc-app/README.md`) ve Ring 3'te ayrı ELF olarak koşabilir.

## Klasör düzeni

Her üst klasör tek bir sorumluluğa sahiptir; bir iş üzerinde çalışırken yalnızca ilgili klasörü yapay zekâya vermek yeter.

| Klasör | İçerik |
|---|---|
| `bootloader/` | `bios/` (stage1, stage2, mbr: Assembly) ve `uefi/` (UEFI yükleyici crate'i) |
| `kernel/` | Çekirdek: `src/boot`, `src/tasking`, `src/ring3`, gömülü ikililer (`embedded/`), `SYSCALL_ABI.md` |
| `aarch64kernel/` | AArch64 (arm64/armv8a/armv9a) çekirdeği; eski adıyla kernel altındaki aarch64 klasörü |
| `crates/foundation/` | Sözleşmeler ve bağımsız kütüphaneler (hal, abi, efra, math, crypto, inflate, i18n) |
| `crates/hardware/` | x86 mimarisi, sürücüler, giriş, depolama, platformlar |
| `crates/services/` | Çekirdek hizmetleri: bellek, dosya sistemi, ağ |
| `crates/graphics/` | Canvas, yazı tipi, medya çözücüler, logo kaynağı |
| `crates/ui/` | Arayüz araç takımı, uygulama sözleşmesi, splash, kurulum, masaüstü |
| `crates/runtime/` | Ring 3 çalışma zamanı: yetkiler, ELF, paketler, SDK, host, cepheler |
| `apps/` | Uygulamalar; her uygulama kendi klasöründe (mantık crate'i + Ring 3 giriş ikilisi), `registry/` yerleşik liste |
| `sdk/` | Üçüncü taraf uygulama araçları (.ntzc paketleme, imza anahtarı, örnek) |
| `tools/` | `build/` (build.sh modülleri), `check/` (düzen denetimi), `docs/` (doküman üretimi), `gen/`, `model/` |
| `assets/` | Derleme zamanında gömülen dosyalar (logo, ses) |
| `docs/` | `architecture/`, `ring3/`, `security/`, `changelog/`, `migration/` |
| `aarch64crates/` | Yalnızca arm64'e ait her şey: `hardware/` (nc-arch-aarch64, nc-device), `runtime/`, `drivers/` (cihaz katmanı), `apps/` (EL0 uygulamaları), `tools/` (arm64 build betikleri) |
| `packages/` | Ayrı `.ntzc` sistem paketleri (mobil/masaüstü) |
| `host_usb/` | Konak tarafı USB deneme klasörü |

Katman kuralı: `foundation ← hardware ← services ← graphics ← ui ← runtime ← apps ← kernel`. Bir crate yalnızca kendi katmanına
ve altındakilere bağlanır (`python3 tools/check/verify_layout.py` denetler). Ayrıntı: `docs/architecture/LAYOUT.md`.

## Profiller

| Profil  | Hedef                | Çözünürlük  | Log çıkışı                          |
|---------|----------------------|-------------|-------------------------------------|
| qemu    | QEMU sanal donanımı  | 1280x720    | seri port, debugcon, isa-debug-exit |
| generic | genel PC, VirtualBox | 1024x768    | seri port (varsa)                   |

## Komutlar

    bash build.sh            qemu ISO üretir: build/netizencore-qemu.iso
    bash build.sh generic    generic ISO üretir: build/netizencore-generic.iso
    bash build.sh all        ikisini birden üretir
    bash build.sh run        qemu ISO üretir ve QEMU'da açar
    bash build.sh vbox       generic ISO üretir ve VirtualBox'ta açar
    bash build.sh clean      build klasörünü siler

Çok mimarili kullanım (`docs/architecture/MULTIARCH.md`, cihaz katmanı: `aarch64crates/drivers/README.md`):

    bash build.sh devices                            tanımlı cihazlar
    bash build.sh armv8a qemu run --mem 1024         aarch64 çekirdeği QEMU'da (--smp --gdb --headless -- ham-arg)
    bash build.sh armv8a android x6826c              boot.img + system.img + super.img
    bash build.sh unpack-boot stok-boot.img          stok boot.img'den device.conf değerlerini çıkarır
    bash build.sh new-device <ad> [soc]              yeni cihaz klasörü

    python3 tools/check/verify_layout.py        klasör düzeni ve yol bütünlüğü denetimi
    python3 tools/docs/sync_module_docs.py      README bağımlılık bloklarını ve DEPENDENCIES.md'yi yeniler

`build.sh` yalnızca sabitleri ve komut yönlendirmesini içerir; işlevler `tools/build/*.sh` içindedir (`tools/build/README.md`).

Gereksinimler (Debian/Ubuntu):

    sudo apt install nasm xorriso binutils qemu-system-x86 zip
    rustup (build.sh x86_64-unknown-none hedefini kendisi ekler)

İlk derlemede png kütüphanesi internetten indirilir.

## Hata ayıklama (QEMU)

Seri çıktı terminalde görünür, debugcon çıktısı `build/debugcon.log` dosyasına yazılır. Çekirdek panik olursa mesaj
ikisine de yazılır. QEMU, /dev/kvm erişilebiliyorsa otomatik hızlandırılır; KVM yoksa animasyon daha az kare ile
oynar ama süre aynı kalır.

## Yapay zekâ araçlarıyla çalışma

Tüm projeyi vermek gerekmez. Her satır, "bu işte çekirdeğe dokunmadan" çalışmak için verilecek klasörleri gösterir:

| İş | Ver | Çekirdeğe ne zaman dokunulur |
|---|---|---|
| Masaüstü / pencere / tema | `crates/ui/nc-desktop`, `crates/ui/nc-ui`, `crates/ui/nc-app` README'leri | Yalnızca `nc-desktop`/`nc-ui` genel API'si değişirse (`kernel/src/main.rs`) |
| Yeni veya mevcut uygulama | `apps/<ad>/`, `crates/ui/nc-app` README'si | Hiç; yeni uygulama `apps/registry/nc-apps`'e eklenir |
| Yazı tipi, çizim, medya | `crates/graphics/<crate>` | Hiç |
| Sürücü / depolama / giriş | `crates/hardware/<crate>`, `crates/foundation/nc-hal` README'si | `kernel/src/boot/` (aygıt başlatma) |
| Ağ | `crates/services/nc-net`, `crates/runtime/nc-net-facade` | `kernel/src/ring3/net.rs` |
| Ring 3, syscall, paketler | `kernel/src/tasking`, `kernel/src/ring3`, `crates/runtime`, `kernel/SYSCALL_ABI.md` | Zaten çekirdektedir |

Bağımlılık haritası: `docs/architecture/DEPENDENCIES.md`. Çekirdeğin diğer crate'lerle temas noktaları: `kernel/README.md`.
