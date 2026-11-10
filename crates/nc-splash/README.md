# nc-splash

Minimalist NetizenCore açılış ekranı. Bağımlılıklar: `nc-assets`, `nc-gfx`, `nc-font`, `nc-math`, `nc-mem`, `nc-ui`.

## Görünüm

- Arka plan tamamen siyahtır.
- Logo ekranın ortasında gösterilir. Mevcut logo korunur; tek renkli açık logo, marka vurgusu için hafif bir vurgu gradyanıyla çizilir.
- Ekranın alt kısmında üç beyaz nokta yumuşak biçimde yukarı-aşağı hareket ederek yükleme animasyonu oluşturur.
- `SplashOptions { subtitle: Some("...") }` verilirse metin animasyonun altında, ortalanmış şekilde görünür. Kurulum ekranındaki mevcut durum metni bu alanla aktarılabilir.
- İsteğe bağlı metin verilmezse sadece logo ve yükleme noktaları görünür.

## Zaman çizelgesi

| Evre | Süre | Davranış |
|---|---:|---|
| `Intro` | 420 ms | Logo, noktalar ve varsa durum metni siyah arka plan üzerinde yumuşakça belirir. |
| `Idle` | Süresiz | Üç beyaz nokta akıcı, döngüsel animasyonla hareket eder. |
| `Outro` | 320 ms | İçerik siyaha doğru solar. |
| `Done` | — | `new` sırasında kaydedilen önceki canvas geri yüklenir. |

## API

- `Splash::new(&canvas, &mut arena, SplashOptions) -> Option<Splash>`
- `Splash::frame(&mut canvas, now_ms) -> Update`
- `Splash::request_finish()`
- `Splash::phase() -> Phase`

`Update` ilk karede tüm ekranı, sonraki karelerde sadece logo ve alt animasyon bölgesini işaretler. Çağıran kod, dönen dikdörtgenleri `present_region` (veya kullandığı eşdeğer sunum fonksiyonu) ile ekrana aktarmaya devam edebilir.
