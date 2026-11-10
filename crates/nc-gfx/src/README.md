# nc-gfx/src

## lib.rs

Rect, rgb, lerp_color (kanal basina agirlikli ortalama), Canvas ve present fonksiyonlari.

- invert: rengin R, G, B kanallarini tersine cevirir (murekkep katmani icin).
- vertical_gradient: satir basina 8.8 sabit noktali renk hesaplar, piksel basina 4x4 dither esigi ekler (DITHER).
- add_glow: dairesel, ikinci derece sonumlenen toplamali parilti (koyu zeminler icin). strength 0..256.
- blend_glow: ayni parilti, ama rengi toplamak yerine pikseli renge dogru karistirir (acik zeminler icin); dither'lidir.
- blend_pixel: alfa 0..255; sinir disi pikselleri yok sayar.
- draw_rgba: RGBA bayt dizisini alfa ile karistirarak cizer.
- copy_rect_from: iki Canvas arasinda sinirlari kontrol ederek bolge kopyalar.
- present_region: kirpilmis bolgeyi framebuffer'a volatile yazar; bicim 32 bpp 0x00RRGGBB ise donusum yapmaz,
  aksi halde pack ile kanal konumlarina gore dizer.
- present: tum ekran.

## blur.rs

box_blur: iki gecisli (yatay+dikey, tekrar yatay+dikey) kayan pencere kutu bulaniklastirma. Pencere disi pikseller
siyah sayilir (murekkep katmanlari icin dogru). Bolme yerine karsilik carpimi kullanir. radius 0 ise kopyalar.

## compose.rs

- lerp_packed(a, b, t): t 0..256, R-B ve G'yi ayri maskeyle karistirir.
- compose_ink(hedef, hx, hy, zemin, murekkep_a, murekkep_b, mix, opaklik, alan): murekkep = karistir(a, b, mix) * opaklik;
  sonuc = zemin * (256 - murekkep) / 256 (kanal basina, multiply). alan, sahne koordinatlarindadir; hedefe (hx, hy) ofsetiyle yazilir.
- lerp_canvases(hedef, kaynak, bitis, t): tum tampon icin dogrusal karistirma (ekran gecisi).
