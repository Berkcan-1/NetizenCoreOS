# NetizenCore — 50-Feature Operating System Maturation Tracking Matrix

**Last Updated**: 2026-11-21  
**Baseline Verified**: ARM64 `qemu-virt` (`2,121,728` B Image, 3.5s QEMU boot, `33280/33280` PMM frames), `x86_64` ISO (`5,529,600` B kernel), SPARK Ada 7 core units **425/425 (100%) proved** + 35/35 KAT tests.

**Allowed Status Values**: `NOT STARTED` | `IN PROGRESS` | `IMPLEMENTED` | `TESTED` | `VERIFIED` | `BLOCKED`

---

## Category A — Kernel and Memory Management

| # | Feature | Priority | Status | Relevant Source Files | Dependencies | Implementation Summary / Verification Evidence / Remaining Blockers |
|---:|---|---|---|---|---|---|
| 1 | Symmetric Multiprocessing (SMP) | MEDIUM | `VERIFIED` | `aarch64crates/hardware/nc-arch-aarch64/src/psci.rs`, `gic.rs`, `mmu.rs`, `aarch64kernel/src/smp.rs`, `entry.rs` | Phase 0-1 | Implemented PSCI v1.1 `CPU_ON`/`CPU_OFF`/`CPU_SUSPEND`/`AFFINITY_INFO`, secondary CPU EL2→EL1 trampoline, per-CPU 16 KiB stacks, secondary MMU W^X + GICv3 redistributor init, SGI IPIs, Inner-Shareable TLB shootdowns (`tlbi vae1is`/`vmalle1is`), and SMP-safe PMM spinlock. Verified in QEMU with `--smp 4`: `smp: 4 CPU online (psci v1.1, mask=0xf)` + 0 PMM leaks. Per-CPU run-queue work-stealing and physical X6826C SMP remain future increments. |
| 2 | Priority-Aware Scheduler | MEDIUM | `VERIFIED` | `aarch64kernel/src/task.rs` | F1 | Replaced basic round-robin with bounded 4-tier priority scheduler (`System`, `Foreground`, `Normal`, `Background`) with starvation-preventing `wait_age` promotion (`AGE_PROMOTION_THRESHOLD = 8`) and security-tier separation (user tasks cannot self-elevate to `System`). Verified in ARM64 QEMU UP & 4-core SMP boots. |
| 3 | Demand Paging and Copy-on-Write | MEDIUM | `IMPLEMENTED` | `aarch64kernel/src/vm.rs`, `aarch64kernel/src/task.rs` | F1 | Added SMP-safe per-frame reference counting (`REFCOUNTS`), software `COW` PTE bit (bit 56), W^X-preserving `handle_page_fault()` for COW write-fault resolution (in-place upgrade when `rc==1`, copy+refcount-decrement+IS-TLB-shootdown when `rc>1`) and demand-zero lazy allocation within valid user stack/heap bounds (rejecting guard page & invalid addresses). Wired into `task::on_fault` (`EC=0x24`). |
| 4 | ASLR and KASLR | NEAR | `IMPLEMENTED` | `aarch64kernel/src/vm.rs`, `aarch64crates/hardware/nc-arch-aarch64/src/rand.rs` | F15 | Implemented `compute_aslr_slide_pages()` backed strictly by hardware CSPRNG (`RNDR`), explicitly returning degraded `(0, false)` when hardware TRNG is unavailable (never using predictable timestamps). PIE ELF relocation & KASLR bootloader relocation remain future work. |
| 5 | Shared Memory and mmap | MEDIUM | `TESTED` | `crates/services/nc-mem/src/shm.rs` | F3, F12 | Implemented bounded capability-controlled `ShmRegistry` with creator ownership, explicit participant grants, RO/RW permission enforcement, and revocation. Verified via `cargo test -p nc-mem --lib`. Syscall `SYS_MMAP`/`SYS_SHM` wire binding pending. |
| 6 | Threads, TLS, and Futex Support | MEDIUM | `TESTED` | `crates/services/nc-mem/src/futex.rs`, `aarch64kernel/src/task.rs` | F1, F2 | Implemented per-task ARM64 `TPIDR_EL0` TLS register save/restore on context switch and bounded address-space-scoped `FutexTable` (wait/wake/timeout/cancel + strict cross-process isolation). Verified via `cargo test -p nc-mem --lib` and QEMU boot. Multi-thread group spawn syscalls pending. |
| 7 | Swap and ZRAM | MEDIUM | `TESTED` | `crates/services/nc-mem/src/zram.rs` | F3, F9 | Implemented bounded 256 KiB in-memory `ZramPool` with byte-run page compression, incompressible-page rejection, CRC32 corruption detection & output zeroization, pool compaction, and memory accounting. Verified via `cargo test -p nc-mem --lib`. |
| 8 | Kernel Heap Hardening | NEAR | `VERIFIED` | `crates/services/nc-mem/src/heap.rs`, `aarch64kernel/src/boot.rs` | Phase 0 | Added 8-byte XOR-masked tail canary redzone (overflow detection on `dealloc`) and volatile payload scrubbing on deallocation (preventing sensitive buffer leaks) atop existing IRQ-safe SMP spinlock, header guard, and double/invalid-free counting. Verified via `cargo test -p nc-mem --lib` (9/9 pass) and QEMU boot selftest. |

