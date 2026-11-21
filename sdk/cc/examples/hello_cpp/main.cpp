// Standalone NetizenCore AArch64 C++ Application Example (SDK only — NOT part of OS)
#include "nc_app.hpp"

extern "C" __attribute__((section(".text.start")))
void _start() {
    nc::print("Hello from NetizenCore C++ SDK (AArch64 EL0)!\n");
    nc::exit(0);
}
