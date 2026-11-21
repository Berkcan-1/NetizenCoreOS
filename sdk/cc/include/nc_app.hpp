// NetizenCore Public C++ SDK Header (Freestanding, no libstdc++ / libc required)
#ifndef NC_APP_HPP
#define NC_APP_HPP

#include "nc_abi.h"

namespace nc {

inline size_t cstr_len(const char *s) {
    size_t n = 0;
    while (s && s[n] != '\0') {
        ++n;
    }
    return n;
}

#if defined(__aarch64__)
inline int64_t print(const char *msg) {
    return nc_write(NC_STDOUT_FD, msg, cstr_len(msg));
}

[[noreturn]] inline void exit(int64_t code = 0) {
    nc_exit(code);
    for (;;) {}
}

inline int64_t ticks() {
    return nc_clock_ticks();
}
#endif

} // namespace nc

#endif // NC_APP_HPP
