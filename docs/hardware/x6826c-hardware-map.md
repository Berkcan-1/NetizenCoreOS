# Infinix Hot 20 (X6826C / MediaTek MT6768) Hardware Evidence Map

This document records verified hardware parameters for the **Infinix Hot 20 (X6826C)** based on the **MediaTek MT6768 (Helio G85, ARMv8-A)** SoC, used to engineer native NetizenCore drivers in `aarch64crates/drivers/x6826c/`.

## Evidence Hierarchy & Confidence Classifications

1. **Primary X6826C Evidence**:
   - `aarch64crates/drivers/x6826c/stock/dtb` (FDT v17, 118,614 bytes, merged with `stock/dtbo` via `fdtoverlay`)
   - `aarch64crates/drivers/x6826c/stock/dtbo` (FDT v17, 95,855 bytes, X6826C board overlay)
   - `aarch64crates/drivers/x6826c/stock/hdl_firmware.img` (158,796 bytes, OmniVision/Synaptics HDL image)
   - `aarch64crates/drivers/x6826c/firmware/td4160_romboot.bin` (102,400 bytes, exact byte-for-byte `ROMBOOT_APP_CODE` section from `hdl_firmware.img`, CRC32 `0xeb776dc0`)
   - Stock X6826C bootloader decompilations (`preloader_x6826c_h69c27.bin.c`, `lk_rafw.bin.c`)
2. **SoC-Level Reference Material (MT6768 / Linux / HakiOS X6826B)**:
   - Used **only** where SoC silicon compatibility (`MT6768` / `MT6358`) is independently established against X6826C DTB/DTBO. Board-specific GPIOs, display panels, charger ICs, and USB connector topology are taken exclusively from X6826C primary evidence.
   - HakiOS (`hakios-mt6768-main`, MIT licensed for loader/platform logic) targets **X6826B**; board-level differences were audited before any algorithm reuse.

**Confidence Levels**:
- **VERIFIED**: Directly supported by X6826C stock DTB/DTBO, stock firmware/bootloader, or verified hardware documentation matching X6826C.
- **HIGH CONFIDENCE**: Consistent across MT6768/MT6358 Linux drivers and X6826C stock evidence.
- **PROVISIONAL**: Plausible from MT6768/MT6358 references, not yet confirmed on physical X6826C hardware.
- **UNKNOWN**: Insufficient evidence; safe read-only fallback or unavailable state is enforced.

> **Real-Hardware Test Status**: All entries below are currently **Not Yet Tested on Physical X6826C Hardware** unless explicitly noted; compilation, static checks, and host-side unit tests verify logic and protocol parsers without claiming real-device validation.

---

## 1. Core SoC, Memory, Interrupts & Boot Handoff

