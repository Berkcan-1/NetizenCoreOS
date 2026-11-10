# Aşama 3 - tema, kilit ekranı, kalıcılık

## Kurulum
1. `crates/nc-crypto` yeni crate. Workspace `members` listesine ekle.
2. `nc-storage` artık `nc-crypto`'ya bağlı (Cargo.toml güncellendi). `nc-app` de öyle.
3. `nc-desktop` artık `nc-media`'ya bağlı (açılışta arka planı geri yüklemek için).
4. `kernel/src/main.rs` tam dosya olarak verildi. Önceki yamalar (kurulum sınırı 16 MiB, günlük mesajı) içinde duruyor, üstüne kilit eklendi. Değişen yerler: `DesktopState::new` satırının altı ve ana döngüde `desktop_state.update(...)` satırının altı.
5. Önceki turlarda verilen `linker.ld`, `nc-uefi` ve `stage2.asm` yamaları aynen kalıyor.

## Test sırası
```
python3 crates/nc-i18n/tools/check_lang.py
cargo test -p nc-crypto -p nc-font -p nc-media
cargo check -p nc-desktop -p nc-app-settings -p nc-app-files -p nc-storage
```
`nc-crypto` testleri SHA-256, HMAC ve PBKDF2 için Python `hashlib` çıktısından üretilmiş vektörlerle gelir. Rust'ı derleyemedim.

## Tema
- Ayarlar > Görünüm'ün en altında Açık / Koyu. Açık tema krem, beyaz ve açık yeşil (varsayılan vurgu rengi artık adaçayı yeşili). Koyu tema antrasit yeşil.
- Seçim `/data/system/theme.cfg` içinde saklanır.
- Kapsam: masaüstü kabuğu (dock, başlat menüsü, menüler, pencere çerçevesi, görev yöneticisi, kilit ekranı). Uygulama içerikleri (Dosyalar, Ayarlar) hâlâ tek açık renk paletiyle çiziliyor, koyu temada çerçeve koyu, içerik açık görünür. Uygulamaları temaya bağlamak `nc-app` palet fonksiyonlarını gerektirir.

## Arka plan kalıcılığı
Resmin yolu `/data/system/wallpaper.cfg` içinde tutulur, açılışta aynı çözünürlüğe göre yeniden hazırlanır. Yalnızca PNG ve JPG (BMP o oturumla sınırlı). Dosya taşınır veya silinirse hazır arka plana döner. Ayarlar'dan hazır arka plan seçilince kayıt silinir.

## Kilit ekranı
- Ayarlar > Güvenlik: 4 haneli PIN veya parola (6-32 karakter). Değiştirmek veya kapatmak için önce mevcut kodu sorar.
- Her açılışta modern cam kilit ekranı gelir: PIN için tuş takımı (fare ve klavye), parola için alan. Üst üste 5 yanlıştan sonra 30 saniye bekleme, sonra katlanarak uzar (en fazla 5 dakika). Sayaç diske yazılır.
- Kod, düz hâliyle hiçbir yere yazılmaz. PBKDF2-HMAC-SHA256 (40.000 tur, rastgele 16 baytlık tuz) ile türetilmiş değer, `/security` bölümünün ham sektörlerine (LBA +1 ve +2) yazılır. Dosya sistemi işlemleri bu alana ulaşamaz. Orada geri döndürülebilir şifreleme kullanmadım; doğrulama için kodu geri çıkarmak gerekmez.
- Bu alan silinir, sıfırlanır veya değiştirilirse (kimlik ve işaret sektörü birbirini tutmazsa) açılışta mevcut "THE SYSTEM HAS BEEN DESTROYED" ekranı çıkar ve sistem açılmaz. Diske veri silme yapmıyor, yalnızca açılışı durduruyor.
- Eski kurulumlar etkilenmez: kilit, vault sektörüne eklenen bir işaretle (`LOCKINIT`) devreye girer. Yeniden kurulumda otomatik hazırlanır.

## Güvenlik sınırları (açık olalım)
- 4 haneli PIN yalnızca 10.000 olasılık: diske erişen biri çevrimdışı deneme yapabilir. Kilit, başı boş bir kişiye ve yanlışlıkla silmeye karşı. Diskin kendisini okuyan saldırgana karşı değil (disk şifreleme yok, güvenli önyükleme yok).
- Aynı saldırgan hem kimlik hem işaret sektörünü birlikte sıfırlarsa kilit "kapalı" görünür. Bunu önlemek bir donanım güven kökü gerektirir.
- Mevcut `hash_sectors_256` fonksiyonu gerçek SHA-256 değil, kendi karıştırıcı. Kilit için ayrı, gerçek SHA-256 yazdım ve onu kullanıyor. Önyükleme bütünlüğü kontrolünü de gerçek SHA-256'ya taşımanı öneririm, ama bu mevcut kurulu disklerin mührünü bozar, o yüzden dokunmadım.
- Güç kesintisi tam kod değiştirilirken (iki sektör yazılırken) olursa sistem "yok edildi" ekranına düşebilir. Pencere çok küçük ama sıfır değil.

## Hareket
Animasyon süreleri kısaltıldı (dock, menüler, başlat menüsü, pencereler yaklaşık %30 daha kısa).

## Yapılmadı
- Dock'un yeniden ölçülendirilmesi (4 piksellik ızgara) ve başlat menüsünün yeni yerleşimi (arama alanı, önerilenler, kullanıcı başlığı). Bu turda yalnızca renk ve cam yüzey. Bir sonraki turda.
- Kurulum sihirbazına PIN adımı ve `nc-setup` dil sistemi.
- Uygulama simgelerinin yenilenmesi.
