# assets/logo

Acilis ekraninin ustunde gorunen logo buradan gelir.

Kurallar

- Klasordeki PNG dosyalarindan alfabetik olarak ilki kullanilir. Bir tane birakmak en saglikli yoldur.
- Desteklenen turler: RGBA, RGB, gri, gri+alfa, palet (palet resimler sorunsuz acilir). 16 bit kanallar 8 bite indirilir.
- Seffaflik (alfa) korunur. Arka plan seffaf olan logolar en iyi sonucu verir; logo acik renkli zemine cizilir.
- Bir kenari 256 pikselden buyuk olan logo, oran korunarak otomatik kucultulur (kutu filtresi).
- Beyaz siluet logolar (gorunen tum piksellerin R, G, B degeri 200 ve ustu) acilista otomatik olarak mavi gradyanla
  boyanir; sadece alfa kullanilir. Aksi halde logo kendi renkleriyle cizilir (acik zeminle carpilarak, renkler zeminin
  tonundan biraz etkilenir).
- Klasorde PNG yoksa logosuz acilir ve derleme sirasinda bir uyari yazilir.
- Baska bir klasor kullanmak icin: NC_LOGO_DIR=/mutlak/yol bash build.sh

Su anki logo.png beyaz, seffaf zeminli bir siluettir ve yukaridaki kuralla boyanarak cizilir. Kendi logonla degistirip
yeniden derlemen yeterlidir.
Logo, bulaniklastirma animasyonu icin yazi ile birlikte islenir; ince detaylar ilk karelerde yumusar, bu normaldir.
