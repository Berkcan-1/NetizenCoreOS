#!/usr/bin/env python3
"""Automated ARM64 QEMU boot test for NetizenCore (Feature 48).

Boots the built ARM64 qemu-virt kernel image with an isolated test userdata
image, captures PL011 serial output, verifies all required initialization
and EL0 self-test markers, rejects kernel panics/fatal errors, and verifies
zero PMM frame leaks.
"""
import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REQUIRED_MARKERS = [
    "NetizenCore aarch64 cekirdegi basladi",
    "mmu acik: W^X",
    "heap sinamasi tamam",
    "[SPARK] Guvenlik cekirdegi ve DysonSphere etkin",
    "gic: true, zamanlayici: true",
    "[depo] super.img: bolum 'system'",
    "[depo] sistem bolumu FAT32 baglandi",
    "[depo] /data baglandi (exFAT, yazilabilir): yazma sinamasi tamam, flush tamam",
    "EL0 oz-sinama basliyor",
    "[gorev] test-hata hata verdi:",
    "[gorev] test-gecersiz-isaretci bitti, cikis kodu -14",
    "[gorev] test-mesgul-dongu bitti, cikis kodu 7",
    "[gorev] test-a bitti, cikis kodu 0",
    "[gorev] test-b bitti, cikis kodu 0",
    "NetizenHome yuklendi (EL0",
    "[ag] dhcp: ip=",
]

FATAL_MARKERS = [
    "CEKIRDEK PANIGI",
    "yigin tasmasi!",
    "kritik hata:",
    "nc_sec_fatal_trap",
]


def create_exfat_disk(path: Path, size_mb: int) -> None:
    if not shutil.which("mkfs.exfat"):
        raise RuntimeError("mkfs.exfat not found (install exfatprogs)")
    with open(path, "wb") as f:
        f.truncate(size_mb * 1024 * 1024)
    subprocess.run(
        ["mkfs.exfat", "-c", "65536", "-L", "NCDATA", str(path)],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="NetizenCore ARM64 QEMU boot test")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[2]))
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--mem", type=int, default=512)
    parser.add_argument("--smp", type=int, default=1)
    parser.add_argument("--cpu", default="cortex-a72")
    parser.add_argument("--log", default=None)
    args = parser.parse_args()

    root = Path(args.root)
    out_dir = root / "build" / "armv8a-qemu-virt"
    image = out_dir / "Image"
    super_img = out_dir / "super.img"

    if not image.is_file():
        print(f"[FAIL] Kernel image not found: {image} (run: bash build.sh armv8a qemu build)", file=sys.stderr)
        return 2
    if not super_img.is_file():
        print(f"[INFO] super.img missing; generating via build.sh helper...")
        subprocess.run(
            ["bash", "-c", f"source '{root}/tools/build/common.sh'; source '{root}/tools/build/device.sh'; source '{root}/aarch64crates/tools/aarch64.sh'; source '{root}/aarch64crates/tools/android.sh'; source '{root}/aarch64crates/tools/qemu-aarch64.sh'; ROOT='{root}'; BUILD='{root}/build'; OUT='{out_dir}'; DEVICE='qemu-virt'; ARCH='armv8a'; TAG='qemu'; load_device qemu-virt; build_qemu_super"],
            check=True,
        )

    if not shutil.which("qemu-system-aarch64"):
        print("[FAIL] qemu-system-aarch64 not found", file=sys.stderr)
        return 2

    log_path = Path(args.log) if args.log else (out_dir / "ci-boot.log")
    if log_path.exists():
        log_path.unlink()

    with tempfile.TemporaryDirectory(prefix="nc-qemu-boot-") as tmpdir:
        userdata = Path(tmpdir) / "userdata.img"
        create_exfat_disk(userdata, 64)

        cmd = [
            "qemu-system-aarch64",
            "-M", "virt,gic-version=3",
            "-cpu", args.cpu,
            "-smp", str(args.smp),
            "-m", str(args.mem),
            "-kernel", str(image),
            "-device", "ramfb",
            "-device", "virtio-keyboard-device",
            "-device", "virtio-tablet-device",
            "-drive", f"if=none,id=super,file={super_img},format=raw,readonly=on",
            "-device", "virtio-blk-device,drive=super",
            "-drive", f"if=none,id=userdata,file={userdata},format=raw",
            "-device", "virtio-blk-device,drive=userdata",
            "-netdev", "user,id=net0",
            "-device", "virtio-net-device,netdev=net0",
            "-serial", f"file:{log_path}",
            "-display", "none",
            "-no-reboot",
        ]

        t0 = time.monotonic()
        proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        boot_done = False
        fatal_hit = None
        serial_text = ""

        try:
            deadline = t0 + args.timeout
            while time.monotonic() < deadline:
                if log_path.exists():
                    serial_text = log_path.read_text(encoding="utf-8", errors="replace")
                    for fatal in FATAL_MARKERS:
                        if fatal in serial_text:
                            fatal_hit = fatal
                            break
                    if fatal_hit:
                        break
                    if "NetizenHome yuklendi (EL0" in serial_text:
                        # Wait briefly for any trailing init lines (e.g. virtio-net)
                        time.sleep(0.5)
                        serial_text = log_path.read_text(encoding="utf-8", errors="replace")
                        boot_done = True
                        break
                if proc.poll() is not None:
                    break
                time.sleep(0.2)
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=3.0)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()

        elapsed = time.monotonic() - t0
        if log_path.exists():
            serial_text = log_path.read_text(encoding="utf-8", errors="replace")

    if fatal_hit:
        print(f"[FAIL] Fatal kernel marker detected: '{fatal_hit}' after {elapsed:.2f}s", file=sys.stderr)
        print(serial_text, file=sys.stderr)
        return 1

    missing = [m for m in REQUIRED_MARKERS if m not in serial_text]
    if missing:
        print(f"[FAIL] ARM64 QEMU boot test failed after {elapsed:.2f}s (boot_done={boot_done})", file=sys.stderr)
        for m in missing:
            print(f"  MISSING MARKER: {m}", file=sys.stderr)
        print("--- Serial Log ---", file=sys.stderr)
        print(serial_text, file=sys.stderr)
        return 1

    pmm_match = re.search(r"pmm: bosta (\d+) kare \(baslangic (\d+)\)", serial_text)
    if not pmm_match:
        print("[FAIL] PMM frame count marker not found", file=sys.stderr)
        return 1
    free_frames, init_frames = int(pmm_match.group(1)), int(pmm_match.group(2))
    if free_frames != init_frames:
        print(f"[FAIL] PMM frame leak detected: free={free_frames} != initial={init_frames}", file=sys.stderr)
        return 1

    if args.smp > 1:
        expected_smp = f"smp: {args.smp} CPU online"
        if expected_smp not in serial_text:
            print(f"[FAIL] SMP verification failed: expected '{expected_smp}' in serial log", file=sys.stderr)
            return 1

    print(f"[PASS] ARM64 QEMU boot verified in {elapsed:.2f}s (smp={args.smp}, mem={args.mem}M, pmm={free_frames}/{init_frames} frames, log={log_path})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
