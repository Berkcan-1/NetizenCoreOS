#!/usr/bin/env bash
# NetizenCore Local & CI Verification Pipeline (Feature 48)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

RUN_PROVE=0
SKIP_X86=0
for arg in "$@"; do
    case "$arg" in
        --prove) RUN_PROVE=1 ;;
        --skip-x86) SKIP_X86=1 ;;
        -h|--help)
            echo "Usage: bash tools/check/run_ci.sh [--prove] [--skip-x86]"
            exit 0
            ;;
        *)
            echo "Unknown option: $arg" >&2
            exit 1
            ;;
    esac
done

echo "=== [1/7] Repository layout verification ==="
python3 tools/check/verify_layout.py

echo "=== [2/7] Shared Rust crate unit tests ==="
cargo test -p nc-hal -p nc-abi -p nc-caps -p nc-pkg -p nc-mem -p nc-fs -p nc-fs-policy -p nc-crypto -p nc-elf -p nc-mobile-ui --lib

echo "=== [3/7] SPARK Ada KAT & security unit tests ==="
bash build.sh spark-test

echo "=== [4/7] SPARK Ada flow / proof analysis ==="
if [ "$RUN_PROVE" = "1" ]; then
    bash build.sh spark-prove
else
    bash build.sh spark-check
fi

echo "=== [5/7] ARM64 (armv8a qemu-virt) kernel + packages build ==="
bash build.sh armv8a qemu build

echo "=== [6/7] Automated ARM64 QEMU boot tests (UP + 4-core SMP) ==="
python3 tools/check/qemu_boot_test.py --timeout 20 --mem 512 --smp 1
python3 tools/check/qemu_boot_test.py --timeout 20 --mem 512 --smp 4

if [ "$SKIP_X86" = "0" ]; then
    echo "=== [7/7] x86_64 (qemu) regression build ==="
    bash build.sh x86_64 qemu build
else
    echo "=== [7/7] x86_64 regression build skipped (--skip-x86) ==="
fi

echo "=== ALL CI STAGES PASSED ==="
