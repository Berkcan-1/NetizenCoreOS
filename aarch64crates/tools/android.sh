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
    local stage_base="${WORK:-$OUT}"
    local stage="$stage_base/system-root"
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
    if [ "$DEV_UI" = "mobile" ]; then
        # ARMv8-A mobil profil: yalnizca aarch64crates/assets ve packages/mobile/*.ntzc paketlenir.
        # x86_64/masaustu varliklari (assets/) veya x86_64 uygulamalari (build/apps/) mobil imaja alinmaz.
        if [ -d "$ROOT/aarch64crates/assets" ]; then
            cp -r "$ROOT/aarch64crates/assets/." "$stage/assets/"
            # Cok yuksek cozunurluklu (orn. 4000x6000) PNG duvar kagitlarini en-boy oranini koruyarak
            # mobil ekran olcegine (en uzun kenar <= 1440 px) indirge; cekirdek bellek tasmasi onlenir.
            if [ -d "$stage/assets/wallpapers" ]; then
                python3 - "$stage/assets/wallpapers" <<'PY'
import sys
from pathlib import Path
try:
    from PIL import Image
except ImportError:
    sys.exit(0)
wp_dir = Path(sys.argv[1])
MAX_SIDE = 1440
for p in sorted(wp_dir.glob("*.png")):
    try:
        with Image.open(p) as img:
            w, h = img.size
            longest = max(w, h)
            if longest > MAX_SIDE:
                scale = MAX_SIDE / float(longest)
                nw, nh = max(1, round(w * scale)), max(1, round(h * scale))
                out = img.convert("RGB").resize((nw, nh), Image.Resampling.LANCZOS)
                out.save(p, "PNG", optimize=True)
            elif img.mode not in ("RGB", "RGBA"):
                img.convert("RGB").save(p, "PNG", optimize=True)
    except Exception as e:
        print(f"UYARI: duvar kagidi hazirlanamadi ({p.name}): {e}", file=sys.stderr)
PY
            fi
        fi
    else
        if [ -d "$ROOT/assets" ]; then
            cp -r "$ROOT/assets/." "$stage/assets/"
        fi
    fi
    local package_dir="$ROOT/packages/$DEV_UI"
    if [ -d "$package_dir" ]; then
        find "$package_dir" -maxdepth 1 -name '*.ntzc' -exec cp {} "$stage/packages/" \;
    fi
    echo "$stage"
}

