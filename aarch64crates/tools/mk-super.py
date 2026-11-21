#!/usr/bin/env python3
# aarch64crates/tools/mk-super.py -- tek bölümlü, salt-okunur, Android LP (liblp) biçiminde super.img üretir.
# Kullanım: python3 aarch64crates/tools/mk-super.py <cikti super.img> <bolum adi> <bolum imaji>
# Biçim kaynağı: AOSP system/core/fs_mgr/liblp/include/liblp/metadata_format.h
#   0..4096 ayrılmış, 4096 geometri, 8192 yedek geometri, 12288'den itibaren metadata yuvaları (önce
#   birincil, sonra yedek). Bölüm verisi first_logical_sector (2048 = 1 MiB) konumundan başlar.
# Bu betik lpmake'in yerini tutmaz: yalnızca QEMU geliştirme akışı içindir. Gerçek cihaz için lpmake kullanın.
import hashlib
import struct
import sys

SECTOR = 512
GEOMETRY_MAGIC = 0x616C4467
HEADER_MAGIC = 0x414C5030
RESERVED = 4096
GEOMETRY_SIZE = 4096
METADATA_MAX = 65536
SLOT_COUNT = 2
FIRST_SECTOR = 2048
ALIGN = 1024 * 1024


def name36(text):
    raw = text.encode("ascii")
    if len(raw) >= 36:
        sys.exit("hata: ad 35 karakteri aşıyor")
    return raw + b"\0" * (36 - len(raw))


def sha(data):
    return hashlib.sha256(data).digest()


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    out_path, part_name, image_path = sys.argv[1:]
    image = open(image_path, "rb").read()
    sectors = (len(image) + SECTOR - 1) // SECTOR
    image += b"\0" * (sectors * SECTOR - len(image))
    data_bytes = FIRST_SECTOR * SECTOR + len(image)
    total = (data_bytes + ALIGN - 1) // ALIGN * ALIGN

    partition = name36(part_name) + struct.pack("<IIII", 1, 0, 1, 0)
    extent = struct.pack("<QIQI", sectors, 0, FIRST_SECTOR, 0)
    group = name36("default") + struct.pack("<IQ", 0, 0)
    block_device = struct.pack("<QIIQ", FIRST_SECTOR, ALIGN, 0, total) + name36("super") + struct.pack("<I", 0)
    assert len(partition) == 52 and len(extent) == 24 and len(group) == 48 and len(block_device) == 64

    tables = partition + extent + group + block_device
    desc = lambda off, n, size: struct.pack("<III", off, n, size)
    descriptors = (
        desc(0, 1, 52)
        + desc(52, 1, 24)
        + desc(76, 1, 48)
        + desc(124, 1, 64)
    )
    header_size = 256

    def header(checksum):
        return (
            struct.pack("<IHHI", HEADER_MAGIC, 10, 2, header_size)
            + checksum
            + struct.pack("<I", len(tables))
            + sha(tables)
            + descriptors
            + struct.pack("<I", 0)
            + b"\0" * 124
        )

    assert len(header(b"\0" * 32)) == header_size
    header_bytes = header(sha(header(b"\0" * 32)))
    metadata = header_bytes + tables
    assert len(metadata) <= METADATA_MAX
    metadata += b"\0" * (METADATA_MAX - len(metadata))

    def geometry_struct(checksum):
        return struct.pack("<II", GEOMETRY_MAGIC, 52) + checksum + struct.pack("<III", METADATA_MAX, SLOT_COUNT, 4096)

    geometry = geometry_struct(sha(geometry_struct(b"\0" * 32)))
    assert len(geometry) == 52
    geometry_block = geometry + b"\0" * (GEOMETRY_SIZE - len(geometry))

    img = bytearray(total)
    img[RESERVED:RESERVED + GEOMETRY_SIZE] = geometry_block
    img[RESERVED + GEOMETRY_SIZE:RESERVED + 2 * GEOMETRY_SIZE] = geometry_block
    base = RESERVED + 2 * GEOMETRY_SIZE
    for slot in range(SLOT_COUNT):
        img[base + slot * METADATA_MAX:base + (slot + 1) * METADATA_MAX] = metadata
        backup = base + (SLOT_COUNT + slot) * METADATA_MAX
        img[backup:backup + METADATA_MAX] = metadata
    img[FIRST_SECTOR * SECTOR:FIRST_SECTOR * SECTOR + len(image)] = image
    with open(out_path, "wb") as handle:
        handle.write(img)
    print(f"super.img: bolum '{part_name}' {sectors} sektor, toplam {total} bayt")


main()
