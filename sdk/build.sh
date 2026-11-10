#!/usr/bin/env bash
# Ring 3 uygulamasını derler ve imzalı NTZ2 paketi üretir.
#   sdk/build.sh <uygulama-dizini> [--seed-file YOL]
# Dizin: Cargo.toml (nc-app-sdk kullanan, no_std/no_main ikili), build.rs (app.ld), manifest.toml.
# Şablon: sdk/ring3-sample. Çıktı: build/apps/<kimlik>.ntzc
set -euo pipefail

SDK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SDK_ROOT")"
BUILD_DIR="$PROJECT_ROOT/build"
TARGET="x86_64-unknown-none"

fail() { echo "hata: $*" >&2; exit 1; }

[ $# -ge 1 ] || fail "kullanim: sdk/build.sh <uygulama-dizini> [--seed-file YOL]"
APP_DIR="$(cd "$1" && pwd)"; shift
[ -f "$APP_DIR/Cargo.toml" ] || fail "$APP_DIR/Cargo.toml yok"
[ -f "$APP_DIR/manifest.toml" ] || fail "$APP_DIR/manifest.toml yok"

PKG_NAME="$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$APP_DIR/Cargo.toml" | head -n1)"
APP_ID="$(sed -n 's/^id *= *"\(.*\)"/\1/p' "$APP_DIR/manifest.toml" | head -n1)"
[ -n "$PKG_NAME" ] && [ -n "$APP_ID" ] || fail "Cargo.toml 'name' ve manifest.toml 'id' gerekli"

bash "$PROJECT_ROOT/tools/ring3-cargo.sh" "$APP_DIR/Cargo.toml" "$BUILD_DIR/cargo-sdk" "$BUILD_DIR/sdk-build.log" \
    || { tail -n 60 "$BUILD_DIR/sdk-build.log" >&2; fail "derleme basarisiz (tam gunluk: $BUILD_DIR/sdk-build.log)"; }

ELF="$BUILD_DIR/cargo-sdk/$TARGET/release/$PKG_NAME"
[ -f "$ELF" ] || fail "ELF bulunamadi: $ELF"
mkdir -p "$BUILD_DIR/apps"
python3 "$SDK_ROOT/build-ntzc.py" "$APP_DIR/manifest.toml" "$ELF" "$BUILD_DIR/apps/$APP_ID.ntzc" "$@"
