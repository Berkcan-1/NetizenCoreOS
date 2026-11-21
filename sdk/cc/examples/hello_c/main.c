/* Standalone NetizenCore AArch64 C Application Example (SDK only — NOT part of OS) */
#include "nc_abi.h"

static size_t nc_strlen(const char *s) {
    size_t n = 0;
    while (s[n] != '\0') {
        ++n;
    }
    return n;
}

__attribute__((section(".text.start")))
void _start(void) {
    const char *msg = "Hello from NetizenCore C SDK (AArch64 EL0)!\n";
    (void)nc_write(NC_STDOUT_FD, msg, nc_strlen(msg));
    nc_exit(0);
}
