# Ring 3 yol haritası ve Adım 1 (çekirdeğin kendi sayfa tabloları)

> **DERLENMEDİ, ÇALIŞTIRILMADI.** Bu belgedeki kod `cargo`/QEMU olmayan bir ortamda yazıldı.
> Yalnızca algoritmanın Python modeli (`kpt.rs` ile aynı mantık) çalıştırıldı ve geçti.
> Gerçek doğrulama aşağıdaki "Test prosedürü" ile yapılır. Önceki turdaki (GUVENLIK_DUZELTMELERI.md)
> değişiklikler de derlenmemiş durumda; **ilk iş `cargo build` ve `cargo test`.**

## 1. Mevcut durum (koddan okundu)

Var olan:
- Çekirdekte tam bir Ring 3 altyapısı: `AddressSpace` (W^X, NX, CoW, demand paging), `copy_from_user`,
  `int 0x80` + SYSCALL/SYSRET, handle tablosu, süreç başına yetki (`nc-caps`), IPC ve CSpace.
  Faz 1–5 self-test'leri her açılışta koşuyor.
- Kullanıcı süreci formatı **NCU1**: tek kod sayfası + tek rodata sayfası + tek yığın sayfası, yeniden
  konumlandırma yok, FNV-1a sağlama (imza değil).

Henüz olmayan (planın asıl işi):
- Uygulamalar hâlâ Ring 0'da `Box<dyn App>` olarak çalışıyor (`nc-apps::builtin`).
- `SYS_GUI_SUBMIT` komutları doğruluyor ama çizmiyor; `op=3` (metin) bir metin yükü taşımıyor.
- `FileRead/FileWrite` handle'ları VFS'e bağlı değil.
- SDK uygulamaları NCBM ile (Ring 0'da, `.bss`'ten) çalıştırılıyor; `unsafe-native-modules` özelliği
  kapalı, yani şu an SDK uygulaması çalışmıyor.
- Çekirdek, firmware'in bıraktığı sayfa tablolarıyla çalışıyordu (bu belgenin konusu).

## 2. Sıra (arkadaşının planı + bulgularımız)

| Adım | İş | Not |
|---|---|---|
| **1a** | Çekirdeğin kendi sayfa tabloları, W^X, NX, yığın guard page | **Bu teslimat.** `kernel-paging` özelliğiyle. |
| 1b | PMM'ye 4 GiB üstü MMIO eşleme API'si, SMAP, `idt::init()` öncesi kör pencerenin kapatılması | Küçük; 1a doğrulanınca. |
| **2** | ELF64 yükleyici (çok segmentli, `ET_EXEC`), FileRead/FileWrite → VFS | **Yazıldı (bölüm 7)**, `ring3-apps` özelliğiyle. |
| **3** | Compositor: çizim komut akışı | **Yazıldı (bölüm 7)**: komutlar, kernel'in doğrulayıp kırparak çizdiği senkron köprü. Asenkron IPC kuyruğu 4'ten sonra. |
| **4** | Kronometre Ring 3'te | **Yazıldı (bölüm 8)**: `nc-app-sdk`, `RemoteApp`, watchdog, `/ram` köprüsü. |
| 5 | SDK paketleri: NTZ2 imzalı paket → ELF yükü → Ring 3; NCBM ve `unsafe-native-modules` silinir | F-01'in kalıcı çözümü bu. |
| 6 | Tarayıcı | Ağ + çizim ABI'si oturunca. |

(Karar verildi: komut akışı. Eski not:) Adım 3'te verilecek karar: uygulama **kendi yüzeyine çizip** (paylaşımlı sayfalar, `SYS_IPC_GRANT_PAGE`
zaten var) compositor'a "hasar dikdörtgeni" mi bildirsin, yoksa çizim **komutları** mı göndersin? İlki
`nc-ui/nc-gfx/nc-font`'u uygulamaya taşımayı, ikincisi `op=3`'e metin yükü eklemeyi gerektirir. Adım 1–2
bundan bağımsız; karar Adım 3 başında.

## 3. Adım 1a: ne yapıyor

`crates/nc-mem/src/kpt.rs` yeni modül. Tablolar çekirdek imajının `.bss`'inde (statik, ~60 KiB), PMM'ye
bağlı değil. `kernel_main` içinde `BootInfo` okunur okunmaz, **`ALLOCATOR.init` ve `init_pmm`'den önce**
devreye alınır. UEFI'de önemli: firmware tabloları `EfiBootServicesData` içinde olabilir ve çekirdek bu
bölgeyi "kullanılabilir" sayıyor; ayırıcılar başlamadan geçersek tabloları ezme riski kalmaz.

