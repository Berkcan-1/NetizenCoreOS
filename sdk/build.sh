#!/usr/bin/env bash
set -euo pipefail

SDK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SDK_ROOT")"
BUILD_DIR="$PROJECT_ROOT/build"
APPS_OUT="$BUILD_DIR/apps"
HOST_USB="$PROJECT_ROOT/host_usb"
TARGET="x86_64-unknown-none"

fail() {
    echo "hata: $*" >&2
    exit 1
}

pick_objcopy() {
    for tool in llvm-objcopy rust-objcopy objcopy; do
        if command -v "$tool" >/dev/null 2>&1; then
            echo "$tool"
            return
        fi
    done
    fail "objcopy bulunamadi"
}

compile_app() {
    local app_name="${1%/}"
    local app_dir="$SDK_ROOT/$app_name"
    [ -d "$app_dir" ] || app_dir="$1"
    [ -f "$app_dir/Cargo.toml" ] || fail "'$app_name' icinde Cargo.toml bulunamadi"
    [ -f "$app_dir/manifest.toml" ] || fail "'$app_name' icinde manifest.toml bulunamadi"

    if command -v rustup >/dev/null 2>&1; then
        rustup target add "$TARGET" >/dev/null 2>&1 || true
    fi

    mkdir -p "$APPS_OUT" "$HOST_USB"

    echo "[SDK] '$app_name' x86_64 PIC modunda derleniyor..."
    RUSTFLAGS="-C relocation-model=pic -C link-arg=-T$SDK_ROOT/app.ld" \
        cargo build --release \
        --manifest-path "$app_dir/Cargo.toml" \
        --target "$TARGET" \
        --target-dir "$BUILD_DIR/cargo-sdk"

    local bin_name
    bin_name=$(grep -E '^name\s*=' "$app_dir/Cargo.toml" | head -n 1 | cut -d= -f2 | tr -d ' "'"'"'')
    local elf_file="$BUILD_DIR/cargo-sdk/$TARGET/release/$bin_name"
    local raw_file="$APPS_OUT/$bin_name.raw"

    "$(pick_objcopy)" -O binary "$elf_file" "$raw_file"

    # Python ile NTZC basligi + manifest + makine kodunu paketle
    python3 - "$app_dir/manifest.toml" "$raw_file" "$APPS_OUT" "$HOST_USB" << 'EOF'
import sys, os, struct, shutil

manifest_path, raw_path, out_dir, usb_dir = sys.argv[1:5]
meta = {
    "id": "nc.user.app",
    "name": "Harici Uygulama",
    "version": "1.0.0",
    "icon": "generic",
    "width_units": "34",
    "height_units": "22",
    "uid": "1001",
    "perms": "1",
}
with open(manifest_path, "r", encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, v = line.split("=", 1)
        meta[k.strip()] = v.strip().strip('"').strip("'")

payload = open(raw_path, "rb").read()
os.remove(raw_path)

manifest_str = (
    f"id={meta['id']}\n"
    f"name={meta['name']}\n"
    f"version={meta['version']}\n"
    f"icon={meta['icon']}\n"
    f"width_units={meta['width_units']}\n"
    f"height_units={meta['height_units']}\n"
)
m_bytes = manifest_str.encode("utf-8")
header = struct.pack(
    "<4sIIIIII",
    b"NTZC",
    1,
    int(meta["uid"]),
    int(meta["perms"]),
    len(m_bytes),
    28 + len(m_bytes),
    len(payload),
)

short_name = meta["id"].split(".")[-1]
ntzc_path = os.path.join(out_dir, f"{short_name}.ntzc")
with open(ntzc_path, "wb") as f:
    f.write(header)
    f.write(m_bytes)
    f.write(payload)

shutil.copy2(ntzc_path, os.path.join(usb_dir, f"{short_name}.ntzc"))
print(f"[SDK] Paket hazir: {ntzc_path} ({len(header) + len(m_bytes) + len(payload)} bayt)")
print(f"[SDK] USB paylasim klasorune kopyalandi: {os.path.join(usb_dir, short_name + '.ntzc')}")
EOF
}

create_template() {
    local name="$1"
    local dir="$SDK_ROOT/$name"
    [ ! -e "$dir" ] || fail "$dir zaten mevcut"
    mkdir -p "$dir/src"
    cat > "$dir/manifest.toml" << EOF
id = "nc.user.$name"
name = "${name^}"
version = "1.0.0"
icon = "generic"
width_units = 34
height_units = 22
uid = 1001
perms = 1
EOF
    cat > "$dir/Cargo.toml" << EOF
[package]
name = "$name"
version = "1.0.0"
edition = "2021"

[dependencies]
nc-sdk = { path = "../nc-sdk" }

[profile.release]
panic = "abort"
opt-level = "z"
lto = true
codegen-units = 1
EOF
    cp "$SDK_ROOT/sample-app/src/main.rs" "$dir/src/main.rs"
    echo "[SDK] Yeni uygulama sablonu olusturuldu: $dir"
    echo "[SDK] Derlemek icin: bash sdk/build.sh $name"
}

cmd="${1:-sample-app}"
case "$cmd" in
    all)
        for d in "$SDK_ROOT"/*/; do
            [ -f "${d}manifest.toml" ] && compile_app "$(basename "$d")"
        done
        ;;
    new)
        [ -n "${2:-}" ] || fail "kullanim: bash sdk/build.sh new <uygulama-adi>"
        create_template "$2"
        ;;
    *)
        compile_app "$cmd"
        ;;
esac