---

## Category B — Security

| # | Feature | Priority | Status | Relevant Source Files | Dependencies | Implementation Summary / Verification Evidence / Remaining Blockers |
|---:|---|---|---|---|---|---|
| 9 | Integrate Full-Data Encryption with Storage | NEAR | `TESTED` | `spark/security/src/nc_security_encryption.ads/.adb`, `nc_security_c_api.ads/.adb`, `crates/foundation/nc-spark-sec/src/lib.rs`, `crates/services/nc-fs/src/fde_blk.rs` | F15, F18, F21 | Exported `nc_sec_fde_encrypt_block`/`decrypt_block` from SPARK Ada and implemented `EncryptedBlockDevice` (`nc_fs::fde_blk`) with fail-closed lock state, ChaCha20-Poly1305 AEAD, per-block `Block_Id` AAD, and tamper zeroization. Verified via `cargo test -p nc-fs --lib` (9/9 pass). Live `/data` mount boot-flow integration gated on hardware TRNG / provisioning UX. |
| 10 | Logical Partition Checksum Validation | NEAR | `VERIFIED` | `crates/services/nc-fs/src/lp.rs`, `aarch64kernel/src/lp.rs`, `aarch64crates/tools/mk-super.py` | F18 | Implemented full AOSP `liblp` SHA-256 verification for geometry, header, and tables (using SPARK Ada SHA-256) plus bounds/overflow/alignment checks. Verified by 7/7 host unit tests (`cargo test -p nc-fs --lib`) AND live QEMU ARM64 boot mounting real `super.img` (`qemu_boot_test.py`). |
| 11 | Verified Boot Chain | MEDIUM | `TESTED` | `crates/runtime/nc-pkg/src/verified_boot.rs` | F10, F18 | Implemented Ed25519 + SPARK Ada SHA-256 `BootManifest` verifier (`NCVB v1`) enforcing production vs dev modes, key revocation override, and monotonic rollback index rejection. Verified via `cargo test -p nc-pkg --lib` (including 1024-round parser fuzz). Hardware RPMB rollback counter remains target-dependent. |
| 12 | Complete System-Call Authorization Audit | NEAR | `VERIFIED` | `aarch64kernel/src/syscall.rs`, `aarch64kernel/src/host.rs`, `docs/security/SYSCALL_SECURITY_MATRIX.md` | F18 | Audited all ARM64 syscalls; documented complete security matrix; fixed `SYS_PKG_INSTALL` to enforce SPARK `nc_sec_ds_authorize(caller_id, PACKAGE_INSTALL)` and restricted `SYS_APP_LAUNCH` WM actions `2..=5` strictly to `home_slot`. Verified by ARM64 build + QEMU boot test. |
| 13 | Move DysonSphere to Isolated EL0 Security Server | MEDIUM | `TESTED` | `crates/foundation/nc-abi/src/dysonsphere.rs`, `spark/security/` | F12, F47 | Implemented bounded EL0 DysonSphere IPC wire protocol (`DsAuthRequest`/`DsAuthResponse`, `NCDS v1`) and fail-closed `ReferenceMonitor` cache with policy-generation invalidation (always denies on offline/crash/ABI mismatch/spoof). Verified via `cargo test -p nc-abi --lib`. Live EL0 daemon spawn pending. |
| 14 | EL2 Mini Security Monitor | LONG TERM | `BLOCKED` | `aarch64crates/hardware/nc-arch-aarch64/src/cpu.rs`, `aarch64kernel/src/boot.rs` | Hardware EL2 | Runtime EL probe implemented (`probe_security_features()`); QEMU virt enters kernel at EL1 (`el2_monitor=unsupported-at-el1`). Stage-2 EL2 monitor blocked on EL2 boot environment. |
| 15 | CSPRNG and Entropy Pool | NEAR | `IMPLEMENTED` | `aarch64crates/hardware/nc-arch-aarch64/src/rand.rs` | None | Upgraded `HardwareRng` to RFC 8439 ChaCha20 DRBG seeded/reseeded from ARMv8.5 `RNDR` with fast key erasure, SMP spinlock, and strict fail-closed `RngError::EntropyUnavailable` (never falls back to xorshift for crypto). QEMU Cortex-A72 lacks `RNDR` and cleanly reports unavailable. |
| 16 | Application Key Store | MEDIUM | `TESTED` | `crates/foundation/nc-crypto/src/keystore.rs` | F9, F12, F15 | Implemented bounded per-app isolated `AppKeyStore` backed by SPARK Ada ChaCha20-Poly1305 AEAD, HMAC-SHA256, and `secure_zero` (handle-only API, cross-app denial, lock-state gating, rotation, revocation). Verified via `cargo test -p nc-crypto --lib`. |
| 17 | Application Sandbox | MEDIUM | `IN PROGRESS` | `aarch64crates/runtime/nc-fs-policy/`, `spark/security/src/nc_security_filesystem.adb`, `aarch64kernel/src/vm.rs` | F12, F18 | EL0 per-app address spaces and SPARK FS sandbox policy verified (8/8 `nc-fs-policy` tests + 100% SPARK proof); per-app FD/resource quotas & namespace views pending. |
| 18 | Complete Remaining SPARK Proof Obligations | NEAR | `VERIFIED` | `spark/security/src/*.ads/.adb`, `spark/security/proofs/PROOF_REPORT.md` | None | Eliminated all remaining obligations across the 7 core SPARK units: **425/425 checks (100.0%) formally proved, 0 unproved, 0 justified** with GNATprove 16.1.0 (`--level=2`) + 35/35 KAT tests passing. Poly1305 nonlinear limb arithmetic explicitly scoped outside SMT proof. |
| 19 | ARMv8.x Hardware Security Hardening | MEDIUM | `VERIFIED` | `aarch64crates/hardware/nc-arch-aarch64/src/cpu.rs`, `aarch64kernel/src/boot.rs` | Hardware | Implemented full runtime ID-register probing (`ID_AA64MMFR1/ISAR0/ISAR1/PFR0/PFR1`) for PAN, PAC, BTI, MTE, and RNDR with safe conditional PAN activation and explicit unsupported fallback. Verified in QEMU boot (`hwsec:` telemetry). |
| 20 | Continuous Fuzz Testing | MEDIUM | `VERIFIED` | `crates/services/nc-fs/src/lp.rs`, `crates/runtime/nc-pkg/src/verified_boot.rs`, `crates/runtime/nc-elf/`, `crates/foundation/nc-abi/` | F48 | Deterministic mutation fuzz suites execute in every CI run (`run_ci.sh`) across ELF loader, LP metadata parser (256 rounds), Verified Boot manifest parser (1024 rounds), ABI draw/VFS parsers with zero panics. |

