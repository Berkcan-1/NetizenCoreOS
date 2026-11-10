# nc-platform-generic

Herhangi bir PC icin asgari Platform uygulamasi (VirtualBox ve cogu gercek makine). Bagimlilik: nc-hal, nc-arch.

## Ne yapar

- write_log: COM1 varsa seri porta yazar, yoksa sessizce atlar.
- shutdown: su an sadece durdurur. ACPI ile kapatma ileride eklenecek.

Bu profilde sanal donanima ozel hicbir sey (debugcon, isa-debug-exit) kullanilmaz; gercek donanimda bilinmeyen
portlara yazmamak icin bilerek sade tutulmustur.
