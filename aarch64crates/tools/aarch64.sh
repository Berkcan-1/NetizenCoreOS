aarch64_rustflags() {
    local flags="-C relocation-model=static -C link-arg=--defsym=KERNEL_BASE=$DEV_KERNEL_BASE"
    case "$ARCH" in
        armv9a) flags="$flags -C target-feature=+v9a" ;;
    esac
    echo "$flags"
}

build_home_app_when_needed() {
    need cargo
    build_home_app
    build_aarch64_apps
}

build_aarch64_app_pkg() {
    local app_dir="$1"
    local bin_name="$2"
    local pkg_out="$3"
    local target="aarch64-unknown-none"
    local log="$BUILD/${bin_name}-build.log"
    mkdir -p "$(dirname "$pkg_out")" "$BUILD"
    if RUSTFLAGS="-C relocation-model=static -C panic=abort" cargo build --release \
        --manifest-path "$app_dir/Cargo.toml" \
        --target "$target" --target-dir "$BUILD/cargo-home-aarch64" > "$log" 2>&1; then
        local elf="$BUILD/cargo-home-aarch64/$target/release/$bin_name"
        python3 "$ROOT/sdk/build-ntzc.py" "$app_dir/manifest.toml" "$elf" "$pkg_out" --arch armv8a --free
    else
        echo "UYARI: $bin_name derlenemedi. Gunluk: $log" >&2
        tail -n 40 "$log" >&2
    fi
}

build_aarch64_apps() {
    build_aarch64_app_pkg \
        "$ROOT/aarch64crates/apps/files/nc-files-v8a" \
        "nc-files-v8a" \
        "$ROOT/packages/mobile/nc.files.ntzc"
    build_aarch64_app_pkg \
        "$ROOT/aarch64crates/apps/settings/nc-settings-v8a" \
        "nc-settings-v8a" \
        "$ROOT/packages/mobile/nc.settings.ntzc"
}

build_home_app() {
    local out="$ROOT/aarch64kernel/embedded/home.elf"
    local target="aarch64-unknown-none"
    mkdir -p "$(dirname "$out")"
    if [ -n "${NC_PREBUILT_HOME:-}" ]; then
        cp "$NC_PREBUILT_HOME" "$out"
        return 0
    fi
    : > "$out"
    local log="$BUILD/home-build.log"
    mkdir -p "$BUILD"
    if RUSTFLAGS="-C relocation-model=static -C panic=abort" cargo build --release \
        --manifest-path "$ROOT/aarch64crates/apps/home/nc-home-r3/Cargo.toml" \
        --target "$target" --target-dir "$BUILD/cargo-home-aarch64" > "$log" 2>&1; then
        cp "$BUILD/cargo-home-aarch64/$target/release/nc-home-r3" "$out"
        echo "NetizenHome ELF: $(file_size "$out") bayt"
    else
        echo "UYARI: NetizenHome derlenemedi; yer tutucu ekran kullanilacak. Gunluk: $log" >&2
        tail -n 40 "$log" >&2
    fi
}

build_aarch64_kernel() {
    local target="aarch64-unknown-none"
    OUT="$BUILD/$ARCH-$DEVICE"
    mkdir -p "$OUT"
    if [ -n "${NC_PREBUILT_IMAGE:-}" ]; then
        [ -f "$NC_PREBUILT_IMAGE" ] || fail "NC_PREBUILT_IMAGE bulunamadi: $NC_PREBUILT_IMAGE"
        cp "$NC_PREBUILT_IMAGE" "$OUT/Image"
        echo "hazir (onceden derlenmis): $OUT/Image"
        return 0
    fi
    need cargo
    select_device_crate "$DEVICE"
    if command -v rustup >/dev/null 2>&1; then
        rustup target add "$target"
    fi
    build_home_app_when_needed
    RUSTFLAGS="$(aarch64_rustflags)" \
        cargo build --release \
        --manifest-path "$ROOT/aarch64kernel/Cargo.toml" \
        --target "$target" \
        --target-dir "$BUILD/cargo-$ARCH-$DEVICE" \
        --features "ui-$DEV_UI"
    python3 "$ROOT/aarch64crates/tools/elf2bin.py" "$BUILD/cargo-$ARCH-$DEVICE/$target/release/kernel-aarch64" "$OUT/Image"
    local size
    size="$(file_size "$OUT/Image")"
    [ "$size" -gt 64 ] || fail "Image bos veya cok kucuk"
    [ "$size" -le 33554432 ] || fail "Image 32 MiB'tan buyuk olamaz (simdiki: $size)"
    local magic
    magic="$(dd if="$OUT/Image" bs=1 skip=56 count=4 2>/dev/null | od -An -tx1 | tr -d ' \n')"
    [ "$magic" = "41524d64" ] || fail "Image basligi gecersiz (offset 56'da ARM\\x64 imzasi yok): $magic"
    echo "hazir: $OUT/Image ($size bayt, taban $DEV_KERNEL_BASE, arayuz $DEV_UI)"
}

build_aarch64_profile() {
    case "$TAG" in
        qemu)
            build_aarch64_kernel
            ;;
        android)
            [ "$ARCH" != "arm64" ] || fail "android etiketi armv8a veya armv9a ile kullanilir"
            build_aarch64_kernel
            build_android_images
            ;;
        pc)
            build_aarch64_kernel
            echo "NOT: arm64 PC icin UEFI (BOOTAA64.EFI) onyukleyicisi henuz yok; simdilik cikti ham Image'dir (docs/architecture/MULTIARCH.md)." >&2
            ;;
        *)
            fail "aarch64 icin etiket qemu, android veya pc olmali"
            ;;
    esac
}
