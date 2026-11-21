use std::env;

fn main() {
    let directory = env::var("CARGO_MANIFEST_DIR").unwrap();
    let embedded = std::path::Path::new(&directory).join("embedded");
    let _ = std::fs::create_dir_all(&embedded);
    let home = embedded.join("home.elf");
    if !home.exists() {
        let _ = std::fs::write(&home, b"");
    }
    println!("cargo:rerun-if-changed=embedded/home.elf");
    println!("cargo:rerun-if-changed=aarch64.ld");
    println!("cargo:rerun-if-env-changed=NC_KERNEL_BASE");
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") {
        println!("cargo:rustc-link-arg-bins=-T{}/aarch64.ld", directory);
    }
}
