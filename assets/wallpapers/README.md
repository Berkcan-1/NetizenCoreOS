# assets/wallpapers

Masaüstü duvar kâğıtları. Bu klasördeki **en fazla 6** resim derleme sırasında çekirdeğe gömülür.

* Biçim: PNG veya JPEG. Dosyalar ada göre sıralanır; ilk 6'sı sırayla Ayarlar'daki 1–6 numaralı kutulara girer.
* Derleme hattı (`crates/graphics/nc-assets/build.rs`) her resmi 1920×1080'e sığacak şekilde küçültür, JPEG olarak yeniden sıkıştırır ve toplam boyutu yaklaşık 2,2 MB altında tutmak için kaliteyi/çözünürlüğü otomatik düşürür (çekirdek sınırı 7 MB). Büyük ve detaylı fotoğraflarda uyarı verir.
* Ayarlar'daki küçük önizleme rengi, resmin üst ve alt bölgesinin ortalamasından otomatik hesaplanır.
* 6'dan az resim koyarsan eksik kutular eski düz gradyan duvar kâğıdını gösterir.
* Şu an klasördeki `wallpaper1..6.jpg` yer tutucudur; kendi resimlerinle üzerine yaz veya sil.
* Başka klasör kullanmak için: `NC_WALLPAPER_DIR=/yol bash build.sh qemu`.

Hareketli duvar kâğıdı henüz yok: `nc-desktop/src/wallpaper.rs` içindeki `draw_animated_wallpaper` boş bir kancadır; resimli duvar kâğıdında her karede çağrılır.
