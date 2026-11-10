# nc-net

DHCP, DNS, TCP, TLS 1.3 ve HTTP istemcisi.

## Son degisiklikler

- http.rs: yanit siniri 6 MiB (`MAX_RESPONSE_BYTES`). Onceki '1 MB' ifadesi kodla uyusmuyordu.
- http.rs: parcali govde cozucude parca sonu hesabi saturating_add ile yapilir, bozuk uzunluk panik uretmez.
- http.rs: `start()` artik host'u ilk `/`, `?` veya `#` karakterinde ayirir; `https://host?x=1` dogru cozulur, fragment sunucuya gonderilmez.
