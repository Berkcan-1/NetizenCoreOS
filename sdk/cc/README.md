# NetizenCore C & C++ Developer SDK (Standalone EL0 Applications)

This SDK enables third-party developers to write freestanding C and C++ applications targeting NetizenCore on ARMv8-A (AArch64 EL0 / Ring 3).

> **Architectural Rule**: C and C++ are supported strictly as **application developer SDK languages**. C/C++ source files, object files, `libc`, and `libstdc++` are **never** linked into the production NetizenCore kernel, security core, or system images.

## Contents

- `include/nc_abi.h` — Public NetizenCore EL0 system-call ABI definitions and inline AArch64 `svc #0` wrappers.
- `include/nc_app.hpp` — Freestanding C++20 wrapper over `nc_abi.h` (no `libstdc++`, no exceptions, no RTTI).
- `linker/aarch64-app.ld` — Standalone EL0 linker script placing `.text.start` at `0x40000000`.
- `examples/hello_c/main.c` — Minimal freestanding C application.
- `examples/hello_cpp/main.cpp` — Minimal freestanding C++ application.
- `build_examples.sh` — Cross-compiler discovery (`NC_CC`, `NC_CXX`) and AArch64 ELF build verification.

## Building Examples

```bash
bash build.sh sdk-cc-test
```

Or with explicit compilers:

```bash
NC_CC=aarch64-linux-gnu-gcc NC_CXX=aarch64-linux-gnu-g++ bash sdk/cc/build_examples.sh
```

Packaging into `.ntzc` uses the standard `sdk/build-ntzc.py` tool. All runtime permissions remain enforced by the kernel and the SPARK Ada DysonSphere engine.
