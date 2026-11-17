#!/usr/bin/env python3
import argparse, hashlib, struct, sys

MAGIC = b"ANDROID!"


def pad(data, page):
    rest = len(data) % page
    return data if rest == 0 else data + b"\0" * (page - rest)


def parse_os_version(text):
    if not text:
        return 0
    parts = [int(x) for x in text.split(".")]
    while len(parts) < 3:
        parts.append(0)
    return (parts[0] << 14) | (parts[1] << 7) | parts[2]


def parse_os_patch(text):
    if not text:
        return 0
    year, month = text.split("-")[:2]
    return ((int(year) - 2000) << 4) | int(month)


def read(path):
    if not path:
        return b""
    with open(path, "rb") as handle:
        return handle.read()


def build_v0_2(args, kernel, ramdisk, second, dtbo, dtb):
    version = args.header_version
    page = args.pagesize
    os_version = (parse_os_version(args.os_version) << 11) | parse_os_patch(args.os_patch_level)
    digest = hashlib.sha1()
    for blob in (kernel, ramdisk, second):
        digest.update(blob)
        digest.update(struct.pack("<I", len(blob)))
    if version >= 1:
        digest.update(dtbo)
        digest.update(struct.pack("<I", len(dtbo)))
    if version >= 2:
        digest.update(dtb)
        digest.update(struct.pack("<I", len(dtb)))
    image_id = digest.digest().ljust(32, b"\0")
    cmdline = args.cmdline.encode()
    if len(cmdline) > 1536:
        sys.exit("cmdline 1536 bayttan uzun olamaz")
    header_size = {0: 1632, 1: 1648, 2: 1660}[version]
    header = struct.pack(
        "<8s10I16s512s32s1024s",
        MAGIC,
        len(kernel), args.kernel_addr,
        len(ramdisk), args.ramdisk_addr,
        len(second), args.second_addr,
        args.tags_addr, page, version, os_version,
        args.board.encode()[:16],
        cmdline[:512], image_id, cmdline[512:1536],
    )
    if version >= 1:
        dtbo_offset = 0
        if dtbo:
            dtbo_offset = page + len(pad(kernel, page)) + len(pad(ramdisk, page)) + len(pad(second, page))
        header += struct.pack("<IQI", len(dtbo), dtbo_offset, header_size)
    if version >= 2:
        header += struct.pack("<IQ", len(dtb), args.dtb_addr)
    out = pad(header, page)
    for blob in (kernel, ramdisk, second):
        out += pad(blob, page)
    if version >= 1:
        out += pad(dtbo, page)
    if version >= 2:
        out += pad(dtb, page)
    return out


def build_v3_4(args, kernel, ramdisk):
    version = args.header_version
    os_version = (parse_os_version(args.os_version) << 11) | parse_os_patch(args.os_patch_level)
    cmdline = args.cmdline.encode()
    if len(cmdline) > 1536:
        sys.exit("cmdline 1536 bayttan uzun olamaz")
    header_size = 1580 if version == 3 else 1584
    header = struct.pack(
        "<8s4I16sI1536s",
        MAGIC, len(kernel), len(ramdisk), os_version, header_size,
        b"\0" * 16, version, cmdline.ljust(1536, b"\0"),
    )
    if version == 4:
        header += struct.pack("<I", 0)
    out = pad(header, 4096)
    out += pad(kernel, 4096)
    out += pad(ramdisk, 4096)
    return out


def main():
    ap = argparse.ArgumentParser(description="Android boot.img uretici (basliklar v0-v4)")
    ap.add_argument("--kernel", required=True)
    ap.add_argument("--ramdisk")
    ap.add_argument("--second")
    ap.add_argument("--recovery-dtbo")
    ap.add_argument("--dtb")
    ap.add_argument("--header-version", type=int, default=2, choices=[0, 1, 2, 3, 4])
    ap.add_argument("--pagesize", type=lambda x: int(x, 0), default=2048)
    ap.add_argument("--kernel-addr", type=lambda x: int(x, 0), default=0x40080000)
    ap.add_argument("--ramdisk-addr", type=lambda x: int(x, 0), default=0x11B00000)
    ap.add_argument("--second-addr", type=lambda x: int(x, 0), default=0)
    ap.add_argument("--tags-addr", type=lambda x: int(x, 0), default=0x07C08000)
    ap.add_argument("--dtb-addr", type=lambda x: int(x, 0), default=0x07C08000)
    ap.add_argument("--os-version", default="")
    ap.add_argument("--os-patch-level", default="")
    ap.add_argument("--board", default="")
    ap.add_argument("--cmdline", default="")
    ap.add_argument("-o", "--output", required=True)
    args = ap.parse_args()

    kernel = read(args.kernel)
    ramdisk = read(args.ramdisk)
    if args.header_version >= 3:
        data = build_v3_4(args, kernel, ramdisk)
    else:
        data = build_v0_2(args, kernel, ramdisk, read(args.second), read(args.recovery_dtbo), read(args.dtb))
    with open(args.output, "wb") as handle:
        handle.write(data)
    print("boot.img: %s (%d bayt, baslik v%d)" % (args.output, len(data), args.header_version))


if __name__ == "__main__":
    main()
