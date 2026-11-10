fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed={}/../../crates/nc-app-sdk/app.ld", dir);
    println!("cargo:rustc-link-arg-bins=-T{}/../../crates/nc-app-sdk/app.ld", dir);
}
