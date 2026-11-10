# nc-platform-qemu

QEMU sanal donanimina ozel Platform uygulamasi. Bagimlilik: nc-hal, nc-arch.

## Ne yapar

- write_log: debugcon (0xE9) ve seri port (COM1) uzerinden yazar.
- shutdown(kod): isa-debug-exit (0xF4) ile QEMU'yu kapatir. QEMU cikis kodu (kod << 1) | 1 olur.
- display_adapter_present(): Bochs/std VGA "dispi" kimligini (0xB0C0-0xB0CF) dogrular.
- set_display_mode(w, h): dispi yazmaclariyla cozunurlugu calisirken degistirir (32 bpp, dogrusal). Su an cekirdek
  bunu cagirmaz; bootloader'in sectigi mod kullanilir.

## QEMU argumanlari (build.sh run bunlari kullanir)

    -vga std -serial stdio -debugcon file:build/debugcon.log
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 -no-reboot -m 256

## Sonraki adaylar

virtio-gpu, virtio-net, virtio-blk, virtio-input, fw_cfg ve RTC. Bunlar bu kutuya ayri alt modul olarak eklenmelidir.
