build_kernel() {
    local features="$PROFILE${NC_KERNEL_PAGING:+,kernel-paging}${NC_KERNEL_EXTRA:+,$NC_KERNEL_EXTRA}"
    if ring3_enabled; then
        build_ring3_apps
        features="$features,ring3-apps"
    else
        echo "UYARI: NC_RING3_APPS=0: yerlesik uygulamalar Ring 0'da calisacak (onerilmez)." >&2
        NC_RING0_FALLBACK=1
    fi
    if [ "${NC_RING0_FALLBACK:-0}" = "1" ]; then
        echo "UYARI: Ring 0 yedek uygulamalari imaja ekleniyor (imaj buyur; 7 MiB siniri asilabilir)." >&2
        features="$features,ring0-fallback"
    fi
    if command -v rustup >/dev/null 2>&1; then
        rustup target add "$TARGET"
    fi
    local limit_mib="${NC_IMAGE_LIMIT_MIB:-7}"
    if [ "$limit_mib" -gt 7 ]; then
        echo "UYARI: imaj siniri $limit_mib MiB: yalnizca QEMU/BIOS icin; UEFI (OVMF) 8 MiB'tan sonrasini ACPI icin ayirir, UEFI'de acilmaz." >&2
    fi
    RUSTFLAGS="-C relocation-model=static --cfg chacha20_force_soft --cfg poly1305_force_soft --cfg curve25519_dalek_backend=\"serial\" -C link-arg=--defsym=__nc_image_limit=$((limit_mib * 1048576))" \
        cargo build --release \
        --manifest-path "$ROOT/kernel/Cargo.toml" \
        --target "$TARGET" \
        --target-dir "$BUILD/cargo-$PROFILE" \
        --no-default-features \
        --features "$features"
    "$(pick_objcopy)" -O binary "$BUILD/cargo-$PROFILE/$TARGET/release/kernel" "$OUT/kernel.bin"
    local size
    size="$(file_size "$OUT/kernel.bin")"
    [ "$size" -gt 0 ] || fail "kernel.bin bos"
    [ "$size" -le "$KERNEL_LIMIT" ] || fail "kernel $KERNEL_LIMIT bayttan buyuk olamaz (simdiki: $size)"
    local padded=$(( (size + SECTOR - 1) / SECTOR * SECTOR ))
    truncate -s "$padded" "$OUT/kernel.bin"
    KERNEL_SECTORS_512=$(( padded / 512 ))
    echo "kernel boyutu: $size bayt ($KERNEL_SECTORS_512 adet 512B sektor)"
}
