# nc-app-browser

NetizenCore tarayici uygulamasi. HTML ayristirici, CSS motoru, yerlesim motoru ve resim cozucu burada yasar. Ag katmani nc-net crate'indedir.

## Dosyalar

- lib.rs: modulleri baglar. BrowserApp, B/F/R gecmisi, ana belge + css + resim kuyrugu, form gonderimi, hata sayfasi, cizim
- html.rs: tokenizer, DOM agaci, metin temizleme, YouTube ve meta yedek kartlari
- css.rs: stylesheet ayristirma, secici eslestirme, var() cozumleme, media sorgusu degerlendirme, SafeClamp
- layout.rs: akis yerlesimi, satir ici yerlesim, tablo satiri, hesaplanan stiller
- image.rs: PNG, JPEG, BMP, SVG cozucu
- url.rs: URL normalize ve cozumleme
- legacy/: eski monolit lib.rs ve eski engine.rs, derlemeye dahil degil, sadece referans

## Kararlilik kurallari

- Tum i32 clamp islemleri SafeClamp::sclamp ile yapilir. Alt sinir ust sinirdan buyukse ust sinir doner, panik olmaz.
- Metin ve metin benzeri ozellikler html.rs icinde sanitize_text ile ASCII'ye indirgenir. Turkce harfler en yakin Latin harfine donusur, fontta olmayan semboller atilir.
- display flex ve grid, sayfa CSS'inde blok akisina cevrilir. inline-flex ve inline-grid inline-block olur. Tablo satiri (tr) yatay kalir.
- max-width, min-height ve height blok kutularini etkilemez. Kutu yuksekligi icerikten gelir. Resim ve svg boyutu height ozelligini kullanir.
- background kisayolunda gradient ve url() degeri yok sayilir.
- Media sorgulari 1100 piksellik masaustu genisligine gore degerlendirilir. print, dark, reduce, coarse, portrait ve not iceren sorgular uygulanmaz.
- var() ifadeleri deger icinde herhangi bir yerde cozulur, yedek deger desteklenir.
- Her ana belge yuklenisinde resim listesi temizlenir. Eski sayfanin svg ve resimleri yeni sayfaya karismaz.
- Resim eslestirmesi tam esitlikle yapilir, sonek eslestirmesi yoktur.
- Alt kaynaklar (css, resim) bittikce her biri icin degil, kuyruk bosalinca bir kez yerlesim yeniden kurulur.
- Ana belge basarisiz olursa veya bos donerse hata sayfasi gosterilir, durum Idle'a doner ve dongu olusmaz.
- Gorunum alani altina durum cubugu zemini cizilir, tasan kutular durum cubugunun ustune cikmaz.
- Gorunum alani yuksekligi cizim, tiklama ve kaydirma icin ayni (area.height - unit * 7) degerini kullanir.
- HTML ayristirma siniri 1 MB, yanit siniri 1 MB.

## JavaScript koprusu (v6)
- `jsdom.rs`: nc-js motorunu DOM'a baglar. Rust tarafi yalnizca ilkel dugum islemlerini (`__dom.*`) ve ag kuyrugunu saglar.
- `prelude.js`: DOM/BOM'un kendisi JavaScript ile yazildi (Node, Element, Document, olay sistemi, classList, style, dataset,
  location, URL/URLSearchParams, localStorage, fetch, XMLHttpRequest, MutationObserver taslaklari...). Sayfa basina bir kez calisir.
- `sel.rs`: querySelector icin secici motoru (`>`, `+`, `~`, `[attr^=]`, `:not()`, `:nth-child()`...).
- Yukleme sirasi: ana belge -> CSS -> scriptler (belge sirasiyla; satir ici ve `src`) -> DOMContentLoaded/load -> resimler.
  JS'in sonradan eklediği `<script src>`, `<link>`, `<img>` ogeleri de yuklenir.
- Hata ayiklama: adres cubuguna `about:debug` yaz (script hatalari + console.log ciktilari), `view-source:URL` ham HTML'i gosterir.
  Durum cubugunda `JS ERR n` gorunur.
- Sinirlar: yalnizca GET isteklerini yapabilir (fetch/XHR POST yok), cerez yok, `<script type=module>` atlanir.
