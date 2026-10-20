# nc-font/src

## lib.rs

Genel API: Style, text_width, draw_text. layout(), metni dolaşır: ayrık aksanları birleştirir, boşluk ve sıfır genişlikli karakterleri ele alır, her harfin piksel ilerlemesini (birim * cap / 1000, yuvarlanmış) hesaplar. text_width ve draw_text aynı layout()'u kullanır, bu yüzden ölçü ile çizim her zaman tutarlıdır.

## raster.rs

draw_glyph(), bir harfi çizer:

1. Harfin parçalarını (data::PARTS) dolaşıp işlemleri (Move, Line, Quad, Cubic) okur.
2. İlk geçişte her eksen için düğüm noktaları toplar: dikey çizgilerin x'leri, yatay çizgilerin y'leri ve harfin uç noktaları. Her düğüm piksel ızgarasına yuvarlanır (tek piksel kalınlıkta merkeze, çift kalınlıkta kenara). Aradaki noktalar doğrusal ara değerlenir, şekil bozulmaz.
3. İkinci geçişte noktalar bu eşlemeyle piksele çevrilir; eğriler boyuta göre 3..14 doğru parçasına bölünür (en fazla 192 parça).
4. Her pikselin merkezinin en yakın parçaya uzaklığı bulunur; kapsama = yarım kalınlık + 0.5 - uzaklik (1/256 piksel). Uçlar ve köşeler yuvarlaktır.

Bu dosya kendi karekökünü (integer_sqrt) içerir; kayan nokta kullanılmaz.

## data.rs (üretilmiştir, elle düzenleme)

- ENTRIES: kod noktasına göre sıralı harf listesi (kod, ilerleme birimi, ilk parça, parça sayısı).
- PARTS: bir harfi oluşturan parçalar. Aksanlı harf = taban harf + aksan işareti, aksan kaydırmasıyla. Taban harfler ve işaretler OPS içinde bir kez saklanır, böylece Ö, Ô, Ò aynı Ö tabanını paylaşır.
- OPS: i16 dizisi. 1 x y = Move, 2 x y = Line, 3 cx cy x y = Quad, 4 c1x c1y c2x c2y x y = Cubic.
- COMPOSE: ayrık aksanlı çiftlerden birleşik koda tablo (base, işaret, sonuç).
- NOTDEF: tanımsız karakter şekli.

## tools/ (isteğe bağlı geliştirme aracı)

data.rs'in kaynağı Python tasarım dosyalarıdır (Cargo derlemesine girmez):

- builder.py: çizim yardımcıları (düz çizgi, elips yayı, Bézier).
- glyphs_latin.py, glyphs_extra.py, glyphs_cg.py: harf ve sembol tasarımları. compose.py aksan yerleştirir, charset.py hangi karakterlerin üretileceğini seçer.
- engine.py: raster.rs ve lib.rs'in birebir Python karşılığı (önizleme ve doğrulama için).
- build.py: sınırları doğrular ve src/data.rs'i yazar. Kullanım: `python3 tools/build.py`
- sheet.py: tools/preview.png gibi bir sayfa üretir (Pillow gerekir).

Yeni harf: glyphs_*.py içine `p = P(); p.seg(...); fin(p, sol_pay, sağ_pay, 'x')` ekle, charset.py'de kod noktasını listeye koy, build.py çalıştır. Python kullanmak istemezseniz data.rs doğrudan da düzenlenebilir.

## tests.rs

`cargo test -p nc-font` (ana makine hedefiyle): her harfin piksel ürettiğini, tablonun sıralı olduğunu, Türkçe ve diğer karakterlerin bulunduğunu, ayrık/birleşik aksan eşitliğini ve taban/üst çizgi netliğini sınar.
