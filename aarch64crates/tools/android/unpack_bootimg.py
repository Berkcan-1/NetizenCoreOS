#!/usr/bin/env python3
import argparse, os, struct, sys


def pages(size, page):
    return (size + page - 1) // page * page


def os_text(value):
    version = value >> 11
    patch = value & 0x7FF
    a, b, c = (version >> 14) & 0x7F, (version >> 7) & 0x7F, version & 0x7F
    year, month = ((patch >> 4) & 0x7F) + 2000, patch & 0xF
    return "%d.%d.%d" % (a, b, c), "%04d-%02d" % (year, month)


def cstr(raw):
    return raw.split(b"\0", 1)[0].decode("ascii", "replace")


def main():
    ap = argparse.ArgumentParser(description="Android boot.img acici; device.conf degerlerini yazdirir")
    ap.add_argument("image")
    ap.add_argument("-o", "--out", default="")
    args = ap.parse_args()

    with open(args.image, "rb") as handle:
        data = handle.read()
    if data[:8] != b"ANDROID!":
        sys.exit("ANDROID! imzasi yok: bu bir boot.img degil (vendor_boot icin imza VNDRBOOT)")
    version = struct.unpack_from("<I", data, 40)[0]
    out = {}
    blobs = []
    if version <= 2:
        (ksize, kaddr, rsize, raddr, ssize, saddr, taddr, page, _ver, osv) = struct.unpack_from("<10I", data, 8)
        name = cstr(data[48:64])
        cmdline = cstr(data[64:576]) + cstr(data[608:1632])
        out.update(android_header_version=version, android_pagesize=page,
                   android_kernel_addr="0x%08x" % kaddr, android_ramdisk_addr="0x%08x" % raddr,
                   android_second_addr="0x%08x" % saddr, android_tags_addr="0x%08x" % taddr)
        offset = page
        blobs.append(("kernel", offset, ksize)); offset += pages(ksize, page)
        blobs.append(("ramdisk", offset, rsize)); offset += pages(rsize, page)
        blobs.append(("second", offset, ssize)); offset += pages(ssize, page)
        if version >= 1:
            dtbo_size, _dtbo_off, _hs = struct.unpack_from("<IQI", data, 1632)
            blobs.append(("recovery_dtbo", offset, dtbo_size)); offset += pages(dtbo_size, page)
        if version >= 2:
            dtb_size, dtb_addr = struct.unpack_from("<IQ", data, 1648)
            blobs.append(("dtb", offset, dtb_size))
            out["android_dtb_addr"] = "0x%08x" % dtb_addr
        out["android_board"] = name
    else:
        ksize, rsize, osv, _hs = struct.unpack_from("<4I", data, 8)
        cmdline = cstr(data[44:44 + 1536])
        page = 4096
        out.update(android_header_version=version, android_pagesize=page)
        blobs.append(("kernel", page, ksize))
        blobs.append(("ramdisk", page + pages(ksize, page), rsize))
    out["android_os_version"], out["android_os_patch"] = os_text(osv)
    out["android_cmdline"] = cmdline

    directory = args.out
    if directory:
        os.makedirs(directory, exist_ok=True)
    for label, start, size in blobs:
        if size == 0:
            continue
        chunk = data[start:start + size]
        head = chunk[:16].hex()
        print("# %s: %d bayt, ilk 16 bayt: %s" % (label, size, head))
        if directory:
            with open(os.path.join(directory, label), "wb") as handle:
                handle.write(chunk)
    for key, value in out.items():
        print("%s=%s" % (key, value))


if __name__ == "__main__":
    main()
