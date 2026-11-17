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
                        if [[ "$snd_file" != *.mp3 ]] && command -v paplay >/dev/null 2>&1; then
                            paplay "$snd_file" >/dev/null 2>&1 &
                        elif [[ "$snd_file" == *.wav ]] && command -v aplay >/dev/null 2>&1; then
                            aplay -q "$snd_file" >/dev/null 2>&1 &
                        elif command -v mpv >/dev/null 2>&1; then
                            mpv --no-video --really-quiet "$snd_file" >/dev/null 2>&1 &
                        elif command -v ffplay >/dev/null 2>&1; then
                            ffplay -nodisp -autoexit -loglevel quiet "$snd_file" >/dev/null 2>&1 &
                        elif command -v mpg123 >/dev/null 2>&1 && [[ "$snd_file" == *.mp3 ]]; then
                            mpg123 -q "$snd_file" >/dev/null 2>&1 &
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
