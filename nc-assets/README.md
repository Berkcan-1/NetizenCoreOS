# nc-assets

Derleme zamaninda dosya kaynaklarini cekirdege gomer. Su an tek kaynak: logo.

## Logo hatti (build.rs)

1. NC_LOGO_DIR ortam degiskeni (yoksa assets/logo) icindeki PNG dosyalarini listeler, alfabetik ilkini secer.
2. png kutuphanesi ile acar; renk turunu RGBA'ya cevirir (EXPAND + STRIP_16).
3. Bir kenari 256'dan buyukse oran korunarak kutu filtresiyle kucultur (alfa agirlikli ortalama).
4. OUT_DIR/logo.raw dosyasina yazar: "NCLG", genislik (u32 LE), yukseklik (u32 LE), RGBA baytlari.
5. PNG yoksa genislik ve yukseklik 0 yazar ve cargo:warning verir.

## Genel API

- logo() -> Option<Logo>; Logo { width, height, pixels: &'static [u8] }

## Notlar

- Buyuk bir logo, cekirdek boyutunu en fazla 256*256*4 = 262144 bayt artirir.
- Yeni kaynak turu: build.rs'e okuma/donusturme ekle, lib.rs'e erisim fonksiyonu ekle.
