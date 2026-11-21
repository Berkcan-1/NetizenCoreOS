# x6826c — Infinix Hot 20 (MT6768, Helio G85)

<!-- otomatik:başla (tools/docs/sync_module_docs.py) -->
| | |
|---|---|
| **Konum** | `aarch64crates/drivers/x6826c` |
| **Katman** | Donanım |
| **Bağımlılıklar** | `nc-device`, `nc-hal` |
| **Kullananlar** | — |

Bu crate üzerinde çalışırken yapay zekâya şunu ver: kendi klasörü + bağımlılıklarının README dosyaları. Genel (pub) API'yi değiştirmediğin sürece `Kullananlar` sütunundaki crate'lere dokunman gerekmez.
<!-- otomatik:bitti -->

Infinix Hot 20 (`X6826C`, MediaTek Helio G85 / `MT6768`, ARMv8-A) cihaz profili, aygıt ağacı (`x6826c.dts`, `stock/dtb`, `stock/dtbo`), donanım kanıt haritası (`docs/hardware/x6826c-hardware-map.md`) ve çekirdeğe (`boot.img` içindeki `Image`) derlenen modüler donanım sürücüleri.

## Sürücü Modülleri (`src/`)

| Dosya | Donanım Bloğu | Taban Adresi | GIC INTID | Açıklama |
|---|---|---|---|---|
| `src/lib.rs` | `DeviceProfile` (`PROFILE`) | — | — | `x6826c` profili, GICv3 (`0x0c000000` / `0x0c040000`), UART0 (`0x11002000`), doğrulanmış 16 girdilik `DRIVERS` tablosu ve bağımlılık sıralı `register()` |
| `src/mmio.rs` | MMIO, Zamanlayıcı & Kilit | — | — | 8/16/32-bit volatile erişim, `dsb/dmb/isb`, `cntpct_el0` tabanlı `Deadline`, IRQ-güvenli `IrqSpinLock` (zaman aşımında kilidi atlamaz), IEEE `crc32_ieee` |
| `src/wdt.rs` | `mtk-toprgu-wdt` | `0x10007000` | `188` | TOPRGU bekçi köpeği (`TOPRGU_MODE`, `TOPRGU_RESTART`), sıfırlama nedeni okuma ve `reboot-mode` (`+0x24`) desteği |
| `src/pwrap.rs` | `mtk-pwrap-mt6358` | `0x1000d000` | `226` | IRQ-güvenli `WACS2` PMIC sarmalayıcısı (`CMD`/`RDATA`/`VLDCLR`, eski `WFVLDCLR` kurtarma, zaman aşımı denetimi) ve `MT6358` PMIC erişimi |
| `src/i2c.rs` | `mtk-i2c5` / `mtk-i2c6` | `0x11016000` / `0x1100d000` | `179` / `180` | MT6768 I2C FIFO ana denetleyici (`clock-div=5`, open-drain, tekrarlı-başlatma salt-okunur okuma ve `SOFTRESET` kurtarma) |
| `src/usb.rs` | `mtk-usb20-musb` & `RT9471` | `0x11200000` / I2C5 `0x53` | `129` / `179` | Richtek `RT9471` ana şarj entegresi salt-okunur telemetri (`DEVINFO`, `ICSTAT`, `STAT0..3`, `VBUS_GD`, `CV/ICHG/AICR`), `IDDIG` (GPIO 41) ve MUSB durum okuyucu |
| `src/battery.rs` | `mt6358-battery` | PWRAP + I2C5 | — | `MT6358` AUXADC (`RQST0=0x110e`, Ch0 `BATADC`, Ch2 `VCDT`, Ch3 `BAT_TEMP`, Ch14 `VBIF`), `CHR_CON0` bit 5 (`RGS_CHRDET`), `FGADC` akım (`0x0d0a`/`0x0d8a`), 5 sıcaklıklı X6826C OCV+R eğrileri (`50°C..-10°C`) ve RT9471 şarj ayrımı |
| `src/display.rs` | `mtk-disp-ovl0` / `rdma0` / `dsi0` | `0x1400b000` | `262` | LK `videolfb` (`td4160_hdp_dsi_vdo_hlt_boe_zal1878`, `720x1640`, stride `2944`, `Bgrx`) sınır doğrulaması, OVL0 katman 0 alfa temizliği ve `DISP_MUTEX0` tetikleme |
| `src/backlight.rs` | `mtk-disp-pwm0` | `0x1100e000` | — | `CUST_BLS_PWM` (`led_mode=5`) arka ışık PWM denetleyicisi; `CON0`/`CON1` dönemini korur ve güvenli parlaklık ölçeklemesi sunar |
| `src/cpufreq.rs` | `mtk-apmixed-cpufreq` | `0x1000c000` | — | Açılışta salt-okunur `ARMPLL_LL` (A55), `ARMPLL_L` (A75) ve `VPROC11/12` telemetrisi (spekülatif PLL değişikliği yapmaz) |
| `src/emmc.rs` | `mtk-msdc0-emmc` | `0x11230000` | `132` | Salt-okunur `MSDC0` PIO sürücüsü: `CMD13`/`CMD7`/`CMD8` (`EXT_CSD` gerçek kapasite), `MSDC_CFG_RST` kurtarma, PMBR + GPT başlık/giriş CRC32 ve kapasite sınır doğrulamalı `super`/`userdata` blok aygıtları |
| `src/touch.rs` | `td4160-spi2-touch` | `0x11012000` | `177` | `SPI2` + GPIO (`RST=0, IRQ=1, CS=25..MI=28`), tek bit `CLK_IFR_SPI2` (`0x100010a8` bit 9), `td4160_romboot.bin` CRC32 (`0xeb776dc0`) doğrulamalı uçucu SRAM yükleyici, TCM `IDENTIFY` ve 10 parmaklı çoklu dokunma izleyicisi |
| `src/keys.rs` | `mtk-kpd-keys` | `0x10010000` | `105` | `KP_MEM1` (Ses Aç = bit 0, Ses Kıs = bit 1) ve `MT6358_TOPSTATUS` (`PWRKEY_DEB` = bit 1) sıçrama önlemeli girdi sürücüsü (yalnızca erişilebilirse kaydolur) |
| `src/scp.rs` | `mtk-scp-sensorhub` | `0x10500000` | `237` | MediaTek SCP Cortex-M4 sensör merkezi durum ve zaman aşımı sınırlı uyanma kilidi sürücüsü |
| `src/mali.rs` | `arm-mali-g52` | `0x13040000` | `306` | `VGPU`/`VSRAM_GPU` besleme doğrulamalı, hata durumunda geri almalı `SPM` 4 güç alanı + `TOPAXI` + `Mali-G52 MC2` donanım başlatıcısı |
| `src/connsys.rs` | `mtk-consys-mt6631` | `0x18002000` | `316` | Salt-okunur `CONNSYS`/`MT6631` ve `BTIF` (`0x1100c000`) güç alanı ve `VCN18/28/33` besleme denetleyicisi |

## Derleme ve Doğrulama

```sh
# 1. Host birim testleri (GPT CRC32, OCV, FGADC, RT9471, TCM multitouch, PWM, display bounds)
cargo test -p nc-dev-x6826c --lib

# 2. ARMv8-A X6826C Android imaj derlemesi (gerçek cihaz userdata_mb değeri ile)
NC_USERDATA_MB=<cihaz_userdata_MiB> bash build.sh armv8a android x6826c
```
