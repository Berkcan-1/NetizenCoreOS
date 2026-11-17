# LBA 0'daki MBR bootstrap kodunu (ilk 446 bayt) koruyarak
# Bolum 1 (LBA 2048..65535, 0x83) ve Bolum 2 (LBA 65536..196607, 0xEF ESP) tablosunu kesin hizalar
ensure_hybrid_partition_table() {
    local disk_path="$1"
    local mbr_backup="$BUILD/mbr_code.tmp"
    dd if="$disk_path" of="$mbr_backup" bs=1 count=446 status=none 2>/dev/null || true
    if command -v sfdisk >/dev/null 2>&1; then
        printf "label: dos\nstart=65536, size=65536, type=ef, bootable\n" \
            | sfdisk --no-reread "$disk_path" >/dev/null 2>&1 || true
    fi
    if [ -f "$mbr_backup" ]; then
        dd if="$mbr_backup" of="$disk_path" bs=1 count=446 conv=notrunc status=none 2>/dev/null || true
        rm -f "$mbr_backup"
    fi
}

write_esp_partition() {
    local disk_path="$1"
    local esp_tmp="$BUILD/esp_part.img"
    rm -f "$esp_tmp"
    dd if=/dev/zero of="$esp_tmp" bs=1M count=32 status=none
    mkfs.vfat -F 16 -n "NETIZEN_ESP" "$esp_tmp" >/dev/null 2>&1
    mmd -i "$esp_tmp" ::EFI ::EFI/BOOT || true

    local efi_bin="$BUILD/cargo-uefi-$PROFILE/$UEFI_TARGET/release/nc-uefi.efi"
    if [ -f "$efi_bin" ]; then
        mcopy -i "$esp_tmp" "$efi_bin" ::EFI/BOOT/BOOTX64.EFI
    fi

    # Tam LBA 65536 (32 MiB) ofsetine 32 MiB'lik ESP yaz (LBA 65536..131071)
    dd if="$esp_tmp" of="$disk_path" bs=1M seek=32 conv=notrunc status=none
    rm -f "$esp_tmp"
}

prepare_hybrid_disk() {
    local disk_path="$1"
    if [ ! -f "$disk_path" ]; then
        echo "Hibrit (BIOS + UEFI) 20 GB disk olusturuluyor: $disk_path"
        truncate -s 20G "$disk_path"
        ensure_hybrid_partition_table "$disk_path"
        write_esp_partition "$disk_path"
    fi
}

resolve_usb_args() {
    USB_ARGS=()
    local dev="${USB_DEV:-}"
    if [ -z "$dev" ] && command -v lsblk >/dev/null 2>&1; then
        dev=$(lsblk -dnpo NAME,TRAN,TYPE 2>/dev/null | awk '$2=="usb" && $3=="disk" {print $1; exit}')
    fi
    if [ -z "$dev" ]; then
        echo "USB bellek bulunamadi. Takip acin ya da USB_DEV=/dev/sdX verin." >&2
        return 0
    fi
    [ -b "$dev" ] || fail "$dev blok aygiti degil"
    [ -r "$dev" ] || fail "$dev okunamiyor: kullaniciyi 'disk' grubuna ekleyin veya sudo ile calistirin"
    lsblk -lnpo NAME,MOUNTPOINT "$dev" 2>/dev/null | awk '$2!="" {print $1}' | while read -r part; do
        umount "$part" >/dev/null 2>&1 || udisksctl unmount -b "$part" --no-user-interaction >/dev/null 2>&1 || true
    done
    local mode="snapshot=on"
    if [ "${USB_RW:-0}" = "1" ]; then
        [ -w "$dev" ] || fail "$dev yazilamiyor"
        mode="snapshot=off"
    fi
    USB_ARGS=(-drive "file=$dev,format=raw,if=ide,index=1,media=disk,cache=none,$mode")
    echo "USB: $dev gercek disk olarak IDE kanal 1'e baglaniyor ($mode)"
}