Harita (kimlik eşlemesi):

| Bölge | Sayfa | İzin | Önbellek |
|---|---|---|---|
| 0 .. 4 GiB | 2 MiB | RW + NX | 0–3 GiB WB, 3–4 GiB UC- (eski stage2 kuralı) |
| Çekirdek imajı (ilk 2 MiB'lık dilimler) | 4 KiB | `.text` R+X, `.rodata` R+NX, `.data/.bss/.stack` RW+NX | WB |
| Yığın alt sayfası | — | **eşlenmez** (guard page) | — |
| Framebuffer | 2 MiB | RW + NX | WC (PAT PA1); 4 GiB üstündeyse ayrı PD |

Ek olarak: `IA32_PAT` stage2'deki değere ayarlanır (UEFI yolunda da WC çalışsın), `CR0.WP` açılır,
`CR4.PGE` kapatılır (global TLB girdileri de temizlensin), sonra `CR3` yazılır.

**Güvenlik ağı:** `prepare()` CR3'e dokunmaz; tabloları kurar ve yazılımla tarar (tüm imaj sayfaları,
0–4 GiB, framebuffer uçları, çalışan RIP/RSP/GDT/IDT, BootInfo, bellek haritası, LAPIC). En küçük
uyumsuzlukta `[KPT ATLANDI]` yazıp **eski tablolarla devam eder**; sistem bugünkü gibi açılır.

