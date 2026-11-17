list_devices() {
    python3 "$ROOT/aarch64crates/tools/select_device.py" --list
}

load_device() {
    local id="$1"
    [ -f "$ROOT/aarch64crates/drivers/$id/device.conf" ] || fail "aarch64crates/drivers/$id/device.conf yok (bash build.sh devices ile listele)"
    eval "$(python3 "$ROOT/aarch64crates/tools/select_device.py" --env "$id")"
    DEVICE="$id"
}

select_device_crate() {
    python3 "$ROOT/aarch64crates/tools/select_device.py" --select "$1"
}

check_device_arch() {
    if [ "$DEV_ARCH" != "$ARCH" ]; then
        fail "aarch64crates/drivers/$DEVICE mimarisi $DEV_ARCH; komut mimarisi $ARCH. Dogrusu: bash build.sh $DEV_ARCH ... $DEVICE"
    fi
}

print_info() {
    echo "mimari : $ARCH"
    echo "etiket : $TAG"
    echo "eylem  : $ACTION"
    if [ -n "$DEVICE" ]; then
        echo "cihaz  : $DEVICE ($DEV_NAME), soc=$DEV_SOC, arayuz=$DEV_UI, kernel_base=${DEV_KERNEL_BASE:-?}"
    else
        echo "cihaz  : (yok)"
    fi
}
