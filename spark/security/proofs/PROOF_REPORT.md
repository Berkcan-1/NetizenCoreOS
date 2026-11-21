# NetizenCore SPARK Ada Security Core — GNATprove Formal Verification Report

**Tool**: GNATprove 16.1.0 (Provers: CVC5, Z3, Alt-Ergo via Alire)  
**Project**: `spark/security/nc_security.gpr`  
**SPARK Mode**: `SPARK_Mode => On` on all 8 authoritative security packages.

## 1. Units Analyzed in SPARK

1. `nc_security_types.ads/.adb`
2. `nc_security_capabilities.ads/.adb`
3. `nc_security_policy.ads/.adb`
4. `nc_security_dysonsphere.ads/.adb`
5. `nc_security_filesystem.ads/.adb`
6. `nc_security_lock.ads/.adb`
7. `nc_security_encryption.ads/.adb`
8. `nc_security_crypto.ads/.adb`

## 2. GNATprove Verification Summary (7 Core Policy / Engine / FS / Lock / FDE Units)

Measured with:
```bash
gnatprove -P nc_security.gpr -XNC_SPARK_ARCH=host --level=2 --timeout=5 --steps=0 \
  --counterexamples=off --report=statistics -j0 \
  -u nc_security_types.adb nc_security_capabilities.adb nc_security_policy.adb \
     nc_security_dysonsphere.adb nc_security_filesystem.adb nc_security_lock.adb \
     nc_security_encryption.adb
```

| Category | Total | Proved (Flow) | Proved (Provers) | Justified | Unproved |
|---|---:|---:|---:|---:|---:|
| Initialization | 80 | 80 | 0 | 0 | 0 |
| Run-time Checks | 200 | 0 | 200 (CVC5 97%, Z3 2%, Alt-Ergo 0%) | 0 | 0 |
| Assertions (Loop Invariants & Variants) | 32 | 0 | 32 (CVC5 89%, Trivial 4%, Z3 5%, Alt-Ergo 2%) | 0 | 0 |
| Functional Contracts (`Pre`/`Post`) | 78 | 0 | 78 (CVC5 76%, Trivial 19%, Z3 5%) | 0 | 0 |
| Termination (`Always_Terminates`) | 35 | 33 | 2 (CVC5) | 0 | 0 |
| **Total (7 Core Security Units)** | **425** | **113 (27%)** | **312 (73%)** | **0** | **0 (0%)** |

**Overall Proven Rate (7 Core Security Units)**: **425 / 425 checks proved (100.0%) — ZERO unproved checks, ZERO `pragma Assume` statements.**

### Phase 1 Proof Obligations Eliminated (Feature 18)
- `NC_Security_Types.Make_App_Id` & `Make_Publisher`: Reordered `S'First + (I - 1)` to eliminate upper-bound overflow checks when `S'First` is near `Positive'Last`.
- `NC_Security_Filesystem.Split_Path`, `Is_Safe_Path`, & `Check_Access`: Added `pragma Loop_Variant (Increases => Pos)` to both `while` loops, strengthened loop invariants (`Effective_Len <= Path.Len`, `Seg_Start >= 2`), and added explicit root-path postcondition on `Split_Path`/`Is_Safe_Path` to achieve 100% termination and functional-contract proof.
- `NC_Security_Lock.Store_LE32` & `NC_Security_Encryption.Store_LE32`/`Store_LE64`: Eliminated partial-slice initialization warnings and added explicit `Dst'First` upper bounds.
- `NC_Security_Lock.Secret_Is_Valid`: Used `Secret'Last - Secret'First` to eliminate `Natural'Last + 1` range-check overflow on unconstrained `Byte_Array`.
- `NC_Security_DysonSphere.Initialize`: Sealed `State.Initialized`, `State.Policy_Version`, and `State.Denied_Count` after built-in registration to prove all postconditions.

### Cryptographic Unit (`nc_security_crypto.adb`) Scope
- Flow analysis and **35 / 35** standalone NIST/RFC Known-Answer Tests (`bash build.sh spark-test`: FIPS 180-4 SHA-256, RFC 4231 HMAC-SHA256, RFC 6070 PBKDF2-HMAC-SHA256, RFC 8439 ChaCha20-Poly1305, DysonSphere, FS policy, Lock, FDE) pass.
- Full SMT proof of nonlinear 26-bit Poly1305 limb modular arithmetic requires manual ghost lemmas and is **explicitly NOT claimed** as formally proved.

## 3. Trusted External Boundaries (Outside Proof Scope)

| Component | Classification | Notes |
|---|---|---|
| `nc_security_c_api.adb` | Trusted FFI adapter (`SPARK_Mode => Off`) | Bounds-checks and copies C pointers into SPARK bounded arrays; zero policy logic. |
| `spark/rts/zfp-64/nc_security_rts.adb` | Trusted bare-metal trap hook (`SPARK_Mode => Off`) | Routes impossible runtime check failures to kernel `nc_sec_fatal_trap`. |
| `crates/foundation/nc-spark-sec` | Trusted Rust FFI binding | Thin `extern "C"` declarations only. |
| `aarch64kernel` / `kernel` MMU & scheduler | Trusted Rust kernel | Provides hardware address-space isolation and authenticated `task::current_app_id`. |
| `ed25519-dalek` in `nc-pkg` | Trusted external crypto crate | Ed25519 signature verification (audited RustCrypto crate). |
| Hardware TRNG (`RNDR`) | Hardware trust assumption | Probed via `ID_AA64ISAR0_EL1`; unavailable on QEMU Cortex-A72 (fails closed). |
