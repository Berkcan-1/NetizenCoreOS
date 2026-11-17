empty_ramdisk() {
    python3 - "$1" <<'PY'
import gzip, sys
name = b"TRAILER!!!\0"
fields = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, len(name), 0]
record = b"070701" + b"".join(b"%08X" % value for value in fields) + name
record += b"\0" * ((4 - len(record) % 4) % 4)
record += b"\0" * ((512 - len(record) % 512) % 512)
with open(sys.argv[1], "wb") as handle:
    handle.write(gzip.compress(record, 9, mtime=0))
PY
}

dev_file() {
    local rel="$1"
    case "$rel" in
        /*) echo "$rel" ;;
        *) echo "$ROOT/aarch64crates/drivers/$DEVICE/$rel" ;;
    esac
}

stage_system_root() {
    local stage="$OUT/system-root"
    rm -rf "$stage"
    mkdir -p "$stage/etc" "$stage/packages" "$stage/assets"
    {
        echo "name=NetizenCore"
        echo "arch=$ARCH"
        echo "device=$DEVICE"
        echo "soc=$DEV_SOC"
        echo "ui=$DEV_UI"
    } > "$stage/etc/netizencore-release"
    cp "$ROOT/aarch64crates/drivers/$DEVICE/device.conf" "$stage/etc/device.conf"
    if [ -d "$ROOT/assets" ]; then
        cp -r "$ROOT/assets/." "$stage/assets/"
    fi
    local package_dir="$ROOT/packages/$DEV_UI"
    if [ -d "$package_dir" ]; then
        find "$package_dir" -maxdepth 1 -name '*.ntzc' -exec cp {} "$stage/packages/" \;
    fi
    if [ -d "$BUILD/apps" ]; then
        find "$BUILD/apps" -maxdepth 1 -name '*.ntzc' -exec cp {} "$stage/packages/" \;
    fi
    echo "$stage"
}

build_system_image() {
    local stage="$1"
    local image="$OUT/system.img"
    rm -f "$image"
    case "${DEV_SYSTEM_FS:-erofs}" in
        erofs)
            if command -v mkfs.erofs >/dev/null 2>&1; then
                mkfs.erofs -zlz4hc "$image" "$stage" >/dev/null
                echo "system.img: EROFS ($(file_size "$image") bayt)"
                return 0
            fi
            echo "UYARI: mkfs.erofs yok (erofs-utils); FAT32 system.img uretiliyor." >&2
            ;;
    esac
    need mkfs.vfat
    need mcopy
    local used_kb
    used_kb="$(du -sk "$stage" | awk '{print $1}')"
    local size_mb=$(( used_kb / 1024 * 3 / 2 + 40 ))
    truncate -s "${size_mb}M" "$image"
    mkfs.vfat -F 32 -n NCSYSTEM "$image" >/dev/null
    mcopy -s -i "$image" "$stage"/* ::
    echo "system.img: FAT32 ${size_mb} MiB"
}

build_super_image() {
    local image="$OUT/system.img"
    local super_size="${DEV_SUPER_SIZE:-0}"
    if [ "$super_size" = "0" ]; then
        echo "UYARI: aarch64crates/drivers/$DEVICE/device.conf icinde super_size=0; super.img uretilmedi." >&2
        echo "       Cihazdaki boyut: adb shell cat /proc/partitions (super satiri, blok x 1024)." >&2
        return 0
    fi
    if ! command -v lpmake >/dev/null 2>&1; then
        echo "UYARI: lpmake bulunamadi (AOSP otools); super.img uretilmedi, system.img hazir." >&2
        return 0
    fi
    local group_size="${DEV_SUPER_GROUP_SIZE:-0}"
    if [ "$group_size" = "0" ]; then
        group_size=$(( super_size - 8 * 1048576 ))
    fi
    local slots="${DEV_SUPER_SLOTS:-2}"
    local image_size
    image_size="$(file_size "$image")"
    local args=(
        --metadata-size 65536
        --super-name super
        --metadata-slots "$slots"
        --device "super:$super_size"
        --group "nc_dynamic:$group_size"
        --output "$OUT/super.img"
    )
    if [ "$slots" = "3" ]; then
        args+=(--partition "system_a:readonly:$image_size:nc_dynamic" --image "system_a=$image")
        args+=(--partition "system_b:readonly:0:nc_dynamic")
    else
        args+=(--partition "system:readonly:$image_size:nc_dynamic" --image "system=$image")
    fi
    if [ "${DEV_SUPER_SPARSE:-no}" = "yes" ]; then
        args+=(--sparse)
    fi
    lpmake "${args[@]}"
    echo "super.img: $(file_size "$OUT/super.img") bayt"
}

build_android_images() {
    need python3
    local kernel="$OUT/Image"
    local payload="$OUT/kernel.payload"
    case "${DEV_ANDROID_KERNEL_COMPRESS:-none}" in
        gzip) gzip -9 -n -c "$kernel" > "$payload" ;;
        *) cp "$kernel" "$payload" ;;
    esac

    local ramdisk="$OUT/ramdisk.cpio.gz"
    case "${DEV_ANDROID_RAMDISK:-empty}" in
        empty|"") empty_ramdisk "$ramdisk" ;;
        *) cp "$(dev_file "$DEV_ANDROID_RAMDISK")" "$ramdisk" ;;
    esac

    local header="${DEV_ANDROID_HEADER_VERSION:-2}"
    local args=(
        --kernel "$payload"
        --ramdisk "$ramdisk"
        --header-version "$header"
        --pagesize "${DEV_ANDROID_PAGESIZE:-2048}"
        --kernel-addr "${DEV_ANDROID_KERNEL_ADDR:-$DEV_KERNEL_BASE}"
        --ramdisk-addr "${DEV_ANDROID_RAMDISK_ADDR:-0x11b00000}"
        --second-addr "${DEV_ANDROID_SECOND_ADDR:-0}"
        --tags-addr "${DEV_ANDROID_TAGS_ADDR:-0x07c08000}"
        --dtb-addr "${DEV_ANDROID_DTB_ADDR:-0x07c08000}"
        --os-version "${DEV_ANDROID_OS_VERSION:-}"
        --os-patch-level "${DEV_ANDROID_OS_PATCH:-}"
        --cmdline "${DEV_ANDROID_CMDLINE:-}"
        -o "$OUT/boot.img"
    )
    if [ "$header" -eq 2 ]; then
        local dtb=""
        if [ -n "${DEV_ANDROID_DTB:-}" ] && [ -f "$(dev_file "$DEV_ANDROID_DTB")" ]; then
            dtb="$(dev_file "$DEV_ANDROID_DTB")"
        elif [ -n "${DEV_ANDROID_DTS:-}" ] && [ -f "$(dev_file "$DEV_ANDROID_DTS")" ] && command -v dtc >/dev/null 2>&1; then
            dtb="$OUT/device.dtb"
            dtc -I dts -O dtb -o "$dtb" "$(dev_file "$DEV_ANDROID_DTS")" 2>/dev/null
        fi
        if [ -n "$dtb" ]; then
            args+=(--dtb "$dtb")
        else
            echo "UYARI: boot.img v2 icin dtb bulunamadi (aarch64crates/drivers/$DEVICE/${DEV_ANDROID_DTB:-stock/dtb}); onyukleyici acmayabilir." >&2
            echo "       Stok boot.img'den cikar: bash build.sh unpack-boot <stok boot.img>" >&2
        fi
    fi
    python3 "$ROOT/aarch64crates/tools/android/mkbootimg.py" "${args[@]}"

    local stage
    stage="$(stage_system_root)"
    build_system_image "$stage"
    build_super_image
    local userdata_mb="${NC_USERDATA_MB:-${DEV_USERDATA_MB:-0}}"
    if [ "$userdata_mb" = "0" ]; then
        echo "UYARI: aarch64crates/drivers/$DEVICE/device.conf icinde userdata_mb=0; userdata.img uretilmedi (paket kurma icin /data gerekir)." >&2
        echo "       Cihazdaki userdata bolumunun MiB boyutunu yazin (adb shell cat /proc/partitions); en cok 4000." >&2
    else
        build_userdata_image "$OUT/userdata.img" "$userdata_mb"
    fi

    echo
    echo "ciktilar: $OUT"
    echo "once kalici yazmadan dene:   fastboot boot $OUT/boot.img"
    echo "kalici yazmak icin:          fastboot flash boot $OUT/boot.img"
    if [ -f "$OUT/super.img" ]; then
        echo "                             fastboot flash super $OUT/super.img"
    fi
    if [ -f "$OUT/userdata.img" ]; then
        echo "                             fastboot flash userdata $OUT/userdata.img   (cihazdaki tum kullanici verisini siler)"
    fi
    echo "onyukleyici kilidi acik degilse imzasiz boot.img acilmaz; super/boot yazmadan once stok yedegi alin."
}
