build_userdata_image() {
    local image="$1"
    local size_mb="$2"
    command -v mkfs.exfat >/dev/null 2>&1 || fail "mkfs.exfat yok (Debian/Ubuntu: sudo apt install exfatprogs); /data bolumu uretilemez"
    [ "$size_mb" -ge 64 ] 2>/dev/null || fail "userdata boyutu en az 64 MiB olmali (simdiki: $size_mb)"
    local cluster=65536
    if [ "$size_mb" -gt 4000 ]; then
        cluster=262144
    fi
    local clusters=$(( size_mb * 1024 * 1024 / cluster ))
    [ "$clusters" -le 1048576 ] || fail "userdata $size_mb MiB cok buyuk: cekirdekteki exFAT surucusu en cok 1048576 kume destekler"
    local tmp_dir="${WORK:-$(dirname "$image")}"
    mkdir -p "$tmp_dir"
    local tmp_img="$tmp_dir/userdata.img.tmp"
    rm -f "$tmp_img"
    truncate -s "${size_mb}M" "$tmp_img"
    mkfs.exfat -c "$cluster" -L NCDATA "$tmp_img" >/dev/null
    mv -f "$tmp_img" "$image"
    echo "userdata.img: exFAT ${size_mb} MiB (kume $((cluster / 1024)) KiB)"
}

ensure_mobile_userdata() {
    local image="$1"
    local size_mb="$2"
    local want_bytes=$(( size_mb * 1024 * 1024 ))
    local cur_bytes=0
    if [ -f "$image" ]; then
        cur_bytes="$(stat -c %s "$image" 2>/dev/null || echo 0)"
    fi
    if [ "${NC_WIPE_DATA:-0}" = "1" ]; then
        echo "userdata.img: --wipe-data / NC_WIPE_DATA=1 ile yeniden bicimlendiriliyor (${size_mb} MiB)"
        build_userdata_image "$image" "$size_mb"
    elif [ ! -f "$image" ]; then
        build_userdata_image "$image" "$size_mb"
    elif [ "$cur_bytes" = "$want_bytes" ]; then
        echo "userdata.img korunuyor (${size_mb} MiB; sifirlamak icin NC_WIPE_DATA=1 ya da --wipe-data)"
    elif [ "${NC_ACCEPT_USERDATA_RESIZE:-0}" = "1" ]; then
        echo "userdata.img yeniden boyutlandiriliyor ($((cur_bytes / 1048576)) MiB -> ${size_mb} MiB)"
        build_userdata_image "$image" "$size_mb"
    else
        fail "mevcut userdata.img boyutu ($((cur_bytes / 1048576)) MiB) istenen boyutla (${size_mb} MiB) uyusmuyor!
       Veri kaybini onlemek icin mevcut imaj otomatik silinmedi.
       Yeniden olusturmak icin --wipe-data (veya NC_WIPE_DATA=1 / NC_ACCEPT_USERDATA_RESIZE=1) verin."
    fi
}

build_qemu_userdata() {
    local size_mb="${NC_USERDATA_MB:-${DEV_USERDATA_MB:-32768}}"
    ensure_mobile_userdata "$OUT/userdata.img" "$size_mb"
}

