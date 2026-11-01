# bootloader

Iki asamali, el yazimi Assembly (NASM) bootloader. BIOS ile acilan ISO (El Torito, no-emulation) icin yazildi.

## Dosyalar

### stage1.asm (tam 2048 bayt)

BIOS bu 2048 baytlik gorseli 0x7C00 adresine yukler. Yapisi:

- Ilk 64 baytin icinde xorriso/genisoimage'in -boot-info-table ile dolduracagi "boot info table" vardir
  (offset 8: pvd, 12: dosya LBA, 16: dosya uzunlugu). Bu alanlara dokunulmamalidir.
- BIOS INT 13h uzantilarinin (AH=41h) varligini denetler.
- Boot dosyasinin geri kalanini (stage2 + cekirdek) 2048 baytlik sektorler halinde, INT 13h AH=42h ile
  0x8800 adresinden itibaren okur. Her sektor 0x800 hizalidir; bu, eski BIOS'larda 64 KB sinirini asmayi onler.
- Okuma bitince DL'deki surucu numarasiyla 0x0000:0x8800'e (stage2) atlar.
- Hata olursa ekrana mesaj yazar ve durur.

### stage2.asm (tam 4096 bayt, 0x8800'de calisir)

Sirasiyla:

1. Segmentleri ve yigini kurar (yigin 0x7C00'in altinda).
2. A20 hattini acar (INT 15h AX=2401 ve port 0x92).
3. CPUID ile 64 bit (long mode) destegini denetler. Yoksa mesaj yazip durur.
4. VBE (setup_video):
   - INT 10h AX=4F00 ile VBE bilgisini 0x0600'e alir.
   - Mod listesini gezer (find_mode); her mod icin AX=4F01 ile 0x0800'e bilgi alir.
   - Kosullar: desteklenen + dogrusal framebuffer + grafik (0x91 bitleri), 32 bpp, dogrudan renk (model 6), tam genislik/yukseklik eslesmesi.
   - Once PREFERRED_WIDTH x PREFERRED_HEIGHT, bulunamazsa FALLBACK_WIDTH x FALLBACK_HEIGHT aranir.
   - Bulunan modu AX=4F02 ile (dogrusal framebuffer biti 0x4000 acik) etkinlestirir.
   - BootInfo yapisini 0x0900'e yazar (fill_boot_info).
5. GDT yukler, korumali moda gecer.
6. Sayfa tablolarini kurar: 4 GB, 2 MB'lik sayfalar. 0-3 GB onbellekli, 3-4 GB onbelleksiz (framebuffer ve MMIO icin).
7. Cekirdegi 0x9800'den 0x100000'e kopyalar (boyut, ISO boot info table'daki dosya uzunlugundan hesaplanir).
8. PAE + long mode'u acar, 64 bit koda gecer ve 0x100000'e atlar.

NASM tanimlari (build.sh verir): PREFERRED_WIDTH, PREFERRED_HEIGHT, FALLBACK_WIDTH, FALLBACK_HEIGHT.
Verilmezse hepsi 1024x768 olur.

## BootInfo (0x0900, little-endian)

| Offset | Boyut | Alan                                  |
|--------|-------|---------------------------------------|
| 0x00   | u32   | magic 0x4942434E ("NCBI")             |
| 0x04   | u32   | version (1)                           |
| 0x08   | u64   | framebuffer fiziksel adresi           |
| 0x10   | u32   | pitch (satir basina bayt)             |
| 0x14   | u32   | genislik                              |
| 0x18   | u32   | yukseklik                             |
| 0x1C   | u8    | bpp                                   |
| 0x1D   | u8    | kirmizi konum, 0x1E kirmizi boyut     |
| 0x1F   | u8    | yesil konum, 0x20 yesil boyut         |
| 0x21   | u8    | mavi konum, 0x22 mavi boyut           |

Bu yapi crates/nc-hal/src/lib.rs icindeki BootInfo ile birebir aynidir. Birini degistirirsen digerini de degistir
ve BOOT_INFO_VERSION degerini artir.

## Bellek kullanimi (dusuk bellek)

    0x0600  VBE bilgi blogu (512 bayt, gecici)
    0x0800  VBE mod bilgisi (256 bayt, gecici)
    0x0900  BootInfo
    0x1000  sayfa tablolari (PML4, PDPT, 4 PD) 0x6FFF'e kadar
    0x7C00  stage1 (2048 bayt), yigin altinda
    0x8800  stage2 (4096 bayt)
    0x9800  cekirdek, gecici (0x100000'e kopyalanir)

Cekirdek boyutu en fazla 589824 bayt olabilir (build.sh denetler); bunun uzerinde 0x9F000 sinirini asar.

## Bilinen sinirlar

- Sadece BIOS. UEFI destegi yok.
- Bellek haritasi (E820) henuz okunmuyor; cekirdek 64 MB RAM oldugunu varsayar.
- VBE pitch, BytesPerScanLine (offset 16) alanindan alinir; bazi VBE 3.0 BIOS'larinda LinBytesPerScanLine (offset 50) gerekebilir.
- Sadece 1280x720 ve 1024x768 modlari aranir.
