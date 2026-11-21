# NetizenCore — Developer Getting Started & Quick Reference (Feature 50)

## 1. Prerequisites

Check installed toolchains with:
```bash
bash build.sh toolchains
```

Required tools for full multi-architecture + SPARK development:
- **Rust** (`rustc` + `cargo` with targets `aarch64-unknown-none`, `x86_64-unknown-none`, `x86_64-unknown-uefi`)
- **SPARK Ada**: `aarch64-elf-gcc`, `x86_64-elf-gcc`, native `gnat` (Alire), `gprbuild`, `gnatprove`
- **QEMU**: `qemu-system-aarch64`, `qemu-system-x86_64`
- **Image tools**: `mkfs.vfat` (`dosfstools`), `mcopy`/`mmd` (`mtools`), `mkfs.exfat` (`exfatprogs`), `xorriso`, `python3`

## 2. One-Command Workflows

### Automated Full CI Check (Layout + Unit Tests + SPARK + ARM64 Build + QEMU Boot + x86_64)
```bash
bash tools/check/run_ci.sh
```
Add `--prove` to include full GNATprove verification, or `--skip-x86` for faster ARM64 iteration.

### Build & Boot ARMv8-A (QEMU virt, Mobile Shell)
```bash
bash build.sh armv8a qemu run --mem 512
```
Headless automated boot verification (exits 0 on full boot + zero PMM frame leaks):
```bash
bash build.sh armv8a qemu build
python3 tools/check/qemu_boot_test.py --timeout 20 --mem 512 --smp 1
```

### Build & Boot x86_64 (QEMU Desktop ISO)
```bash
bash build.sh x86_64 qemu build
bash build.sh x86_64 qemu run --mem 1024
```

### SPARK Security Core Commands
```bash
bash build.sh spark-test    # Run 35 NIST/RFC KAT + DysonSphere/FS/Lock/FDE tests
bash build.sh spark-build   # Rebuild libnc_security.a for aarch64, x86_64, host
bash build.sh spark-check   # GNATprove flow analysis
bash build.sh spark-prove   # GNATprove formal verification
```

### List Device Profiles
```bash
bash build.sh devices
```

## 3. Architecture & Verification Documentation
- **50-Feature Roadmap & Status Matrix**: [docs/roadmap/NETIZENCORE_50_FEATURE_IMPLEMENTATION.md](roadmap/NETIZENCORE_50_FEATURE_IMPLEMENTATION.md)
- **Public ABI Specification (v1.1.0)**: [docs/architecture/ABI_SPECIFICATION.md](architecture/ABI_SPECIFICATION.md)
- **Syscall Security Matrix**: [docs/security/SYSCALL_SECURITY_MATRIX.md](security/SYSCALL_SECURITY_MATRIX.md)
- **SPARK Formal Verification Report**: [spark/security/proofs/PROOF_REPORT.md](../spark/security/proofs/PROOF_REPORT.md)

## 4. Physical Device Safety Notice (X6826C)
Never automatically flash a physical device. Always back up stock `boot`, `vbmeta`, and `userdata` partitions before testing on real hardware, and verify target compatibility in the 50-feature roadmap matrix.
