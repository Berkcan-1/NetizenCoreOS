# nc-i18n

Tek yerden yönetilen dil sistemi. 11 dil: tr, en, fr, de, es, pt, ru, zh, ja, ko, hi.

- `langs/en.lang` ana dosyadır. Anahtarların listesi ve sırası buradan gelir.
- Her satır `Anahtar = değer` biçimindedir. `\n` satır sonu, `{0}` `{1}` yer tutucudur.
- Başka bir dilde eksik anahtar olursa İngilizce kullanılır, derleme durmaz (uyarı verir).
- `build.rs`, `TextId` enum'unu ve tüm tabloları kendisi üretir. Elle enum yazılmaz.
- Koddan kullanım: `tr(lang, TextId::Anahtar)` veya `trf(lang, TextId::Anahtar, &["a", "b"])`.

Yeni yazı eklemek:
1. `en.lang` dosyasının sonuna `YeniAnahtar = ...` ekle.
2. Diğer 10 dosyaya aynı anahtarı çeviriyle ekle.
3. `python3 tools/check_lang.py` çalıştır (eksik anahtar ve yer tutucu hatalarını bulur).
4. Çince, Japonca, Korece veya Hintçe yeni karakter geldiyse `python3 ../nc-font/tools/build_outline.py` çalıştır.

Yeni dil eklemek: `build.rs` içindeki `ORDER`, `src/lib.rs` içindeki `Language` ve `LANGUAGE_COUNT`, bir de yeni `.lang` dosyası gerekir. Mevcut `locale.cfg` dosyalarıyla uyum için ilk iki dilin sırası (tr=0, en=1) değiştirilmemelidir.
