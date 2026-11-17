build_bootloader() {
    nasm -f bin "$ROOT/bootloader/bios/mbr.asm" -o "$ROOT/crates/hardware/nc-storage/mbr.bin"
    [ "$(file_size "$ROOT/crates/hardware/nc-storage/mbr.bin")" -eq 446 ] || fail "mbr boyutu 446 olmali"

    nasm -f bin "$ROOT/bootloader/bios/stage1.asm" -o "$OUT/stage1.bin"
    [ "$(file_size "$OUT/stage1.bin")" -eq 2048 ] || fail "stage1 boyutu 2048 olmali"

    nasm -f bin \
        -DPREFERRED_WIDTH="$PREFERRED_WIDTH" \
        -DPREFERRED_HEIGHT="$PREFERRED_HEIGHT" \
        -DFALLBACK_WIDTH="$FALLBACK_WIDTH" \
        -DFALLBACK_HEIGHT="$FALLBACK_HEIGHT" \
        -DKERNEL_SECTORS_512="${KERNEL_SECTORS_512:-4096}" \
        "$ROOT/bootloader/bios/stage2.asm" -o "$OUT/stage2.bin"
    [ "$(file_size "$OUT/stage2.bin")" -eq 4096 ] || fail "stage2 boyutu 4096 olmali"
}

build_uefi_optional() {
    if [ -f "$ROOT/bootloader/uefi/Cargo.toml" ]; then
        echo "UEFI ikilisi derleniyor..."
        if command -v rustup >/dev/null 2>&1; then
            rustup target add "$UEFI_TARGET" >/dev/null 2>&1 || true
        fi
        NC_KERNEL_BIN="$OUT/kernel.bin" \
        NC_STAGE2_BIN="$OUT/stage2.bin" \
            cargo build --release \
            --manifest-path "$ROOT/bootloader/uefi/Cargo.toml" \
            --target "$UEFI_TARGET" \
            --target-dir "$BUILD/cargo-uefi-$PROFILE"

        local efi_bin="$BUILD/cargo-uefi-$PROFILE/$UEFI_TARGET/release/nc-uefi.efi"
        [ -f "$efi_bin" ] || fail "nc-uefi.efi derlenemedi"

        mkdir -p "$OUT/iso/EFI/BOOT"
        cp "$efi_bin" "$OUT/iso/EFI/BOOT/BOOTX64.EFI"

        mkdir -p "$OUT/iso/boot"
        local img="$OUT/iso/boot/efiboot.img"
        rm -f "$img"
        dd if=/dev/zero of="$img" bs=1M count=16 status=none
        mkfs.vfat -F 16 -n "NETIZEN_EFI" "$img" >/dev/null 2>&1
        mmd -i "$img" ::EFI || true
        mmd -i "$img" ::EFI/BOOT || true
        mcopy -i "$img" "$efi_bin" ::EFI/BOOT/BOOTX64.EFI
        echo "UEFI FAT boot imaji olusturuldu: $img"
    fi
}

build_iso() {
    select_iso_tool
    mkdir -p "$OUT/iso/boot"
    cat "$OUT/stage1.bin" "$OUT/stage2.bin" "$OUT/kernel.bin" > "$OUT/iso/boot/ncboot.bin"
    build_uefi_optional

    local uefi_args=()
    if [ -f "$OUT/iso/boot/efiboot.img" ]; then
        uefi_args=(
            -eltorito-alt-boot
            -e boot/efiboot.img
            -no-emul-boot
            -isohybrid-gpt-basdat
        )
    fi

    "${ISO_CMD[@]}" \
        -V NETIZENCORE \
        -b boot/ncboot.bin \
        -c boot/boot.cat \
        -no-emul-boot \
        -boot-load-size 4 \
        -boot-info-table \
        "${uefi_args[@]}" \
        -o "$ISO" \
        "$OUT/iso"
}

build_profile() {
    configure_profile "$1"
    rm -rf "$OUT"
    mkdir -p "$OUT"
    build_kernel
    build_bootloader
    build_iso
    echo "hazir: $ISO"
}