| Subsystem | Component / Compatible | Base Address & Size | GIC INTID (`SPI+32` / `PPI+16`) | Evidence Source | Confidence |
|---|---|---|---|---|---|
| CPU Cluster 0 | `arm,cortex-a55` (6 cores: `0x000..0x500`) | — | — | `stock/dtb` `/cpus` | VERIFIED |
| CPU Cluster 1 | `arm,cortex-a75` (2 cores: `0x600..0x700`) | — | — | `stock/dtb` `/cpus` | VERIFIED |
| RAM (DTB default) | `/memory` | `0x4000_0000` (`0x3e60_5000` base; LK patches full DRAM at boot) | — | `stock/dtb` `/memory` | VERIFIED |
| GICv3 | `arm,gic-v3` | Dist `0x0c00_0000` (`0x40000`), Redist `0x0c04_0000` (`0x200000`) | PPI 9 -> `25` | `stock/dtb` `/interrupt-controller` | VERIFIED |
| ARMv8 Timer | `arm,armv8-timer` | `clock-frequency = 13_000_000` (`0xc65d40`) | PPI 13,14,11,10 -> `29,30,27,26` | `stock/dtb` `/timer` | VERIFIED |
| PSCI | `arm,psci-1.0` | Conduit: `smc` | — | `stock/dtb` `/psci` | VERIFIED |
| UART0 | `mediatek,mt6577-uart` | `0x1100_2000` (`0x1000`), 32-bit `reg_shift=2` | SPI `0x70` (112) -> `144` | `stock/dtb` `/serial@11002000` | VERIFIED |
| TOPCKGEN | `mediatek,topckgen` | `0x1000_0000` (`0x1000`) | — | `stock/dtb` `/topckgen@10000000` | VERIFIED |
| INFRACFG_AO | `mediatek,infracfg_ao` | `0x1000_1000` (`0x1000`) | SPI `0x47` (71) -> `103` | `stock/dtb` `/infracfg_ao@10001000` | VERIFIED |
| GPIO / Pinctrl | `mediatek,mt6768-pinctrl` | `0x1000_5000` (`0x1000`, 186 pins `0..0xb9`) | EINT SPI `0xba` (186) -> `218` | `stock/dtb` `/pinctrl` | VERIFIED |
| SPM (Sleep/Power) | `mediatek,sleep` | `0x1000_6000` (`0x1000`) | SPI `0xc3` (195) -> `227` | `stock/dtb` `/sleep@10006000` | VERIFIED |
| TOPRGU Watchdog | `mediatek,mt6768-wdt` | `0x1000_7000` (`0x1000`); `reboot-mode` @ `+0x24` | SPI `0x9c` (156) -> `188` | `stock/dtb` `/toprgu@10007000` | VERIFIED |
| APMIXEDSYS | `mediatek,apmixed` | `0x1000_c000` (`0x1000`) | — | `stock/dtb` `/apmixed@1000c000` | VERIFIED |

**Reserved Memory Regions** (`stock/dtb` `/reserved-memory`):
- `reserve-memory-sspm_share`: `0x510000` within `0x4000_0000..0xa000_0000` (`no-map`)
- `reserve-memory-scp_share`: `0xd00000` within `0x4000_0000..0x9000_0000` (`no-map`)
- `consys-reserve-memory`: `0x400000` within `0x4000_0000..0xc000_0000` (`no-map`)
- `wifi-reserve-memory`: `0x300000` within `0x4000_0000..0xc000_0000` (`no-map`)

---

## 2. Storage — eMMC (`MSDC0`)

| Parameter | Value | Evidence Source | Confidence |
|---|---|---|---|
| Controller | `mediatek,mt6768-mmc` (`mmc@11230000`) | `stock/dtb` `/mmc@11230000` | VERIFIED |
| Register Regions | MSDC0 `0x1123_0000` (`0x10000`), MSDC0_TOP `0x11cd_0000` (`0x1000`) | `stock/dtb` `/mmc@11230000` | VERIFIED |
| GIC INTID | `132` (SPI `0x64` = 100, active-low level `0x08`) | `stock/dtb` `/mmc@11230000` | VERIFIED |
| Bus Capabilities | 8-bit, up to 200 MHz (`0xbebc200`), DDR 1.8V, HS200, HS400 (`hs400-ds-delay = 0x12814`), non-removable | `stock/dtb` `/mmc@11230000` | VERIFIED |
| Power Supply | `vmmc-supply` -> `MT6358` `ldo_vemc` | `stock/dtb` phandle `0x3c` | VERIFIED |
| Clocks | `topckgen` `0x3c` (`source`), `infracfg_ao` `0x1c` (`hclk`), `0x4c` (`source_cg`), `0x44` (`crypto_clk`) | `stock/dtb` `/mmc@11230000` | VERIFIED |
| Bootloader Handoff | Preloader/LK powers eMMC (`ldo_vemc`), configures 8-bit bus/clocks, assigns RCA (`0x0001`), and leaves card in transfer state | `lk_rafw.bin.c`, HakiOS `msdc.h` / `redwood_emmc.c` | HIGH CONFIDENCE |
| Safety Policy | **Strictly READ-ONLY** (`CMD13`, `CMD7`, `CMD8` EXT_CSD, `CMD17`, `CMD18`, `CMD12` only; zero write commands) | Engineering safety mandate | VERIFIED |

---

## 3. PMIC Wrapper (`PWRAP`) & PMIC (`MT6358`)

