fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed={}/../../crates/runtime/nc-app-sdk/app.ld", dir);
    println!("cargo:rustc-link-arg-bins=-T{}/../../crates/runtime/nc-app-sdk/app.ld", dir);
}
