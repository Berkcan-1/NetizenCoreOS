# nc-ui/src

## lib.rs

- text_style: stroke_sixteenths = max(cap*16/18, 22); tracking = cap*yuzde/100.
- rounded_distance: yuvarlatilmis dikdortgen icin isaretli mesafe (1/256 piksel). Sadece koselerde karekok alinir.
- panel_pixel: tek pikselin dolgu ve kenar kapsamasini hesaplar. draw_panel, derin ic satirlarda (kenardan yaricap+2
  piksel iceride) yatay aralik olarak doldurur (opak ise fill_rect), kenar ve kose sutunlarinda panel_pixel kullanir.
- draw_shadow: area, offset_y kadar asagi kaydirilir; kenardan spread piksel uzakliga kadar karesel solar. strength 0..255.
  area'nin derin ic kismi (yaricap+2 piksel iceride) atlanir.
- draw_card: golge (offset 4, yayilma 12, guc 18) ve beyaz panel.
- draw_badge, draw_radio, draw_button, draw_accent_button: kucuk kontrol bilesenleri. Dugme yuksekligi metin boyutunu belirler.
- draw_cursor: 11x16 piksellik ok imleci (CURSOR_PIXELS: 1 = koyu kenar, 2 = beyaz ic).
