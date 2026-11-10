# nc-js - NetizenCore JavaScript motoru (v0.2)

Sifirdan yazilmis, `no_std + alloc` calisan, agac yururuyen (tree-walking) bir JavaScript yorumlayicisi.
Tek bagimlilik: `libm`. Baska NetizenCore crate'ine bagli degil, bu yuzden kendi bilgisayarinda tek basina derlenip test edilebilir:

    cd nc-js
    cargo test            # ~150 uyumluluk vakasi + promise/async/timer testleri

Uyumluluk vakalarinin beklenen sonuclari gercek Node.js ile uretildi (tests.rs).

## Kullanim

    let mut js = nc_js::Interp::new();
    js.now_ms = simdiki_zaman_ms;          // Date.now() ve zamanlayicilar icin
    js.run("console.log(1 + 2)")?;         // son ifadenin degerini dondurur
    js.run_timers(simdiki_zaman_ms, 50);   // her karede cagir, vadesi gelen setTimeout/setInterval'lari calistirir
    js.logs                                // console.log ciktilari

Sistem tarafi derinlik siniri: `js.max_depth` (varsayilan 90). Yorumlayici ozyinelemeli calisir, cekirdek stack'in kucukse bunu dusur (ornegin 40).

## Desteklenenler
var/let/const, fonksiyonlar, ok fonksiyonlari, closure, varsayilan/rest parametreler, destructuring, spread, template literal,
tagged template, optional chaining, `??`, `**`, `||=` `&&=` `??=`, label/break/continue, switch, try/catch/finally,
for / for-in / for-of, `new`, prototype zinciri, `instanceof`, `typeof`, `in`, `delete`.

v0.2 ile gelenler:
- `class` (extends, super(), super.metot(), static, alanlar, `#ozel` alanlar, getter/setter, static blok)
- getter/setter (nesne literalinde, class'ta, `Object.defineProperty`)
- Gercek duzenli ifade motoru (geri izlemeli): gruplar, adli gruplar, geri basvuru, lookahead/lookbehind, `i g m s y` bayraklari,
  `exec/test/match/matchAll/replace/replaceAll/split/search`, `$1 $& $<ad>` degistirme sablonlari
- `function*` ve `yield` / `yield*` (bkz. sinir: eager)
- `Proxy` (get/set/has trap'leri), `Reflect` (JS prelude icinde)
- Ana bilgisayar kancalari: `Interp::host`, `Interp::pump` (await ag beklerken kullanir), `call_global`, `set_global`

Yerlesikler: Object, Function (call/apply/bind), Array, String, Number, Boolean, Math, JSON, Error ailesi, Map, Set, Promise,
setTimeout/setInterval/queueMicrotask, Date (UTC, temel), encodeURIComponent, Symbol (basitlestirilmis), RegExp, Proxy, console.

## Bilinen sinirlar
- Generator'lar EAGER calisir: govde bastan sona hemen kosar, yield edilen degerler tamponlanir, sonra `next()` tampondan okur.
  Sonsuz generator RangeError verir, `next(deger)` ile govdeye veri gondermek calismaz.
- `async/await` bloklayicidir: `await` beklerken mikro gorev, zamanlayici ve ag kuyrugunu ceker. Sonuc dogru, calisma sirasi gercek JS'ten farkli olabilir.
  Gercek (askiya alinabilen) async icin bayt kodlu sanal makine gerekir, sonraki buyuk adim bu.
- Agac yururuyen yorumlayici yavastir; megabaytlik uretim bundle'lari icin bayt kodlu VM sart olacak.
- ES modulleri (`import/export`), WeakRef semantigi, Intl, typed array, BigInt (sayiya cevrilir), tam UTF-16 semantigi yok.