| Parameter | Value | Evidence Source | Confidence |
|---|---|---|---|
| Controller | `mediatek,mt6768-pwrap` (`pwrap@1000d000`) | `stock/dtb` `/pwrap@1000d000` | VERIFIED |
| Register Base & Size | `0x1000_d000` (`0x1000`) | `stock/dtb` `/pwrap@1000d000` | VERIFIED |
| GIC INTID | `226` (SPI `0xc2` = 194, high-level `0x04`) | `stock/dtb` `/pwrap@1000d000` | VERIFIED |
| WACS2 Registers | `WACS2_EN = 0x09c`, `CMD = 0xc20`, `RDATA = 0xc24`, `VLDCLR = 0xc28`; FSM `(RDATA>>16)&7` (`IDLE=0`, `WFVLDCLR=6`) | HakiOS `mtkpwrap.h`, Linux `mtk-pmic-wrap.c` | HIGH CONFIDENCE |
| PMIC Child | `mediatek,mt6358-pmic` (`HWCID = 0x0008`, `TOPSTATUS = 0x0028`: `PWRKEY_DEB` bit 1, `HOMEKEY_DEB` bit 3) | `stock/dtb`, `lk_rafw.bin.c` L26259/26277 | VERIFIED |
| Buck/LDO Telemetry | `VPROC11_DBG0 = 0x139e`, `VPROC12_DBG0 = 0x141e`, `VGPU_DBG0 = 0x151e`, `VGPU_DBG1 = 0x1520`, `VSRAM_GPU_DBG0 = 0x1be2`, `VSRAM_GPU_DBG1 = 0x1be4`, `VCN18/28/33` (`0x1a08`/`0x1a26`/`0x19d0`/`0x19d8`) | HakiOS `mtkpwrap.h`, `redwood_mtkkeys.c`, `redwood_connsys.c` | HIGH CONFIDENCE |
| Bootloader Handoff | Preloader initializes PWRAP & WACS2; kernel validates `HWCID` and clears stale `WFVLDCLR` | `lk_rafw.bin.c`, HakiOS `mtkpwrap.h` | VERIFIED |

---

## 4. Battery, Fuel Gauge (`MT6358`) & Charging (`RT9471` + `MT6358`)

| Parameter | Value | Evidence Source | Confidence |
|---|---|---|---|
| Battery Profile Node | `mediatek,bat_gm30` (`/battery`), 5000 mAh HV cell (`max_mah = 5131.9 mAh`, `CV = 4.45 V`) | `stock/dtb` `/battery` | VERIFIED |
| Temperature OCV Curves | `T0 = 50°C`, `T1 = 25°C`, `T2 = 10°C`, `T3 = 0°C`, `T4 = -10°C` (100 rows each with OCV & internal resistance) | `stock/dtb` `/battery` `battery0_profile_t0..t4` | VERIFIED |
| Sense Resistor / Tune | `R_FG_VALUE = 5` mOhm (`0x05`), `CAR_TUNE_VALUE = 100` (`0x64`), `FG_METER_RESISTANCE = 75` (`0x4b`) | `stock/dtb` `/battery` | VERIFIED |
| `MT6358_CHR_CON0` | `0x0a88`, **`RGS_CHRDET` = bit 5 (`1 << 5`)** (fixed from erroneous bit 1) | `preloader_x6826c_h69c27.bin.c` L35962, `lk_rafw.bin.c` L26227 | VERIFIED |
| `MT6358` AUXADC | `AUXADC_ADC0 = 0x1088` (Ch0 `BATADC`, 15-bit `0x7fff`, `3:1` -> `raw*5400>>15`), **`AUXADC_RQST0 = 0x110e`** (fixed from MT6359 `0x1108`!), Ch2 `VCDT` (`0x108c`), Ch3 `BAT_TEMP` (`0x108e`, `2:1`), Ch14 `VBIF` (`0x10a4`, `2:1`) | `stock/dtb` `/mt635x-auxadc`, Linux `mt6359-auxadc.c` (`mt6358_auxadc_regs`) | VERIFIED / HIGH CONFIDENCE |
| `MT6358` Fuel Gauge | `FGADC_CON1 = 0x0d0a` (`SW_READ_PRE = bit 0`, `SW_CLEAR = bit 3`, `LATCHDATA_ST = bit 15`), `FGADC_CUR_CON0 = 0x0d8a` (signed 16-bit current, `UNIT_FGCURRENT = 381470`) | `lk_rafw.bin.c` L71051–71124, MT6358 register map | VERIFIED / HIGH CONFIDENCE |
| Battery NTC Pull-Up & R-T Table | `BAT_TEMP` ADC channel (`Ch 3`, `2:1`) verified, but board-specific `RBAT_PULL_UP_R` & NTC R-to-T table are not in DTB/DTBO | `stock/dtb` / `stock/dtbo` audit | **UNKNOWN** (raw mV sampled for open/short fault check; `temp_deci_c` reports `i16::MIN` unavailable rather than fabricating temperature) |
| Primary Switching Charger IC | **Richtek RT9471** (`richtek,swchg`, subnode `rt9471`) on `i2c5@11016000` addr **`0x53`**, INT GPIO 9; `DEVINFO = 0x0B` (`dev_id` in `{0x09,0x0A,0x0D,0x0E}`), `ICSTAT = 0x0F`, `STAT0..3 = 0x10..0x13` (`VBUS_GD = STAT0 bit 7`), `CV = 0x07`, `ICHG = 0x08`, `AICR = 0x03`, `MIVR = 0x04` | `stock/dtbo` `/i2c5@11016000/rt-swchg@53`, `lk_rafw.bin.c` L71490–71810 | VERIFIED |
| Safety Policy | **Strictly READ-ONLY** battery and charger monitoring; zero writes to RT9471 or MT6358 charger-control registers | Engineering safety mandate | VERIFIED |

