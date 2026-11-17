# tools/docs

Doküman üretimi: crate bağımlılık bilgisi `Cargo.toml` dosyalarından okunur, tek satırlık özetler `modules.toml` içinden gelir.

    python3 tools/docs/sync_module_docs.py            dosyaları günceller
    python3 tools/docs/sync_module_docs.py --check    güncel değilse çıkış kodu 1 (hiçbir şey yazmaz)

Ürettikleri:

- Her crate `README.md` dosyasının başındaki `<!-- otomatik:başla -->` ... `<!-- otomatik:bitti -->` bloğu (konum, katman, bağımlılıklar, kullananlar).
  Bloğun dışındaki metin elle yazılır ve korunur; README yoksa oluşturulur.
- `crates/<katman>/README.md` içindeki crate tablosu.
- `docs/architecture/DEPENDENCIES.md` ve `docs/architecture/MODULES.md` (tamamen üretilir).

Yeni crate eklerken önce `modules.toml`'a `[paket-adı]` + `summary` satırı ekle.
