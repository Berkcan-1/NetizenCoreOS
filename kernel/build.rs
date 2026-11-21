use std::env;

fn main() {
    let directory = env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed=linker.ld");
    println!("cargo:rustc-link-arg-bins=-T{}/linker.ld", directory);
}