---

## 5. USB, Connector & Power Input

| Parameter | Value | Evidence Source | Confidence |
|---|---|---|---|
| USB 2.0 Controller | `mediatek,mt6768-usb20` (`usb0@11200000`), MUSB HDRC (`0x1120_0000` `0x10000`, `0x11cc_0000` `0x10000`), `dr_mode = "otg"` | `stock/dtb` `/usb0@11200000`, `lk_rafw.bin.c` L66092 | VERIFIED |
| USB GIC INTID | `129` (SPI `0x61` = 97, active-low level `0x08`) | `stock/dtb` `/usb0@11200000` | VERIFIED |
| USB PHY | `mediatek,generic-tphy-v1` (`0x11cc_0000` `0x800`, port `0x11cc_0800` `0x100`), `usb1p_sif` (`0x1121_0000`) | `stock/dtb` `/usb-phy@11210000` | VERIFIED |
| Extcon / Connector Role | `/extcon_usb` (`mediatek,extcon-usb`): `fake_typec_otg_detect`, `mediatek,bypss-typec-sink = 1`, **IDDIG = GPIO 41 (`0x29`)**, OTG_DCP = GPIO 40 (`0x28`), `AUX_CHG` = AP AUXADC Ch 4 (`0x1100_1000`) | `stock/dtbo` `/extcon_usb` | VERIFIED |
| I2C5 Bus (Charger) | `i2c5@11016000` (`0x1101_6000`, DMA `0x1100_0580`), GIC INTID `179` (SPI `0x93` = 147), SCL=GPIO 48 (`0x30`), SDA=GPIO 49 (`0x31`), `clock-div = 5` (`0x404`), open-drain | `stock/dtb` `/i2c5@11016000`, `lk_rafw.bin.c` L9132/9586 | VERIFIED |
| Type-C / PD | X6826C DTBO configures `fake_typec_otg_detect` and `bypss-typec-sink = 1` (not active USB-PD negotiation); `usb_type_c@4e` exists on I2C5 as optional stub | `stock/dtbo` `/extcon_usb` | VERIFIED |
| Safety Policy | Read-only VBUS, charger IC, MUSB `DEVCTL`/`POWER`, and IDDIG GPIO 41 role monitoring; OTG VBUS boost is **never** enabled | Engineering safety mandate | VERIFIED |

---

## 6. Display & Backlight

