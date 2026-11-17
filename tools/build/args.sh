ARCH=""
TAG=""
DEVICE=""
ACTION=""
ACTION_ARGS=()
QEMU_MEM=""
QEMU_SMP=""
QEMU_GDB=0
QEMU_HEADLESS=0
QEMU_DEBUG=0
QEMU_NO_KVM=0
QEMU_FLAG_ARGS=()
QEMU_USER_ARGS=()
DRY_RUN=0

usage() {
    cat <<'TXT'
kullanim: bash build.sh [mimari] [etiket] [cihaz] [eylem] [secenekler] [-- ham-qemu-argumanlari]

mimari   x86_64 (varsayilan) | arm64 | armv8a | armv9a
etiket   qemu | generic (x86_64) | pc (arm64) | android (armv8a/armv9a)
cihaz    aarch64crates/drivers/ altindaki klasor adi (ornek: x6826c, rpi4, qemu-virt)
eylem    build (varsayilan) | run | all | vbox | clean | info | devices | drivers
         new-device <ad> [soc] | unpack-boot <boot.img>

secenekler (QEMU denemesi icin)
  --mem MB        sanal makine RAM'i (x86: 512, aarch64: cihaz ayari)
  --wipe-data     aarch64 qemu: userdata.img'yi (/data) bastan olustur
  --smp N         sanal islemci sayisi
  --gdb           QEMU'yu gdb icin durdur (-s -S, port 1234)
  --headless      pencere acma, yalnizca seri cikti
  --debug         QEMU istisna/kesme gunlugu (build/qemu-debug.log)
  --no-kvm        KVM/HVF hizlandirmasini kapat
  --qemu "ARGS"   ek QEMU argumanlari
  --dry-run       komutlari calistirmadan yazdir (yalnizca QEMU/eylem cozumu)
  -- ARGS...      '--' sonrasi her sey QEMU'ya aynen verilir

ornekler
  bash build.sh                                  x86_64 qemu ISO
  bash build.sh run --mem 1024 --smp 2           x86_64 QEMU'da ac
  bash build.sh armv8a qemu run                  aarch64 (qemu-virt) QEMU'da ac
  bash build.sh armv8a android x6826c            boot.img + super.img
  bash build.sh armv8a qemu run --gdb -- -d int  gdb bekleyerek ac, ham arguman ekle
TXT
}

is_arch() {
    case "$1" in
        x86_64|arm64|armv8a|armv9a) return 0 ;;
    esac
    return 1
}

is_aarch64_arch() {
    case "$1" in
        arm64|armv8a|armv9a) return 0 ;;
    esac
    return 1
}

parse_args() {
    local seen_raw=0
    while [ $# -gt 0 ]; do
        local token="$1"
        shift
        if [ "$seen_raw" = "1" ]; then
            QEMU_USER_ARGS+=("$token")
            continue
        fi
        case "$token" in
            --) seen_raw=1 ;;
            --mem) [ $# -gt 0 ] || fail "--mem bir deger ister"; QEMU_MEM="$1"; shift ;;
            --mem=*) QEMU_MEM="${token#--mem=}" ;;
            --smp) [ $# -gt 0 ] || fail "--smp bir deger ister"; QEMU_SMP="$1"; shift ;;
            --smp=*) QEMU_SMP="${token#--smp=}" ;;
            --gdb) QEMU_GDB=1 ;;
            --headless) QEMU_HEADLESS=1 ;;
            --debug) QEMU_DEBUG=1 ;;
            --no-kvm) QEMU_NO_KVM=1 ;;
            --wipe-data) export NC_WIPE_DATA=1 ;;
            --dry-run) DRY_RUN=1 ;;
            --qemu) [ $# -gt 0 ] || fail "--qemu bir deger ister"; read -r -a _extra <<< "$1"; QEMU_USER_ARGS+=("${_extra[@]}"); shift ;;
            --qemu=*) read -r -a _extra <<< "${token#--qemu=}"; QEMU_USER_ARGS+=("${_extra[@]}") ;;
            -h|--help|help) usage; exit 0 ;;
            x86_64|arm64|armv8a|armv9a) ARCH="$token" ;;
            android|generic|pc) TAG="$token" ;;
            qemu) TAG="qemu" ;;
            build|run|all|vbox|clean|info|devices|drivers) ACTION="$token" ;;
            new-device|unpack-boot)
                ACTION="$token"
                while [ $# -gt 0 ] && [ "${1#-}" = "$1" ]; do ACTION_ARGS+=("$1"); shift; done
                ;;
            *)
                if [ -d "$ROOT/aarch64crates/drivers/$token" ] && [ -f "$ROOT/aarch64crates/drivers/$token/device.conf" ]; then
                    DEVICE="$token"
                else
                    echo "bilinmeyen arguman: $token" >&2
                    usage >&2
                    exit 1
                fi
                ;;
        esac
    done
    ARCH="${ARCH:-x86_64}"
    ACTION="${ACTION:-build}"
    if [ -z "$TAG" ]; then
        TAG="qemu"
    fi
}

build_qemu_flag_args() {
    QEMU_FLAG_ARGS=()
    if [ -n "$QEMU_SMP" ]; then
        QEMU_FLAG_ARGS+=(-smp "$QEMU_SMP")
    fi
    if [ "$QEMU_GDB" = "1" ]; then
        QEMU_FLAG_ARGS+=(-s -S)
    fi
    if [ "$QEMU_HEADLESS" = "1" ]; then
        QEMU_FLAG_ARGS+=(-display none)
    fi
    if [ "$QEMU_DEBUG" = "1" ]; then
        QEMU_FLAG_ARGS+=(-d int,guest_errors,unimp -D "$BUILD/qemu-debug.log")
    fi
}
