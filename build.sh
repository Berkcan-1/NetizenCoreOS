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

for module in common args device ring3 kernel boot disk radio-bridge run-qemu run-vbox drivers; do
    source "$ROOT/tools/build/$module.sh"
done
for module in aarch64 qemu-aarch64 android; do
    source "$ROOT/aarch64crates/tools/$module.sh"
done

cd "$ROOT"

parse_args "$@"

case "$ACTION" in
    devices)
        list_devices
        exit 0
        ;;
    new-device)
        [ "${#ACTION_ARGS[@]}" -ge 1 ] || fail "kullanim: bash build.sh new-device <ad> [soc]"
        python3 "$ROOT/aarch64crates/tools/select_device.py" --new "${ACTION_ARGS[0]}" --soc "${ACTION_ARGS[1]:-}" --arch "${ARCH}"
        exit 0
        ;;
    unpack-boot)
        [ "${#ACTION_ARGS[@]}" -ge 1 ] || fail "kullanim: bash build.sh unpack-boot <boot.img> [cikti-klasoru]"
        python3 "$ROOT/aarch64crates/tools/android/unpack_bootimg.py" "${ACTION_ARGS[0]}" ${ACTION_ARGS[1]:+-o "${ACTION_ARGS[1]}"}
        exit 0
        ;;
    clean)
        rm -rf "$BUILD"
        exit 0
        ;;
esac

if is_aarch64_arch "$ARCH"; then
    if [ -z "$DEVICE" ]; then
        case "$TAG" in
            qemu) DEVICE="qemu-virt" ;;
            *) fail "$ARCH $TAG icin cihaz gerekli: bash build.sh $ARCH $TAG <cihaz>  (cihazlar: bash build.sh devices)" ;;
        esac
    fi
    load_device "$DEVICE"
    check_device_arch
    case "$ACTION" in
        info)
            print_info
            ;;
        build|all)
            build_aarch64_profile
            ;;
        run)
            [ "$TAG" = "qemu" ] || fail "run yalnizca qemu etiketiyle calisir (gercek cihaz icin fastboot/SD kullanin)"
            build_aarch64_profile
            run_qemu_aarch64
            ;;
        *)
            fail "$ARCH icin desteklenmeyen eylem: $ACTION"
            ;;
    esac
    exit 0
fi

if [ -n "$DEVICE" ]; then
    load_device "$DEVICE"
    [ "$DEV_ARCH" = "x86_64" ] || fail "aarch64crates/drivers/$DEVICE mimarisi $DEV_ARCH; x86_64 icin bir cihaz secmediniz"
    echo "NOT: x86_64 cihaz suruculeri henuz cekirdege baglanmiyor; cihaz klasoru yalnizca kaydedildi." >&2
fi

case "$TAG" in
    qemu|generic) PROFILE_NAME="$TAG" ;;
    *) fail "x86_64 icin etiket qemu veya generic olmali" ;;
esac

case "$ACTION" in
    info)
        print_info
        ;;
    build)
        check_tools
        build_profile "$PROFILE_NAME"
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
    *)
        usage >&2
        exit 1
        ;;
esac
