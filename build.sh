#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD="$ROOT/build"
TARGET="x86_64-unknown-none"
UEFI_TARGET="x86_64-unknown-uefi"
SECTOR=2048
KERNEL_LIMIT=16777216
VM_NAME="NetizenCore"

FALLBACK_WIDTH=1024
FALLBACK_HEIGHT=768

export NC_LOGO_DIR="${NC_LOGO_DIR:-$ROOT/assets/logo}"

fail() {
    echo "hata: $*" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || fail "$1 bulunamadi"
}

file_size() {
    stat -c %s "$1"
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

select_iso_tool() {
    if command -v xorriso >/dev/null 2>&1; then
        ISO_CMD=(xorriso -as mkisofs)
    elif command -v genisoimage >/dev/null 2>&1; then
        ISO_CMD=(genisoimage)
    elif command -v mkisofs >/dev/null 2>&1; then
        ISO_CMD=(mkisofs)
    else
        fail "xorriso, genisoimage veya mkisofs gerekli"
    fi
}

check_tools() {
    need nasm
    need cargo
    need stat
    need truncate
    need mkfs.vfat
    need mcopy
    need mmd
}

configure_profile() {
    PROFILE="$1"
    case "$PROFILE" in
        qemu)
            PREFERRED_WIDTH=1280
            PREFERRED_HEIGHT=720
            ;;
        generic)
            PREFERRED_WIDTH=1024
            PREFERRED_HEIGHT=768
            ;;
        *)
            fail "bilinmeyen profil: $PROFILE"
            ;;
    esac
    OUT="$BUILD/$PROFILE"
    ISO="$BUILD/netizencore-$PROFILE.iso"
}