---

## Category C — Storage and Filesystem

| # | Feature | Priority | Status | Relevant Source Files | Dependencies | Implementation Summary / Verification Evidence / Remaining Blockers |
|---:|---|---|---|---|---|---|
| 21 | Fully Functional `/data` Write Path | NEAR | `VERIFIED` | `aarch64kernel/src/storage.rs`, `aarch64kernel/src/blk.rs`, `crates/services/nc-fs/src/exfat.rs` | None | VirtIO-blk writable + flush + exFAT create/write/modify/delete/sync verified in QEMU boot (`yazma sinamasi tamam, flush tamam`). Physical X6826C eMMC write path remains read-only until hardware write/cache-flush testing. |
| 22 | Journaling or Dedicated NetizenCore Filesystem | MEDIUM | `TESTED` | `crates/services/nc-fs/src/journal.rs` | F21 | Implemented bounded SHA-256-sealed write-ahead metadata journal (`JournalTx`, `NCJL`) discarding uncommitted/torn/corrupted transactions on recovery and replaying committed blocks atomically. Verified via `cargo test -p nc-fs --lib`. |
| 23 | A/B System Updates and Automatic Rollback | MEDIUM | `TESTED` | `crates/services/nc-fs/src/ab_slot.rs` | F10, F11 | Implemented redundant SHA-256-protected A/B `BootControl` (`NCAB v1`) with monotonic generation counters, inactive-slot staging, rollback index protection, boot-attempt countdown, and automatic fallback to healthy slot. Verified via `cargo test -p nc-fs --lib`. |
| 24 | Signed OTA Updates | MEDIUM | `IN PROGRESS` | `crates/runtime/nc-pkg/src/verified_boot.rs`, `crates/services/nc-fs/src/ab_slot.rs` | F11, F23, F31 | Cryptographic manifest verification (F11) and inactive A/B slot staging (F23) tested; HTTPS download resumption awaits ARM64 `nc-net` integration. |
| 25 | Snapshots and Backup | MEDIUM | `NOT STARTED` | `crates/services/nc-fs/` | F9, F22 | Encrypted snapshot & restore framework pending. |
| 26 | USB Hotplug and Safe Removal | NEAR | `IN PROGRESS` | `aarch64kernel/src/storage.rs`, `aarch64crates/tools/qemu-aarch64.sh` | F21 | Safe read-only-by-default USB storage mounting & eject exist via virtio-blk bridge in QEMU; dynamic USB host-controller hotplug pending. |
| 27 | Storage Quotas, Trash, and TRIM | NEAR | `TESTED` | `crates/services/nc-fs/src/quota.rs` | F21 | Implemented per-app `QuotaTable`, owner-isolated `TrashRegistry` with expiration purge, and capability-gated `validate_discard_range()` TRIM validator (never advertises unsupported TRIM). Verified via `cargo test -p nc-fs --lib`. |

