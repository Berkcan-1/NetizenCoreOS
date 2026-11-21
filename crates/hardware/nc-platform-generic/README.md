# nc-platform-generic

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `crates/hardware/nc-platform-generic` |
| **Katman** | Donanım |
| **Bağımlılıklar** | `nc-arch`, `nc-hal` |
| **Kullananlar** | `nc-kernel` |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Herhangi bir PC icin asgari Platform uygulamasi (VirtualBox ve cogu gercek makine). Bagimlilik: nc-hal, nc-arch.

## Ne yapar

- write_log: COM1 varsa seri porta yazar, yoksa sessizce atlar.
- shutdown: su an sadece durdurur. ACPI ile kapatma ileride eklenecek.

Bu profilde sanal donanima ozel hicbir sey (debugcon, isa-debug-exit) kullanilmaz; gercek donanimda bilinmeyen
portlara yazmamak icin bilerek sade tutulmustur.
