# nc-splash/src

## lib.rs

- Arka plan ilk karede saf siyaha boyanır.
- Ortadaki logo ters renk/invert katmanı kullanılmadan normal ön plan renkleriyle çizilir; tek renkli açık logo için hafif marka gradyanı korunur.
- Üç beyaz nokta sinüs tabanlı faz kaymasıyla farklı yüksekliklerde hareket eder; `DOT_PERIOD_MS` animasyon hızını kontrol eder.
- Opsiyonel alt yazı UTF-8 olarak sınırlı bir sabit arabelleğe kopyalanır ve animasyonun altına ortalanır. Bu, `Splash` yapısının çağıranın geçici metnine referans tutmasını önler.
- Başlangıç ve bitiş geçişleri yalnızca içerik opaklığını değiştirir; `request_finish` sonrası ekran sonunda `new` çağrısında kaydedilen görüntüye geri döner.
- İlk `Update` tam ekranı, sonraki güncellemeler logo ile alt animasyon alanlarını kapsar.
