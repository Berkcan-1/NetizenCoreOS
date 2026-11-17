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
