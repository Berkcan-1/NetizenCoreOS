# NetizenCore — SPARK Ada Security Core & Multi-Language Architecture

## 1. Language Separation & Authoritative Responsibilities

NetizenCore enforces a strict three-language architectural boundary:

| Language | Role | Production OS? |
|---|---|---|
| **SPARK Ada** | Single authoritative security engine: DysonSphere, capabilities, filesystem access policy, process/IPC authorization, cryptography (SHA-256, HMAC-SHA256, PBKDF2-HMAC-SHA256, ChaCha20-Poly1305 AEAD), screen-lock state machine, and `/data` Full-Data Encryption (FDE) state management. | **Yes** (statically linked as `libnc_security.a` for AArch64 and x86_64) |
| **Rust** | General-purpose OS infrastructure: kernel scheduling, MMU, interrupts, device drivers, eMMC/virtio-blk I/O, ELF loader, display/input, user-space runtime, mobile/desktop shells, and thin `extern "C"` SPARK ABI adapters (`nc-spark-sec`). | **Yes** |
| **C / C++** | Standalone developer application SDK (`sdk/cc/`) for third-party EL0 applications using only the public syscall ABI. | **No** (strictly excluded from production kernels and system images) |

## 2. SPARK Ada Package Structure (`spark/security/src/`)

- `nc_security_types.ads/.adb`: Bounded strings (`Bounded_App_Id`, `Bounded_Publisher`), fixed byte arrays (`Hash_256`, `Key_256`, `Nonce_96`, `Tag_128`, `Salt_128`, `Sector_512`), ABI version (`1`), and fail-closed decision enums (`Auth_Decision`, `Fs_Decision`).
- `nc_security_capabilities.ads/.adb`: Authoritative 64-bit capability mask (`Cap_Mask`, preserving exact bit compatibility with legacy `u32` bits `0..21`), `User_Allowed_Mask`, `System_Only_Mask`, `Clamp_User`, `Compute_Effective_Grant`, and `Has_Capability`.
- `nc_security_policy.ads/.adb`: Reserved built-in system identities, `Resolve_Trust_Tier`, `Authorize_Process_Launch`, and `Authorize_IPC`.
- `nc_security_dysonsphere.ads/.adb`: Central DysonSphere authorization engine with static 32-app table, 64-entry bounded audit ring, explicit caller identity (`Auth_Request`), distinct `Requested_Caps` / `Approved_Caps` / `Granted_Caps`, impersonation protection, rollback prevention, and persistent revocation.
- `nc_security_filesystem.ads/.adb`: Authoritative filesystem policy enforcing read-only `/` and `/system`, exact ownership of `/data/appdata/<owner>`, capability checks for `/data/media` and `/storage/usb0`, and rejection of `..`, `.`, `//`, `\`, and NUL path traversal.
- `nc_security_crypto.ads/.adb`: FIPS 180-4 SHA-256, RFC 2104 HMAC-SHA256, RFC 2898 PBKDF2-HMAC-SHA256, RFC 8439 ChaCha20-Poly1305 AEAD, constant-time equality, and secure memory zeroization.
- `nc_security_lock.ads/.adb`: `NCLOCKV1` / `NCLOCKMK` 512-byte sector validation, progressive retry lockout (`30s / 60s / 120s`), and fail-closed `Lock_Destroyed` handling (`Locked => True`).
- `nc_security_encryption.ads/.adb`: `NCENCRV1` `/data` FDE metadata validation, KEK/DEK key wrapping via PBKDF2-HMAC-SHA256 + ChaCha20-Poly1305 AEAD, fail-closed unlock/lock state transitions with immediate DEK zeroization on lock, and authenticated block encryption/decryption.
- `nc_security_c_api.ads/.adb`: Narrow, versioned `Convention => C` FFI boundary (`nc_sec_*`). Pointer marshalling is isolated to this unit (`SPARK_Mode => Off`); all decision logic executes in `SPARK_Mode => On` units.

## 3. Caller Identity & Authorization Flow

The legacy global mutable `active_app_id` string (`set_active_caller` / `authorize_active`) has been eliminated. Every authorization check follows:

1. An EL0 user-space task invokes a syscall (`SYS_VFS_CALL`, `SYS_APP_LAUNCH`, `SYS_PKG_INSTALL`, `SYS_SYS_CALL`).
2. The Rust kernel reads the authenticated caller identity and capability mask directly from the active kernel task record (`task::current_app_id`, `task::current_caps`).
3. The Rust kernel invokes the SPARK ABI (`nc_spark_sec`) passing the trusted identity and bounded request parameters.
4. SPARK validates all inputs, evaluates policy/DysonSphere state, records bounded audit events, and returns an explicit `Auth_Decision` or `Fs_Decision`.
5. The Rust kernel enforces the returned decision (fail-closed on any error or uninitialized state).

## 4. Zero-Footprint Bare-Metal Runtime (`spark/rts/zfp-64/`)

Both bare-metal `aarch64-elf` and `x86_64-elf` builds use the auditable Zero-Footprint (ZFP) runtime in `spark/rts/zfp-64/`:
- Zero heap allocation, zero tasking, zero exception propagation, zero secondary stack, zero `libc` / `libgnat`.
- Impossible runtime check violations trap fail-closed via `nc_sec_fatal_trap` in the Rust kernel.

## 5. Formal Verification & Assurance Boundaries

Run formal verification with:
```bash
bash build.sh spark-check   # GNATprove flow analysis
bash build.sh spark-prove   # GNATprove proof pass
bash build.sh spark-test    # Standalone SPARK KAT + security invariant tests
bash build.sh spark-build   # Cross-compile AArch64, x86_64, and host static archives
bash build.sh toolchains    # Inspect discovered Rust, SPARK, GNATprove, and C/C++ SDK toolchains
```

### Trusted External Boundaries (Outside GNATprove Proof Scope)
- `nc_security_c_api.adb` pointer-to-array marshalling (`SPARK_Mode => Off`).
- Rust kernel MMU address-space isolation, syscall trap framing, and `task::current_app_id` authenticity.
- Hardware TRNG entropy quality (`ID_AA64ISAR0_EL1.RNDR` is probed at boot; QEMU Cortex-A72 lacks hardware TRNG, so FDE key generation must only be provisioned on targets with hardware CSPRNG).
- Ed25519 signature verification (`ed25519-dalek` in `nc-pkg`).
- Compiler-level constant-time code generation (audited separately from functional safety proofs).

## 6. Mobile Responsive Layout (`nc-mobile-ui` & `nc-settings-v8a`)

- `Dp::fit(area_w, area_h, design_h)` scales logical dp units against both width (`BASE_DP = 360`) and target vertical design height (`SETTINGS_DESIGN_H = 476`), preventing bottom clipping or navigation-bar overlap across compact, standard, tall, wide, and landscape screens.
- `UiRect` provides shared geometry between `draw()` and `handle_click()` across all 14 Settings screens.
- The NetizenHome bottom application dock (`nc-home-r3`) displays **icons only** (centered vertically), while the main home-screen grid and app drawer retain labels according to `show_labels`.
