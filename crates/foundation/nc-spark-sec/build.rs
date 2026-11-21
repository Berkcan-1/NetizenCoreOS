use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("nc-spark-sec must live under crates/foundation/nc-spark-sec");

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let spark_arch = if target_os == "none" && target_arch == "aarch64" {
        "aarch64"
    } else if target_os == "none" && target_arch == "x86_64" {
        "x86_64"
    } else {
        "host"
    };

    let lib_dir = root.join("spark").join("security").join("lib").join(spark_arch);
    let lib_file = lib_dir.join("libnc_security.a");

    println!("cargo:rerun-if-changed={}", lib_file.display());
    println!("cargo:rerun-if-env-changed=NC_SPARK_LIB_DIR");

    let search_dir = env::var("NC_SPARK_LIB_DIR")
        .map(PathBuf::from)
        .unwrap_or(lib_dir);

    if !search_dir.join("libnc_security.a").exists() {
        panic!(
            "SPARK Ada security library not found at {}. Run `bash build.sh spark-build` first.",
            search_dir.join("libnc_security.a").display()
        );
    }

    println!("cargo:rustc-link-search=native={}", search_dir.display());
    println!("cargo:rustc-link-lib=static=nc_security");
}
