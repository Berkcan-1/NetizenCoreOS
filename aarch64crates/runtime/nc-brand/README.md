# nc-brand

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/runtime/nc-brand` |
| **Katman** | Ring 3 çalışma zamanı |
| **Bağımlılıklar** | `nc-app-sdk`, `nc-mobile-ui` |
| **Kullananlar** | `nc-files-v8a`, `nc-settings-v8a` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

ARMv8-A mobil uygulamalarının (ana ekran, Ayarlar, Dosyalar) ortak görünüm ve tema katmanı. `nc-mobile-ui` içindeki temel mobil bileşenleri yeniden dışa aktarır ve aşağıdaki bileşenleri marka diliyle değiştirir. x86_64 masaüstü uygulamalarından tamamen bağımsızdır.

## Marka dili: Damla

Su ve sıvı cam hissi. İmza öğe, parlak bir yansıma boncuğudur ("bead"): simge plakalarında sol üstte, kaydırıcı düğmesinde halka olarak, sekme çubuğunda seçili sekmenin altında nokta olarak, başlığın önünde ve sayfa göstergesinde yeniden görünür.

- `draw_app_tile`, `draw_folder_tile`: gölge, kenar ışığı, alt gölgelendirme, üst parlama ve boncuk
- `draw_card`, `draw_pill_btn`, `draw_switch`, `draw_slider`: yumuşak çift gölge, parlama, düğme gölgesi ve boncuk halkası
- `draw_title`, `draw_segmented`: Ayarlar ve Dosyalar'ın aynı başlık ve sekme çubuğunu kullanması için
- `draw_dock`, `draw_tide_indicator`: buzlu cam kapsül dock ve sayfa göstergesi
- `draw_soft_glow`: kademeli halkalarla yumuşak ışık, duvar kağıdındaki akışkan lekeler için

Çizim komutu bütçesi (yüzey başına 512 kayıt, 16 KiB) gözetilerek yazıldı; yeni bileşen eklerken bunu koru.

<!-- otomatik:baslangic -->
<!-- otomatik:bitis -->
