# NetizenCore Desktop UI Standardı

## 1. Tasarım hedefi

NetizenCore arayüzü Frutiger Aero'nun cam, ışık ve hava hissini taşır; ancak renk yoğunluğu, boşluk, metin kontrastı ve hareket miktarı kurumsal bir masaüstü deneyimini destekleyecek şekilde kontrol edilir. macOS ve Windows 7 burada yalnızca kalite ve akış referansıdır; ikon, yerleşim ve etkileşimler NetizenCore'a ait kalmalıdır.

## 2. Ekrana uyum

- Bütün ana ölçüler `layout::unit_for(width, height)` üzerinden türetilir.
- Ölçek birimi 8–44 px aralığında sınırlandırılır. Bu aralık yazılım tabanlı ölçeklemedir; fiziksel DPI algılaması değildir.
- Pencere ölçüleri gerçek kullanılabilir alanı aşamaz. Alt dock alanı ayrılmışsa yeni pencereler bu alanı dikkate alır.
- Dock, başlat menüsü, diyaloglar ve simge ızgarası yalnızca ekran koordinatlarını kullanır; uygulama içeriği kendi istemci dikdörtgeninde kırpılır.
- Test edilmesi gereken temel profiller: 320×240, 640×480, 1024×768, 1280×720, 1366×768, 1920×1080, 2560×1440, 3840×2160 ve 1080×1920.

## 3. Dock standardı

- Dock'un başlat ve durum bölümleri sabit kalır; yalnızca orta uygulama alanı sayfalanır.
- Sığan durumda uygulamalar tek sıra gösterilir. Taşma durumunda simgeler okunabilir alt boyuta kadar küçültülür; bundan sonra daha fazla küçültülmez.
- Taşma oluştuğunda sol/sağ kontrolleri görünür. Kontroller son sayfadan ilk sayfaya da geçebilir.
- Yeni uygulama dock'a eklendiğinde taşma varsa son sayfa görünür hâle gelir.
- Sayfa değişimi 230 ms easing tabanlı yatay geçiş kullanır. Uygulama simgeleri orta alan içinde kırpılır; gezinme kontrollerine veya durum alanına çizilmez.
- Görünür simge, hit-test ve pencere küçültme/hedef koordinatları aynı sayfa indeksini kullanmalıdır.

## 4. Renk ve yüzey

- Ana metin: koyu mürekkep mavisi. İkincil metin: yumuşak arduvaz mavisi.
- Cam: beyaz/açık buz tonu, sınırlı opaklık, ince açık kenar ve yumuşak gölge.
- Uygulama karoları: mercan, kehribar, deniz yeşili, aqua, gök mavisi, mavi, lavanta ve arduvaz paleti.
- Parlaklık öğeleri metin okunurluğunun önüne geçmemeli. Koyu metin, beyaz cam yüzeyde; açık simgeler ise koyu/renkli karo üzerinde kullanılmalı.
- Renk paleti uygulama kimliğinin hash'inden seçilmeye devam eder; aynı uygulama her açılışta aynı renge sahip olur.

## 5. Hareket ve performans

- Hareket süresi kare sayısından değil geçen milisaniyeden hesaplanır.
- Yerleşim/odak geçişleri kısa ve easing tabanlı olmalıdır; hareket yeni girdiyi engellememeli.
- Dock sayfa geçişi yaklaşık 230 ms'dir. Hover büyütmesi taşma modunda azaltılır, dock ekranın dışına taşmaya başlarsa büyütme devre dışı kalır.
- Cam bulanıklığı maliyetli olduğundan mevcut `glass_blur` ayarı korunur. Düşük performanslı cihazlarda bulanıklık kapatılabilmeli.

## 6. Doğrulama listesi

- [ ] Her hedef çözünürlükte dock, durum alanı ve başlat düğmesi ekranın içinde kalır.
- [ ] 4, 8, 12, 20 ve 40 uygulamada görünür simgelere doğru tıklanır.
- [ ] İlk/son dock sayfası arasında dolaşım çalışır.
- [ ] Yeni bir uygulama yüklenmesi ya da sabitlenmemiş uygulamanın ilk açılışı, taşma varken son sayfayı gösterir.
- [ ] Sayfa geçişinde uygulama simgeleri durum alanının veya ok düğmelerinin üstüne çizilmez.
- [ ] Pencere aç/küçült/büyüt ve diyaloglar dock sayfaları değişse de hedef koordinatları doğru kullanır.
- [ ] Otomatik gizleme, tam ekran dock animasyonu ve 1080×1920 dikey ekran birlikte denenir.

## 7. Sonraki kalite aşaması

Bu düzenleme çözünürlük tabanlı yazılım ölçeği sağlar; henüz fiziksel DPI algılama, kullanıcı tarafından seçilebilir UI ölçeği, klavye ile dock sayfalarını değiştirme ve azaltılmış hareket tercihi sağlamaz. Bunlar ayrı bir erişilebilirlik/ayar işi olarak eklenmelidir.