| Parameter | Value | Evidence Source | Confidence |
|---|---|---|---|
| Active Panel | `td4160_hdp_dsi_vdo_hlt_boe_zal1878` (BOE TD4160 TDDI, MIPI-DSI video mode, 60 Hz `fps = 0x1770`) | `stock/dtb` `/chosen` `atag,videolfb-lcmname` | VERIFIED |
| Resolution & Framebuffer | `720 x 1640`, 32 bpp (`Bgrx`), LK stride `2944` bytes, default `lfb_base = 0x787b_0000`, `vramSize = 0x0105_0000` (16.3 MiB) | `stock/dtb` `/chosen`, HakiOS `mtkloader.c` | VERIFIED |
| Display Pipeline Blocks | `disp_mutex0@14001000` (INTID `255`), `disp_ovl0@1400b000` (INTID `262`), `disp_ovl0_2l@1400c000` (INTID `274`), `disp_rdma0@1400d000` (INTID `263`), `dsi0@14014000` (INTID `270`) | `stock/dtb` | VERIFIED |
| Panel GPIOs | `LCM_RST` = GPIO 45 (`0x2d`), `DSI_TE` = GPIO 44 (`0x2c` func 1), `LCD_BIAS_ENN` = GPIO 152 (`0x98`), `LCD_BIAS_ENP` = GPIO 153 (`0x99`) | `stock/dtbo` `/mtkfb@0`, `/pinctrl` | VERIFIED |
| LCD Bias IC | `i2c_lcd_bias@3e` on `i2c6@1100d000` (INTID `180`, `0x1100_d000`) | `stock/dtbo` `/i2c6@1100d000/i2c_lcd_bias@3e` | VERIFIED |
| Backlight PWM | `disp_pwm@1100e000` (`0x1100_e000`), `/odm/led@6` `mediatek,lcd-backlight`: `led_mode = 5` (`CUST_BLS_PWM`), `pwm_config = <1 0 0 0 0>` | `stock/dtb` `/disp_pwm@1100e000`, `stock/dtbo` `/odm/led@6` | VERIFIED |
| Bootloader Handoff | LK initializes DSI0/RDMA0/OVL0/LCD bias/panel; NetizenCore preserves active video-mode pipeline, sanitizes OVL0 layer 0 alpha bit (`L0_CON` bit 8) and hides LK layer 3 overlay (`SRC_CON & 1`), then commits `DISP_MUTEX0` | `stock/dtb`, HakiOS `mtkloader.c` | VERIFIED |
| Native Panel Init Table | Exact DCS vendor init command table for `td4160_hdp_dsi_vdo_hlt_boe_zal1878` is not extracted | Audit | **UNKNOWN** (LK handoff preserved intentionally for safety) |

---

## 7. Touchscreen (`TD4160` over `SPI2`)

| Parameter | Value | Evidence Source | Confidence |
|---|---|---|---|
| Controller | OmniVision/Synaptics `TD4160` TCM (`mediatek,cap_touch_2` on `spi2@11012000`) | `stock/dtbo` `/spi2@11012000/touch@0`, `stock/hdl_firmware.img` | VERIFIED |
| SPI2 Controller | `mediatek,mt6765-spi` at `0x1101_2000` (`0x1000`), GIC INTID `177` (SPI `0x91` = 145), `pad-select = 0`, Mode 3 (`CPOL=1, CPHA=1`), max `8 MHz` (`0x7a1200`) | `stock/dtb` + `stock/dtbo` `/spi2@11012000` | VERIFIED |
| SPI2 Clock Gate | `TOPCKGEN CLK_CFG_2` (`0x60/0x68`, `spi_sel` bits `25:24`, PDN bit 31); `INFRACFG_AO IFR4_CG_CLR` (`0x1000_10a8`) **bit 9** (`CLK_IFR_SPI2`) | `stock/dtb`, Linux `clk-mt6765.c` / `clk-mt6768.c` | HIGH CONFIDENCE |
| SPI2 Pins (MT6768 pad 0) | GPIO 25 (CSB), GPIO 26 (MO), GPIO 27 (CLK), GPIO 28 (MI), all Function 2 (`GPIO_MODE3 @ 0x1000_5330`) | MT6768 pinmux table, HakiOS `mtkspi.h` | HIGH CONFIDENCE |
| Touch Reset & IRQ | `RST` = **GPIO 0** (`pinmux = 0x00`, active low, `reset-active-ms = 20`, `reset-delay-ms = 200`); `IRQ` = **GPIO 1** (`pinmux = 0x100`, active low, EINT 1 falling edge) | `stock/dtbo` `/touch` + `/pinctrl` | VERIFIED |
| Power Rail | `vtouch-supply` -> `MT6358` `ldo_vldo28` | `stock/dtbo` `/touch` phandle `0xa2` | VERIFIED |
| Firmware (`ROMBOOT`) | `firmware/td4160_romboot.bin` (`102,400` bytes, IEEE CRC32 `0xeb776dc0`) matches `ROMBOOT_APP_CODE` in `stock/hdl_firmware.img` (offset `0x168`); **volatile SRAM host-download only** (TD4160 has no flash) | Binary & CRC32 verification | VERIFIED |
| Protocol Timing | `omnivision,ubl-byte-delay-us = 20` (`0x14`), `byte-delay-us = 0`; 88-bit per-finger TCM touch report (up to 10 contacts) | `stock/dtbo`, HakiOS `tcm.h` / `redwood_tcm.c` | VERIFIED / HIGH CONFIDENCE |