---

## Category D — Networking

| # | Feature | Priority | Status | Relevant Source Files | Dependencies | Implementation Summary / Verification Evidence / Remaining Blockers |
|---:|---|---|---|---|---|---|
| 28 | Integrate TCP/IP Stack into AArch64 | MEDIUM | `VERIFIED` | `crates/foundation/nc-hal/src/net.rs`, `aarch64kernel/src/net.rs`, `aarch64kernel/src/host.rs` | None | Added portable `NetDevice` HAL trait and real ARM64 QEMU `virtio-net` MMIO RX/TX driver (`0xa003600`, v1/v2 virtqueues, DMA barriers). Verified live packet TX/RX & DHCP in QEMU (`3.31s`, `10.0.2.15`). Full `nc-net` TCP/TLS socket bridge to EL0 `SYS_NET_CALL` remains next increment. |
| 29 | Real Wi-Fi Driver | LONG TERM | `IN PROGRESS` | `crates/foundation/nc-hal/src/net.rs`, `aarch64kernel/src/net.rs`, `aarch64kernel/src/host.rs`, `aarch64crates/apps/settings/nc-settings-v8a/src/pages.rs` | F28, Hardware | Implemented complete `WifiAdapter` HAL, `WifiScanEntry` / `WifiAdapterState`, kernel Wi-Fi detection & scan registry (`OP_WL_POLL`/`OP_WL_CMD`), and Settings UI Wi-Fi scan/status display. Honestly reports `HardwareUnavailable` on QEMU (`virtio-net`) with zero fabricated SSIDs. Physical X6826C CONNSYS driver deferred. |
| 30 | Bluetooth Stack | LONG TERM | `BLOCKED` | `aarch64crates/drivers/x6826c/src/connsys.rs` | F29, Hardware | Blocked on X6826C CONNSYS BT transport/firmware and physical hardware. |
| 31 | TLS, HTTPS, and Certificate Store | MEDIUM | `IN PROGRESS` | `crates/services/nc-net/src/tls.rs`, `crates/services/nc-net/src/tls13.rs`, `crates/services/nc-net/src/http.rs` | F28 | TLS 1.3 + `webpki-roots` certificate store + HTTP exist in `nc-net` for x86_64; wiring over ARM64 `virtio-net` TCP socket pending. |
| 32 | Complete DNS and DHCP | NEAR | `VERIFIED` | `aarch64kernel/src/net.rs`, `crates/services/nc-net/src/dhcp.rs`, `dns.rs` | F28 | Implemented real ARM64 DHCP Discover→Offer→Request→Ack with randomized XID, parsing subnet mask, gateway, DNS, and lease duration. Verified live in QEMU (`[ag] dhcp: ip=10.0.2.15 gw=10.0.2.2 dns=10.0.2.3 lease=86400s`) and surfaced in Settings UI. |
| 33 | Per-Application Network Permissions and Firewall | MEDIUM | `TESTED` | `crates/foundation/nc-hal/src/net.rs` | F12, F28 | Implemented default-deny `PerAppNetFirewall` bound to kernel-authenticated `app_id` enforcing `NET_CAP_INTERNET`, `NET_CAP_LOCAL`, `NET_CAP_DNS`, `NET_CAP_LISTEN`, background restrictions, revocation, and denial audit counter. Verified via `cargo test -p nc-hal --lib`. |

