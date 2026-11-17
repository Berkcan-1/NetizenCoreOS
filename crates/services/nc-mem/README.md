# nc-mem

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/services/nc-mem` |
| **Katman** | Çekirdek hizmetleri |
| **Bağımlılıklar** | — |
| **Kullananlar** | `nc-kernel`, `nc-kernel-aarch64`, `nc-net`, `nc-splash` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

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
