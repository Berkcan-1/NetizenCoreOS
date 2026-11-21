# docs

Proje geneli dokümanlar. Modüle özel ayrıntılar ilgili klasördeki README.md dosyalarındadır; burası yalnızca genel resmi verir.

| Klasör / Dosya | İçerik |
|---|---|
| `architecture/ARCHITECTURE.md` | Açılış akışı, bellek haritası, profiller, kurallar |
| `architecture/LAYOUT.md` | Klasör düzeni, katman kuralı, "yeni şey nereye konur" |
| `architecture/MODULES.md` | Tüm modüllerin tek satırlık özeti |
| `architecture/DEPENDENCIES.md` | Bağımlılık haritası (`tools/docs/sync_module_docs.py` üretir, elle düzenlemeyin) |
| `ring3/RING3_YOL_HARITASI.md` | Ring 3 yol haritası ve durum |
| `security/GUVENLIK_DUZELTMELERI.md` | Güvenlik düzeltme kayıtları |
| `changelog/` | Tarihli güncelleme notları (tarihsel kayıt; yollar o günkü düzene göredir) |
| `migration/ESKI-YENI-YOL-HARITASI.md` | Klasör yeniden düzenlemesinin eski → yeni yol tablosu |
| `assets/` | Dokümanlarda kullanılan görseller |

Yeni bir crate eklediğinde: `tools/docs/modules.toml`'a özet ekle, `python3 tools/docs/sync_module_docs.py` çalıştır.
