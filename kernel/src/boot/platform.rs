#[cfg(all(feature = "qemu", feature = "generic"))]
compile_error!("yalnizca bir platform ozelligi secilmeli: qemu veya generic");

#[cfg(not(any(feature = "qemu", feature = "generic")))]
compile_error!("bir platform ozelligi gerekli: --features qemu veya --features generic");

#[cfg(feature = "qemu")]
pub use nc_platform_qemu::QemuPlatform as Active;

#[cfg(feature = "generic")]
pub use nc_platform_generic::GenericPlatform as Active;

pub fn create() -> Active {
    Active::new()
}