build_kernel() {
    if command -v rustup >/dev/null 2>&1; then
        rustup target add "$TARGET"
    fi
    RUSTFLAGS='-C relocation-model=static --cfg chacha20_force_soft --cfg poly1305_force_soft --cfg curve25519_dalek_backend="serial"' \
        cargo build --release \
        --manifest-path "$ROOT/kernel/Cargo.toml" \
        --target "$TARGET" \
        --target-dir "$BUILD/cargo-$PROFILE" \
        --features "$PROFILE"
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

build_bootloader() {
    nasm -f bin "$ROOT/bootloader/mbr.asm" -o "$ROOT/crates/nc-storage/mbr.bin"
    [ "$(file_size "$ROOT/crates/nc-storage/mbr.bin")" -eq 446 ] || fail "mbr boyutu 446 olmali"

    nasm -f bin "$ROOT/bootloader/stage1.asm" -o "$OUT/stage1.bin"
    [ "$(file_size "$OUT/stage1.bin")" -eq 2048 ] || fail "stage1 boyutu 2048 olmali"

    nasm -f bin \
        -DPREFERRED_WIDTH="$PREFERRED_WIDTH" \
        -DPREFERRED_HEIGHT="$PREFERRED_HEIGHT" \
        -DFALLBACK_WIDTH="$FALLBACK_WIDTH" \
        -DFALLBACK_HEIGHT="$FALLBACK_HEIGHT" \
        -DKERNEL_SECTORS_512="${KERNEL_SECTORS_512:-4096}" \
        "$ROOT/bootloader/stage2.asm" -o "$OUT/stage2.bin"
    [ "$(file_size "$OUT/stage2.bin")" -eq 4096 ] || fail "stage2 boyutu 4096 olmali"
}

build_uefi_optional() {
    if [ -f "$ROOT/crates/nc-uefi/Cargo.toml" ]; then
        echo "UEFI ikilisi derleniyor..."
        if command -v rustup >/dev/null 2>&1; then
            rustup target add "$UEFI_TARGET" >/dev/null 2>&1 || true
        fi
        NC_KERNEL_BIN="$OUT/kernel.bin" \
        NC_STAGE2_BIN="$OUT/stage2.bin" \
            cargo build --release \
            --manifest-path "$ROOT/crates/nc-uefi/Cargo.toml" \
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

ascii_clean() {
    sed 's/ğ/g/g; s/Ğ/G/g; s/ü/u/g; s/Ü/U/g; s/ş/s/g; s/Ş/S/g; s/ı/i/g; s/İ/I/g; s/ö/o/g; s/Ö/O/g; s/ç/c/g; s/Ç/C/g' | tr -cd '\11\12\40-\176'
}

start_radio_bridge() {
    rm -f "$BUILD/radio.in" "$BUILD/radio.out"
    mkfifo "$BUILD/radio.in" "$BUILD/radio.out"
    (
        exec 3<>"$BUILD/radio.out"
        exec 4<>"$BUILD/radio.in"

        send_hw_info() {
            local w_hw="802.11 Kablosuz Ag Denetleyicisi"
            if command -v lspci >/dev/null 2>&1; then
                local found_w
                found_w=$(lspci | grep -i -E 'network controller|wireless|wi-fi|802\.11' | head -n 1 | cut -d: -f3- | sed 's/^ *//' | ascii_clean)
                if [ -n "$found_w" ]; then
                    w_hw="$found_w"
                fi
            fi
            local b_hw="Bluetooth HCI 5.x Denetleyici (hci0)"
            if command -v bluetoothctl >/dev/null 2>&1; then
                local ctrl_name
                ctrl_name=$(timeout 1 bluetoothctl list 2>/dev/null | head -n 1 | awk '{$1=""; $2=""; print $0}' | sed 's/^ *//' | ascii_clean || true)
                if [ -n "$ctrl_name" ]; then
                    b_hw="Bluetooth HCI: $ctrl_name"
                fi
            fi
            local w_on="1"
            if command -v nmcli >/dev/null 2>&1; then
                local r_state
                r_state=$(nmcli radio wifi 2>/dev/null || echo "enabled")
                [ "$r_state" = "disabled" ] && w_on="0"
            fi
            printf "H|%s|%s|%s|1\n" "$w_hw" "$b_hw" "$w_on" >&4
        }

        scan_host_wifi() {
            printf "W_CLEAR\n" >&4
            if command -v nmcli >/dev/null 2>&1; then
                nmcli -t -f IN-USE,SSID,CHAN,FREQ,SIGNAL,SECURITY dev wifi list --rescan auto 2>/dev/null | ascii_clean | while IFS=: read -r inuse ssid chan freq sig sec; do
                    [ -z "$ssid" ] && continue
                    [ "$ssid" = "--" ] && continue
                    local active="0"
                    [ "$inuse" = "*" ] && active="1"
                    [ -z "$sec" ] && sec="Acik"
                    printf "W|%s|%s|%s|%s|%s|%s\n" "$active" "$ssid" "${chan:-1}" "${freq:-2412 MHz}" "${sig:-50}" "$sec" >&4
                done
            fi
            printf "W_END\n" >&4
        }

        scan_host_bt() {
            printf "B_CLEAR\n" >&4
            if command -v bluetoothctl >/dev/null 2>&1; then
                timeout 1 bluetoothctl --timeout 1 scan on >/dev/null 2>&1 || true
                bluetoothctl devices 2>/dev/null | ascii_clean | while read -r tag mac rest; do
                    [ "$tag" != "Device" ] && continue
                    [ -z "$mac" ] && continue
                    local info
                    info=$(bluetoothctl info "$mac" 2>/dev/null | ascii_clean || true)
                    local conn="0"
                    echo "$info" | grep -q "Connected: yes" && conn="1"
                    local icon_kind="Bluetooth Cihaz"
                    if echo "$info" | grep -q -i "audio"; then
                        icon_kind="Ses / Kulaklik"
                    elif echo "$info" | grep -q -i "phone"; then
                        icon_kind="Akilli Telefon"
                    elif echo "$info" | grep -q -i "keyboard"; then
                        icon_kind="Kablosuz Klavye"
                    elif echo "$info" | grep -q -i "mouse"; then
                        icon_kind="Kablosuz Fare"
                    fi
                    local batt
                    batt=$(echo "$info" | grep "Battery Percentage" | sed -E 's/.*\(([0-9]+)\).*/\1/' | head -n 1 || echo "0")
                    [ -z "$batt" ] && batt="0"
                    printf "B|%s|%s|%s|%s|%s\n" "$conn" "$mac" "$icon_kind" "$batt" "${rest:-$mac}" >&4
                done
            fi
            printf "B_END\n" >&4
        }

        while IFS= read -r cmd <&3; do
            cmd=$(echo "$cmd" | tr -d '\r')
            case "$cmd" in
                CMD:STARTUP_SOUND)
                    snd_file=$(find "$ROOT/assets/sound" -maxdepth 1 -type f \( -name "*.wav" -o -name "*.mp3" -o -name "*.ogg" \) | head -n 1 || true)
                    if [ -n "$snd_file" ]; then
                        if command -v ffplay >/dev/null 2>&1; then
                            ffplay -nodisp -autoexit -loglevel quiet "$snd_file" >/dev/null 2>&1 &
                        elif command -v mpv >/dev/null 2>&1; then
                            mpv --no-video --really-quiet "$snd_file" >/dev/null 2>&1 &
                        elif command -v mpg123 >/dev/null 2>&1 && [[ "$snd_file" == *.mp3 ]]; then
                            mpg123 -q "$snd_file" >/dev/null 2>&1 &
                        elif command -v paplay >/dev/null 2>&1 && [[ "$snd_file" != *.mp3 ]]; then
                            paplay "$snd_file" >/dev/null 2>&1 &
                        elif command -v aplay >/dev/null 2>&1 && [[ "$snd_file" == *.wav ]]; then
                            aplay -q "$snd_file" >/dev/null 2>&1 &
                        fi
                    fi
                    ;;
                CMD:INIT)
                    send_hw_info
                    ;;
                CMD:WIFI_SCAN)
                    scan_host_wifi
                    ;;
                CMD:WIFI_ON)
                    command -v nmcli >/dev/null 2>&1 && nmcli radio wifi on >/dev/null 2>&1 || true
                    send_hw_info
                    scan_host_wifi
                    ;;
                CMD:WIFI_OFF)
                    command -v nmcli >/dev/null 2>&1 && nmcli radio wifi off >/dev/null 2>&1 || true
                    send_hw_info
                    ;;
                CMD:WIFI_CONN\|*)
                    IFS='|' read -r _ target_ssid target_pass <<< "$cmd"
                    if command -v nmcli >/dev/null 2>&1; then
                        if [ -n "$target_pass" ]; then
                            nmcli dev wifi connect "$target_ssid" password "$target_pass" >/dev/null 2>&1 && \
                                printf "MSG|'%s' agina basariyla baglanildi.\n" "$target_ssid" >&4 || \
                                printf "MSG|'%s' baglantisi basarisiz oldu (Parolayi kontrol edin).\n" "$target_ssid" >&4
                        else
                            nmcli dev wifi connect "$target_ssid" >/dev/null 2>&1 && \
                                printf "MSG|'%s' agina basariyla baglanildi.\n" "$target_ssid" >&4 || \
                                printf "MSG|'%s' baglantisi basarisiz oldu.\n" "$target_ssid" >&4
                        fi
                    fi
                    scan_host_wifi
                    ;;
                CMD:WIFI_DISC\|*)
                    IFS='|' read -r _ target_ssid <<< "$cmd"
                    if command -v nmcli >/dev/null 2>&1; then
                        nmcli con down "$target_ssid" >/dev/null 2>&1 || true
                        printf "MSG|'%s' ag baglantisi kesildi.\n" "$target_ssid" >&4
                    fi
                    scan_host_wifi
                    ;;
                CMD:BT_SCAN)
                    scan_host_bt
                    ;;
                CMD:BT_ON)
                    command -v bluetoothctl >/dev/null 2>&1 && bluetoothctl power on >/dev/null 2>&1 || true
                    scan_host_bt
                    ;;
                CMD:BT_OFF)
                    command -v bluetoothctl >/dev/null 2>&1 && bluetoothctl power off >/dev/null 2>&1 || true
                    ;;
                CMD:BT_CONN\|*)
                    IFS='|' read -r _ target_mac <<< "$cmd"
                    if command -v bluetoothctl >/dev/null 2>&1; then
                        bluetoothctl connect "$target_mac" >/dev/null 2>&1 && \
                            printf "MSG|%s Bluetooth cihazi baglandi.\n" "$target_mac" >&4 || \
                            printf "MSG|%s cihazi yanit vermedi.\n" "$target_mac" >&4
                    fi
                    scan_host_bt
                    ;;
                CMD:BT_DISC\|*)
                    IFS='|' read -r _ target_mac <<< "$cmd"
                    if command -v bluetoothctl >/dev/null 2>&1; then
                        bluetoothctl disconnect "$target_mac" >/dev/null 2>&1 || true
                        printf "MSG|%s cihaz baglantisi kesildi.\n" "$target_mac" >&4
                    fi
                    scan_host_bt
                    ;;
            esac
        done
    ) &
    RADIO_PID=$!
    trap 'kill "$RADIO_PID" "${USB_WATCH_PID:-}" 2>/dev/null || true' EXIT
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

