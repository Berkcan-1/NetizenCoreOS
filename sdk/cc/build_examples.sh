#!/usr/bin/env bash
set -euo pipefail

SDK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_DIR="${1:-$SDK_DIR/build}"
mkdir -p "$OUT_DIR"

# Discover AArch64 C compiler (configurable via NC_CC)
find_cc() {
    if [ -n "${NC_CC:-}" ] && command -v "$NC_CC" >/dev/null 2>&1; then
        echo "$NC_CC"
        return 0
    fi
    for cand in aarch64-linux-gnu-gcc aarch64-elf-gcc "$HOME/.local/share/nc-spark-tools/bin/aarch64-elf-gcc"; do
        if command -v "$cand" >/dev/null 2>&1 || [ -x "$cand" ]; then
            echo "$cand"
            return 0
        fi
    done
    # Fallback to Android NDK clang if present
    local ndk_clang
    ndk_clang="$(ls -1d "$HOME"/Android/Sdk/ndk/*/toolchains/llvm/prebuilt/linux-x86_64/bin/clang 2>/dev/null | sort -V | tail -n 1 || true)"
    if [ -n "$ndk_clang" ] && [ -x "$ndk_clang" ]; then
        echo "$ndk_clang"
        return 0
    fi
    return 1
}

# Discover AArch64 C++ compiler (configurable via NC_CXX)
find_cxx() {
    if [ -n "${NC_CXX:-}" ] && command -v "$NC_CXX" >/dev/null 2>&1; then
        echo "$NC_CXX"
        return 0
    fi
    for cand in aarch64-linux-gnu-g++ aarch64-elf-g++ "$HOME/.local/share/nc-spark-tools/bin/aarch64-elf-g++"; do
        if command -v "$cand" >/dev/null 2>&1 || [ -x "$cand" ]; then
            echo "$cand"
            return 0
        fi
    done
    local ndk_cxx
    ndk_cxx="$(ls -1d "$HOME"/Android/Sdk/ndk/*/toolchains/llvm/prebuilt/linux-x86_64/bin/clang++ 2>/dev/null | sort -V | tail -n 1 || true)"
    if [ -n "$ndk_cxx" ] && [ -x "$ndk_cxx" ]; then
        echo "$ndk_cxx"
        return 0
    fi
    return 1
}

CC_BIN="$(find_cc)" || { echo "ERROR: No AArch64 C cross-compiler found (set NC_CC)" >&2; exit 1; }
CXX_BIN="$(find_cxx)" || { echo "ERROR: No AArch64 C++ cross-compiler found (set NC_CXX)" >&2; exit 1; }

CC_FLAGS=(-ffreestanding -nostdlib -nostartfiles -fno-pic -O2 -Wall -Wextra -I"$SDK_DIR/include" -T"$SDK_DIR/linker/aarch64-app.ld")
CXX_FLAGS=(-ffreestanding -nostdlib -nostartfiles -fno-exceptions -fno-rtti -fno-pic -std=c++20 -O2 -Wall -Wextra -I"$SDK_DIR/include" -T"$SDK_DIR/linker/aarch64-app.ld")

case "$(basename "$CC_BIN")" in
    clang*) CC_FLAGS=(--target=aarch64-unknown-none-elf -fuse-ld=lld "${CC_FLAGS[@]}") ;;
esac
case "$(basename "$CXX_BIN")" in
    clang++*) CXX_FLAGS=(--target=aarch64-unknown-none-elf -fuse-ld=lld "${CXX_FLAGS[@]}") ;;
esac

echo "[C SDK]   $CC_BIN -> $OUT_DIR/hello_c.elf"
"$CC_BIN" "${CC_FLAGS[@]}" "$SDK_DIR/examples/hello_c/main.c" -o "$OUT_DIR/hello_c.elf"

echo "[C++ SDK] $CXX_BIN -> $OUT_DIR/hello_cpp.elf"
"$CXX_BIN" "${CXX_FLAGS[@]}" "$SDK_DIR/examples/hello_cpp/main.cpp" -o "$OUT_DIR/hello_cpp.elf"

if command -v aarch64-linux-gnu-readelf >/dev/null 2>&1; then
    for bin in "$OUT_DIR/hello_c.elf" "$OUT_DIR/hello_cpp.elf"; do
        mach="$(LC_ALL=C aarch64-linux-gnu-readelf -h "$bin" | awk '/Machine:/ {print $2}')"
        [ "$mach" = "AArch64" ] || { echo "ERROR: $bin is not AArch64 ($mach)" >&2; exit 1; }
        echo "  verified: $(basename "$bin") (Machine: $mach, $(stat -c %s "$bin") bytes)"
    done
fi
echo "[SDK OK] Standalone C and C++ AArch64 examples built successfully (isolated from production OS)."
