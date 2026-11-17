# nc-home-r3 — NetizenHome

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/apps/home/nc-home-r3` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-abi`, `nc-app-sdk` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Ring 3 (EL0t) birleştirici (compositor), pencere yöneticisi, durum çubuğu, Kontrol Merkezi, alt gezinme çubuğu, Son Uygulamalar ekranı ve ana başlatıcı.
Çizim `nc-app-sdk::Gfx` ve paylaşılan `nc_app_sdk::ui` bileşenleriyle yapılır; alt uygulamaların (`nc.files`, `nc.settings`) yüzey akışları `OP_EMBED_SURFACE` ile `nc.home` içinde animasyonlu olarak birleştirilir.

## Mimari ve Özellikler (v3 — Tam Ring 3 Birleştirici)

- **Ring 3 Pencere Birleştirici ve Animasyon Motoru (`app_anim`)**: Uygulama açılış ve kapanışlarında `ease_out_cubic` eğrisiyle ölçekleme ve dikey kayma geçişi uygular; `RET_ANIMATING` bayrağıyla çekirdekten 50 Hz animasyon zamanlaması alır.
- **Animasyonlu 3 Düğmeli Alt Gezinme Çubuğu (`nav_anim`)**: Bir uygulama veya Son Uygulamalar ekranı aktif olduğunda alttan yumuşakça yükselen **Geri (`<`)**, **Ana Ekran (`O`)** ve **Son Uygulamalar (`[]`)** düğmeleri.
- **Son Uygulamalar (`Recents`) Ekranı**: Arka planda (`Paused`) bekleyen EL0 uygulamalarının canlı yüzey önizlemelerini (`OP_EMBED_SURFACE`) kartlar halinde gösterir; kartı yukarı kaydırmak (`swipe-up`) uygulamayı `sys::app_stop` ile sonlandırıp EL0 adres uzayını serbest bırakır.
- **Üst Durum Çubuğu ve Kontrol Merkezi (`cc_anim`)**: Üst çubuğa veya sol widget'a dokunulduğunda yukarıdan kayarak açılan Kontrol Merkezi; **Wi-Fi**, **Bluetooth** ve **Parlaklık (%20–%100)** kaydırıcısını barındırır ve **Ayarlar (`nc.settings`)** uygulamasıyla çift yönlü senkronize çalışır (`/data/appdata/nc.settings/settings.cfg`).
- **Ana Ekran Widget Sistemi, Serbest Izgara ve 4'lü Klasörler**:
  - **Saat & Bağlantı Widget'ı** ve **Depolama & Görev Widget'ı**.
  - Simgelere basılı tutarak (`long-press`, `340 ms`) veya **Düzenle** modunda 4×3 ızgara üzerinde serbest taşıma.
  - Bir uygulamayı diğerinin üzerine bırakarak **4'lü Klasör** (2×2 mini simge önizlemeli squircle ve klasör içi açılır pencere / grubu çözme) oluşturma.

## Derleme

`bash build.sh armv8a qemu` bunu `aarch64crates/tools/aarch64.sh` içinde `aarch64-unknown-none` için derler.
