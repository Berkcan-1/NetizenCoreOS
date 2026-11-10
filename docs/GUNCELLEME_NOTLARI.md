# Büyük güncelleme - Aşama 1

Bu pakette değişen veya yeni olan crate'ler `crates/` altında. Aynı adlı klasörlerin üzerine yaz.

## Kurulum
1. `crates/nc-i18n` yeni bir crate. Workspace `Cargo.toml` içindeki `members` listesine ekle.
2. `nc-font` klasörünü komple değiştir (yeni: `src/outline.rs`, `src/data_outline.rs`).
3. `nc-app`, `nc-app-files`, `nc-app-settings`, `nc-app-system`, `nc-app-stopwatch`, `nc-desktop` değişti.
4. Başka crate'ler `nc_app::locale::...` ve `Language::Turkish/English` kullanıyorsa derlenir; ikisinin sayısal değeri eskisi gibi (tr=0, en=1), eski `locale.cfg` dosyaları çalışır.

## Doğrulama (bende derleyici yoktu)
```
python3 crates/nc-i18n/tools/check_lang.py
cargo test -p nc-font
cargo check -p nc-desktop -p nc-app-files -p nc-app-settings
```
Rust kodunu derleyemedim. `nc-ui` ve `nc-gfx` kaynakları pakette olmadığı için onların API'sini senin mevcut kullanımlarından çıkardım. Derleme hatası çıkarsa hatayı olduğu gibi yapıştır, düzeltirim. Yazı tipi verisi ve çeviriler ise Python ile üretildi ve kontrol edildi (önizleme: `docs/yazi-tipi-onizleme.png`).

## Dil sistemi
- 11 dil: İngilizce, Türkçe, Fransızca, Almanca, İspanyolca, Portekizce, Rusça, Çince, Japonca, Korece, Hintçe. 266 anahtar.
- Eskiden sıra numarasına bağlı iki dosya vardı, şimdi anahtar adına bağlı. Çeviri eksikse İngilizce görünür.
- Ayarlar > Sistem ve Dil içinde 11 dilli seçici var. Dil adları kendi yazısıyla görünür.
- Bağlam menüsü artık harf rozeti yerine vektör ikon kullanıyor (harfler dile bağlıydı).
- Teknik adlar sadeleştirildi: "ARP Gateway MAC" yerine "Bağlantıyı sına", "cekirdek.zamanlayici" yerine "Sistem saati", "DHCP: Bound" yerine "Ağa bağlandı" gibi.
- Kapsam: masaüstü kabuğu, görev yöneticisi, pencereler, Dosyalar, Ayarlar, Hakkında, Kronometre.

## Yazı tipi
`nc-font` artık Çince, Japonca, Korece ve Hintçe çizer. Çizgi tabanlı harfler aynen duruyor, çizgi yazı tipinde olmayan karakterler dolgulu kontur olarak çiziliyor.
- Han karakterleri dile göre seçilir (Çince, Japonca biçimi). `nc_font::set_script_hint(lang.script_hint())` masaüstü ve ayarlar tarafından çağrılıyor.
- Hintçe harf kümeleri (bağlaçlar, mātrā) önceden şekillendirilip gömüldü.
- Sınır: sadece dil dosyalarında geçen karakterler gömülü (825 CJK karakteri, 274 Hintçe küme, yaklaşık 380 KiB). Kullanıcının yazdığı rastgele CJK/Hintçe dosya adı kutu olarak görünür. Ek karakter için `tools/extra_chars.txt` dosyasına yaz ve `build_outline.py` çalıştır.
- Lisans: Noto Sans CJK ve Poppins (OFL). `LICENSES.md` dosyasını ürünle birlikte tut.

## Görünüm
- Cam yüzeyler sadeleştirildi: parlama azaltıldı, kenarlar inceltildi, opaklık arttı (okunabilirlik).
- Dinamik ışık (`nc-desktop/src/light.rs`): imleç cam yüzeye, dock simgelerine, başlat menüsü hücrelerine, menü satırlarına ve pencere düğmelerine yumuşak bir ışık ve kenar parıltısı düşürüyor. Yalnızca imleç bu öğelerin üzerindeyken yeniden çizim tetikler.
- Pencere başlığındaki üçlü kapsül kalktı. Düğmeler düz, kapat düğmesi üzerine gelince kırmızı.
- Başlat menüsündeki güç düğmelerine yazılı ipucu eklendi.

## Yapılmadı (sonraki aşama)
- Kurulum sihirbazı (`nc-setup`) hâlâ kendi içindeki iki dilli sabit yazıları kullanıyor. En büyük bölüm, bir sonraki turda.
- Dosyalar uygulamasının iç yerleşimi ve Ayarlar sayfalarının görsel yenilemesi (yalnızca yazılar çevrildi). Bunlar `nc-ui` bileşenleriyle birlikte ele alınmalı.
- Klavye düzenleri hâlâ yalnızca Türkçe Q ve ABD. Fransızca (AZERTY), Almanca (QWERTZ) vb. çekirdek tarafında tuş eşlemesi ister.
- Saat, tarih ve sayı biçimleri henüz dile göre değişmiyor.
- Dosya adları (Masaüstünde oluşan "Yeni klasör" gibi) seçili dile göre oluşuyor. VFS'in UTF-8 adları sorunsuz taşıdığını test etmek gerek.
