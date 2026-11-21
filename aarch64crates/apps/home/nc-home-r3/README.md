# nc-home-r3 — NetizenHome

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/apps/home/nc-home-r3` |
| **Katman** | Uygulamalar |
| **Bağımlılıklar** | `nc-abi`, `nc-app-sdk`, `nc-mobile-ui` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

NetizenHome: Ring 3 (EL0) kabuk. Android'deki Aerolon başlatıcısının (Kotlin + C++) Aero tarzının Rust karşılığı. Birleştirici, pencere yöneticisi, durum çubuğu, Kontrol Merkezi, Son Uygulamalar, çekmece, arama, klasör ve widget sayfasını içerir. Alt uygulamalar (`nc.files`, `nc.settings`) `embed_surface` ile gömülür.

## Dosyalar

- `main.rs`: giriş, olay döngüsü, çizim sırası.
- `shell.rs`: durum, animasyon adımlama, başlatma/ana ekran/geri/son uygulamalar, ızgara ve dock modeli (taşıma, klasörleme, çözme).
- `input.rs`: dokunma durum makinesi (basma, tutma, bırakma), vuruş testi, klavye (ESC, arama yazımı).
- `layout.rs`: tüm geometri (`Geo`). Çizim ve vuruş testi aynı dikdörtgenlerden beslenir.
- `view.rs`: duvar kağıdı, ana sayfa (saat kartı, arama hapı, ızgara), dock, noktalar, widget sayfası (analog saat, bağlantı, takvim).
- `overlay.rs`: açılış kartı, durum çubuğu, Kontrol Merkezi, çekmece, çalışma alanı menüsü, klasör, arama sayfası, Son Uygulamalar, alt gezinme.
- `aero.rs`: Aero paleti (Android `AeroTheme.kt` ile aynı renkler), sahte cam, simge plakası, etiket gölgesi, nokta göstergesi, sin/cos tablosu.
- `motion.rs`: süreye bağlı animasyon. Bezier eğrileri (Standard, Decel, Accel, Spring), `Tween`, süre sabitleri.
- `model.rs`, `clock.rs`: uygulama tablosu, hücre tipleri, takvim yardımcıları.

## Android ile eşleşmeler

- Sade stil: düz renkli simge plakaları, tek katmanlı yarı saydam paneller, kartsız saat. Gölge, parlama, ışık lekesi ve çift metin yok (hem sade durması hem hız için).
- Büyük saat + tarih, ince arama hapı, yüzen dock (30 dp köşe), lavanta sayfa noktaları.
- Çekmece: yukarı kaydırma parmağı izler, koyu cam sayfa, tutamaç.
- Klasör: simgeler daire üzerinde yaylı (Spring) eğriyle açılır, merkezde cam daire.
- Arama sayfası üstten açılır; klavye yazımı uygulama adını süzer.
- Simge açılışı: simgenin yerinden büyüyen cam kart (`AppLaunchExplosionOverlay` karşılığı).
- Uzun basma ile simge kaldırma, ızgaraya/dock'a bırakma, üst üste bırakınca klasör.

## Pencere ve animasyon düzeltmeleri

- Alt uygulama kendi boyutunda (`Geo::app`: genişlik x (yükseklik - durum - alt çubuk)) 1:1 çizilir, ölçeklenmez. Eski sürümdeki bozuk oran ve bulanık metin buradan geliyordu.
- Durum çubuğundaki Pencere/Tam Ekran düğmesi kaldırıldı. Çekirdek alt uygulamaya yalnızca üst/alt boşluk bildirebiliyor, genişlik bildiremiyor; gerçek pencere boyutlandırma çekirdek desteği gerektirir.
- `wm_set_viewport`, ölçüler değiştiği anda çağrılır (eskiden yalnızca bayrak değişince).
- Açılış/kapanış (240/200 ms): opak kart simge konumundan tam alana büyür, sonda uygulama yüzeyine sert geçer (kartın son rengi uygulama zemin rengiyle aynı). Animasyon sırasında canlı yüzey hiç çizilmez. Tam ekran yarı saydam katman yok.
- Duvar kağıdındaki ışık lekeleri (kare başına ~9M piksel) kaldırıldı; duvar kağıdı düz renk bantları. Çekmece ve Son Uygulamalar tam örtünce altındaki ana ekran hiç çizilmez.
- Tam ekran uygulamada ana ekran hiç çizilmez.
- Alt uygulamaya basma, sürükleme ve bırakma iletilir (eskiden yalnızca ilk dokunuş gidiyordu).
- Başlatma başarısız olursa (700 ms içinde ön plan bayrağı gelmezse) kart kapanır.
- Sahte veri kaldırıldı ("32 GB" yazısı).

## Çizim bütçesi

Akış en fazla 16 KiB, 512 kayıt, 16M piksel iş. Ölçülen en kötü durum 1080x2400 çekmece: 126 kayıt, 3,3 KB, 2,6M piksel (önceki sürümde 11,3M). Izgara 4x6 hücreyle sınırlı.

## Derleme

`bash build.sh armv8a qemu`. `nc-brand` bağımlılığı kaldırıldı (Aero çizimi `aero.rs` içinde); Settings ve Files `nc-brand` kullanmaya devam eder.