---

## Category E — Hardware and Drivers

| # | Feature | Priority | Status | Relevant Source Files | Dependencies | Implementation Summary / Verification Evidence / Remaining Blockers |
|---:|---|---|---|---|---|---|
| 34 | GPU and 2D Hardware Acceleration | LONG TERM | `BLOCKED` | `aarch64crates/drivers/x6826c/src/mali.rs` | Hardware | Power/init groundwork only; real Mali command submission requires physical X6826C hardware validation. Software renderer + dirty-rect shadow buffer active. |
| 35 | Power Management | MEDIUM | `IN PROGRESS` | `aarch64crates/drivers/x6826c/src/cpufreq.rs`, `psci.rs` | F1 | WFI idle active; verified OPP tables and PSCI suspend/resume pending Phase 6. |
| 36 | Battery and Charging Interface | NEAR | `IMPLEMENTED` | `aarch64crates/drivers/x6826c/src/battery.rs`, `aarch64kernel/src/host.rs` | Hardware | PMIC OCV battery driver & `OP_SYSTEM_STATUS` reporting implemented; QEMU cleanly reports `bat_avail=0`; physical X6826C calibration untested. |
| 37 | Real Audio Driver | LONG TERM | `BLOCKED` | `crates/graphics/nc-audio/` | Hardware | Software mixer exists; ARM64 hardware codec/I2S/DMA blocked on physical hardware interface documentation & testing. |
| 38 | Camera Support | LONG TERM | `BLOCKED` | `aarch64crates/drivers/x6826c/` | Hardware | Blocked on X6826C sensor/CSI/ISP hardware access & DTB verification. |
| 39 | Modem and SIM Integration | LONG TERM | `BLOCKED` | `aarch64crates/drivers/x6826c/` | Hardware | Blocked on proprietary MT6768 CCCI/MD1 modem firmware & physical SIM hardware. |
| 40 | Sensor Framework | MEDIUM | `BLOCKED` | `aarch64crates/drivers/x6826c/src/scp.rs` | Hardware | SCP groundwork exists; real sensor IPI protocol requires physical X6826C hardware. |

---

## Category F — Application Platform and User Interface

