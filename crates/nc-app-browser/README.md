# nc-app-browser

NetizenCore tarayici uygulamasi. HTML ayristirici, CSS motoru, yerlesim motoru ve resim cozucu burada yasar. Ag katmani nc-net crate'indedir.

## Dosyalar

- lib.rs: modulleri baglar. BrowserApp, B/F/R gecmisi, ana belge + css + resim kuyrugu, form gonderimi, hata sayfasi, cizim
- html.rs: tokenizer, DOM agaci, metin temizleme, bos sayfa icin acikca isaretli meta yedek karti (siteye ozel yeniden yazim YOKTUR)
- css.rs: stylesheet ayristirma, secici eslestirme, var() cozumleme, media sorgusu degerlendirme, SafeClamp
- layout.rs: akis yerlesimi, satir ici yerlesim, tablo satiri, hesaplanan stiller
- image.rs: PNG, JPEG, BMP, SVG cozucu
- url.rs: URL normalize ve cozumleme
- legacy/: eski monolit lib.rs ve eski engine.rs, derlemeye dahil degil, sadece referans

## Kararlilik kurallari

- Tum i32 clamp islemleri SafeClamp::sclamp ile yapilir. Alt sinir ust sinirdan buyukse ust sinir doner, panik olmaz.
- Metin ve metin benzeri ozellikler html.rs icinde sanitize_text ile ASCII'ye indirgenir. Turkce harfler en yakin Latin harfine donusur, fontta olmayan semboller atilir.
- `display:flex` ve `display:grid` icin basit kutu yerlesimi vardir; `flex-wrap`, temel hizalama/ara bosluklari ve grid kolon izleri islenir. `inline-flex` ve `inline-grid` inline-block akista cizilir.
- `max-width`, `min-height` ve `height` blok kutularinda sinirli desteklidir; icerik yuksekligi ile cakismalarda kutu yuksekligi `MAX_HEIGHT` siniriyla korunur.
- `linear-gradient()` icin iki renk-stoplu yatay/dikey cizim vardir; kosegen, radial/conic/repeating gradient ve `url()` arka plan gorselleri henuz cizilmez. Gradyanli kutularda border-radius su an uygulanmaz.
- Media sorgulari gercek gorunum alani olculerine gore degerlendirilir. `screen`/`all`, `not`/`only`, width/height araliklari, orientation, aspect-ratio ve resolution sorgulari vardir. Renk semasi, hareket tercihi, pointer/hover, forced-colors ve display-mode tercihleri sabit varsayilanlarla eslestirilir; cihazdan gercek OS tercihi okunmaz.
- var() ifadeleri deger icinde herhangi bir yerde cozulur, yedek deger desteklenir.
- Her ana belge yuklenisinde resim listesi temizlenir. Eski sayfanin svg ve resimleri yeni sayfaya karismaz.
- Resim eslestirmesi tam esitlikle yapilir, sonek eslestirmesi yoktur.
- Alt kaynaklar (css, resim) bittikce her biri icin degil, kuyruk bosalinca bir kez yerlesim yeniden kurulur.
- Ana belge basarisiz olursa veya bos donerse hata sayfasi gosterilir, durum Idle'a doner ve dongu olusmaz.
- Gorunum alani altina durum cubugu zemini cizilir, tasan kutular durum cubugunun ustune cikmaz.
- Gorunum alani yuksekligi cizim, tiklama ve kaydirma icin ayni (area.height - unit * 7) degerini kullanir.
- Sinirlar (kaynaktan dogrulandi): HTML 3 MiB (`MAX_HTML_BYTES`), 16.000 token, 6.000 DOM dugumu, 28 derinlik; CSS 256 KiB ve 1.500 kural; ag yanit tamponu 6 MiB (nc-net). Limit asilinca sayfa sessizce kesilmez, `about:debug` 'PARSER:' satiri gosterir.
- Tirnaksiz oznitelik degerleri WHATWG kuralina uyar: yalnizca bosluk ve `>` degeri bitirir, `/` degerin parcasidir.
- Gorsel formatlari: PNG, JPEG, BMP, SVG cozulur. GIF/WebP/AVIF/APNG/ICO henuz cozulmez; bu URL'ler indirilmez ve `about:debug` icinde 'IMAGE SKIPPED' olarak nedeniyle listelenir.
- Siteye ozel DOM yeniden yazimi (eski YouTube karti) kaldirildi; gercek sayfa DOM'u degistirilmez.

## JavaScript koprusu (v6)
- `jsdom.rs`: nc-js motorunu DOM'a baglar. Rust tarafi yalnizca ilkel dugum islemlerini (`__dom.*`) ve ag kuyrugunu saglar.
- `prelude.js`: DOM/BOM'un kendisi JavaScript ile yazildi (Node, Element, Document, olay sistemi, classList, style, dataset,
  location, URL/URLSearchParams, localStorage, fetch, XMLHttpRequest, MutationObserver taslaklari...). Sayfa basina bir kez calisir.
- `sel.rs`: querySelector icin secici motoru (`>`, `+`, `~`, `[attr^=]`, `:not()`, `:nth-child()`...).
- Yukleme sirasi: ana belge -> CSS -> scriptler (belge sirasiyla; satir ici ve `src`) -> DOMContentLoaded/load -> resimler.
  JS'in sonradan eklediği `<script src>`, `<link>`, `<img>` ogeleri de yuklenir.
