# nc-mem

Basit bellek havuzu (bump allocator). Bagimliligi yoktur.

## Genel API

- Arena::new(taban, boyut) (unsafe): bu aralik sadece bu Arena'ya aittir.
- Arena::alloc_pixels(adet) -> Option<&'static mut [u32]>: 16 bayt hizali, sifirlanmamis.
- Arena::remaining()

## Kurallar

- Ayrilan bellek hic geri verilmez. Havuz dolarsa None doner.
- Bellek sifirlanmaz; kullanan taraf Canvas::fill ile baslatmalidir.
- Ayni bellek araligi icin ikinci bir Arena kurulmamalidir.
- Ileride sayfa cercevesi yoneticisi gelince bu modul onun ustune oturacaktir.