---

## 8. Keys, Watchdog, CPU Clocks, SCP, GPU & Connectivity

| Subsystem | Compatible / Node | Base Address | GIC INTID | Key Parameters & Safety Notes | Confidence |
|---|---|---|---|---|---|
| Keypad & Power Key | `mediatek,kp` (`kp@10010000`) + `MT6358` | `0x1001_0000` | `105` (SPI `0x49`) | `KP_MEM1 (+0x04)`: bit 0 = VolUp (`0x73`), bit 1 = VolDown (`0x72`); `MT6358 TOPSTATUS (0x28)`: bit 1 = Power (active-low), bit 3 = Home | VERIFIED |
| Watchdog | `mediatek,mt6768-wdt` (`toprgu@10007000`) | `0x1000_7000` | `188` (SPI `0x9c`) | `MODE (+0x00, key 0x22000000)`, `RESTART (+0x08, 0x1971)`, `STATUS (+0x0c)`, `SWRST (+0x14, 0x1209)`, `REBOOT_MODE (+0x24)` | VERIFIED |
| CPU Clocks | `mediatek,apmixed` (`apmixed@1000c000`) | `0x1000_c000` | — | `ARMPLL_L_CON1 (+0x20c)` (A75), `ARMPLL_LL_CON1 (+0x21c)` (A55); **read-only at boot** to preserve stable LK OPP | VERIFIED / HIGH CONFIDENCE |
| SCP Sensor Hub | `mediatek,scp` (`scp@10500000`) | SRAM `0x1050_0000`, CFG `0x105c_0000`, CLK `0x105c_4000` | `237` (SPI `0xcd`) | Cortex-M4 tinysys; read-only status/PC/sleep/IPI probe; no fabricated sensor readings | VERIFIED / HIGH CONFIDENCE |
| GPU (Mali-G52 MC2) | `mediatek,mali` (`mali@13040000`) | `0x1304_0000` (`0x4000`), MFGCFG `0x1300_0000` | `306..310` (SPI `0x112..0x116`) | SPM 4 domains (`MFG_ASYNC/MFG/CORE0/CORE1`), TOPAXI bus protect, `VGPU`/`VSRAM_GPU` rail check, `syspll_d3` 364 MHz; hardware bring-up & ID only (no 3D userspace driver yet) | VERIFIED / HIGH CONFIDENCE |
| CONNSYS (Wi-Fi/BT) | `mediatek,mt6768-consys` (`consys@18002000`) + `btif@1100c000` | MCU `0x1800_2000`, MISC `0x180b_1000`, BTIF `0x1100_c000` | `316, 110, 319`; BTIF `165, 147, 173` | Strictly read-only SPM + `VCN18/28/33` probe; never reads `0x1800_2000` while SPM domains are off (prevents bus hang); `has_hw_bt = false` until firmware integration | VERIFIED / HIGH CONFIDENCE |