run_qemu() {
    need qemu-system-x86_64
    local accel=()
    if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
        accel=(-enable-kvm -cpu host)
    fi

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
            -m 512 \
            -drive file="$BUILD/disk.img",format=raw,if=ide,index=0,media=disk \
            ${USB_ARGS[@]+"${USB_ARGS[@]}"} \
            -boot order=c \
            -netdev user,id=net0 \
            -device e1000,netdev=net0 \
            -vga std \
            -audiodev pa,id=snd0 -machine pcspk-audiodev=snd0 \
            -device AC97,audiodev=snd0 \
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
            -m 512 \
            -drive file="$BUILD/disk.img",format=raw,if=ide,index=0,media=disk \
            ${USB_ARGS[@]+"${USB_ARGS[@]}"} \
            -cdrom "$ISO" \
            -boot once=d,order=c \
            -netdev user,id=net0 \
            -device e1000,netdev=net0 \
            -vga std \
            -serial stdio \
            -chardev pipe,id=ncradio,path="$BUILD/radio" \
            -serial chardev:ncradio \
            -debugcon "file:$BUILD/debugcon.log" \
            -device isa-debug-exit,iobase=0xf4,iosize=0x04
    fi
}

build_drivers() {
    mkdir -p "$ROOT/drivers" "$BUILD/drivers"
    cat > "$ROOT/drivers/generic-display.efra.src" << 'EOF'
id=efra.display.generic
name=Netizen VBE/GOP Generic Turbo Framebuffer
version=2.1.0
file=generic-display.efra
class=display
vendor_id=0xFFFF
device_id=0xFFFF
pci_class=0x03
pci_subclass=0x00
write_combining=true
dma_burst=true
EOF
    printf "EFRA\x01\x00\x00\x00" > "$BUILD/drivers/generic-display.efra"
    echo "Harici .efra derleme alani hazir: $BUILD/drivers/generic-display.efra"
}

