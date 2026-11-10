# nc-arch/src

## lib.rs

Port G/C fonksiyonlari asm! ile yazilmistir; hepsi unsafe'dir. halt_forever cli+hlt dongusudur.
serial ve time alt modullerini disari acar.

## serial.rs

16550 UART, COM1 (0x3F8). init: kesmeleri kapatir, bolen 1 (115200), 8N1, FIFO acik. present: durum yazmaci
0xFF degilse UART vardir. write_byte: gonderme yazmaci bosalana kadar en fazla 200000 tur bekler, bosalmazsa vazgecer
(UART olmayan makinelerde kilitlenmemek icin). write_str: satir sonunu CR+LF yapar.

## time.rs

PIT (0x40/0x43) ve rdtsc. Clock { ticks_per_ms, start, fallback_ms }.
calibrate: PIT'i 0x34 komutuyla programlar, 1193182/50 tik (20 ms) bekler. now_ms: (tsc - start) / ticks_per_ms.
sleep_until: hedef zamana kadar spin_loop ile bekler (u32 sarmasina dayanikli karsilastirma).
