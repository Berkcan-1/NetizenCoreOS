# nc-desktop/src

## Modüller

- `lib.rs`: Durum makinesi, pencere yaşam döngüsü, hedef bulma, giriş ve animasyon zamanlaması.
- `motion.rs`: Tamsayı tabanlı easing ve kare hızından bağımsız animasyon ilerlemesi.
- `paint.rs`: Kenar yumuşatma, gradyan, gölge, bulanıklık ve cam yüzey çizimi.
- `theme.rs`: Renk sistemi, uygulama karo paleti ve cam yüzey ayarları.
- `layout.rs`: Ekrana uyumlu geometri, pencere sınırları, masaüstü ızgarası ve dock yerleşimi.
- `icons.rs`: Uygulama karoları ve sistem simgeleri.
- `window.rs`: Pencere çerçevesi ve başlık düğmeleri.
- `wallpaper.rs`: Duvar kâğıdı gradyanları, ışıklar ve hareketli parçalar.
- `shell.rs`: Masaüstü simgeleri, dock, başlat menüsü, bağlam menüsü, görev yöneticisi ve diyaloglar.

## Yerleşim standardı

`unit_for(width, height)` ekran genişliği ve yüksekliğini birlikte değerlendirir. Ölçek birimi 8–44 piksel arasında tutulur; böylece küçük çözünürlüklerde ölçüler makul kalırken 4K ekranlarda arayüz gereksiz yere büyümez. Pencere ölçüleri de kullanılabilir alan içinde sınırlandırılır.

## Dock davranışı

Dock üç bölüme ayrılır: başlat düğmesi, uygulama şeridi ve sistem durumu. Sığan uygulamalar tek sırada gösterilir. Uygulamalar ayrılan alanı doldurduğunda karo boyutları okunabilir alt sınıra iner ve dock'un ortası sayfalı bir karusele dönüşür. Sol/sağ kontrolleri sayfalar arasında döngüsel gezinir. Yeni kurulan ya da dock'ta sabitlenmemiş olup ilk kez açılan uygulama, taşma varsa son sayfaya alınır.

Sayfa geçişleri yaklaşık 230 ms sürer; mevcut ve yeni simgeler yatay yönde hareket eder, uygulama şeridi dışına taşan pikseller kırpılır. Hit-test aynı görünür sayfayı kullandığı için tıklanan simge ile çizilen simge eşleşir.

## Görsel dil

Renkler daha kontrollü mavi, aqua, deniz yeşili, lavanta ve sıcak nötr tonlara çekildi. Cam yüzeyler belirgin bir üst ışık, ince kenar ve yumuşatılmış gölge taşır; saat ve durum simgelerinde açık cam üzerinde koyu mürekkep rengi kullanılır.

Daha kapsamlı kararlar için `UI_DESIGN_STANDARD.md` dosyasına bakın.