run_vbox() {
    need VBoxManage
    local fw_mode="${VBOX_FIRMWARE:-efi}"

    if ! VBoxManage showvminfo "$VM_NAME" >/dev/null 2>&1; then
        VBoxManage createvm --name "$VM_NAME" --ostype Other_64 --register
        VBoxManage modifyvm "$VM_NAME" --memory 512 --vram 64 --firmware "$fw_mode" \
            --graphicscontroller vboxvga --nic1 nat --nictype1 82540EM \
            --boot1 dvd --boot2 disk --boot3 none --boot4 none
        VBoxManage storagectl "$VM_NAME" --name IDE --add ide
    else
        VBoxManage modifyvm "$VM_NAME" --firmware "$fw_mode"
    fi

    if [ ! -f "$BUILD/vbox-disk.vdi" ]; then
        echo "VirtualBox icin Hibrit (BIOS + UEFI) sabit disk hazirlaniyor..."
        local raw_disk="$BUILD/vbox-raw.img"
        rm -f "$raw_disk"
        prepare_hybrid_disk "$raw_disk"
        VBoxManage convertfromraw "$raw_disk" "$BUILD/vbox-disk.vdi" --format VDI
        rm -f "$raw_disk"
        echo "Hibrit vbox-disk.vdi olusturuldu."
    fi

    VBoxManage storageattach "$VM_NAME" --storagectl IDE --port 0 --device 0 \
        --type hdd --medium "$BUILD/vbox-disk.vdi"

    # Eger VDI diske kurulum yapildiysa ("NETIZEN" imzasi varsa), ESP tablosunu onar ve ISO'yu cikar
    local check_raw="$BUILD/vbox-head.bin"
    rm -f "$check_raw"
    VBoxManage internalcommands converttoraw -start 0 -size 1024 "$BUILD/vbox-disk.vdi" "$check_raw" >/dev/null 2>&1 || true
    local vbox_sig=""
    if [ -f "$check_raw" ]; then
        vbox_sig=$(dd if="$check_raw" bs=1 count=7 skip=3 2>/dev/null | tr -d '\0' || true)
        rm -f "$check_raw"
    fi

    if [ "$vbox_sig" = "NETIZEN" ] && [ "${FORCE_ISO:-0}" != "1" ]; then
        echo "VirtualBox: Kurulu sistem tespit edildi, ISO cikariliyor ve diskten baslatiliyor..."
        VBoxManage storageattach "$VM_NAME" --storagectl IDE --port 1 --device 0 \
            --type dvddrive --medium emptydrive >/dev/null 2>&1 || true
        VBoxManage modifyvm "$VM_NAME" --boot1 disk --boot2 dvd
    else
        VBoxManage storageattach "$VM_NAME" --storagectl IDE --port 1 --device 0 \
            --type dvddrive --medium "$ISO"
        VBoxManage modifyvm "$VM_NAME" --boot1 dvd --boot2 disk
    fi

    VBoxManage startvm "$VM_NAME"
}

cd "$ROOT"

case "${1:-qemu}" in
    qemu)
        check_tools
        build_profile qemu
        ;;
    generic)
        check_tools
        build_profile generic
        ;;
    drivers)
        build_drivers
        ;;
    all)
        check_tools
        build_profile qemu
        build_profile generic
        ;;
    run)
        check_tools
        build_profile qemu
        run_qemu
        ;;
    vbox)
        check_tools
        build_profile generic
        run_vbox
        ;;
    clean)
        rm -rf "$BUILD"
        ;;
    *)
        echo "kullanim: bash build.sh [qemu|generic|drivers|all|run|vbox|clean]" >&2
        exit 1
        ;;
esac
