# nc-app

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/ui/nc-app` |
| **Katman** | Arayüz |
| **Bağımlılıklar** | `nc-crypto`, `nc-font`, `nc-gfx`, `nc-hal`, `nc-i18n`, `nc-ui` |
| **Kullananlar** | `nc-app-browser`, `nc-app-files`, `nc-app-host`, `nc-app-settings`, `nc-app-stopwatch`, `nc-app-system`, `nc-apps`, `nc-browser-r3`, `nc-desktop`, `nc-files-r3`, `nc-pkg`, `nc-settings-r3`, `nc-system-r3`, `nc-kernel`, `nc-kernel-aarch64`, `nc-sys-facade` |
| **Testlerde kullananlar** | `nc-r3-stopwatch` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Uygulama sozlesmesi. Masaustu kabugu (nc-desktop) ile uygulama crate'leri arasindaki tek ortak arayuz budur. Kabuk hicbir
uygulamayi adiyla bilmez; uygulamalar kabugu bilmez. Bagimlilik: nc-hal, nc-gfx, nc-font, nc-ui. alloc kullanir
(cekirdegin heap'i gerekir).

## Genel API

- App (trait): info, opened, closed, update, draw
- AppInfo: id, name, version, icon, width_units, height_units (pencere boyutu, unit cinsinden). Alanlar String'dir; cunku
  ileride paket yoneticisi calisma aninda uygulama kaydedecek.
- Context: platform, screen_width, screen_height, unit, uptime_ms, settings. Kabuk her cagrida taze bir kopya verir.
- Settings: wallpaper, accent (Copy). Uygulama ctx.settings'i degistirirse kabuk farki alir ve duvar kagidini/arayuzu
  yeniler.
- Input: area (pencerenin icerik alani), pointer (Option<Pointer>), key, focused.
- Pointer: x, y, down, clicked (ekran koordinatlari). Imlec uygulamanin alaninda degilse None gelir (hover temizlenir).
- Update: redraw, close. Sabitler: Update::NONE, Update::REDRAW, Update::CLOSE.
- theme: WALLPAPERS, ACCENTS, accent_tint, accent_hover, SIDEBAR, SIDEBAR_HOVER
- icon: Icon (Sliders, Monitor, Stopwatch, Generic), draw_icon ve kucuk cizim yardimcilari (draw_line, draw_cross,
  draw_dash, draw_grid)
- text: Line (heap'siz metin tamponu), centered, shadowed, horizontal_line

## Yasam dongusu

1. Kabuk uygulamayi acinca opened(ctx) cagrilir; kapaninca closed(ctx).
2. update(ctx, input) acik ve kucultulmemis her uygulama icin her karede cagrilir. Gorunur bir degisiklik varsa
   Update::REDRAW dondurmelidir; donmezse ekran yeniden cizilmez. Surekli animasyon (ornegin calisan kronometre) her
   karede REDRAW dondurur.
3. draw(canvas, area, ctx) sadece kabuk sahneyi yeniden cizerken cagrilir. Saf olmalidir: durumu degistirmez, sadece
   area icine cizer.
4. Uygulama kendini kapatmak isterse Update::CLOSE dondurur.

## Yeni uygulama eklemek

1. apps/<ad>/nc-app-<ad> olustur (Cargo.toml: nc-app, nc-gfx, nc-font, nc-ui bagimliliklari; ornek: apps/stopwatch/nc-app-stopwatch).
2. Kok Cargo.toml uye listesine ekle.
3. apps/registry/nc-apps/Cargo.toml'a bagimlilik, apps/registry/nc-apps/src/lib.rs'e bir apps.push(Box::new(...)) satiri ekle.
4. Uygulamanin kendi klasorune README.md ve src/README.md yaz.
5. Ring 3'te de calissin istiyorsan apps/README.md'deki Ring 3 adimlarini izle.

Uygulama boyutunu AppInfo icinde unit cinsinden ver. Tum olculer ctx.unit ile carpilmalidir; boylece her cozunurlukte
orantili kalir.

## Sinirlar (bilerek)

- Ring 0 yerlesik uygulamalar derleme zamaninda cekirdege baglanir ve cekirdekle ayni yetkide calisir (Ring 3 secenegi icin apps/README.md). Bir Ring 0 uygulamasinin cokmesi
  tum sistemi dusurur; bu uygulamalar icin izolasyon yoktur. Izolasyon Ring 3 yoluyla saglanir (apps/README.md).
- Ayarlar kalici degildir; yeniden baslatinca varsayilana doner. Kalici saklama dosya sistemi gerektirir.
- Klavye girdisi su an sadece KeyCode (yon tuslari, Enter, Escape, Space, Tab); metin girisi yoktur.
- Kabuk, acilmis uygulamalari simge izgarasinda, baslat menusunde ve gorev cubugunda listeler. Baslat menusu 720p'de en
  fazla 6 uygulama gosterir; gorev cubugu dolunca taser. Cok sayida uygulama icin kaydirma gerekir.