build_qemu_super() {
    local missing=""
    local tool
    for tool in mkfs.vfat mcopy mmd python3; do
        command -v "$tool" >/dev/null 2>&1 || missing="$missing $tool"
    done
    if [ -n "$missing" ]; then
        echo "UYARI: super.img uretilmedi; eksik arac:$missing (dosya sistemi araclari: dosfstools mtools)" >&2
        return 0
    fi
    local stage image super used_kb size_mb
    stage="$(stage_system_root)"
    image="$OUT/system.img"
    super="$OUT/super.img"
    used_kb="$(du -sk "$stage" | awk '{print $1}')"
    size_mb=$(( used_kb / 1024 * 3 / 2 + 40 ))
    rm -f "$image" "$super"
    truncate -s "${size_mb}M" "$image"
    mkfs.vfat -F 32 -n NCSYSTEM "$image" >/dev/null
    mcopy -s -i "$image" "$stage"/* ::
    #mmd -i "$image" ::packages 2>/dev/null || true
    if [ "${NC_USE_LPMAKE:-0}" = "1" ]; then
        need lpmake
        local bytes
        bytes="$(stat -c %s "$image")"
        lpmake --metadata-size 65536 --super-name super --metadata-slots 2 \
            --device "super:$((bytes + 16 * 1024 * 1024))" --group "main:$bytes" \
            --partition "system:readonly:$bytes:main" --image "system=$image" --output "$super"
        echo "super.img: lpmake ile uretildi"
    else
        python3 "$ROOT/aarch64crates/tools/mk-super.py" "$super" system "$image"
    fi
}

discover_host_usb_storage() {
    # Guvenli Linux host USB yigin depolama kesfi:
    # - Yalnizca /sys/block/* altinda removable=1 VE gercek yolu /usb iceren blok aygitlarini tarar.
    # - Dahili diskler (nvme*, mmcblk*, loop*, dm-*, USB olmayan sd*) kesinlikle secilmez.
    # - Varsayilan mod SALT-OKUNUR (readonly=on). Yazma yalnizca acik USB_RW=1 ile acilir.
    local dev="${USB_DEV:-}"
    local rw="${USB_RW:-0}"
    local found=()

    if [ -z "$dev" ]; then
        local b name real rem
        for b in /sys/block/*; do
            [ -e "$b" ] || continue
            name="$(basename "$b")"
            case "$name" in
                loop*|ram*|dm-*|nvme*|mmcblk*|sr*|zram*) continue ;;
            esac
            rem="$(cat "$b/removable" 2>/dev/null || echo 0)"
            [ "$rem" = "1" ] || continue
            real="$(readlink -f "$b" 2>/dev/null || true)"
            case "$real" in
                */usb*) [ -b "/dev/$name" ] && found+=("/dev/$name") ;;
            esac
        done
        if [ "${#found[@]}" -eq 0 ]; then
            return 1
        elif [ "${#found[@]}" -gt 1 ]; then
            echo "UYARI: Birden fazla USB depolama aygiti algilandi; belirsizlik nedeniyle otomatik secilmedi:" >&2
            printf '  - %s\n' "${found[@]}" >&2
            echo "       Acikca secin: USB_DEV=/dev/sdX bash build.sh armv8a qemu run" >&2
            return 1
        fi
        dev="${found[0]}"
    fi

    if [ ! -b "$dev" ] && [ ! -f "$dev" ]; then
        fail "USB_DEV bulunamadi (blok aygiti veya dosya degil): $dev"
    fi

    # Kok dosya sistemini barindiran dahili diski yanlislikla baglamayi onle
    if [ -b "$dev" ]; then
        local root_src
        root_src="$(findmnt -n -o SOURCE / 2>/dev/null || true)"
        case "$root_src" in
            "${dev}"*)
                fail "GUVENLIK: $dev ana sistemin kok diskidir; VM'e baglanamaz!"
                ;;
        esac
    fi

    # Ana makinede bagli (mounted) mi kontrol et
    local mounted=""
    if [ -b "$dev" ] && [ -r /proc/mounts ]; then
        mounted="$(awk -v d="$dev" 'index($1, d) == 1 {print $1 " -> " $2}' /proc/mounts)"
    fi

    if [ "$rw" = "1" ]; then
        if [ -n "$mounted" ]; then
            fail "GUVENLIK: $dev ana makinede bagli iken USB_RW=1 ile yazilabilir baglanamaz! Once ayirin (umount):\n$mounted"
        fi
        if [ ! -w "$dev" ]; then
            fail "USB_DEV yazma izni yok ($dev); salt-okunur kullanin veya izin verin"
        fi
        USB_ATTACH_DEV="$dev"
        USB_ATTACH_RO="off"
        echo "[usb] harici depolama baglaniyor (virtio-blk koprusu -> /storage/usb0): $dev [YAZILABILIR]"
    else
        if [ ! -r "$dev" ]; then
            echo "UYARI: $dev okunamiyor (izin gerekli, or. disk grubu veya sudo); USB atlandi." >&2
            return 1
        fi
        if [ -n "$mounted" ]; then
            echo "[usb] NOT: $dev ana makinede bagli; veri guvenligi icin SALT-OKUNUR baglaniyor."
        fi
        USB_ATTACH_DEV="$dev"
        USB_ATTACH_RO="on"
        echo "[usb] harici depolama baglaniyor (virtio-blk koprusu -> /storage/usb0): $dev [SALT-OKUNUR]"
    fi
    return 0
}

