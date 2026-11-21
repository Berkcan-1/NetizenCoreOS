# NetizenCore SPARK Ada Security Core

Authoritative, formally verified security core for NetizenCore (ARMv8-A and x86_64).

## Commands

```bash
# Inspect discovered toolchains
bash build.sh toolchains

# Build bare-metal AArch64, bare-metal x86_64, and host static libraries
bash build.sh spark-build

# Run GNATprove flow analysis
bash build.sh spark-check

# Run GNATprove formal proof pass
bash build.sh spark-prove

# Run standalone SPARK KAT & security invariant tests
bash build.sh spark-test
```

See [SPARK_SECURITY.md](../../docs/architecture/SPARK_SECURITY.md) for the full architecture and [proofs/PROOF_REPORT.md](proofs/PROOF_REPORT.md) for formal verification results.
