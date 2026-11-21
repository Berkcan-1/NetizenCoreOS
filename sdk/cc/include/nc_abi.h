/*
 * NetizenCore Public C ABI Header (Ring 3 / EL0 Application SDK)
 *
 * Developer SDK header ONLY. Do NOT include internal kernel or DysonSphere
 * structures here. All security decisions are mediated by the kernel and
 * the SPARK Ada Security Core across the syscall boundary.
 */
#ifndef NC_ABI_H
#define NC_ABI_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define NC_SDK_ABI_VERSION 1u

/* Public NetizenCore EL0 Syscall Numbers (mirrors crates/foundation/nc-abi) */
#define NC_SYS_EXIT        1u
#define NC_SYS_YIELD       2u
#define NC_SYS_SLEEP       3u
#define NC_SYS_WRITE       4u
#define NC_SYS_CLOCK_GET   5u
#define NC_SYS_APP_DONE    6u
#define NC_SYS_GUI_DRAW    7u
#define NC_SYS_TEXT_WIDTH  8u
#define NC_SYS_PKG_INSTALL 9u
#define NC_SYS_VFS_CALL    10u
#define NC_SYS_SYS_CALL    11u
#define NC_SYS_APP_LAUNCH  12u

#define NC_STDOUT_FD       1u
#define NC_STDERR_FD       2u

#if defined(__aarch64__)
static inline int64_t nc_syscall3(uint64_t num, uint64_t a0, uint64_t a1, uint64_t a2) {
    register uint64_t x8 __asm__("x8") = num;
    register uint64_t x0 __asm__("x0") = a0;
    register uint64_t x1 __asm__("x1") = a1;
    register uint64_t x2 __asm__("x2") = a2;
    __asm__ volatile("svc #0"
                     : "+r"(x0)
                     : "r"(x1), "r"(x2), "r"(x8)
                     : "memory");
    return (int64_t)x0;
}

static inline void nc_exit(int64_t code) {
    (void)nc_syscall3(NC_SYS_EXIT, (uint64_t)code, 0, 0);
    for (;;) {
        __asm__ volatile("wfi");
    }
}

static inline int64_t nc_write(uint64_t fd, const void *buf, size_t len) {
    return nc_syscall3(NC_SYS_WRITE, fd, (uint64_t)(uintptr_t)buf, (uint64_t)len);
}

static inline void nc_yield(void) {
    (void)nc_syscall3(NC_SYS_YIELD, 0, 0, 0);
}

static inline int64_t nc_clock_ticks(void) {
    return nc_syscall3(NC_SYS_CLOCK_GET, 0, 0, 0);
}
#endif /* __aarch64__ */

#ifdef __cplusplus
}
#endif

#endif /* NC_ABI_H */
