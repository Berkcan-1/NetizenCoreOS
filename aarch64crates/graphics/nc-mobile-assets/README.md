# nc-mobile-assets

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/graphics/nc-mobile-assets` |
| **Katman** | Grafik |
| **Bağımlılıklar** | — |
| **Kullananlar** | `nc-kernel-aarch64` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

ARMv8-A mobil hedefinin bağımsız varlık gömme katmanı (`aarch64crates/assets/icons/*.png` → derleme zamanında yuvarlatılmış ARGB mipmap tablosu).

x86_64/masaüstü `nc-assets` crate'inden tamamen bağımsızdır; masaüstü duvar kâğıtları, açılış sesi veya masaüstü logosu mobil çekirdeğe gömülmez.

<!-- otomatik:baslangic -->
<!-- otomatik:bitis -->
