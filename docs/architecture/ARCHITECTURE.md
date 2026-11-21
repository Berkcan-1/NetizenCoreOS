# NetizenCore mimarisi

Klasör düzeni ve katman kuralı: `LAYOUT.md`. Bağımlılık haritası: `DEPENDENCIES.md`.

## Açılış akışı

    BIOS  -> El Torito (no-emulation) -> stage1 -> stage2 -> çekirdek (_start asm, kernel_main Rust)
    UEFI  -> bootloader/uefi (GOP + bellek haritası -> BootInfo) -> çekirdek

`kernel_main` sırası (`kernel/src/main.rs`):

1. Platformu başlat (seri port, debugcon vb.), `BootInfo`'yu oku.
2. PIT ve TSC ile saati kalibre et (`nc-arch::time::Clock`).
3. `BootInfo` bellek haritasından kullanılabilir RAM sınırlarını bul (`detect_ram_bounds`); ekran, geçiş ve kurulum
   tamponlarını bellek havuzundan (Arena) ayır, geri kalanı fiziksel bellek yöneticisi ve heap'e ver.
4. Sistemin diskte kurulu olup olmadığını denetle (`nc_storage::find_installed_disk`); zemini çiz, splash'i hazırla.
5. Animasyon döngüsü: hazırlık bitene kadar Intro ve Idle, sonra Outro. Her karede yalnızca değişen dikdörtgenler framebuffer'a yazılır.
6. Kurulu değilse kurulum sihirbazı (`nc-setup`) çalışır; kuruluysa doğrudan masaüstüne geçilir.
7. Masaüstü: `DesktopState`, `nc_apps::builtin()` listesiyle oluşturulur. `ring3-apps` özelliği açıkken liste
   `ring3::apps::with_ring3_apps` ile Ring 3 ELF'lere bağlanır (`kernel/src/ring3/`); kapalıyken liste doğrudan kullanılır.
   `nc-apps`'in `ring0-fallback` özelliği, uygulamaların Ring 0 gerçeklemelerinin derlenip derlenmeyeceğini belirler.

## Bellek

Düşük bellek (BIOS yolu):

    0x0600   VBE kontrolcü bilgisi (geçici)
    0x0800   VBE mod bilgisi (geçici)
    0x0900   BootInfo (sürüm 2: çerçeve tamponu + bellek haritası işaretçisi/sayısı)
    0x0A00   E820 kayıtları (stage2, en çok 64 kayıt)
    0x1000   sayfa tabloları
    0x7C00   stage1
    0x8800   stage2
    0x9800   çekirdek yüklü hali (geçici)
    0x100000 çekirdek imajı (.text .rodata .data .bss)

Çekirdek imajı için varsayılan üst sınır 7 MiB'tır (`kernel/linker.ld` → `__nc_image_limit`; `NC_IMAGE_LIMIT_MIB`
ortam değişkeniyle değişir; `tools/build/kernel.sh` ayrıca `kernel.bin` için 16 MiB üst sınırı denetler). Bellek havuzu (Arena), fiziksel bellek yöneticisi
ve heap 16 MiB (`ARENA_BASE = 0x0100_0000`) ve üstündeki en büyük kullanılabilir bölgeden ayrılır; yaklaşık 16 MiB
kesintisiz bölge gerekir, 4 GiB üstü RAM kullanılmaz. Ayırma ayrıntıları `kernel_main` içindedir.

Her ekran tamponu piksel başına 4 bayttır (1280x720'de yaklaşık 3,7 MB). Sayfalama: 0-3 GB önbellekli, 3-4 GB önbelleksiz
(framebuffer ve MMIO için), 2 MB sayfalar.

## Katmanlar

    foundation ← hardware ← services ← graphics ← ui ← runtime ← apps ← kernel

Crate bazında ok listesi `DEPENDENCIES.md` içindedir ve araçla üretilir. Birkaç önemli ilke:

- `nc-desktop` uygulamaları adlarıyla bilmez; kendisine verilen listeyi (`nc_apps::builtin()`) çalıştırır.
- Çekirdek, uygulama crate'lerine doğrudan bağlanmaz; yalnızca `nc-apps` kaydına bağlanır.
- `nc-ui`'nin `remote` özelliği Ring 3 uygulamalarının çizim komutlarını (`nc-abi`) tuvale çevirir.

## Profiller

Profil, kernel kutusunun cargo özelliğidir: `qemu` veya `generic`. İkisi birden ya da hiçbiri seçilirse derleme hatası
verir (`kernel/src/boot/platform.rs`). Yeni platform eklemek için:

1. `crates/hardware/nc-platform-<ad>` oluştur, `nc-hal::Platform`'u uygula.
2. Kök `Cargo.toml` üye listesine ekle.
3. `kernel/Cargo.toml`'a özellik ve isteğe bağlı bağımlılık ekle.
4. `kernel/src/boot/platform.rs`'e bir cfg satırı ekle.
5. `tools/build/common.sh` içindeki `configure_profile`'a çözünürlük ayarını ekle.

## Kurallar

- Modüller yalnızca katman kuralına göre birbirini kullanır (`tools/check/verify_layout.py` denetler).
- Donanıma özel kod yalnızca `crates/hardware/` içinde olur.
- Ekrana çizen kod bir Canvas'a çizer. Framebuffer'a yalnızca `nc-gfx::present` ve `present_region` yazar.
- Ekran tamponları `nc-mem::Arena`'dan alınır; modüllerin kendi sabit adresleri olmaz.
- Kodlarda yorum satırı yok. Açıklamalar README.md dosyalarında.
- Yeni bir crate eklenince kök `Cargo.toml`'daki üye listesine elle eklenir (glob kullanılmaz).

## Yapay zekâ araçlarıyla çalışırken

Bir modül üzerinde çalışırken: o modülün README.md'si ve src klasörü, bağımlı olduğu modüllerin README.md dosyaları
(her crate README'sinin başındaki tablo bağımlılıkları ve kullananları listeler). İş türüne göre hazır liste için kök README'ye bak.
