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
    rm -f "$image"
    truncate -s "${size_mb}M" "$image"
    mkfs.exfat -c "$cluster" -L NCDATA "$image" >/dev/null
    echo "userdata.img: exFAT ${size_mb} MiB (kume $((cluster / 1024)) KiB)"
}

build_qemu_userdata() {
    local image="$OUT/userdata.img"
    local size_mb="${NC_USERDATA_MB:-${DEV_USERDATA_MB:-32768}}"
    local want_bytes=$(( size_mb * 1024 * 1024 ))
    local cur_bytes=0
    if [ -f "$image" ]; then
        cur_bytes="$(stat -c %s "$image" 2>/dev/null || echo 0)"
    fi
    if [ "${NC_WIPE_DATA:-0}" = "1" ] || [ ! -f "$image" ] || [ "$cur_bytes" != "$want_bytes" ]; then
        build_userdata_image "$image" "$size_mb"
    else
        echo "userdata.img korunuyor (${size_mb} MiB; sifirlamak icin NC_WIPE_DATA=1 ya da --wipe-data)"
    fi
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

run_qemu_aarch64() {
    [ "$DRY_RUN" = "1" ] || need qemu-system-aarch64
    local image="$OUT/Image"
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
    if [ "$DRY_RUN" != "1" ]; then
        build_qemu_super
        build_qemu_userdata
    fi
    local storage=()
    if [ -f "$OUT/super.img" ]; then
        storage=(-drive "if=none,id=super,file=$OUT/super.img,format=raw,readonly=on" -device virtio-blk-device,drive=super)
    fi
    if [ -f "$OUT/userdata.img" ] || [ "$DRY_RUN" = "1" ]; then
        storage+=(-drive "if=none,id=userdata,file=$OUT/userdata.img,format=raw" -device virtio-blk-device,drive=userdata)
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