build_system_image() {
    local stage="$1"
    local image="${2:-$WORK/system.img}"
    local req_fs="${DEV_SYSTEM_FS:-fat32}"
    rm -f "$image"
    case "$req_fs" in
        fat32)
            need mkfs.vfat
            need mcopy
            local used_kb
            used_kb="$(du -sk "$stage" | awk '{print $1}')"
            local size_mb=$(( used_kb / 1024 * 3 / 2 + 40 ))
            truncate -s "${size_mb}M" "$image"
            mkfs.vfat -F 32 -n NCSYSTEM "$image" >/dev/null
            mcopy -s -i "$image" "$stage"/* ::
            echo "ara system.img (staged): FAT32 ${size_mb} MiB ($image)"
            ;;
        *)
            fail "desteklenmeyen system_fs='$req_fs' (aarch64kernel/src/storage.rs yalnizca FAT32 sistem bolumunu baglar; device.conf icinde system_fs=fat32 kullanin)"
            ;;
    esac
}

mobile_system_partition_name() {
    if [ "${DEV_SUPER_SLOTS:-2}" = "3" ]; then
        echo "system_a"
    else
        echo "system"
    fi
}

build_mobile_boot_image() {
    need python3
    local kernel="$WORK/Image"
    [ -f "$kernel" ] || fail "cekirdek imaji bulunamadi: $kernel"

    local payload="$WORK/kernel.payload"
    case "${DEV_ANDROID_KERNEL_COMPRESS:-none}" in
        gzip) gzip -9 -n -c "$kernel" > "$payload" ;;
        none|"") cp "$kernel" "$payload" ;;
        *) fail "desteklenmeyen android_kernel_compress=${DEV_ANDROID_KERNEL_COMPRESS}" ;;
    esac

    local ramdisk="$WORK/ramdisk.cpio.gz"
    case "${DEV_ANDROID_RAMDISK:-empty}" in
        empty|"") empty_ramdisk "$ramdisk" ;;
        *)
            local rsrc
            rsrc="$(dev_file "$DEV_ANDROID_RAMDISK")"
            [ -f "$rsrc" ] || fail "ramdisk bulunamadi: $rsrc"
            cp "$rsrc" "$ramdisk"
            ;;
    esac

    local header="${DEV_ANDROID_HEADER_VERSION:-2}"
    local tmp_boot="$WORK/boot.img.tmp"
    rm -f "$tmp_boot"

    local args=(
        --kernel "$payload"
        --ramdisk "$ramdisk"
        --header-version "$header"
        --pagesize "${DEV_ANDROID_PAGESIZE:-4096}"
        --kernel-addr "${DEV_ANDROID_KERNEL_ADDR:-$DEV_KERNEL_BASE}"
        --ramdisk-addr "${DEV_ANDROID_RAMDISK_ADDR:-0x44000000}"
        --second-addr "${DEV_ANDROID_SECOND_ADDR:-0}"
        --tags-addr "${DEV_ANDROID_TAGS_ADDR:-0x40000000}"
        --dtb-addr "${DEV_ANDROID_DTB_ADDR:-0x40000000}"
        --os-version "${DEV_ANDROID_OS_VERSION:-}"
        --os-patch-level "${DEV_ANDROID_OS_PATCH:-}"
        --cmdline "${DEV_ANDROID_CMDLINE:-}"
        -o "$tmp_boot"
    )
    if [ "$header" -eq 2 ]; then
        local dtb=""
        if [ -n "${DEV_ANDROID_DTB:-}" ] && [ -f "$(dev_file "$DEV_ANDROID_DTB")" ]; then
            dtb="$(dev_file "$DEV_ANDROID_DTB")"
        elif [ -n "${DEV_ANDROID_DTS:-}" ] && [ -f "$(dev_file "$DEV_ANDROID_DTS")" ] && command -v dtc >/dev/null 2>&1; then
            dtb="$WORK/device.dtb"
            dtc -I dts -O dtb -o "$dtb" "$(dev_file "$DEV_ANDROID_DTS")" 2>/dev/null
        fi
        if [ -n "$dtb" ]; then
            args+=(--dtb "$dtb")
        elif [ "$TAG" = "android" ]; then
            echo "UYARI: boot.img v2 icin dtb bulunamadi (aarch64crates/drivers/$DEVICE/${DEV_ANDROID_DTB:-stock/dtb}); onyukleyici acmayabilir." >&2
            echo "       Stok boot.img'den cikar: bash build.sh unpack-boot <stok boot.img>" >&2
        fi
    fi

    python3 "$ROOT/aarch64crates/tools/android/mkbootimg.py" "${args[@]}"
    [ -s "$tmp_boot" ] || fail "boot.img uretilemedi"
    mv -f "$tmp_boot" "$OUT/boot.img"
}

build_mobile_super_image() {
    local image="$1"
    [ -s "$image" ] || fail "ara system.img bulunamadi veya bos: $image"
    local part_name
    part_name="$(mobile_system_partition_name)"
    local tmp_super="$WORK/super.img.tmp"
    rm -f "$tmp_super"

    if [ "$TAG" = "android" ]; then
        local super_size="${DEV_SUPER_SIZE:-0}"
        [ "$super_size" != "0" ] || fail "aarch64crates/drivers/$DEVICE/device.conf icinde super_size tanimli olmali (0 olamaz)"
        if command -v lpmake >/dev/null 2>&1; then
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
                --output "$tmp_super"
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
        else
            echo "NOT: lpmake bulunamadi; aarch64crates/tools/mk-super.py ile ham LP super.img uretiliyor (bolum='$part_name')." >&2
            python3 "$ROOT/aarch64crates/tools/mk-super.py" "$tmp_super" "$part_name" "$image"
        fi
    else
        # QEMU mobil profili
        if [ "${NC_USE_LPMAKE:-0}" = "1" ]; then
            need lpmake
            local bytes
            bytes="$(file_size "$image")"
            lpmake --metadata-size 65536 --super-name super --metadata-slots 2 \
                --device "super:$((bytes + 16 * 1024 * 1024))" --group "main:$bytes" \
                --partition "$part_name:readonly:$bytes:main" --image "$part_name=$image" --output "$tmp_super"
            echo "super.img: lpmake ile uretildi"
        else
            python3 "$ROOT/aarch64crates/tools/mk-super.py" "$tmp_super" "$part_name" "$image"
        fi
    fi

    [ -s "$tmp_super" ] || fail "super.img uretilemedi"
    mv -f "$tmp_super" "$OUT/super.img"
}

resolve_mobile_userdata_mb() {
    if [ "$TAG" = "android" ]; then
        local val="${NC_USERDATA_MB:-${DEV_USERDATA_MB:-0}}"
        if ! [[ "$val" =~ ^[0-9]+$ ]] || [ "$val" -eq 0 ]; then
            fail "HATA: '$DEVICE' (android) profili icin dogrulanmis userdata_mb tanimli degil!
       Gercek Android cihazlar icin userdata boyutu tahmin edilmez ve QEMU'nun 32 GiB degeri kullanilmaz.
       Cihazdaki gercek userdata bolum boyutunu MiB olarak olcun (adb shell cat /proc/partitions) ve:
         1) aarch64crates/drivers/$DEVICE/device.conf dosyasina 'userdata_mb=<MiB>' ekleyin, veya
         2) derleme sirasinda acikca verin: NC_USERDATA_MB=<MiB> bash build.sh $ARCH android $DEVICE"
        fi
        echo "$val"
    else
        local val="${NC_USERDATA_MB:-${DEV_USERDATA_MB:-32768}}"
        if ! [[ "$val" =~ ^[0-9]+$ ]] || [ "$val" -eq 0 ]; then
            fail "gecersiz QEMU userdata_mb degeri: '$val'"
        fi
        echo "$val"
    fi
}

cleanup_mobile_out_dir() {
    # Final cikti dizininde yalnizca boot.img, super.img ve userdata.img kalmalidir.
    rm -rf \
        "$OUT/Image" \
        "$OUT/kernel.payload" \
        "$OUT/ramdisk.cpio.gz" \
        "$OUT/device.dtb" \
        "$OUT/system.img" \
        "$OUT/system-root" \
        "$OUT/boot.img.tmp" \
        "$OUT/super.img.tmp" \
        "$OUT/userdata.img.tmp"
}

build_mobile_images() {
    need python3
    mkdir -p "$OUT" "$WORK"

    # Once userdata kapasitesini dogrula (eksik fiziksel cihaz kapasitesi erken ve net hata versin)
    local userdata_mb
    userdata_mb="$(resolve_mobile_userdata_mb)"

    # 1. boot.img
    build_mobile_boot_image

    # 2. Ara system.img (WORK altinda) -> final super.img (OUT altinda)
    local stage
    stage="$(stage_system_root)"
    local staged_system_img="$WORK/system.img"
    build_system_image "$stage" "$staged_system_img"
    build_mobile_super_image "$staged_system_img"

    # 3. userdata.img (mevcut gecerli veri bolumunu koru; --wipe-data ile sifirla)
    ensure_mobile_userdata "$OUT/userdata.img" "$userdata_mb"

    # 4. Cikti dizininde eski ara dosya kalmamasini garanti et
    cleanup_mobile_out_dir

    # 5. Uc final imaji da format ve butunluk acisindan dogrula
    local part_name
    part_name="$(mobile_system_partition_name)"
    python3 "$ROOT/aarch64crates/tools/validate-mobile-images.py" \
        --out-dir "$OUT" \
        --userdata-mb "$userdata_mb" \
        --system-part "$part_name"

    echo
    echo "Final mobil imajlar ($OUT):"
    echo "  boot.img     : $OUT/boot.img ($(file_size "$OUT/boot.img") bayt)"
    echo "  super.img    : $OUT/super.img ($(file_size "$OUT/super.img") bayt)"
    echo "  userdata.img : $OUT/userdata.img ($(file_size "$OUT/userdata.img") bayt)"
    if [ "$TAG" = "android" ]; then
        echo
        echo "Bilgilendirici fastboot komutlari (OTOMATIK CALISTIRILMAZ):"
        echo "  fastboot boot $OUT/boot.img"
        echo "  fastboot flash boot $OUT/boot.img"
        echo "  fastboot flash super $OUT/super.img"
        echo "  fastboot flash userdata $OUT/userdata.img   (cihazdaki tum kullanici verisini siler)"
        echo "Not: Onyukleyici kilidi acik degilse imzasiz boot.img acilmaz; yazmadan once stok yedegi alin."
    else
        echo
        echo "QEMU notu: 'bash build.sh $ARCH qemu run' dogrudan ham cekirdegi ($WORK/Image) -kernel ile baslatir,"
        echo "           $OUT/super.img (salt-okunur) ve $OUT/userdata.img (yazilabilir) disklerini baglar."
        echo "           $OUT/boot.img Android boot-image teslimati olarak uretildi ve dogrulandi."
    fi
}

build_android_images() {
    build_mobile_images
}
