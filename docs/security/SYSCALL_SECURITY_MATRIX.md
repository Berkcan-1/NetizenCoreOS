# NetizenCore ARMv8-A System-Call Security Matrix (Feature 12)

**Audit Target**: `aarch64kernel/src/syscall.rs`, `aarch64kernel/src/host.rs`, `aarch64kernel/src/storage.rs`  
**Security Authority**: SPARK Ada (`spark/security/`, `nc-spark-sec`)  
**Caller Identity Source**: Kernel-authenticated `task::current_app_id()` & `task::current_slot()` (caller-supplied IDs are NEVER trusted).

| # | Syscall | Caller Identity | Required Capability / Policy | Argument Validation | SPARK Authorization | Denial Return | Safe for Unprivileged EL0? |
|---:|---|---|---|---|---|---|---|
| 1 | `SYS_EXIT` | `current_slot()` | None (self-termination) | Exit code `i64` | N/A | N/A | Yes |
| 2 | `SYS_YIELD` | `current_slot()` | None | None | N/A | N/A | Yes |
| 3 | `SYS_SLEEP` | `current_slot()` | None | Bounded ticks | N/A | N/A | Yes |
| 4 | `SYS_WRITE` | `current_slot()` | None (stdout/stderr only) | `fd in {1,2}`, `len <= 4096`, `copy_from_user` | N/A | `-EBADF`, `-EINVAL`, `-EFAULT` | Yes |
| 5 | `SYS_CLOCK_GET` | `current_slot()` | None (read-only time) | `kind in {0,1}` | N/A | `-EINVAL` | Yes |
| 16 | `SYS_APP_DONE` | `current_slot()` | None (own event completion) | Flags `u32` | N/A | N/A | Yes |
| 20 | `SYS_GUI_DRAW` | `current_slot()` | Owns assigned surface | Bounded command stream, `copy_from_user` | Surface ownership enforced by kernel `host` | `-EACCES`, `-EINVAL`, `-EFAULT` | Yes (isolated surface) |
| 21 | `SYS_TEXT_WIDTH` | `current_slot()` | None (pure metric) | Bounded UTF-8, `copy_from_user` | N/A | `-EINVAL`, `-EFAULT` | Yes |
| 23 | `SYS_PKG_INSTALL` | `current_app_id()` | `PACKAGE_INSTALL` | `0 < len <= 256`, UTF-8 path, `copy_from_user` | `nc_sec_ds_authorize(caller_id, PACKAGE_INSTALL)` | `-EACCES`, `-EINVAL`, `-EFAULT` | Only if granted |
| 26 | `SYS_VFS_CALL` | `current_app_id()` + `current_caps()` | Per-path & per-op capability | Bounded req/resp, `copy_from_user`/`copy_to_user` | `nc_sec_fs_check(app_id, caps, path, op)` via `nc-fs-policy` | `-EACCES`, `-EINVAL`, `-EFAULT` | Yes (sandboxed) |
| 28 | `SYS_SYS_CALL` | `current_slot()` + `current_caps()` | Op-specific (`SECURITY_MANAGE`, `SETTINGS_WRITE`, etc.) | Bounded `nc_abi::sys` wire parse | Per-op capability & lock-state checks | `-EACCES`, `-EINVAL`, `-EFAULT` | Restricted by cap |
| 30 | `SYS_APP_LAUNCH` | `current_slot()` + `current_app_id()` | Actions `2..=5`: `home_slot` only; Actions `0..=1`: `policy_authorize_launch` + unlocked check | `0 < len <= 64`, UTF-8, `copy_from_user` | `nc_sec_policy_authorize_launch(caller_id, caps, target_id)` | `-EACCES`, `-EINVAL`, `-EFAULT` | Restricted |
| 31 | `SYS_ABI_INFO` | `current_slot()` | None (read-only ABI discovery) | `out_len >= 32`, `copy_to_user` | N/A | `-EINVAL`, `-EFAULT` | Yes |
| * | All other numbers | `current_slot()` | N/A | N/A | Fail-closed | `-ENOSYS` | Safe (rejected) |

## Audit Findings Fixed in Phase 1
1. **`SYS_PKG_INSTALL`**: Previously checked only Rust `task::current_caps() & PACKAGE_INSTALL`; now also routes through authoritative SPARK `nc_spark_sec::ds_authorize(caller_id, PACKAGE_INSTALL)` before reading or registering any package.
2. **`SYS_APP_LAUNCH` (actions `2..=5`)**: Previously allowed any EL0 task to invoke shell window-manager actions (pause foreground, inject back key, forward touch, set screen insets); now strictly restricted to `home_slot` (`EACCES` for all other tasks).
