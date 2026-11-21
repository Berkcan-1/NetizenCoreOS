# bootloader

| Klasör | İçerik |
|---|---|
| `bios/` | El yazımı Assembly (NASM): `mbr.asm`, `stage1.asm`, `stage2.asm`. BIOS ile açılan ISO (El Torito, no-emulation) |
| `uefi/` | `nc-uefi` crate'i: UEFI yükleyici (GOP + bellek haritası → BootInfo → çekirdek) |

`mbr.asm` diske kurulumda yazılan MBR'dir; `build.sh` onu derleyip `crates/hardware/nc-storage/mbr.bin` olarak yazar,
`nc-storage` bunu gömer. Aşağıdaki ayrıntılar BIOS yolu (`bios/`) içindir.

### stage1.asm (tam 2048 bayt)

BIOS bu 2048 baytlık görüntüyü 0x7C00 adresine yükler. Yapısı:

- İlk 64 baytın içinde xorriso/genisoimage'in `-boot-info-table` ile dolduracağı "boot info table" vardır
  (offset 8: pvd, 12: dosya LBA, 16: dosya uzunluğu). Bu alanlara dokunulmamalıdır.
- BIOS INT 13h uzantılarının (AH=41h) varlığını denetler.
- Boot dosyasının geri kalanını (stage2 + çekirdek) 2048 baytlık sektörler halinde, INT 13h AH=42h ile
  0x8800 adresinden itibaren okur. Her sektör 0x800 hizalıdır; bu, eski BIOS'larda 64 KB sınırını aşmayı önler.
- Okuma bitince DL'deki sürücü numarasıyla 0x0000:0x8800'e (stage2) atlar.
- Hata olursa ekrana mesaj yazar ve durur.

### stage2.asm (tam 4096 bayt, 0x8800'de çalışır)

Sırasıyla:

1. Segmentleri ve yığını kurar (yığın 0x7C00'in altında).
2. A20 hattını açar (INT 15h AX=2401 ve port 0x92).
3. CPUID ile 64 bit (long mode) desteğini denetler. Yoksa mesaj yazıp durur.
4. `detect_memory`: INT 15h AX=E820 ile bellek haritasını 0x0A00'a okur (en çok 64 kayıt).
5. VBE (`setup_video`):
   - INT 10h AX=4F00 ile VBE bilgisini 0x0600'e alır.
   - Mod listesini gezer (`find_mode`); her mod için AX=4F01 ile 0x0800'e bilgi alır.
   - Koşullar: desteklenen + doğrusal framebuffer + grafik (0x91 bitleri), 32 bpp, doğrudan renk (model 6), tam genişlik/yükseklik eşleşmesi.
   - Önce PREFERRED_WIDTH x PREFERRED_HEIGHT, bulunamazsa FALLBACK_WIDTH x FALLBACK_HEIGHT aranır.
   - Bulunan modu AX=4F02 ile (doğrusal framebuffer biti 0x4000 açık) etkinleştirir.
   - BootInfo yapısını 0x0900'e yazar (`fill_boot_info`).
6. GDT yükler, korumalı moda geçer.
7. Sayfa tablolarını kurar: 4 GB, 2 MB'lik sayfalar. 0-3 GB önbellekli, 3-4 GB önbelleksiz (framebuffer ve MMIO için).
8. Çekirdeği 0x9800'den 0x100000'e kopyalar; sektör sayısı `KERNEL_SECTORS_512` değerinden gelir (`tools/build/boot.sh` verir).
9. PAE + long mode'u açar, 64 bit koda geçer ve 0x100000'e atlar.

NASM tanımları (`tools/build/boot.sh` verir): PREFERRED_WIDTH, PREFERRED_HEIGHT, FALLBACK_WIDTH, FALLBACK_HEIGHT,
KERNEL_SECTORS_512. Verilmezse çözünürlükler 1024x768 olur.

## BootInfo (0x0900, little-endian, sürüm 2)

Sözleşmenin kaynağı `crates/foundation/nc-hal/src/lib.rs` içindeki `BootInfo`'dur (alanlar: magic, version, framebuffer,
memory_map_addr, memory_map_count). stage2 `BOOT_INFO_VERSION equ 2` ve `BOOT_INFO_WORDS equ 32` ile aynı sürümü yazar.
Çerçeve tamponu kısmının düzeni:

| Offset | Boyut | Alan                                  |
|--------|-------|---------------------------------------|
| 0x00   | u32   | magic 0x4942434E ("NCBI")             |
| 0x04   | u32   | version (2)                           |
| 0x08   | u64   | framebuffer fiziksel adresi           |
| 0x10   | u32   | pitch (satır başına bayt)             |
| 0x14   | u32   | genişlik                              |
| 0x18   | u32   | yükseklik                             |
| 0x1C   | u8    | bpp                                   |
| 0x1D   | u8    | kırmızı konum, 0x1E kırmızı boyut     |
| 0x1F   | u8    | yeşil konum, 0x20 yeşil boyut         |
| 0x21   | u8    | mavi konum, 0x22 mavi boyut           |

Bellek haritası alanlarının tam yerleşimi için `nc-hal` kaynağına bak (`MemoryRegion`: base, length, region_type, acpi_extended).
Birini değiştirirsen diğerini de değiştir ve `BOOT_INFO_VERSION` değerini artır.

## Bellek kullanımı (düşük bellek)

    0x0600  VBE bilgi bloğu (512 bayt, geçici)
    0x0800  VBE mod bilgisi (256 bayt, geçici)
    0x0900  BootInfo
    0x0A00  E820 kayıtları
    0x1000  sayfa tabloları (PML4, PDPT, PD'ler)
    0x7C00  stage1 (2048 bayt), yığın altında
    0x8800  stage2 (4096 bayt)
    0x9800  çekirdek, geçici (0x100000'e kopyalanır)

Çekirdek imajı sınırı için `docs/architecture/ARCHITECTURE.md` (Bellek bölümü) ve `kernel/linker.ld`'ye bak.

## Bilinen sınırlar

- VBE pitch, BytesPerScanLine (offset 16) alanından alınır; bazı VBE 3.0 BIOS'larında LinBytesPerScanLine (offset 50) gerekebilir.
- stage2 yalnızca belirtilen tercih ve yedek çözünürlükleri arar (varsayılan profillerde 1280x720 ve 1024x768).
- UEFI yolu için `uefi/README.md`'ye bak.
