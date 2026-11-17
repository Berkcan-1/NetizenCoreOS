RING3_DONE=""

ring3_enabled() {
    [ "${NC_RING3_APPS:-1}" != "0" ]
}

build_ring3_apps() {
    # Ring 3 uygulamalari: x86_64-unknown-none, 0x80_0100_0000'a baglanmis ELF'ler. Ring 3 artik VARSAYILAN:
    # NC_RING3_APPS=0 verilmedikce her profilde (qemu, generic, UEFI) derlenir. ELF'ler cekirdege gomuldugu icin
    # profiller arasinda ortaktir ve bir build.sh cagrisinda bir kez uretilir.
    # Basarisizlik varsayilan olarak olumcul degil: dosya bos birakilir ve uygulama ACILMAZ (Ring 0 yedegi icin NC_RING0_FALLBACK=1)
    # ve asagida yuksek sesle uyarilir. NC_RING3_STRICT=1 ile basarisizlik derlemeyi durdurur.
    [ -z "$RING3_DONE" ] || return 0
    RING3_DONE=1
    mkdir -p "$ROOT/kernel/embedded/ring3-apps" "$BUILD"
    local spec dir bin out log failed=""
    for spec in "stopwatch:nc-stopwatch-r3" "system:nc-system-r3" "files:nc-files-r3" "browser:nc-browser-r3" "settings:nc-settings-r3"; do
        dir="${spec%%:*}"
        bin="${spec##*:}"
        out="$ROOT/kernel/embedded/ring3-apps/$dir.elf"
        log="$BUILD/ring3-build-$dir.log"
        : > "$out"
        : > "$out.gz"
        if [ ! -f "$ROOT/apps/$dir/$bin/Cargo.toml" ]; then
            echo "UYARI: apps/$dir/$bin yok; Ring 3 $dir atlandi." >&2
            failed="$failed $dir"
            continue
        fi
        if bash "$ROOT/tools/build/ring3-cargo.sh" "$ROOT/apps/$dir/$bin/Cargo.toml" "$BUILD/cargo-ring3" "$log"; then
            cp "$BUILD/cargo-ring3/$TARGET/release/$bin" "$out"
            gzip -9 -n -c "$out" > "$out.gz"
            echo "Ring 3 $dir ELF: $(wc -c < "$out") bayt (gomulen sikistirilmis: $(wc -c < "$out.gz") bayt)"
        else
            failed="$failed $dir"
            echo "UYARI: Ring 3 $dir derlenemedi; uygulama acilmayacak (NC_RING0_FALLBACK=1 ile Ring 0 yedegi eklenir)." >&2
            echo "------ cargo gunlugunun son 40 satiri (tamami: $log) ------" >&2
            tail -n 40 "$log" >&2
            echo "------------------------------------------------------------" >&2
        fi
    done
    if [ -n "$failed" ]; then
        echo "================================================================" >&2
        echo "UYARI: su uygulamalar Ring 3 OLARAK DERLENEMEDI:$failed" >&2
        echo "Bunlar ACILMAZ (NC_RING0_FALLBACK=1 ile Ring 0 yedegi eklenir). Gunlukler: $BUILD/ring3-build-<ad>.log" >&2
        echo "================================================================" >&2
        if [ "${NC_RING3_STRICT:-0}" = "1" ]; then
            fail "NC_RING3_STRICT=1: Ring 3 derlemesi basarisiz:$failed"
        fi
    fi
}
