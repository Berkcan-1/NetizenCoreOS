#!/usr/bin/env python3
"""Validate NetizenCore mobile build deliverables (boot.img, super.img, userdata.img).

Checks:
1. Output directory contains ONLY the three expected image artifacts:
   boot.img, super.img, userdata.img.
2. boot.img has a valid ANDROID! header (v0..v4), bounds-checked payload offsets,
   and a valid AArch64 Linux/Image kernel magic ("ARM\\x64" at offset 56, handling gzip).
3. super.img has valid Android LP (liblp) geometry & metadata matching aarch64kernel/src/lp.rs
   (raw or Android sparse format), valid SHA-256 checksums, a readable logical system
   partition ('system' or 'system_a') within bounds, and a valid FAT32 volume (NCSYSTEM).
4. userdata.img has the exact expected capacity, valid exFAT VBR, cluster count <= 1048576
   (matching crates/services/nc-fs/src/exfat.rs), and the expected NCDATA volume label.
"""
import argparse
import gzip
import hashlib
import os
import struct
import sys
from pathlib import Path

EXPECTED_DELIVERABLES = {"boot.img", "super.img", "userdata.img"}

SECTOR = 512
LP_GEOMETRY_MAGIC = 0x616C4467
LP_HEADER_MAGIC = 0x414C5030
LP_RESERVED = 4096
LP_GEOMETRY_SIZE = 4096
LP_MAJOR = 10
LP_MINOR_MAX = 2
SPARSE_MAGIC = 0xED26FF3A
EXFAT_MAX_CLUSTERS = 1_048_576


def fail(msg: str) -> None:
    print(f"HATA [imaj dogrulama]: {msg}", file=sys.stderr)
    sys.exit(1)


