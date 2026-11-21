# Tur 2 — hız, PNG simgeler, Ayarlar ve Dosyalar

Bu klasörü deponun köküne olduğu gibi kopyala (aynı yollar). Sonra normal derle.
Not: bu ortamda Rust derleyicisi yoktu, hiçbir şey derlenmedi. Derleme hatası çıkarsa çıktıyı gönder.

## Kasma (çekirdek + çizim)
- aarch64kernel/src/host.rs: aynı komut akışı tekrar gelirse kare tamamen atlanır (çizim + ekrana yazma yok)
- nc-gfx present_diff: ekran belleğine yalnızca değişen piksel aralıkları yazılır (boot.rs bir gölge tampon ayırır; bellek yoksa eski tam sunuma düşer)
- nc-gfx blend_span / fill_rect_alpha: yarı saydam alanlar bölmesiz hızlı karıştırma (Kontrol Merkezi karartması, ekran parlaklığı, paneller)
- nc-ui draw_panel / draw_dot: düz kenar satırları ve daire içi şerit olarak çizilir
- wallpaper.rs: duvar kağıdı sayacı (kare atlama imzasına dahil)

## PNG simgeler
- assets/icons/ic_files.png, ic_settings.png (verdiğin dosyalar, 512 px)
- nc-assets build.rs: 48/96/144/192 px boyutlarına küçültür, köşeleri %27 yuvarlar, önceden çarpılmış ARGB gömer
- Yeni ABI komutu OP_ICON (nc-abi), Gfx::icon(id, x, y, w, h) (SDK), compositor çizimi (nc-ui remote)
- Yeni simge eklemek: assets/icons/ic_browser.png, ic_system.png, ic_stopwatch.png koy (adlar nc-assets build.rs ICON_NAMES'te hazır), home aero.rs icon_id()'ye Glyph eşle

## Ayarlar
- Ana liste sayfası (cihaz kartı + gruplu kartlar, renkli yuvarlak rozetler), detay sayfaları
- Sayfa geçişi kayma animasyonu, açılışta kademeli yükselme, anahtarlar kayarak değişir
- Geri: sol üst düğme veya Esc

## Dosyalar
- Başlıkta PNG simge, kayan sekme tutamağı, kayan seçim vurgusu, klasör değişince satırlar kademeli belirir

## Ortak (nc-brand)
- Düz kart (parlama yok), animasyonlu anahtar, sade kaydırıcı ve başlık, kayan segment kontrolü

## Çekirdek
- Ön plandaki Ayarlar/Dosyalar RET_ANIMATING dönünce 50 Hz zamanlayıcı güncellemesi alır