run_qemu_aarch64() {
    [ "$DRY_RUN" = "1" ] || need qemu-system-aarch64
    local image="$OUT/Image"
    if is_mobile_image_profile; then
        image="$WORK/Image"
    fi
    [ -f "$image" ] || fail "$image yok; once derleyin"
    local machine="${DEV_QEMU_MACHINE:-virt,gic-version=3}"
    local cpu="${DEV_QEMU_CPU:-cortex-a72}"
    local mem="${QEMU_MEM:-${NC_QEMU_MEM:-${DEV_QEMU_MEM:-4096}}}"
    if [ "$mem" -lt 512 ] 2>/dev/null; then
        fail "aarch64 qemu-virt icin en az 512 MiB RAM gerekir (--mem 512)"
    fi
    local accel=(-cpu "$cpu")
    if [ "$QEMU_NO_KVM" = "0" ] && [ "$(uname -m)" = "aarch64" ] && [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
        accel=(-enable-kvm -cpu host)
        machine="${machine%%,*},gic-version=host"
    fi
    if [ "$DRY_RUN" != "1" ] && ! is_mobile_image_profile; then
        build_qemu_super
        build_qemu_userdata
    fi
    local storage=()
    if is_mobile_image_profile && [ "$DRY_RUN" != "1" ]; then
        [ -f "$OUT/super.img" ] || fail "$OUT/super.img yok; once derleyin"
        [ -f "$OUT/userdata.img" ] || fail "$OUT/userdata.img yok; once derleyin"
    fi
    if [ -f "$OUT/super.img" ] || [ "$DRY_RUN" = "1" ]; then
        storage=(-drive "if=none,id=super,file=$OUT/super.img,format=raw,readonly=on" -device virtio-blk-device,drive=super)
    fi
    if [ -f "$OUT/userdata.img" ] || [ "$DRY_RUN" = "1" ]; then
        storage+=(-drive "if=none,id=userdata,file=$OUT/userdata.img,format=raw" -device virtio-blk-device,drive=userdata)
    fi
    USB_ATTACH_DEV=""
    USB_ATTACH_RO="on"
    if discover_host_usb_storage; then
        storage+=(-drive "if=none,id=usb0,file=$USB_ATTACH_DEV,format=raw,readonly=$USB_ATTACH_RO" -device virtio-blk-device,drive=usb0)
    fi
    build_qemu_flag_args
    local display_args=()
    if [ "$QEMU_HEADLESS" = "0" ]; then
        display_args=(-display gtk,zoom-to-fit=on)
    fi
    local extra=()
    if [ -n "${NC_QEMU_EXTRA:-}" ]; then
        read -r -a extra <<< "$NC_QEMU_EXTRA"
    fi
    local cmd=(
        qemu-system-aarch64
        -M "$machine"
        "${accel[@]}"
        -m "$mem"
        -kernel "$image"
        -device ramfb
        -device virtio-keyboard-device
        -device virtio-tablet-device
        ${storage[@]+"${storage[@]}"}
        -netdev user,id=net0
        -device virtio-net-device,netdev=net0
        -serial stdio
        ${display_args[@]+"${display_args[@]}"}
        ${QEMU_FLAG_ARGS[@]+"${QEMU_FLAG_ARGS[@]}"}
        ${extra[@]+"${extra[@]}"}
        ${QEMU_USER_ARGS[@]+"${QEMU_USER_ARGS[@]}"}
    )
    if [ "$DRY_RUN" = "1" ]; then
        printf '%q ' "${cmd[@]}"
        echo
        return 0
    fi
    "${cmd[@]}"
}