def align_up(value: int, page: int) -> int:
    return ((value + page - 1) // page) * page


def validate_output_dir(out_dir: Path) -> None:
    if not out_dir.is_dir():
        fail(f"cikti dizini bulunamadi: {out_dir}")
    entries = {p.name for p in out_dir.iterdir()}
    missing = sorted(EXPECTED_DELIVERABLES - entries)
    extra = sorted(entries - EXPECTED_DELIVERABLES)
    if missing:
        fail(f"eksik final imaj(lar): {', '.join(missing)}")
    if extra:
        fail(f"cikti dizininde beklenmeyen ara dosya/klasor bulundu: {', '.join(extra)}")


def validate_boot_img(path: Path) -> str:
    size = path.stat().st_size
    if size < 2048:
        fail(f"boot.img cok kucuk ({size} bayt)")
    data = path.read_bytes()
    if data[:8] != b"ANDROID!":
        fail("boot.img gecersiz: ANDROID! sihirli basligi yok")
    version = struct.unpack_from("<I", data, 40)[0]
    if version <= 2:
        ksize, _kaddr, rsize, _raddr, ssize, _saddr, _taddr, page, _ver, _osv = struct.unpack_from("<10I", data, 8)
        if page not in (2048, 4096, 8192, 16384):
            fail(f"boot.img gecersiz sayfa boyutu: {page}")
        koff = page
        if ksize == 0 or koff + ksize > len(data):
            fail(f"boot.img cekirdek yuku gecersiz (offset={koff}, boyut={ksize}, dosya={len(data)})")
        roff = koff + align_up(ksize, page)
        if roff + rsize > len(data):
            fail("boot.img ramdisk bolgesi dosya sinirini asiyor")
        soff = roff + align_up(rsize, page)
        if soff + ssize > len(data):
            fail("boot.img second bolgesi dosya sinirini asiyor")
        dtbo_size = 0
        dtb_size = 0
        cur = soff + align_up(ssize, page)
        if version >= 1:
            dtbo_size, _dtbo_off, _hs = struct.unpack_from("<IQI", data, 1632)
            if cur + dtbo_size > len(data):
                fail("boot.img dtbo bolgesi dosya sinirini asiyor")
            cur += align_up(dtbo_size, page)
        if version >= 2:
            dtb_size, _dtb_addr = struct.unpack_from("<IQ", data, 1648)
            if cur + dtb_size > len(data):
                fail("boot.img dtb bolgesi dosya sinirini asiyor")
        kernel_blob = data[koff : koff + ksize]
    elif version in (3, 4):
        ksize, rsize, _osv, _hs = struct.unpack_from("<4I", data, 8)
        page = 4096
        koff = page
        if ksize == 0 or koff + ksize > len(data):
            fail("boot.img v3/v4 cekirdek yuku gecersiz")
        roff = koff + align_up(ksize, page)
        if roff + rsize > len(data):
            fail("boot.img v3/v4 ramdisk bolgesi dosya sinirini asiyor")
        dtb_size = 0
        kernel_blob = data[koff : koff + ksize]
    else:
        fail(f"boot.img desteklenmeyen baslik surumu: {version}")

    # Verify ARM64 Linux Image header magic at offset 56 ("ARM\x64"), decompressing gzip if used
    if kernel_blob[:2] == b"\x1f\x8b":
        try:
            raw_kernel = gzip.decompress(kernel_blob)
        except Exception as exc:
            fail(f"boot.img gzip cekirdek yuku acilamadi: {exc}")
        comp = "gzip"
    else:
        raw_kernel = kernel_blob
        comp = "raw"

    if len(raw_kernel) < 64 or raw_kernel[56:60] != b"ARM\x64":
        fail("boot.img icindeki cekirdek gecerli bir ARM64 Image degil (offset 56'da ARM\\x64 yok)")

    return f"v{version}, kernel={ksize}B ({comp}, raw={len(raw_kernel)}B), ramdisk={rsize}B, dtb={dtb_size}B"


def read_sparse_range(fh, block_size: int, chunks: list, offset: int, length: int) -> bytes:
    """Read `length` bytes at logical `offset` from an Android sparse image without expanding the whole image."""
    out = bytearray(length)
    pos = 0
    while pos < length:
        cur = offset + pos
        blk = cur // block_size
        in_blk = cur % block_size
        # Binary search chunk containing blk
        lo, hi = 0, len(chunks) - 1
        found = None
        while lo <= hi:
            mid = (lo + hi) // 2
            c_blk, c_cnt, c_type, c_foff, c_fill = chunks[mid]
            if blk < c_blk:
                hi = mid - 1
            elif blk >= c_blk + c_cnt:
                lo = mid + 1
            else:
                found = chunks[mid]
                break
        if found is None:
            fail(f"sparse super.img: mantiksal ofset {cur} kapsam disinda")
        c_blk, c_cnt, c_type, c_foff, c_fill = found
        chunk_end = (c_blk + c_cnt) * block_size
        take = min(length - pos, chunk_end - cur)
        if c_type == 0xCAC1:  # RAW
            fh.seek(c_foff + (cur - c_blk * block_size))
            buf = fh.read(take)
            if len(buf) != take:
                fail("sparse super.img: RAW parcasi kesik")
            out[pos : pos + take] = buf
        elif c_type == 0xCAC2:  # FILL
            pattern = struct.pack("<I", c_fill)
            for i in range(take):
                out[pos + i] = pattern[(cur + i) & 3]
        elif c_type == 0xCAC3:  # DONT_CARE
            pass  # already zeroed
        else:
            fail(f"sparse super.img: desteklenmeyen parca turu {c_type:#x}")
        pos += take
    return bytes(out)


def open_super_reader(path: Path):
    file_size = path.stat().st_size
    if file_size < LP_RESERVED + LP_GEOMETRY_SIZE * 2 + 4096:
        fail(f"super.img cok kucuk ({file_size} bayt)")
    fh = path.open("rb")
    head = fh.read(28)
    magic = struct.unpack_from("<I", head, 0)[0]
    if magic == SPARSE_MAGIC:
        maj, min_v, fhdr_sz, chdr_sz, blk_sz, total_blks, total_chunks, _csum = struct.unpack_from("<HHHHIIII", head, 4)
        if maj != 1 or fhdr_sz < 28 or chdr_sz < 12 or blk_sz == 0 or (blk_sz % SECTOR) != 0:
            fh.close()
            fail("super.img sparse basligi gecersiz")
        fh.seek(fhdr_sz)
        chunks = []
        cur_blk = 0
        for _ in range(total_chunks):
            ch = fh.read(chdr_sz)
            if len(ch) < 12:
                fh.close()
                fail("super.img sparse parca basligi kesik")
            ctype, _res, cblks, ctotal = struct.unpack_from("<HHII", ch, 0)
            data_off = fh.tell()
            data_len = ctotal - chdr_sz
            if data_len < 0 or data_off + data_len > file_size:
                fh.close()
                fail("super.img sparse parca boyutu dosya sinirini asiyor")
            fill = 0
            if ctype == 0xCAC2:
                if data_len != 4:
                    fh.close()
                    fail("super.img sparse FILL parcasi 4 bayt degil")
                fill = struct.unpack("<I", fh.read(4))[0]
            else:
                fh.seek(data_off + data_len)
            if ctype != 0xCAC4:  # skip CRC32 chunk
                chunks.append((cur_blk, cblks, ctype, data_off, fill))
                cur_blk += cblks
        logical_size = total_blks * blk_sz
        if cur_blk != total_blks:
            fh.close()
            fail(f"super.img sparse blok toplami uyumsuz ({cur_blk} != {total_blks})")
        return fh, True, logical_size, lambda off, ln: read_sparse_range(fh, blk_sz, chunks, off, ln)
    else:
        def read_raw(off: int, ln: int) -> bytes:
            if off + ln > file_size:
                fail(f"super.img okuma sinir disi ({off} + {ln} > {file_size})")
            fh.seek(off)
            return fh.read(ln)
        return fh, False, file_size, read_raw


def verify_embedded_sha256(buf: bytes, csum_off: int) -> bool:
    expected = buf[csum_off : csum_off + 32]
    zeroed = bytearray(buf)
    zeroed[csum_off : csum_off + 32] = b"\x00" * 32
    return hashlib.sha256(zeroed).digest() == expected


def validate_super_img(path: Path, expected_part: str) -> str:
    fh, is_sparse, logical_size, reader = open_super_reader(path)
    try:
        geom_prefix = reader(LP_RESERVED, 52)
        g_magic, g_size = struct.unpack_from("<II", geom_prefix, 0)
        if g_magic != LP_GEOMETRY_MAGIC:
            fail(f"super.img LP geometri imzasi gecersiz: {g_magic:#x}")
        if g_size < 52 or g_size > LP_GEOMETRY_SIZE:
            fail(f"super.img LP geometri boyutu gecersiz: {g_size}")
        geom = geom_prefix if g_size == 52 else reader(LP_RESERVED, g_size)
        if not verify_embedded_sha256(geom, 8):
            fail("super.img LP geometri SHA-256 dogrulamasi basarisiz")
        meta_max, slots, logical_blk = struct.unpack_from("<III", geom, 40)
        if meta_max == 0 or slots == 0 or logical_blk == 0 or (logical_blk % SECTOR) != 0:
            fail("super.img LP geometri parametreleri gecersiz")

        meta_off = LP_RESERVED + LP_GEOMETRY_SIZE * 2
        hdr_probe = reader(meta_off, 128)
        h_magic, major, minor, h_size = struct.unpack_from("<IHHI", hdr_probe, 0)
        if h_magic != LP_HEADER_MAGIC:
            fail(f"super.img LP baslik imzasi gecersiz: {h_magic:#x}")
        if major != LP_MAJOR or minor > LP_MINOR_MAX:
            fail(f"super.img LP surumu desteklenmiyor: {major}.{minor}")
        if h_size < 128 or h_size > 4096:
            fail(f"super.img LP baslik boyutu gecersiz: {h_size}")
        hdr = reader(meta_off, h_size)
        if not verify_embedded_sha256(hdr, 12):
            fail("super.img LP baslik SHA-256 dogrulamasi basarisiz")
        tables_size = struct.unpack_from("<I", hdr, 44)[0]
        tables_csum = hdr[48:80]
        tables = reader(meta_off + h_size, tables_size)
        if hashlib.sha256(tables).digest() != tables_csum:
            fail("super.img LP tablo SHA-256 dogrulamasi basarisiz")

        p_off, p_cnt, p_esz = struct.unpack_from("<III", hdr, 80)
        e_off, e_cnt, e_esz = struct.unpack_from("<III", hdr, 92)
        if p_esz < 52 or e_esz < 24:
            fail("super.img LP tablo giris boyutu gecersiz")

        accepted_names = [expected_part] if expected_part else ["system", "system_a"]
        found_part = None
        for i in range(p_cnt):
            base = p_off + i * p_esz
            raw_name = tables[base : base + 36]
            name = raw_name.split(b"\x00", 1)[0].decode("ascii", errors="replace")
            if name in accepted_names:
                _attrs, first_ext, num_ext, _grp = struct.unpack_from("<IIII", tables, base + 36)
                found_part = (name, first_ext, num_ext)
                break
        if found_part is None:
            fail(f"super.img icinde beklenen mantiksal bolum bulunamadi ({', '.join(accepted_names)})")

        part_name, first_ext, num_ext = found_part
        if num_ext == 0 or first_ext + num_ext > e_cnt:
            fail(f"super.img '{part_name}' bolumu gecersiz uzanti sayisina sahip")

        total_sectors = 0
        first_sector = None
        for j in range(num_ext):
            eb = e_off + (first_ext + j) * e_esz
            num_sec, target_type, target_data, _target_src = struct.unpack_from("<QIQI", tables, eb)
            if target_type != 0 or num_sec == 0:
                fail(f"super.img '{part_name}' yalnizca dogrusal (TARGET_LINEAR) uzantilari destekler")
            if (target_data + num_sec) * SECTOR > logical_size:
                fail(f"super.img '{part_name}' uzantisi imaj sinirini asiyor")
            if first_sector is None:
                first_sector = target_data
            total_sectors += num_sec

        # Verify embedded FAT32 system partition boot sector
        vbr = reader(first_sector * SECTOR, 512)
        if vbr[510:512] != b"\x55\xAA" or vbr[82:90] != b"FAT32   ":
            fail(f"super.img '{part_name}' bolumu gecerli bir FAT32 dosya sistemi degil (cekirdek FAT32 bekler)")
        label = vbr[71:82].decode("ascii", errors="replace").strip()
        if label != "NCSYSTEM":
            fail(f"super.img '{part_name}' FAT32 etiketi 'NCSYSTEM' degil: '{label}'")

        kind = "sparse" if is_sparse else "raw"
        return f"{kind}, logical={logical_size}B, part='{part_name}' ({total_sectors} sectors, FAT32 '{label}')"
    finally:
        fh.close()


def validate_userdata_img(path: Path, expected_mb: int) -> str:
    size = path.stat().st_size
    expected_bytes = expected_mb * 1024 * 1024
    if size != expected_bytes:
        fail(f"userdata.img boyutu uyumsuz: {size} bayt (beklenen {expected_bytes} bayt / {expected_mb} MiB)")
    with path.open("rb") as fh:
        vbr = fh.read(512)
        if len(vbr) != 512 or vbr[3:11] != b"EXFAT   " or vbr[510:512] != b"\x55\xAA":
            fail("userdata.img gecerli bir exFAT birimi degil")
        fat_off = struct.unpack_from("<I", vbr, 80)[0]
        cluster_heap_off = struct.unpack_from("<I", vbr, 88)[0]
        cluster_count = struct.unpack_from("<I", vbr, 92)[0]
        root_cluster = struct.unpack_from("<I", vbr, 96)[0]
        bps_shift = vbr[108]
        spc_shift = vbr[109]
        if bps_shift != 9:
            fail(f"userdata.img sektor boyutu 512 degil (shift={bps_shift})")
        if cluster_count == 0 or cluster_count > EXFAT_MAX_CLUSTERS:
            fail(f"userdata.img kume sayisi cekirdek sinirini asiyor: {cluster_count} > {EXFAT_MAX_CLUSTERS}")
        if fat_off == 0 or cluster_heap_off == 0 or root_cluster < 2:
            fail("userdata.img exFAT geometrisi gecersiz")
        cluster_bytes = (1 << bps_shift) * (1 << spc_shift)
        root_off = (cluster_heap_off + (root_cluster - 2) * (1 << spc_shift)) * SECTOR
        fh.seek(root_off)
        root_data = fh.read(min(cluster_bytes, 65536))
        found_label = ""
        for off in range(0, len(root_data), 32):
            etype = root_data[off]
            if etype == 0x00:
                break
            if etype == 0x83:
                char_cnt = root_data[off + 1]
                raw_utf16 = root_data[off + 2 : off + 2 + min(char_cnt, 11) * 2]
                found_label = raw_utf16.decode("utf-16le", errors="replace")
                break
        if found_label != "NCDATA":
            fail(f"userdata.img exFAT birim etiketi 'NCDATA' degil: '{found_label}'")
        return f"exFAT {expected_mb} MiB, label='{found_label}', cluster={cluster_bytes // 1024}KiB, clusters={cluster_count}"


def main() -> None:
    ap = argparse.ArgumentParser(description="NetizenCore mobil 3-imaj dogrulayici")
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--userdata-mb", required=True, type=int)
    ap.add_argument("--system-part", default="")
    args = ap.parse_args()

    out_dir = Path(args.out_dir)
    validate_output_dir(out_dir)
    boot_info = validate_boot_img(out_dir / "boot.img")
    super_info = validate_super_img(out_dir / "super.img", args.system_part)
    data_info = validate_userdata_img(out_dir / "userdata.img", args.userdata_mb)

    print("[dogrulama] Mobil 3-imaj ciktisi gecerli:")
    print(f"  - {out_dir / 'boot.img'} ({(out_dir / 'boot.img').stat().st_size} bayt) : {boot_info}")
    print(f"  - {out_dir / 'super.img'} ({(out_dir / 'super.img').stat().st_size} bayt) : {super_info}")
    print(f"  - {out_dir / 'userdata.img'} ({(out_dir / 'userdata.img').stat().st_size} bayt) : {data_info}")


if __name__ == "__main__":
    main()
