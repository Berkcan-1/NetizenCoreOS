# nc-font

Kendi vektör çizgi yazı tipi. Yazı tipi dosyası veya bitmap yoktur; her harf düz çizgilerden ve Bézier eğrilerinden oluşur ve her boyutta alias'sız çizilir. Tasarım: yuvarlak uçlu, tek kalınlıklı (monoline) geometrik hümanist bir sans.

Bağımlılık gerekmez. Cargo.toml'daki nc-math satırı yalnızca Cargo.lock değişmesin diye bırakıldı; istenirse silinebilir.

## Genel API (değişmedi)

- Style { cap_height, stroke_sixteenths, tracking }
- Style::new(cap_height): boyuta uygun kalınlıkla hazır stil (yeni)
- text_width(metin, &Style) -> piksel
- draw_text(metin, x, y, &Style, &mut dyn FnMut(x, y, kapsama 0..=255))

y, büyük harflerin üst çizgisidir. Aksanlar ve alt uzantılar bunun dışına taşabilir. Fonksiyon piksel yazmaz; her pikselin kapsama değerini geri çağırmaya verir.

tracking artık gerçekten uygulanır: her harften sonra eklenen ek pikseldir (normal metin için 0). Eski sürüm bu alanı yok sayıyordu; çağıranlarda 0'dan farklı değer varsa metin genişler.

## Desteklenen karakterler (552)

- Latin: ASCII, Latin-1 Supplement, Latin Extended-A tamamı (Türkçe, Almanca, Fransızca, İspanyolca, Portekizce, İtalyanca, Lehçe, Çekçe, Slovakça, Macarca, Romence, Hırvatça, Baltık dilleri, İskandinav dilleri, İzlandaca, Maltaca kısmen), Ə ə (Azerice), Ș ș Ț ț
- Yunanca: Α-Ω, α-ω, tonos ve dialytika
- Kiril: Rusça, Ukraynaca, Belarusça, Bulgarca, Sırpça, Makedonca ve Kazakça/Özbekçe ek harfleri
- Semboller: ₺ ₽ € £ ¥ $ ¢, matematik, oklar, ✓, kesirler, üst simgeler, tipografik tırnaklar ve tireler
- NFD (ayrık aksanlı) metin otomatik birleştirilir: "e" + U+0301 çizilirken "é" olur.
- Tanımsız karakter içi boş bir dikdörtgen çizer (sessizce kaybolmaz). Sıfır genişlikli ve kontrol karakterleri atlanır.

Henüz yok: Arapça, İbranice, Çince/Japonca/Korece, Hint yazıları, Vietnamca (yığılmış aksanlar). Arapça ve Hint yazıları harf birleştirme (shaping) gerektirir; bu motor tek karakter tek şekil çalışır.

## Kalite için yapılanlar

- Izgaraya hizalama (hinting): dikey gövdeler ve yatay çizgiler piksel merkezine oturtulur. Küçük boyutta bile üst çizgi, taban çizgisi ve gövdeler keskin çıkar; aynı şekil her yerde aynı çıkar.
- Eğriler boyuta göre düzleştirilir: büyük harflerde pürüzsüz, küçükte hızlı.
- Harf aralıkları harfe özel bırakma payıyla gelir; ilerleme tam piksele yuvarlanır, ritim bozulmaz.
- Çizim satır bazlı kırpma yapar, eskisinden belirgin hızlıdır.

Önizleme: tools/preview.png

## Birimler

Harfler yükseklik 1000 birimlik kutuda tanımlıdır: y=0 büyük harf üstü, y=1000 taban çizgisi, y=270 küçük harf üstü, y=1250 alt uzantı. Ayrıntı ve veri biçimi için src/README.md.
