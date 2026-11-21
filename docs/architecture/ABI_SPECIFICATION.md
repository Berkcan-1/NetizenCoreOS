# NetizenCore Public ABI & SDK Specification (v1.1.0)

**Status**: Authoritative (Feature 41)  
**Primary Implementation**: `crates/foundation/nc-abi/`  
**Security Authority**: SPARK Ada (`spark/security/`, ABI v1)

## 1. Versioning Policy

NetizenCore uses semantic ABI versioning (`ABI_VERSION_MAJOR`, `ABI_VERSION_MINOR`, `ABI_VERSION_PATCH`):

- **Major (`1`)**: Breaking changes to calling convention, syscall numbers, or fixed struct layouts.
- **Minor (`1`)**: Backward-compatible additions (new syscalls, new optional feature flags, additive request opcodes).
- **Patch (`0`)**: Documentation or non-breaking clarifications.

Applications MUST query `SYS_ABI_INFO` (syscall `31`) at startup to inspect kernel architecture, highest supported syscall, and active runtime features (`AbiInfo`).

## 2. Calling Conventions

### ARMv8-A (AArch64, EL0 → EL1)
- Instruction: `svc #0`
- Syscall number: `x8`
- Arguments: `x0`, `x1`, `x2`, `x3`
- Return value: `x0` (non-negative = success / bytes written; negative = `-errno`)

### x86_64 (Ring 3 → Ring 0)
- Instruction: `syscall`
- Syscall number: `rax`
- Arguments: `rdi`, `rsi`, `rdx`, `r10`
- Return value: `rax`

## 3. Fixed Syscall Table (ABI v1.1.0)

| # | Name | Arguments | Description |
|---:|---|---|---|
| 1 | `SYS_EXIT` | `(code)` | Terminate current task |
| 2 | `SYS_YIELD` | `()` | Yield CPU slice |
| 3 | `SYS_SLEEP` | `(ms)` | Sleep for bounded milliseconds |
| 4 | `SYS_WRITE` | `(fd, ptr, len)` | Write to stdout/stderr (`fd` 1 or 2, max 4096 B) |
| 5 | `SYS_CLOCK_GET` | `(kind)` | `0` = kernel ticks, `1` = RTC Unix time |
| 16 | `SYS_APP_DONE` | `(flags)` | Complete current event-loop dispatch |
| 20 | `SYS_GUI_DRAW` | `(surface, ptr, len)` | Submit bounded draw command stream |
| 21 | `SYS_TEXT_WIDTH` | `(ptr, len, cap_h)` | Measure UTF-8 string width |
| 23 | `SYS_PKG_INSTALL` | `(path_ptr, path_len)` | Install `.ntzc` package (SPARK `PACKAGE_INSTALL` required) |
| 26 | `SYS_VFS_CALL` | `(req, req_len, resp, cap)` | Filesystem RPC (`nc_abi::vfs`, SPARK `nc_sec_fs_check`) |
| 27 | `SYS_NET_CALL` | `(req, req_len, resp, cap)` | Network RPC (`nc_abi::net`) |
| 28 | `SYS_SYS_CALL` | `(req, req_len, resp, cap)` | System settings/status RPC (`nc_abi::sys`) |
| 29 | `SYS_SHELL_CALL` | `(req, req_len, resp, cap)` | Shell audio/notification RPC (`nc_abi::shell`) |
| 30 | `SYS_APP_LAUNCH` | `(ptr, len, action)` | App lifecycle & shell WM actions (SPARK `policy_authorize_launch`) |
| 31 | `SYS_ABI_INFO` | `(out_ptr, out_len)` | Write 32-byte `AbiInfo` struct to user buffer |

## 4. `AbiInfo` Wire Layout (32 bytes, Little-Endian)

| Offset | Size | Field | Description |
|---:|---:|---|---|
| 0 | 4 | `magic` | `0x4942414E` (`"NABI"`) |
| 4 | 4 | `struct_size` | `32` |
| 8 | 2 | `major` | `ABI_VERSION_MAJOR` (`1`) |
| 10 | 2 | `minor` | `ABI_VERSION_MINOR` (`1`) |
| 12 | 2 | `patch` | `ABI_VERSION_PATCH` (`0`) |
| 14 | 2 | `reserved` | `0` |
| 16 | 4 | `arch_id` | `1` = x86_64, `2` = AArch64 |
| 20 | 4 | `max_syscall` | Highest supported syscall number |
| 24 | 8 | `features` | Bitmask (`FEAT_SPARK_SEC`, `FEAT_VFS_CALL`, `FEAT_HW_CSPRNG`, etc.) |

## 5. Error Codes

| Constant | Value | Meaning |
|---|---:|---|
| `EPERM` | `-1` | Operation not permitted |
| `ENOENT` | `-2` | No such file or entry |
| `EBADF` | `-9` | Bad file descriptor |
| `EAGAIN` | `-11` | Resource temporarily unavailable |
| `ENOMEM` | `-12` | Out of memory |
| `EACCES` | `-13` | Permission / SPARK authorization denied |
| `EFAULT` | `-14` | Invalid user-space memory address |
| `EINVAL` | `-22` | Invalid argument or malformed request |
| `EMFILE` | `-24` | Too many open files |
| `EFBIG` | `-27` | File too large |
| `ENOSYS` | `-38` | Syscall not implemented |

## 6. Verification

Compile-time assertions and unit tests in `crates/foundation/nc-abi/src/lib.rs` (`cargo test -p nc-abi --lib`) verify struct sizes, alignment, wire round-trip encoding, and syscall number stability.
