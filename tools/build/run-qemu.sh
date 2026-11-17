audio_args() {
    local backend="${NC_AUDIO_BACKEND:-pa}"
    AUDIO_ARGS=()
    case "$backend" in
        none)
            AUDIO_ARGS=(-audiodev none,id=snd0)
            ;;
        wav)
            AUDIO_ARGS=(-audiodev "wav,id=snd0,path=${NC_AUDIO_WAV:-$BUILD/audio.wav}")
            ;;
        *)
            AUDIO_ARGS=(-audiodev "$backend,id=snd0")
            ;;
    esac
    AUDIO_ARGS+=(-machine pcspk-audiodev=snd0 -device AC97,audiodev=snd0)
}

run_qemu() {
    need qemu-system-x86_64
    audio_args
    local accel=()
    if [ -z "${NC_NO_KVM:-}" ] && [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
        accel=(-enable-kvm -cpu host)
    fi
    local qemu_extra=()
    if [ -n "${NC_QEMU_EXTRA:-}" ]; then
        read -r -a qemu_extra <<< "$NC_QEMU_EXTRA"
    fi
    if [ "$QEMU_NO_KVM" = "1" ]; then
        accel=()
    fi
    build_qemu_flag_args
    qemu_extra+=(${QEMU_FLAG_ARGS[@]+"${QEMU_FLAG_ARGS[@]}"} ${QEMU_USER_ARGS[@]+"${QEMU_USER_ARGS[@]}"})

    prepare_hybrid_disk "$BUILD/disk.img"
    resolve_usb_args
    start_radio_bridge

    local sig=""
    if [ -f "$BUILD/disk.img" ]; then
        sig=$(dd if="$BUILD/disk.img" bs=1 count=7 skip=3 2>/dev/null | tr -d '\0' || true)
    fi

    local ovmf_bios=()
    for ovmf_path in /usr/share/ovmf/OVMF.fd /usr/share/edk2/ovmf/OVMF_CODE.fd /usr/share/qemu/OVMF.fd; do
        if [ -f "$ovmf_path" ]; then
            ovmf_bios=(-bios "$ovmf_path")
            break
        fi
    done

    if [ "$sig" = "NETIZEN" ] && [ "${FORCE_ISO:-0}" != "1" ]; then
        echo "Kurulu NetizenCore sabit diskten (ISO olmadan) baslatiliyor..."
        # Kurulum sirasinda MBR bolum tablosundaki 0xEF ESP kaydi ezildiyse geri yukle
        ensure_hybrid_partition_table "$BUILD/disk.img"
        dd if="$OUT/stage2.bin" of="$BUILD/disk.img" bs=512 seek=2048 conv=notrunc status=none
        dd if="$OUT/kernel.bin" of="$BUILD/disk.img" bs=512 seek=2056 conv=notrunc status=none
        write_esp_partition "$BUILD/disk.img"

        qemu-system-x86_64 \
            ${accel[@]+"${accel[@]}"} \
            ${ovmf_bios[@]+"${ovmf_bios[@]}"} \
            -m "${QEMU_MEM:-${NC_QEMU_MEM:-512}}" \
            ${qemu_extra[@]+"${qemu_extra[@]}"} \
            -drive file="$BUILD/disk.img",format=raw,if=ide,index=0,media=disk \
            ${USB_ARGS[@]+"${USB_ARGS[@]}"} \
            -boot order=c \
            -netdev user,id=net0 \
            -device e1000,netdev=net0 \
            -vga std \
            ${AUDIO_ARGS[@]+"${AUDIO_ARGS[@]}"} \
            -serial stdio \
            -chardev pipe,id=ncradio,path="$BUILD/radio" \
            -serial chardev:ncradio \
            -debugcon "file:$BUILD/debugcon.log" \
            -device isa-debug-exit,iobase=0xf4,iosize=0x04
    else
        echo "Kurulum medyasi (ISO) ile baslatiliyor..."
        qemu-system-x86_64 \
            ${accel[@]+"${accel[@]}"} \
            ${ovmf_bios[@]+"${ovmf_bios[@]}"} \
            -m "${QEMU_MEM:-${NC_QEMU_MEM:-512}}" \
            ${qemu_extra[@]+"${qemu_extra[@]}"} \
            -drive file="$BUILD/disk.img",format=raw,if=ide,index=0,media=disk \
            ${USB_ARGS[@]+"${USB_ARGS[@]}"} \
            -cdrom "$ISO" \
            -boot once=d,order=c \
            -netdev user,id=net0 \
            -device e1000,netdev=net0 \
            -vga std \
            ${AUDIO_ARGS[@]+"${AUDIO_ARGS[@]}"} \
            -serial stdio \
            -chardev pipe,id=ncradio,path="$BUILD/radio" \
            -serial chardev:ncradio \
            -debugcon "file:$BUILD/debugcon.log" \
            -device isa-debug-exit,iobase=0xf4,iosize=0x04
    fi
}