`kernel/linker.ld` değişti: bölümler 4 KiB hizalı, `__kpt_*` sembolleri eklendi (imaj yaklaşık 12 KiB büyür;
6 MiB ASSERT'i hâlâ geçerli ve link anında kırılır, sessiz değil).

## 4. Test prosedürü (sırayla; her adımın çıktısını bana yapıştır)

```bash
cargo test -p nc-mem                                  # kpt + mevcut testler (host)
bash build.sh run                                     # KPT KAPALI: eski davranış, "[KPT] kapali" satırı
NC_KERNEL_PAGING=1 bash build.sh run                  # KPT AÇIK
```

Beklenen seri çıktı (KPT açık):

```
[KPT] imaj: text=0x100000..0x... rodata=... rw=... guard=0x... fb=0x...+0x...
[KPT] tablolar hazir ve dogrulandi: PML4=0x... NX=true WC=true 4K-parca=4 ornek=8
[KPT] CR3 degistiriliyor. ...
[KPT OK] CR3 0x... -> 0x...: W^X, NX, yigin koruma sayfasi, WP acik. ...
Bellek: ... 
[Faz 1 OK] ... [Faz 2 OK] ... [Faz 5 OK] ...
```

Hepsini şu kombinasyonlarda dene (hata daha önce 2 GB'de çıkmıştı):

```bash
NC_KERNEL_PAGING=1 NC_QEMU_MEM=512  bash build.sh run   # BIOS/ISO ve kurulu disk
NC_KERNEL_PAGING=1 NC_QEMU_MEM=2048 bash build.sh run   # eski "Kernel code mapping bulunamadi" senaryosu
NC_KERNEL_PAGING=1 NC_QEMU_EXTRA="-cpu max" bash build.sh run   # NX, PAT, SMAP/SMEP için
```

### Çıktısız çökerse (CR3 satırından sonra hiçbir şey yoksa)

`[KPT] CR3 degistiriliyor` yazıldıysa ama `[KPT OK]` yoksa, `CR3` yazımından sonra bir sayfa hatası oldu
ve henüz IDT yok (`idt::init()` sonra geliyor). QEMU'ya şunu ver:

```bash
NC_KERNEL_PAGING=1 NC_QEMU_EXTRA="-d int,cpu_reset,guest_errors -D build/qemu-debug.log -no-reboot -no-shutdown" bash build.sh run
```

`build/qemu-debug.log` içinde ilk `check_exception` / `v=0e` (veya `v=08`) bloğu `CR2`, `RIP`, `ERR`
verir. `build/debugcon.log` içindeki harfler hangi noktaya gelindiğini gösterir:
`K` = CR3 yazılmadan hemen önce, `PT` = CR3 yazıldı ve ilk komutlar çalıştı.

### Sayfa hatası satırı (`[KERNEL PAGE FAULT (RING-0)] CR2=.. ERR=.. RIP=..`)

`idt::init()` sonrası bir W^X ihlali olursa çekirdek bunu yazıp durur. `ERR` bitleri: `0x1` korumadan,
`0x2` yazma, `0x10` yürütme (NX). Çözümü:

| ERR | CR2 nerede | Anlamı |
|---|---|---|
| `0x3` | `.rodata` içinde | Çekirdek salt-okunur veriye yazıyor (gerçek bir hata, düzeltilecek) |
| `0x11` | `.data/.bss`/heap | Veriden kod çalıştırılıyor (ör. NCBM `EXEC_SLOTS`; beklenen) |
| `0x2` | `__kpt_stack_start` sayfası | **Çekirdek yığını taştı** (guard page yakaladı) |
| `0x2` / `0x0` | 0 sayfası civarı | Null göstericisi |

RIP'ten fonksiyon adı: `addr2line -f -e build/cargo-qemu/x86_64-unknown-none/release/kernel 0x<RIP>`.

## 5. Bilinen sınırlar (bilerek yapılmayanlar)

1. **Derlenmedi.** Beklenen ilk sürtünme: `cargo build` uyarı/hataları; küçük düzeltmeler olabilir.
2. **4 GiB üstü MMIO BAR'ları** (ör. 64-bit NVMe/xHCI BAR) eşlenmiyor; BIOS yolunda da yoktu. Adım 1b'de
   bir `map_mmio` API'si. 4 GiB üstü *framebuffer* ise ele alındı.
3. **Önbellek kuralı eski haliyle korundu** (0–3 GiB WB). Gerçek donanımda PCI MMIO penceresi 3 GiB
   altına düşerse WB olur; bellek haritasına göre UC- vermek 1b'de.
4. **Sayfa 0 eşli** (`BootInfo` 0x900'de); null guard için `BootInfo`'yu kopyalamak gerekir.
5. **`idt::init()` öncesi kısa kör pencere**: `CR3` yazımı ile `idt::init()` arası. Yazılımla doğrulama ve
   `0xE9` işaretleriyle azaltıldı; 1b'de kapatılacak.
6. **NCBM (`unsafe-native-modules`) bu özellik açıkken tamamen çalışmaz**: `.bss` artık NX. Zaten
   varsayılan kapalıydı ve Adım 5'te silinecek.
7. `[Faz 4]` `SMEP` testi `-cpu qemu64`'te desteklenmeyebilir; sorun KPT'den bağımsız, `-cpu max` ver.

## 6. Adım 1a bitti sayılır, ancak

- `cargo test -p nc-mem` geçiyor,
- KPT açık: BIOS ve UEFI × 512 MB ve 2 GB açılışları `[Faz 5 OK]` ve masaüstüne kadar gidiyor,
- seri günlükte `[KPT OK]` var ve `[KERNEL PAGE FAULT]` yok.

Sonra `kernel-paging` varsayılan açılır ve Adım 1b/2'ye geçilir.

## 7. Adım 2+3: ELF yükleyici, olay-tabanlı Ring 3, çizim komut akışı (`ring3-apps`)

**Tasarım kararı (koddan çıkan bulgu).** Mevcut oturum modelinde `SYS_YIELD/SLEEP` oturumu çekirdeğe
geri atar ve yalnızca `rip/rsp` saklanır; genel yazmaçlar kaybolur. Derlenmiş Rust kodu bunu kaldıramaz.
Bu yüzden uygulamalar **olay-tabanlıdır** (NCBM'deki `INIT/UPDATE/DRAW/CLOSE` modeli): çekirdek her
olayda uygulamayı `e_entry`'den, yazmaçlar sıfır ve yığın boş girer; durum uygulamanın belleğinde
(`.data/.bss`/heap) kalır; olay `0x80_00F0_0000`'daki salt-okunur 96 baytlık `EventBlock`'tan okunur;
uygulama `SYS_APP_DONE(bayraklar)` ile döner. Bloklayan syscall yok, yazmaç kaydı gerekmiyor.

| Parça | Yer | Test |
|---|---|---|
| ABI sabitleri, `EventBlock`, çizim komut akışı (yazıcı/okuyucu/doğrulayıcı) | `crates/nc-abi` | `cargo test -p nc-abi` |
| Katı ELF64 → `LoadPlan` | `crates/nc-elf` | `cargo test -p nc-elf` (gerçek `ld` çıktısı dahil) |
| Compositor tekrar oynatıcı (klip, doğrulama, metin ölçümü) | `crates/nc-ui/src/remote.rs` | `cargo test -p nc-ui --features remote` |
| ELF yükleme, olay çalıştırma, sonlandırma | `kernel/src/process.rs` (`spawn_user_elf`, `run_app_event`, `kill_app`) | Faz 6 |
| Syscall 16–22, dosya köprüsü, Faz 6 | `kernel/src/ring3.rs`, `kernel/faz6/` | Faz 6 |

### Güvenlik modeli
- **ELF**: yalnızca ET_EXEC/x86-64/PT_LOAD; W+X, yürütülebilir yığın, INTERP/DYNAMIC/TLS, taşan/çakışan/hizasız/aralık dışı segment ve giriş noktası dışı entry **reddedilir**. Adres aralığı `0x80_0100_0000..0x80_0800_0000`, en çok 8 segment, 4096 sayfa. Her sayfa sıfırlanıp doldurulur (çekirdek verisi sızmaz). Başarısız yükleme tüm çerçeveleri geri verir.
- **Çizim**: akış önce tamamen doğrulanır (uzunluk, hizalama, aralıklar, UTF-8, kayıt ≤ 512, bayt ≤ 16 KiB, iş bütçesi ≤ 16 Mpiksel); bozuk akış **hiç çizilmez**. Koordinatlar pencereye göredir ve pencere ∩ canvas klibine kırpılır.
- **Dosya**: `FS_USER_READ/WRITE` yetkisi gerekir. Okuma yalnızca `/ram/`, `/data/documents/`, `/data/desktop/`; yazma yalnızca `/ram/`, `/data/documents/` (VFS'in `CALLER_USER_APP` rolü ikinci kat). `..`, `.`, boş segment, `\`, NUL reddedilir. Okuma açılışta anlık görüntüdür (≤ 1 MiB); yazma **kapatmada** VFS'e yazılır; kapatılmadan ölen uygulamanın yazması atılır.
- **İzolasyon**: uygulama hatası (#UD, #PF, ...) yalnızca o süreci sonlandırır; çekirdek sürer.

### Test prosedürü
```bash
cargo test -p nc-mem -p nc-abi -p nc-elf
cargo test -p nc-ui --features remote
bash build.sh run                                   # Ring 3 KAPALI: "[Faz 6] kapali"
NC_RING3_APPS=1 bash build.sh run                   # Ring 3 AÇIK
NC_RING3_APPS=1 NC_KERNEL_PAGING=1 NC_QEMU_MEM=2048 bash build.sh run   # hepsi birlikte
```
Beklenen: `[Faz 6 OK] Ring 3 ELF yolu: ...` ve `faz6 uygulamasi INIT` satırı. Başarısızlıkta `[FAZ 6 HATA] <neden>`
yazar ve **açılış sürer** (Faz 1–5'in aksine durmaz). Nedenler: *INIT/UPDATE/DRAW* = ABI uyuşmazlığı
(hangi denetim kırıldı mesajda); *klip ihlali* = compositor kırpması bozuk; *sizdirdi* = çerçeve sahipliği hatası.

### Bilinen sınırlar
1. **Watchdog yok.** Ring 3'te sonsuz döngü sistemi kilitler (oturum içinde IRQ'dan iptal henüz yok). Bu yüzden
   **yalnızca güvenilen/gömülü ELF'ler** yüklenmeli; üçüncü taraf yükleme Adım 3b (zamanlayıcıdan iptal) sonrası.
2. Masaüstü entegrasyonu (`RemoteApp: App`) yok; Faz 6 çekirdekten sürüyor. Bu Adım 4 (Kronometre).
3. `/ram` Ring 3 köprüsünün **kendi** `VfsManager` örneğinde; Ring 0 Dosyalar uygulaması aynı RAM'i görmez. Disk (`/data/documents`) ortaktır.
4. Tek eşzamanlı Ring 3 oturumu (`KERNEL_RETURN_RSP` tekil): olaylar sırayla çalışır, iç içe değil.
5. SDK henüz ELF üretmiyor (NCBM düz ikili); `app.ld` ELF çıktısı ve NTZ2 → ELF yükü Adım 5.
6. `kill_app` tüm `Terminated` kullanıcı süreçlerini toplar.
7. `Cargo.lock` yeni crate'lerle güncellenir; `--locked` kullanıyorsan önce `cargo update -w`.

## 8. Watchdog, masaüstü entegrasyonu, `/ram` köprüsü ve Ring 3 Kronometre

**Watchdog.** `run_app_event` her olay için TSC son tarihi kurar (varsayılan 200 ms; `ring3::set_event_budget_ms`).
İki yakalama noktası var: (1) zamanlayıcı kesmesi (`rust_timer_irq_handler`, artık kesilen `cs`'yi alır) süre
dolmuşsa **ve kesilen kod Ring 3 ise** oturumu iptal eder; Ring 0'da (ör. VFS yazarken) asla iptal etmez;
(2) `dispatch_syscall` girişi (sistem çağrısı yağdıran uygulama için). İptal edilen süreç `Terminated(-62)` olur
ve `run_app_event` `"Uygulama zaman asimi (watchdog)"` döndürür. TSC kalibre edilemediyse Ring 3 olayları **reddedilir**
(watchdog'suz çalıştırma yok). Faz 6b iki senaryoyu (saf sonsuz döngü, sistem çağrısı yağdırma) 30 ms bütçeyle
dener ve ardından zamanlayıcı kesmesinin hâlâ işlediğini doğrular.

**Masaüstü.** `kernel/src/ring3_app.rs::RemoteApp`, bir ELF'i `nc_app::App` olarak sarar: `opened` → spawn + `OP_INIT`,
`update` → `OP_UPDATE` (işaretçi/tuş/dil/vurgu rengi olay bloğunda), `draw` → `OP_DRAW` (komut akışı pencereye kırpılarak
çizilir), `closed` → `OP_CLOSE` + `kill_app`. Uygulama çökerse/watchdog tetiklenirse pencere açık kalır, "Uygulama
durduruldu" ve neden gösterilir; seri günlüğe de yazılır. Faz 6 geçmediyse veya ELF yoksa/bozuksa Ring 0 Kronometre kalır.

**`/ram` köprüsü.** `nc_fs::SharedMemFs`: tüm `VfsManager` örnekleri tek bir `MemFs`'i paylaşır; Ring 3'ün yazdığı
`/ram/...` Ring 0 Dosyalar uygulamasında da görünür (Faz 6 ikinci bir örnekle doğrular).

**Çizim ABI eklemeleri.** `OP_BUTTON` (tema düğmesini compositor çizer) ve `OP_TEXT` için `tracking`/`align`
(ortalama); `EventBlock.lang`. Böylece uygulama tema/yerleşim kodunu çoğaltmaz.

**Kronometre.** `crates/nc-r3-stopwatch` (mantık, ana makinede test edilir), `apps/stopwatch` (ELF girişi),
`crates/nc-app-sdk` (sistem çağrıları, `Gfx`, çalışma döngüsü, `app.ld`). `tests/parity.rs`, Ring 0 ve Ring 3
sürümlerinin **11 dilde, 7 durum dizisinde piksel piksel aynı** olduğunu ve `redraw` bayraklarının eşit olduğunu doğrular.
Dil tablosu `tools/gen_r3_strings.py` ile `nc-i18n`'den üretilir (testle karşılaştırılır).

### Test prosedürü
```bash
cargo test -p nc-mem -p nc-abi -p nc-elf -p nc-fs -p nc-app-sdk -p nc-r3-stopwatch
cargo test -p nc-ui --features remote
NC_RING3_APPS=1 bash build.sh run
```
`build.sh`, `NC_RING3_APPS=1` iken `apps/stopwatch`'u `x86_64-unknown-none` için derler
(`-C code-model=large -C relocation-model=static`, `crates/nc-app-sdk/app.ld`) ve `kernel/ring3_apps/stopwatch.elf`'e koyar.
Derleme başarısız olursa **uyarı verir, boş dosya bırakır ve Ring 0 Kronometre ile devam eder.**
Beklenen günlük: `[Faz 6b] watchdog iki senaryoda da sonlandirdi...`, `[Faz 6 OK] ...`,
`[Ring 3] Kronometre artik Ring 3'te calisiyor`. `[Faz 6b] watchdog testi basliyor` satırından sonra açılış
**takılırsa** watchdog çalışmıyor demektir; o satırı bana gönder.

### Bilinen sınırlar (güncel)
1. Watchdog zamanlayıcı kesmesine bağlı: LAPIC zamanlayıcı çalışmıyorsa yalnızca sistem çağrısı girişi yakalar
   (saf sonsuz döngü yakalanmaz). Faz 6b bunu açılışta sınar.
2. Ring 3 uygulama başına olay maliyeti ~0,2 ms (Faz 4 ölçümü); bu bütçe tüm Ring 3 uygulamaları için karedir.
3. Yalnızca gömülü uygulamalar: NTZ2 paketinden ELF yükü ve üçüncü taraf yükleme Adım 5.
4. `unsafe-native-modules` (NCBM) hâlâ duruyor; Adım 5'te silinecek.
5. Kronometre dışındaki uygulamalar Ring 0'da; her biri için `nc-app-sdk` + bir `nc-r3-<ad>` mantık crate'i
   gerekir. Çizim yüzeyi (paneller, düğmeler, metin, nokta) yeterli olanlar kolayca taşınır; Tarayıcı gibi
   ağ/dosya gerektirenler `SYS_FILE_*` üzerinden, ağ ise Adım 6'da.

## 9. Zorunlu Ring 3 ve yerleşik uygulamaların taşınması

### 9.1 Yapıldı: yüklenen her uygulama yalnızca Ring 3
- `nc-app-files`: NCBM (338 satır) ve `unsafe-native-modules` silindi. Üçüncü taraf paket → `nc_pkg::load_packaged_app`.
- `nc-pkg`: manifest `abi=ring3-v1`; `NtzcPackage::runnable()` (imzalı + abi + geçerli ELF), `capability_bits()`,
  `set_package_loader`/`load_packaged_app` kancası. `runnable()` olmayan paket kod çalıştırmayan bir bilgi kartı olarak kalır.
- Çekirdek: `ring3_app::install_package_loader` (Faz 6 geçtiyse; `nc_apps::builtin()`'den ÖNCE) paketi `RemoteApp`
  olarak masaüstüne bağlar; yetkiler mevcut politika maskesiyle (`SETTINGS_READ|FS_USER_READ|FS_USER_WRITE|NETWORK_CLIENT`)
  sınırlanır. Faz 6 geçmezse **hiçbir paket çalışmaz**.
- SDK: `sdk/build.sh <uygulama-dizini>` (Ring 3 ELF -> imzalı NTZ2), `sdk/build-ntzc.py` ELF'i paketlemeden önce denetler,
  şablon `sdk/ring3-sample`. Eski NCBM örnekleri `sdk/legacy-ncbm/`.
- Test: `cargo test -p nc-pkg` (ELF'li/ELF'siz/W+X/imzasız-v1 paketler, yetki eşleme, kanca).

### 9.2 Yerleşik uygulamalar: ölçülen bağımlılıklar
| Uygulama | Satır | Çekirdekten ne istiyor | Hizmet |
|---|---|---|---|
| Kronometre | 120 | hiçbir şey | **Ring 3'e taşındı** (komut akışı) |
| Sistem | 77 | `ctx` (ayarlar) | yok; ana-bilgisayar çalışma zamanı yeter |
| Dosyalar | 4461 | `VfsManager` ≈ 10 metot (`list_dir/stat/read/read_range/write/mkdir/remove/usb_*`), `nc_pkg` kurulum, `nc_media` (saf) | **VFS hizmeti** + paket kurma çağrısı |
| Ayarlar | 2658 | `nc_driver::global` (7), `nc_net::global` (3, kablosuz/DHCP), VFS | **Sistem-bilgi/ayar hizmeti** |
| Tarayıcı | 12688 | `TcpHttpClient`/`HttpState` (2 dosya, ~6 çağrı); `nc_js` saf | **HTTP hizmeti** |

`TcpHttpClient`, TCP/TLS durum makinesini kendisi yürütüp `NetworkStack` üzerinden paket gönderdiği için Tarayıcı'nın
ham soketi yok: doğru ayrım, TLS/TCP'nin çekirdekte kalıp HTML/CSS/JS/görsel çözücülerin (asıl saldırı yüzeyi) Ring 3'e
taşınmasıdır.

### 9.3 Tasarım (sıradaki adımlar)
1. **Ana-bilgisayar çalışma zamanı (`nc-app-host`)**: mevcut `nc_app::App` uygulamalarını DEĞİŞTİRMEDEN Ring 3'te çalıştırır.
   Pencere için kullanıcı belleğinde bir `Canvas`; `SYS_GUI_BACKDROP` (alttaki masaüstünü tampona kopyalar, alfa
   karışımı birebir kalsın) ve `SYS_GUI_PRESENT` (tamponu pencereye kırparak basar). Ring 3'e özel basit bir yığın
   ayırıcı (çekirdeğinki `cli` kullanıyor). `APP_HEAP_BYTES` 8 MiB'a çıkarılır.
2. **Cephe crate'leri**: `nc-fsapi`, `nc-netapi`, `nc-sysapi`. Uygulamalar `nc_fs`/`nc_net`/`nc_driver` yerine bunlara
   bağlanır; çekirdek derlemesinde gerçek crate'leri, Ring 3 derlemesinde hizmet istemcisini yeniden dışa aktarır.
3. **Hizmet ABI'si** (numaralar 23+), hepsi `nc-abi`'de, yetki ve kimlik denetimi çekirdekte:
   - `SYS_VFS_CALL(op, istek, uzunluk, yanıt, kapasite)`: tek çoğullanmış çağrı; rol (Files/Settings/Kullanıcı) süreç kimliğinden;
   - `SYS_HTTP_START/POLL/READ/CLOSE`: istek başına çekirdekte bir `TcpHttpClient`, `NETWORK_CLIENT` yetkisi, uygulama başına sınır;
   - `SYS_SYSINFO`/`SYS_SETTINGS_SET`: yetki bitlerine (`SETTINGS_WRITE_*`) göre;
   - `SYS_PKG_INSTALL(yol)`: çekirdek doğrular, kaydeder, masaüstüne uygulama bağlar (`Update.install_app` karşılığı).
4. **Sıra**: (0) Kronometre ELF derlemesinin doğrulanması → (1) ana-bilgisayar çalışma zamanı + Sistem →
   (2) VFS hizmeti + Dosyalar → (3) HTTP hizmeti + Tarayıcı → (4) Sistem-bilgi hizmeti + Ayarlar →
   (5) yerleşik Ring 0 kopyalarının çekirdek imajından kaldırılması (`nc-apps::builtin` yalnızca yükleyiciye iner).
5. Ring 0'da kalacaklar: sürücüler, VFS/NET/TLS hizmetleri, compositor, çekirdek.

> Adım (0) kapanmadan sonrası yazılmamalı: Ring 3 ELF zinciri (code-model, bağlayıcı betiği) gerçek derleyiciyle
> bir kez doğrulanmalı; aksi halde üç büyük uygulama aynı bilinmeyen üzerine kurulur.

## 10. Derleme zinciri düzeltmesi ve Sistem + Dosyalar'ın Ring 3'e taşınması

### 10.1 Ring 3 derleme zinciri (Kronometre ELF'inin derlenmemesinin kök nedeni)
`error: failed to load bitcode of module "core..."` + `Code Model: 'i32 2' vs 'i32 4'`: rustup'in önceden derlenmiş `core`'u
`kernel` kod modeliyle gelir; Ring 3 imajı 0x80_0100_0000'a (4 GiB üstü) bağlandığı için `large` gerekir ve ikisi karışamaz
(LTO patlar; LTO kapalıyken bile `core`'daki 32-bit mutlak adresler bağlanamaz). Çözüm `tools/ring3-cargo.sh`:
`RUSTC_BOOTSTRAP=1 cargo build -Zbuild-std=core,alloc,compiler_builtins` (nightly gerekmez; `rustup component add rust-src`
betik tarafından yapılır), LTO kapalı. Kripto bağımlılıkları çekirdekle aynı `--cfg ..._force_soft/serial` ayarlarını alır.
Başarısızlıkta `build/ring3-build-<ad>.log` ve son 40 satır basılır; ilgili uygulama Ring 0'da kalır.

### 10.2 Ana-bilgisayar çalışma zamanı (`crates/nc-app-host`)
Mevcut `nc_app::App` uygulamalarını DEĞİŞTİRMEDEN Ring 3'te çalıştırır: Ring 3 yığın ayırıcısı (`heap.rs`, ana makinede
rastgele iş yüküyle ve Python modeliyle sınanır), `Context`/`Input` ↔ `EventBlock`+`ContextBlock` dönüşümleri,
pencere boyutunda piksel tamponu (`SYS_GUI_BACKDROP` altta kalan masaüstünü kopyalar, `SYS_GUI_PRESENT` kırparak basar),
uygulamaya pencere (0,0)-(w,h) ve pencereye göreli işaretçi verilir. Uygulama ikilisi yalnızca `_start`, `#[global_allocator]`
ve fabrika işlevidir (`apps/system`, `apps/files`).

### 10.3 VFS hizmeti (`SYS_VFS_CALL`) ve Dosyalar
`nc-fs-facade`: `kernel` özelliği gerçek `nc-fs`'i, `client` özelliği Ring 3 istemcisini dışa aktarır; `nc-app-files` bağımlılığı
`nc-fs = { package = "nc-fs-facade" }` olarak yeniden adlandırıldığından Dosyalar'ın KAYNAĞI DEĞİŞMEDİ. Tel biçimi `nc-abi::vfs`
(alloc'suz, sıkı doğrulamalı, fuzz'lı); çekirdek hizmeti rolü süreç kimliğinden türetir (yalnızca `BuiltinSystemApp` + `nc.files`;
paket uygulamaları bu hizmeti KULLANAMAZ, `SYS_FILE_*` ile `/ram` ve `/data/documents` ile sınırlıdır). Yazma parçalıdır
(≤ 48 KiB), okuma aralıklıdır. Paket kurma: Dosyalar paketi `/data/app`'e yazar, `SYS_PKG_INSTALL` ile çekirdeğe bildirir;
çekirdek paketi **kendisi** yeniden doğrular (imza + `abi` + ELF) ve uygulamayı kendisi bağlar (`RET_INSTALL`).

### 10.4 Test prosedürü
```bash
cargo test -p nc-abi -p nc-elf -p nc-pkg -p nc-fs -p nc-fs-facade --features nc-fs-facade/client -p nc-app-host \
           -p nc-app-sdk -p nc-r3-stopwatch -p nc-app-files
cargo test -p nc-ui --features remote
python3 tools/model/heap_model.py
NC_RING3_APPS=1 bash build.sh run
```
(`nc-fs-facade` testleri `client` özelliğini ister; `--features` yerine `cargo test -p nc-fs-facade --no-default-features --features client`.)
Beklenen günlük: `Ring 3 stopwatch/system/files ELF: N bayt`, `[Ring 3] nc.files artik Ring 3'te calisiyor`.

### 10.5 Kalan (Ring 0'da duran): Ayarlar ve Tarayıcı
- **Tarayıcı**: HTTP hizmeti (`SYS_HTTP_START/POLL/READ/CLOSE`): çekirdek istek başına bir `TcpHttpClient` tutar (TLS/TCP çekirdekte),
  `NETWORK_CLIENT` yetkisi ve uygulama başına sınır; Tarayıcı'nın `nc_net` kullanımı (2 dosya, ~6 çağrı) `nc-net-facade` ile değişir.
- **Ayarlar**: `nc_driver::global` (7) ve `nc_net::global` (3) çağrıları için sistem-bilgi/ayar hizmeti ve `SETTINGS_WRITE_*` denetimi;
  ayar değişikliklerinin çekirdeğe geri yazımı (ana-bilgisayarda `ctx.settings` farkı).

### 10.6 Bellek düzeni değişikliği (gömülü ELF'ler çekirdek imajını büyüttü)
Ring 3 ELF'leri `include_bytes!` ile çekirdeğe gömüldüğü için imaj 6 MiB rezervasyonunu aştı (`kernel runtime image ... exceeds`).
Düzen: çekirdek imajı `0x10_0000`'dan en çok **14 MiB** (`linker.ld` ASSERT; UEFI yükleyici aynı miktarı ayırır), arena/PMM
**16 MiB'ta** başlar (`ARENA_BASE`), `.text` Stage2'nin 0..8 MiB yürütülebilir penceresinde kalmak ZORUNDA (ayrı ASSERT; büyüyen
`.rodata/.data` 8 MiB'ı aşabilir, NX olması doğrudur). KPT'nin ince bölme kapasitesi (8 x 2 MiB) 16 MiB'a kadar yeter.
Ring 3 ELF'leri `strip = "symbols"` ile derlenir. Kalıcı çözüm: uygulamaları çekirdeğe gömmek yerine mühürlü `/system`
bölümünden yüklemek (Verified Boot ile korunur); bu, tüm yerleşik uygulamalar Ring 3'e geçtikten sonra yapılacak.
