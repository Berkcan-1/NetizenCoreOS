# nc-assets/src

## lib.rs

LOGO_BLOB: include_bytes! ile OUT_DIR/logo.raw. logo(): basligi ("NCLG") ve boyutlari dogrular, piksel diliminin
yeterli uzunlukta oldugunu kontrol eder; gecersizse veya genislik/yukseklik 0 ise None.
Piksel sirasi: satir satir, RGBA (R, G, B, A baytlari).

Not: build.rs bu klasorde degil, kutunun kokundedir (crates/nc-assets/build.rs).
