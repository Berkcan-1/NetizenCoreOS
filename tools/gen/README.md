# tools/gen

`gen_r3_strings.py`: `crates/foundation/nc-i18n/langs/*.lang` dosyalarından `apps/stopwatch/nc-r3-stopwatch/src/strings.rs`
dosyasını üretir (Running, Stopped, Start, Stop, Reset sütunları). `nc-i18n` metinlerini değiştirdikten sonra çalıştır:

    python3 tools/gen/gen_r3_strings.py

Üretilen dosyayı elle düzenleme; `apps/stopwatch/nc-r3-stopwatch/tests/parity.rs` güncel olup olmadığını denetler.
