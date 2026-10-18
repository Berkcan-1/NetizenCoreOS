# nc-app

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

1. crates/nc-app-<ad> olustur (Cargo.toml: nc-app, nc-gfx, nc-font, nc-ui bagimliliklari; ornek: nc-app-stopwatch).
2. Kok Cargo.toml uye listesine ekle.
3. crates/nc-apps/Cargo.toml'a bagimlilik, crates/nc-apps/src/lib.rs'e bir apps.push(Box::new(...)) satiri ekle.
4. Uygulamanin kendi klasorune README.md ve src/README.md yaz.

Uygulama boyutunu AppInfo icinde unit cinsinden ver. Tum olculer ctx.unit ile carpilmalidir; boylece her cozunurlukte
orantili kalir.

## Sinirlar (bilerek)

- Uygulamalar derleme zamaninda cekirdege baglanir ve cekirdekle ayni yetkide (ring 0) calisir. Bir uygulamanin cokmesi
  tum sistemi dusurur; izolasyon yoktur. Gercek izolasyon icin kullanici modu, sistem cagrilari ve sayfa tablosu ayrimi
  gerekir (cekirdek isi).
- Ayarlar kalici degildir; yeniden baslatinca varsayilana doner. Kalici saklama dosya sistemi gerektirir.
- Klavye girdisi su an sadece KeyCode (yon tuslari, Enter, Escape, Space, Tab); metin girisi yoktur.
- Kabuk, acilmis uygulamalari simge izgarasinda, baslat menusunde ve gorev cubugunda listeler. Baslat menusu 720p'de en
  fazla 6 uygulama gosterir; gorev cubugu dolunca taser. Cok sayida uygulama icin kaydirma gerekir.
