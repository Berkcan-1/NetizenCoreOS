fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let script = if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") {
        format!("{}/../../aarch64crates/runtime/app-aarch64.ld", dir)
    } else {
        format!("{}/../../crates/runtime/nc-app-sdk/app.ld", dir)
    };
    println!("cargo:rerun-if-changed={}", script);
    println!("cargo:rustc-link-arg-bins=-T{}", script);
}