- Hata ayiklama: adres cubuguna `about:debug` yaz (script hatalari + console.log ciktilari), `view-source:URL` ham HTML'i gosterir.
  Durum cubugunda `JS ERR n` gorunur.
- Sinirlar: bkz. FEATURE_MATRIX.md (tek dogru kaynak). README ile matris celisirse matris gecerlidir.

## Testler

- `src/url.rs` ve `src/html.rs` icinde `#[cfg(test)]` birim testleri vardir.
- Bu crate `no_std` ve nc-gfx/nc-ui/nc-font'a bagli oldugu icin testler ana makinede `host-tests/` ile calisir:
  `cd nc-app-browser/host-tests && cargo test`

## Adım 2 testleri

- Rust (saf modüller): `cd nc-app-browser/host-tests && cargo test` (html, url, supports ve prelude/Rust liste eşitliği)
- JS (prelude.js, Node/V8): `cd nc-app-browser/host-tests/js && node run_prelude_tests.js`
- nc-js yorumlayıcısı: `cd nc-js && cargo test` (dom_prelude vakalarına 16 yeni vaka eklendi)
- nc-font: `cd nc-font && cargo test` (`has_glyph_matches_the_drawn_repertoire`)

## Adım 3a: CSS çekirdeği

- CSS custom properties artik element kapsamına ve mirasa gore tutulur; `var()` ic ice fallback ve dongu korumasiyla cozulur. `initial`, `inherit` ve `unset` parent kapsamiyla uyumlu ele alinir. Onceki sayfa-geneli degisken tablosu kaldirildi.
- Declaration ayırma; tırnak, `()`, `[]` ve `!important` sınırlarını korur. CSS yorumları string içindeki `/* ... */` metinlerini silmez. Kural tarayıcısı tırnak içindeki `{` / `}` karakterlerini blok sınırı saymaz.
- Cascade sıralaması `!important` önceliğini specificity'den ayrı uygular.
- `:has()` basit descendant ve direct-child argümanlarını destekler; karmaşık sibling-relative `:has(+ ...)` / `:has(~ ...)` henüz desteklenmez.
- Media sorguları bilinmeyen özelliklerde fail-closed davranır; aspect-ratio aralıkları ve çözünürlük birimleri eklenmiştir.

- `src/cssvalues.rs`: saf (nc-* bagimsiz) deger katmani: birimler, `calc/min/max/clamp`, `font-weight`, `line-height`, shorthand acicilar (`border`, `font`, `background`, `overflow`, `inset`, `text-decoration`), gradyan renk-stop ayiklama ve kapsamli `var()` cozumleme.
- `src/supports.rs`: `@supports` + `CSS.supports` için desteklenen özellik listesi (86). `prelude.js` kopyası `host-tests` ile eşitlik testinden geçer.
- `layout.rs`: yeni `ComputedStyle` alanları, UA varsayılanları (b/strong/h1-h6 kalın, a/u altı çizili, s/del üstü çizili, mark, small...), `position`, kenar başına border, overflow kırpma, boş kutu düzeltmesi, uzun kelime bölme.
- `lib.rs`: `page_text_style` ile agirlik/aralik, `text-decoration` cizimi, kenar seritleri, konumlu kutularin ustte boyanmasi ve iki stoplu linear-gradient piksel cizimi.
- Rust testleri: `cd host-tests && cargo test` (css, cssvalues, html, url, supports). Duzen/cizim kodu (`layout.rs`, `lib.rs`) nc-gfx'e bagli oldugundan bu host paketiyle derlenmez; gercek PC/QEMU/VBox uzerinde goruntu testi gerekir.

## Adım 3b: tanılama

- `about:compat` sayfasını açın (`src/compat_probe.html`): motorun içinde ES/DOM/Web API yoklaması yapar, eksikleri listeler.
- `about:debug`: betik hataları artık `[receiver: …, value: …]` ile gelir; Türkçe karakterler doğru görünür.
- Node'da: `cd host-tests/js && node run_prelude_tests.js` (74 vaka) ve `node run_compat_probe.js` (prelude DOM/Web API eksikleri; `Function` yokken 52 syntax vakasi `NOT TESTED` olarak raporlanir).

## Adım 4

- `src/gif.rs`: saf GIF çözücü. Test: `cd host-tests && cargo test` + doğrulama betiği `python3 host-tests/tools/gif_port_check.py` (Pillow gerekir).
- `nc-js`: destructuring ataması ve `new.target` (`cd nc-js && cargo test`).
- prelude: `<style>.sheet` / CSSOM; Node testleri 74 vaka.

## Performans ve kilitlenme korumaları

Güncel kaynak bütçeleri, JavaScript işlerinin karelere dağıtılması, ağ kuyruğu davranışı, görsel/DOM limitleri, uyumluluk etkileri ve test kapsamı için depo kökündeki `PERFORMANCE_AUDIT.md` dosyasına bakın. Bu rapor kaynak denetimini ve Node host testlerini kapsar; Cargo/QEMU/gerçek donanım benchmark'ı yerine geçmez.
