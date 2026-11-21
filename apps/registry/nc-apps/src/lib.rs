#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use nc_app::App;
#[cfg(feature = "ring0-fallback")]
use nc_app_browser::BrowserApp;
use nc_app_files::{create_packaged_app, FilesApp};
#[cfg(feature = "ring0-fallback")]
use nc_app_settings::SettingsApp;
#[cfg(feature = "ring0-fallback")]
use nc_app_stopwatch::StopwatchApp;
#[cfg(feature = "ring0-fallback")]
use nc_app_system::SystemApp;
use nc_fs::VfsManager;
use nc_pkg::NtzcPackage;

#[cfg(not(feature = "ring0-fallback"))]
mod ring3_only {
    use alloc::boxed::Box;
    use nc_app::{App, AppInfo, Context, Icon, Input, TextId, Update};
    use nc_gfx::{Canvas, Rect};

    pub struct Ring3Only {
        info: AppInfo,
    }

    impl Ring3Only {
        pub fn browser() -> Box<dyn App> {
            Box::new(Ring3Only { info: AppInfo::new("nc.browser", "BROWSER", "6.0.0", Icon::Globe, 48, 30) })
        }

        pub fn settings() -> Box<dyn App> {
            Box::new(Ring3Only {
                info: AppInfo::localized("nc.settings", "Settings", TextId::AppSettings, "2.8.0", Icon::Sliders, 48, 36),
            })
        }

        pub fn system() -> Box<dyn App> {
            Box::new(Ring3Only {
                info: AppInfo::localized("nc.system", "About this device", TextId::AppSystem, "1.0.0", Icon::Monitor, 30, 24),
            })
        }

        pub fn stopwatch() -> Box<dyn App> {
            Box::new(Ring3Only {
                info: AppInfo::localized("nc.stopwatch", "Stopwatch", TextId::AppStopwatch, "0.1.0", Icon::Stopwatch, 26, 16),
            })
        }
    }

    impl App for Ring3Only {
        fn info(&self) -> &AppInfo {
            &self.info
        }

        fn update(&mut self, _ctx: &mut Context, _input: &Input) -> Update {
            Update::NONE
        }

        fn draw(&self, _canvas: &mut Canvas<'_>, _area: Rect, _ctx: &Context) {}
    }
}

fn instantiate_from_pkg(pkg: &NtzcPackage) -> Box<dyn App> {
    match pkg.manifest.id.as_str() {
        "nc.files" => Box::new(FilesApp::new()),
        #[cfg(feature = "ring0-fallback")]
        "nc.browser" => Box::new(BrowserApp::new()),
        #[cfg(feature = "ring0-fallback")]
        "nc.settings" => Box::new(SettingsApp::new()),
        #[cfg(feature = "ring0-fallback")]
        "nc.system" => Box::new(SystemApp::new()),
        #[cfg(feature = "ring0-fallback")]
        "nc.stopwatch" => Box::new(StopwatchApp::new()),
        #[cfg(not(feature = "ring0-fallback"))]
        "nc.browser" => ring3_only::Ring3Only::browser(),
        #[cfg(not(feature = "ring0-fallback"))]
        "nc.settings" => ring3_only::Ring3Only::settings(),
        #[cfg(not(feature = "ring0-fallback"))]
        "nc.system" => ring3_only::Ring3Only::system(),
        #[cfg(not(feature = "ring0-fallback"))]
        "nc.stopwatch" => ring3_only::Ring3Only::stopwatch(),
        _ => create_packaged_app(pkg),
    }
}

fn scan_directory_for_packages(
    vfs: &mut VfsManager,
    dir_path: &str,
    installed_ids: &mut Vec<String>,
    apps: &mut Vec<Box<dyn App>>,
) {
    let entries = match vfs.list_dir_path(dir_path) {
        Ok(list) => list,
        Err(_) => return,
    };
    for entry in entries {
        if !entry.name.ends_with(".ntzc") {
            continue;
        }
        let full_path = alloc::format!("{}/{}", dir_path.trim_end_matches('/'), entry.name);
        if let Ok(bytes) = vfs.read_file_path(&full_path) {
            // F-01/F-02: yalnızca Ed25519 imzası DOĞRULANMIŞ paketler uygulama listesine girer.
            match NtzcPackage::parse_checked(&bytes) {
                Ok(pkg) if pkg.is_verified => {
                    if !installed_ids.iter().any(|id| id == &pkg.manifest.id) {
                        installed_ids.push(pkg.manifest.id.clone());
                        apps.push(instantiate_from_pkg(&pkg));
                    }
                }
                Ok(pkg) => {
                    // Yerleşik uygulamaların kodu çekirdek imajının içindedir; diskteki (imzasız)
                    // `files.ntzc` vb. dosyalar yalnızca üst veridir ve burada yok sayılır.
                    if !NtzcPackage::is_trusted_builtin(&pkg.manifest.id) {
                        log_rejected(&full_path, "unsigned package (NTZC v1)");
                    }
                }
                Err(reason) => log_rejected(&full_path, reason.as_str()),
            }
        }
    }
}

/// Reddedilen paketin nedenini seri günlüğe yazar.
fn log_rejected(path: &str, reason: &str) {
    nc_arch::serial::write_str("[pkg] reddedildi: ");
    nc_arch::serial::write_str(path);
    nc_arch::serial::write_str(" -- ");
    nc_arch::serial::write_str(reason);
    nc_arch::serial::write_str("\n");
}

pub fn builtin() -> Vec<Box<dyn App>> {
    nc_pkg::init_dysonsphere();
    let mut apps: Vec<Box<dyn App>> = Vec::new();
    let mut installed_ids: Vec<String> = Vec::new();
    let mut vfs = VfsManager::new();

    // Yerleşik uygulamalar çekirdek imajına derlenmiştir; kurulumun diske yazdığı imzasız
    // `*.ntzc` üst veri dosyalarına bağlı DEĞİLDİR (F-01: imzasız paketler artık yüklenmez).
    // Sıra, eski davranışla (dizin sırası) aynıdır.
    apps.push(Box::new(FilesApp::new()));
    #[cfg(feature = "ring0-fallback")]
    {
        apps.push(Box::new(BrowserApp::new()));
        apps.push(Box::new(SettingsApp::new()));
        apps.push(Box::new(SystemApp::new()));
        apps.push(Box::new(StopwatchApp::new()));
    }
    #[cfg(not(feature = "ring0-fallback"))]
    {
        apps.push(ring3_only::Ring3Only::browser());
        apps.push(ring3_only::Ring3Only::settings());
        apps.push(ring3_only::Ring3Only::system());
        apps.push(ring3_only::Ring3Only::stopwatch());
    }
    for id in ["nc.files", "nc.browser", "nc.settings", "nc.system", "nc.stopwatch"] {
        installed_ids.push(String::from(id));
    }

    // Üçüncü taraf paketler: yalnızca Ed25519 imzası doğrulanmış olanlar.
    scan_directory_for_packages(&mut vfs, "/system/app", &mut installed_ids, &mut apps);
    scan_directory_for_packages(&mut vfs, "/data/app", &mut installed_ids, &mut apps);

    apps
}
