# bootloader/bios

NASM kaynakları. Ayrıntılı davranış ve bellek haritası için üstteki `bootloader/README.md`.

| Dosya | Boyut | Görev |
|---|---|---|
| `mbr.asm` | 512 bayt | Diske kurulumda yazılan MBR (`build.sh` → `crates/hardware/nc-storage/mbr.bin`) |
| `stage1.asm` | tam 2048 bayt | El Torito önyükleme sektörü; stage2 + çekirdeği yükler |
| `stage2.asm` | tam 4096 bayt | A20, long mode denetimi, E820, VBE, sayfalama, çekirdeğe geçiş |

Derleme: `tools/build/boot.sh` (`build_bootloader`) ve `tools/build/kernel.sh`.
