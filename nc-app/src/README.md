# nc-app/src

## lib.rs

Sozlesmenin tum tipleri (App, AppInfo, Settings, Context, Pointer, Input, Update). Ayrintili aciklama ust klasordeki
README.md'de. Settings::accent_color, ACCENTS dizisinden rengi dondurur (indeks tasmasina karsi modulo alir).

## theme.rs

Duvar kagitlari (WALLPAPERS: ad, ust-alt renk, iki parilti) ve vurgu renkleri (ACCENTS). Duvar kagidini nc-desktop
draw_wallpaper ile cizer; Ayarlar uygulamasi onizleme icin ayni verileri okur. Yeni duvar kagidi eklemek icin dizi
uzunlugunu (WALLPAPER_COUNT) ve girdiyi birlikte arttir.

## icon.rs

Icon listesi ve vektor ikonlar. Hepsi tek renkle cizilir (color); boyutu size verir. Yeni ikon eklemek icin enum'a bir
deger, draw_icon'a bir kol ve bir cizim fonksiyonu ekle.

## text.rs

Line: 56 baytlik sabit tampon; text() metin, number(deger, en_az_hane) sayi ekler. as_str() okur. Tampon dolarsa fazlasi
sessizce atilir. centered, shadowed ve horizontal_line kucuk cizim yardimcilaridir.
