# nc-arch

x86_64'e ozel dusuk seviye islemler. Bagimliligi yoktur.

## Genel API

- outb, inb, outw, inw, outl, inl (unsafe): I/O portlari
- halt_forever(): kesmeleri kapatip sonsuza kadar hlt
- serial: init, present, write_byte, write_str (COM1, 115200 8N1)
- time::Clock: calibrate, now_ms, sleep_until, is_precise; time::read_tsc

## Saat nasil calisir

Clock::calibrate PIT kanal 0'i mod 2'de programlar, 20 ms boyunca PIT sayacini okuyup TSC'nin ne kadar ilerledigini
olcer ve TSC hizini bulur. now_ms, TSC farkini bu hiza bolerek milisaniye verir. Bu yontem, kare suresi PIT'in
sarma suresinden (yaklasik 55 ms) uzun olsa bile dogru kalir. TSC olculemezse (hiz 1 MHz altinda cikarsa) her now_ms
cagrisi 16 ms ilerleyen sahte bir saate duser; is_precise() bunu bildirir.
