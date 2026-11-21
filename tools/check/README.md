# tools/check

`verify_layout.py` — derleyici gerektirmeyen yol ve düzen denetimi (Python 3.11+).

    python3 tools/check/verify_layout.py

Denetlediği şeyler (hata = çıkış kodu 1):

- Tüm `Cargo.toml` `path` bağımlılıkları var olan bir pakete çözülüyor ve paket adı eşleşiyor.
- Kök workspace üyeleri var; üye olmayan ve bağımlılık da olmayan paket yok.
- Katman kuralı (`foundation ← hardware ← services ← graphics ← ui ← runtime ← apps ← kernel`); bootloader yalnızca foundation'a bağlanır.
- `include_bytes!`, `include_str!` ve `#[path]` hedefleri mevcut.
- Kabuk betiklerindeki proje yolları mevcut.

Uyarı olarak raporladıkları: `build.rs` içinde görülen ama bulunmayan isteğe bağlı dosyalar ve README/Python içindeki bulunamayan yol metinleri
(`--strict-docs` bunları hata yapar). `docs/changelog` ve `docs/migration` (tarihsel) ile `legacy*` klasörleri hariç tutulur.

Seçenekler: `--root YOL`, `--no-layers`, `--strict-docs`.