| # | Feature | Priority | Status | Relevant Source Files | Dependencies | Implementation Summary / Verification Evidence / Remaining Blockers |
|---:|---|---|---|---|---|---|
| 41 | Stable SDK and ABI Versioning | NEAR | `VERIFIED` | `crates/foundation/nc-abi/src/lib.rs`, `aarch64kernel/src/syscall.rs`, `docs/architecture/ABI_SPECIFICATION.md` | None | Added semantic ABI v1.1.0, `AbiInfo` (32-byte `#[repr(C)]`), `SYS_ABI_INFO` (syscall 31) wired in ARM64 kernel, compile-time layout assertions, and 38/38 `nc-abi` unit tests passing. |
| 42 | Package Repository and Dependency Management | MEDIUM | `TESTED` | `crates/runtime/nc-pkg/src/resolver.rs`, `crates/runtime/nc-pkg/` | F31 | NTZ3 signed packages (28 tests) + bounded topological `resolve_install_order()` detecting missing deps, cycles, version conflicts, and arch mismatches verified via `cargo test -p nc-pkg --lib`. Remote HTTPS repo client awaits ARM64 `nc-net`. |
| 43 | System Notification Service | MEDIUM | `TESTED` | `crates/foundation/nc-abi/src/notify.rs` | F12 | Implemented bounded `NotificationService` with SPARK authorization gating, authenticated sender identity, priority/channels, dismissal, app-uninstall purge, and lock-screen privacy redaction (`Public`/`PrivateRedacted`/`SecretHidden`). Verified via `cargo test -p nc-abi --lib`. |
| 44 | Window Manager and Compositor | MEDIUM | `TESTED` | `crates/foundation/nc-abi/src/compositor.rs`, `aarch64kernel/src/host.rs` | F41 | Implemented bounded multi-surface `Compositor` with owner-slot isolation, Z-order, focus, and merged dirty-rect `DamageRect` partial-redraw tracking (verified via `cargo test -p nc-abi --lib`), complementing the active mobile shadow-buffer compositor. |
| 45 | Input Methods and Localization | MEDIUM | `TESTED` | `aarch64crates/runtime/nc-mobile-ui/src/keyboard.rs`, `crates/foundation/nc-i18n/` | None | Implemented native NetizenCore `VirtualKeyboard` using `Palette`/`Dp`/`Gfx` design tokens (Light & Dark themes, rounded pill keys, tonal modifiers, accent Return pill), authentic Turkish Q layout (`ı↔I`, `i↔İ`, `ğ ü ş ö ç`), English QWERTY, Numbers/Symbols, and ShiftOnce/CapsLock state machine. Verified via `cargo test -p nc-mobile-ui --lib`. |
| 46 | Accessibility Infrastructure | MEDIUM | `TESTED` | `aarch64crates/runtime/nc-mobile-ui/src/a11y.rs` | F37, F44 | Implemented `SemanticTree`, `SemanticRole` (Button/Label/TextField/Switch/ListItem/Navigation), `A11yPrefs` (100–200% text scaling, high-contrast Light/Dark palettes, reduce-motion), focus traversal, and screen-reader event queue (speech output explicitly marked unavailable pending audio hardware). Verified via `cargo test -p nc-mobile-ui --lib`. |
| 47 | Secure Inter-Application IPC and Intents | NEAR | `TESTED` | `crates/foundation/nc-abi/src/ipc.rs`, `kernel/src/tasking/ipc.rs`, `spark/security/src/nc_security_policy.adb` | F12, F18 | Implemented bounded identity-bound `IpcBroker` with SPARK authorization gating, scoped content handles, queue/payload limits, and endpoint lifecycle (verified via `cargo test -p nc-abi --lib`) alongside 100%-proved SPARK `Authorize_IPC`. |

---

## Category G — Developer Experience and Quality

| # | Feature | Priority | Status | Relevant Source Files | Dependencies | Implementation Summary / Verification Evidence / Remaining Blockers |
|---:|---|---|---|---|---|---|
| 48 | CI and Automated QEMU Boot Testing | NEAR | `VERIFIED` | `tools/check/qemu_boot_test.py`, `tools/check/run_ci.sh`, `.github/workflows/ci.yml` | None | Automated headless ARM64 QEMU boot verifier checks 15 boot/selftest markers, rejects panics, and verifies 0 PMM frame leaks (`33280/33280`) in ~3.5s. Local CI pipeline (`run_ci.sh`) and GitHub Actions workflow created and executed end-to-end. |
| 49 | Crash Logging and Debugging Infrastructure | NEAR | `IMPLEMENTED` | `aarch64kernel/src/crash.rs`, `aarch64kernel/src/boot.rs` | F48 | Zero-allocation, CRC32-checksummed bounded crash ring buffer (`NCRS v1`) capturing `ESR_EL1`, `FAR_EL1`, `ELR_EL1`, `SPSR_EL1`, `SP`, `MPIDR_EL1`, `CurrentEL`, ticks, stack canary health, and authenticated `app_id` (zero credentials/keys). Wired into ARM64 panic handler. |
| 50 | Documentation, Demonstration, and Easy Installation | NEAR | `VERIFIED` | `docs/GETTING_STARTED.md`, `docs/architecture/ABI_SPECIFICATION.md`, `docs/security/SYSCALL_SECURITY_MATRIX.md`, `docs/roadmap/NETIZENCORE_50_FEATURE_IMPLEMENTATION.md` | F48 | Complete developer getting-started guide, reproducible one-command QEMU & CI workflows, ABI v1.1.0 spec, syscall security matrix, and 100% SPARK proof report verified against live builds. |
