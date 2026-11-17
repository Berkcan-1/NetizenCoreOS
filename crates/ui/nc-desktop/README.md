# nc-desktop

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/ui/nc-desktop` |
| **Katman** | Arayüz |
| **Bağımlılıklar** | `nc-app`, `nc-assets`, `nc-audio`, `nc-font`, `nc-fs`, `nc-gfx`, `nc-hal`, `nc-math`, `nc-media`, `nc-ui` |
| **Kullananlar** | `nc-kernel` |
| **Testlerde kullananlar** | `nc-apps` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

NetizenCore masaustu kabugu. Duvar kagidi, masaustu simge izgarasi, pencere yoneticisi, dock, baslat menusu, sag tik menusu, gorev yoneticisi ve masaustu diyaloglari bu crate icinde calisir. `no_std` + `alloc`, tum cizim tamsayi matematigi ile yapilir.

Genel API (kernel tarafi): `DesktopState::new`, `install_app`, `update`, `take_dirty`, `take_wallpaper_dirty`, `wallpaper_index`, `app_count`, `sync_desktop_fs`, `draw_wallpaper`, `draw`, `DesktopAction`.

Tasarim dili: Frutiger Aero cam. Her yuzey arkasindaki goruntuyu bulaniklastirir, ustune beyaz bir cam tonu, ust yansima ve ince beyaz kenar cizgisi koyar. Uygulama simgeleri parlak, yuvarlak kose renkli cam karolardir (uygulama kimliginden sabit renk secilir). Yazilar cam ustunde okunsun diye acik zeminde koyu, havadar zeminde golgeli beyazdir.

Yerlesim `unit_for(width, height)` ile cozumunurluge gore olceklenir. Uygulama sayisi dock'a sigmadiginda orta alan sayfali karusele donusur; oklarla dongusel gezinme ve yeni uygulamaya son sayfadan otomatik gecis desteklenir. Sayfa hareketi easing tabanli yatay animasyonla cizilir.

Belgeler: `src/README.md` modul ve davranislari, `UI_DESIGN_STANDARD.md` ise olcek, dock, renk, hareket ve test standartlarini tanimlar.

## Görünüm katmanı (bu sürümde)

* `art.rs`: uygulama ikonları. 256×256 ana çizim; tam sayı SDF ile kenar yumuşatmalı vektör şekiller, yumuşak gölge, dört kademeli mip zinciri (256/128/64/32), çift doğrusal örnekleme. İkonlar arka plansızdır. Yeni ikon eklemek için `paint_*` fonksiyonu yaz, `ArtKind`'e ekle, `nc_app::Icon` ile eşle. Önizleme: `NC_ART_DUMP=/tmp/ikonlar.ppm cargo test -p nc-desktop art`.
* `wallpaper.rs`: gömülü resimler (`assets/wallpapers`, bkz. oradaki README) ekran boyuna çözülür; resim yoksa eski gradyan çizilir. `draw_animated_wallpaper` boş kancadır.
* Arayüz ölçeği: `layout::unit_for` temel birimi (`min(yükseklik/40, genişlik/50)`) `nc_app::theme::UI_SCALES` yüzdesiyle çarpar (%80–150, varsayılan %100). Ayarlar → Görünüm → Arayüz boyutu. Seçim `/data/system/theme.cfg` içinde `scale=` olarak saklanır; ölçek değişince açık pencereler yeniden yerleşir.
* Tema: nötr serin palet (grafit mürekkep, buz beyazı), Aero cam çerçeve korunur (cam yoğunluğu ve parlama daha sade).
* Ana makinede ekran görüntüsü: `NC_UI_DUMP=/tmp/ui cargo test -p nc-apps --test ui_render` açık/koyu, 720p/1080p ve %150 ölçekte masaüstünü `.ppm` olarak yazar.
