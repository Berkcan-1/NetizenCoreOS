#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use nc_app::App;
use nc_app_browser::BrowserApp;
use nc_app_files::{create_packaged_app, FilesApp};
use nc_app_settings::SettingsApp;
use nc_app_stopwatch::StopwatchApp;
use nc_app_system::SystemApp;
use nc_fs::VfsManager;
use nc_pkg::NtzcPackage;

fn instantiate_from_pkg(pkg: &NtzcPackage) -> Box<dyn App> {
    match pkg.manifest.id.as_str() {
        "nc.files" => Box::new(FilesApp::new()),
        "nc.browser" => Box::new(BrowserApp::new()),
        "nc.settings" => Box::new(SettingsApp::new()),
        "nc.system" => Box::new(SystemApp::new()),
        "nc.stopwatch" => Box::new(StopwatchApp::new()),
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
            if let Some(pkg) = NtzcPackage::parse(&bytes) {
                if !installed_ids.iter().any(|id| id == &pkg.manifest.id) {
                    installed_ids.push(pkg.manifest.id.clone());
                    apps.push(instantiate_from_pkg(&pkg));
                }
            }
        }
    }
}

pub fn builtin() -> Vec<Box<dyn App>> {
    let mut apps: Vec<Box<dyn App>> = Vec::new();
    let mut installed_ids: Vec<String> = Vec::new();
    let mut vfs = VfsManager::new();

    scan_directory_for_packages(&mut vfs, "/system/app", &mut installed_ids, &mut apps);
    scan_directory_for_packages(&mut vfs, "/data/app", &mut installed_ids, &mut apps);

    if !installed_ids.iter().any(|id| id == "nc.files") {
        installed_ids.push(String::from("nc.files"));
        apps.insert(0, Box::new(FilesApp::new()));
    }
    if !installed_ids.iter().any(|id| id == "nc.browser") {
        installed_ids.push(String::from("nc.browser"));
        apps.push(Box::new(BrowserApp::new()));
    }

    apps
}
