# NetizenCore Ring 3 Syscall ABI (Current Implementation)

**Status:** documented snapshot of the current experimental ABI. This is not yet a versioned compatibility guarantee.

## Entry and register convention

- Entry instruction: `int 0x80` (IDT gate configured for user invocation).
- `RAX`: syscall number on entry; signed result in `RAX` on return.
- `RDI`, `RSI`, `RDX`: arguments 1–3.
- User-originated calls are accepted only when the saved code-segment privilege level is Ring 3 (`CS & 3 == 3`).
- User buffers must be accessed through `copy_from_user` / `copy_to_user`; syscall implementations must not directly dereference untrusted user pointers.
- Success values are non-negative. Errors are negative errno-style values.

## Syscall table

| Number | Name | Arguments | Current behaviour |
|---:|---|---|---|
| 1 | `SYS_EXIT` | `RDI = exit_code` | Terminates the calling user process and returns to the kernel scheduler. Does not return to the caller. |
| 2 | `SYS_YIELD` | None | Yields the Ring 3 session to the scheduler. |
| 3 | `SYS_SLEEP` | `RDI = ticks` | Requests sleep for up to 10,000 timer ticks, then returns to the scheduler. |
| 4 | `SYS_WRITE` | `RDI = handle`, `RSI = user_buffer`, `RDX = length` | Writes up to 512 bytes to process stdout/stderr handles (IDs 1 and 2). A zero length returns zero. Bytes that are not valid UTF-8 are logged as `<binary>`. |
| 5 | `SYS_CLOCK_GET` | `RDI = optional user pointer to `u64`` | Returns current timer ticks in `RAX`; when `RDI != 0`, also copies the `u64` tick value to that user address. |
| 6 | `SYS_HANDLE_OPEN` | `RDI = kind` | Creates a process-local handle token. Kind 1 requests `FileRead` and requires `CAP_FS_USER_READ`; kind 2 requests `FileWrite` and requires `CAP_FS_USER_WRITE`; kind 3 requests `GuiSurface` and currently requires no capability. |
| 7 | `SYS_HANDLE_CLOSE` | `RDI = handle_id` | Removes a non-standard process-local handle token. Standard handles 1 and 2 cannot be closed. |
| 8 | `SYS_GUI_SUBMIT` | `RDI = GUI handle`, `RSI = command array`, `RDX = command count` | Copies and validates 1–32 packed draw-command records. Returns the accepted command count; this implementation does not yet bind the handle to a real compositor surface or render the commands. |

## Error values

| Name | Value | Meaning in the current dispatcher |
|---|---:|---|
| `EPERM` | -1 | Caller is not Ring 3 or there is no active user process/address space. |
| `EBADF` | -9 | Handle does not exist or has an incompatible kind. |
| `ENOMEM` | -12 | Handle vector cannot reserve the required capacity. |
| `EACCES` | -13 | Process capability does not permit the requested file-handle kind. |
| `EFAULT` | -14 | User pointer/range failed validation or copying. |
| `EINVAL` | -22 | Invalid syscall argument, handle kind, count, geometry, or handle operation. |
| `ENOSYS` | -38 | Unknown syscall number. |

`ENOENT` is defined in the source but is not currently returned by the dispatcher.

## GUI command record

`GuiDrawCommand` is `#[repr(C, packed)]`, 13 bytes:

| Field | Type | Meaning |
|---|---|---|
| `op` | `u8` | 1 = clear, 2 = rectangle, 3 = text operation code |
| `x`, `y` | `i16` | Top-left position; must be non-negative for operations 2 and 3 |
| `width`, `height` | `u16` | Must be non-zero and at most 2048 for operations 2 and 3 |
| `color` | `u32` | ARGB8888 value |

For `op = 1`, geometry fields must all be zero. For `op = 2` and `op = 3`, checked right/bottom edges must not exceed 2048. Unknown operation codes are rejected. The `op = 3` record currently carries no string pointer or text payload; it is only validated as a geometry command at this stage.

## Current limitations

- `FileRead`, `FileWrite`, and `GuiSurface` are handle-kind tokens, not fully implemented VFS or graphics resources. Handle closing currently removes the token only; resource destructors and resource ownership are not implemented.
- GUI submit validates commands but does not render to a registered surface.
- Syscall numbers and structures are documented for the current source state but are not yet frozen under an ABI version/compatibility policy.
- Current process state and scheduler globals are single-CPU. This ABI must not be treated as SMP-safe.
- NCU1 FNV-1a checksums are not cryptographic signatures and do not establish publisher identity.


## Ring 3 ABI v1 eklemeleri (`ring3-apps`; kaynak gerçek: `crates/nc-abi`)

Olay-tabanlı model: her olayda `e_entry`'e girilir (RSP = `USER_APP_STACK_TOP - 8`, tüm yazmaçlar 0),
olay `0x80_00F0_0000`'daki salt-okunur `EventBlock`'tan (96 bayt, sabit ofsetler `nc_abi::event::OFF_*`)
okunur. Bloklayan syscall (`YIELD/SLEEP`) bu modelde kullanılmaz.

| No | Ad | Argümanlar | Dönüş |
|---|---|---|---|
| 16 | `SYS_APP_DONE` | rdi = bayraklar (`RET_REDRAW`=1, `RET_CLOSE`=2; üst bitler uygulamaya özel) | dönmez |
| 17 | `SYS_FILE_OPEN` | rdi = yol işaretçisi, rsi = uzunluk (≤ 256), rdx = 1 okuma / 2 yazma | handle ya da -errno |
| 18 | `SYS_FILE_READ` | rdi = handle, rsi = tampon, rdx = uzunluk | okunan bayt (0 = EOF) |
| 19 | `SYS_FILE_WRITE` | rdi = handle, rsi = tampon, rdx = uzunluk (≤ 64 KiB; toplam ≤ 1 MiB) | yazılan bayt |
| 20 | `SYS_GUI_DRAW` | rdi = yüzey handle (olay bloğundaki `surface`), rsi = akış, rdx = bayt (≤ 16 KiB) | kabul edilen kayıt sayısı; yalnızca `OP_DRAW` sırasında, aksi `EAGAIN` |
| 21 | `SYS_TEXT_WIDTH` | rdi = UTF-8, rsi = uzunluk (≤ 256), rdx = cap yüksekliği | piksel genişliği |
| 22 | `SYS_FILE_CLOSE` | rdi = handle | 0 ya da -errno (yazma tamponu burada VFS'e yazılır) |

`SYS_HANDLE_CLOSE` dosya handle'ında `EINVAL` döner (`SYS_FILE_CLOSE` kullanın). Çizim akışı biçimi ve
sınırları `crates/nc-abi/src/draw.rs` başlığında.
