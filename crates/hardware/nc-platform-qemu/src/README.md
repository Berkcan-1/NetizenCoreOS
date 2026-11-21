# nc-platform-qemu/src

## lib.rs

QemuPlatform: birim yapi. new() const. dispi_read/dispi_write: 0x1CE (indeks) ve 0x1CF (veri) portlari.
Dispi yazmaclari: 0 kimlik, 1 genislik, 2 yukseklik, 3 bpp, 4 etkinlestirme (0x41 = acik + dogrusal).
Platform uygulamasi: init seri portu kurar; write_log her baytı debugcon'a, ayrica seri porta (satir sonlarini CR+LF yaparak) yazar.
