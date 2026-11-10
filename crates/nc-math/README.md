# nc-math

Tam sayi matematigi. Bagimliligi yoktur. Cekirdekte kayan nokta kullanilmaz (SSE kapali), bu modul onun yerini tutar.

## Genel API

- sin_milli(derece), cos_milli(derece): sonuc -1000..1000 (binde bir). 5 derece cozunurluk.
- integer_sqrt(u64): tam karekok (Newton).
- smoothstep256(t): girdi 0..256, cikti 0..256, 3t^2 - 2t^3 egrisi.
- ratio256(deger, toplam): deger/toplam oranini 0..256 araligina kirpar.
- lerp_i32(a, b, t): t 0..256.

## Sabit nokta kurali

Animasyon ve karistirma degerleri 0..256 araliginda (8 bit kesir) tutulur; 256 "tamamen" demektir. Boylece carpmalar
bolme yerine kaydirma ile yapilabilir.
