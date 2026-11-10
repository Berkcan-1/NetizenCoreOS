#!/usr/bin/env bash
# Ring 3 uygulamasini x86_64-unknown-none icin derler:  tools/ring3-cargo.sh <Cargo.toml> <hedef-dizin> <gunluk>
#
# Neden -Zbuild-std: rustup'in onceden derlenmis `core`'u "kernel" kod modeliyle gelir (32-bit isaret-genisletmeli
# mutlak adresler); Ring 3 imaji 0x80_0100_0000'a (4 GiB ustu) baglandigi icin ayni kod modeliyle (large) yeniden
# derlenmelidir. nightly gerekmez: RUSTC_BOOTSTRAP=1 + rust-src bileseni yeterlidir.
set -uo pipefail

MANIFEST="$1"; TARGET_DIR="$2"; LOG="$3"
TARGET="x86_64-unknown-none"
# Kriptografi bagimliliklari (nc-pkg) cekirdek derlemesiyle AYNI yazilim arka planini kullanir.
FLAGS='-C relocation-model=static -C code-model=large -C panic=abort --cfg chacha20_force_soft --cfg poly1305_force_soft --cfg curve25519_dalek_backend="serial"'

if command -v rustup >/dev/null 2>&1; then
    rustup component add rust-src >/dev/null 2>&1 || true
    rustup target add "$TARGET" >/dev/null 2>&1 || true
fi

: > "$LOG"
echo "=== deneme 1: -Zbuild-std (core/alloc ayni kod modeliyle) ===" >> "$LOG"
if RUSTC_BOOTSTRAP=1 RUSTFLAGS="$FLAGS" cargo build --release \
    -Zbuild-std=core,alloc,compiler_builtins -Zbuild-std-features=compiler-builtins-mem \
    --manifest-path "$MANIFEST" --target "$TARGET" --target-dir "$TARGET_DIR" >> "$LOG" 2>&1; then
    exit 0
fi

echo "=== deneme 2: onceden derlenmis core (yalnizca cok kucuk uygulamalar baglanabilir) ===" >> "$LOG"
RUSTFLAGS="$FLAGS" cargo build --release \
    --manifest-path "$MANIFEST" --target "$TARGET" --target-dir "$TARGET_DIR" >> "$LOG" 2>&1
