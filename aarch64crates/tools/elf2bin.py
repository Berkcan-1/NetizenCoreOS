#!/usr/bin/env python3
import struct, sys


def main():
    if len(sys.argv) != 3:
        sys.exit("kullanim: elf2bin.py <girdi.elf> <cikti.bin>")
    with open(sys.argv[1], "rb") as handle:
        data = handle.read()
    if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        sys.exit("hata: ELF64 little-endian degil")
    phoff, = struct.unpack_from("<Q", data, 0x20)
    phentsize, phnum = struct.unpack_from("<HH", data, 0x36)
    segments = []
    for index in range(phnum):
        base = phoff + index * phentsize
        kind, _flags, offset, vaddr, paddr, filesz, _memsz, _align = struct.unpack_from("<IIQQQQQQ", data, base)
        if kind == 1 and filesz > 0:
            segments.append((paddr, offset, filesz))
    if not segments:
        sys.exit("hata: yuklenebilir segment yok")
    segments.sort()
    start = segments[0][0]
    end = max(paddr + filesz for paddr, _offset, filesz in segments)
    if end - start > 64 * 1024 * 1024:
        sys.exit("hata: segmentler arasi bosluk cok buyuk (%d bayt); bağlayici betigini kontrol edin" % (end - start))
    image = bytearray(end - start)
    for paddr, offset, filesz in segments:
        image[paddr - start:paddr - start + filesz] = data[offset:offset + filesz]
    with open(sys.argv[2], "wb") as handle:
        handle.write(image)
    print("elf2bin: taban 0x%x, %d bayt" % (start, len(image)))


if __name__ == "__main__":
    main()
