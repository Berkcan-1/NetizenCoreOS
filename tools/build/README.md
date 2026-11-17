# tools/build

Kökteki `build.sh` yalnızca sabitleri tanımlar, bu klasördeki modülleri `source` eder ve komutu yönlendirir.
Her dosya yalnızca işlev tanımlar (istisna: `ring3.sh` başında `RING3_DONE=""`).

| Dosya | İşlevler |
|---|---|
| `common.sh` | `fail`, `need`, `file_size`, `pick_objcopy`, `select_iso_tool`, `check_tools`, `configure_profile` |
| `ring3.sh` | `ring3_enabled`, `build_ring3_apps` (uygulama listesi burada: `<ad>:nc-<ad>-r3`) |
| `kernel.sh` | `build_kernel` (görüntü sınırı, `NC_IMAGE_LIMIT_MIB`) |
| `boot.sh` | `build_bootloader`, `build_uefi_optional`, `build_iso`, `build_profile` |
| `disk.sh` | `ensure_hybrid_partition_table`, `write_esp_partition`, `prepare_hybrid_disk`, `resolve_usb_args` |
| `radio-bridge.sh` | `ascii_clean`, `start_radio_bridge` (QEMU için konak Wi-Fi/Bluetooth köprüsü) |
| `run-qemu.sh` | `run_qemu` |
| `run-vbox.sh` | `run_vbox` |
| `drivers.sh` | `build_drivers` |
| `args.sh` | `parse_args` (mimari/etiket/cihaz/eylem/QEMU seçenekleri), `usage`, `build_qemu_flag_args` |
| `device.sh` | `list_devices`, `load_device`, `select_device_crate`, `check_device_arch`, `print_info` |
| `ring3-cargo.sh` | Ring 3 crate'lerini `x86_64-unknown-none` için derleyen cargo sarmalayıcısı (`build.sh` ve `sdk/build.sh` kullanır) |

arm64 modülleri (`aarch64.sh`, `qemu-aarch64.sh`, `android.sh`) `aarch64crates/tools/` altındadır; `build.sh` bunları ayrıca `source` eder.

Yeni bir komut eklerken: işlevi uygun modüle (ya da yeni bir `.sh` dosyasına) yaz, yeni dosyaysa `build.sh` içindeki
`for module in ...` listesine ekle, komutu `case` bloğuna bağla.
