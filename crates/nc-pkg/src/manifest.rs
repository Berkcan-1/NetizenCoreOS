extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use nc_app::Icon;

#[derive(Clone, Debug)]
pub struct PackageManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub version_code: u32,
    pub icon: Icon,
    pub width_units: i32,
    pub height_units: i32,
    pub publisher_key_id: String,
    pub payload_hash: String,
    pub capabilities: Vec<String>,
    pub signature: String,
    pub raw_signed_manifest: String,
}

impl PackageManifest {
    pub fn parse(text: &str) -> Option<Self> {
        let mut id = String::new();
        let mut name = String::new();
        let mut version = String::from("1.0.0");
        let mut version_code = 1u32;
        let mut icon = Icon::Generic;
        let mut width_units = 32;
        let mut height_units = 20;
        let mut publisher_key_id = String::new();
        let mut payload_hash = String::new();
        let mut capabilities = Vec::new();
        let mut signature = String::new();
        let mut raw_signed_manifest = String::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if trimmed.starts_with("sig=") {
                signature = String::from(&trimmed[4..]);
                continue;
            }

            raw_signed_manifest.push_str(trimmed);
            raw_signed_manifest.push('\n');

            if let Some((key, val)) = trimmed.split_once('=') {
                let k = key.trim();
                let v = val.trim().trim_matches('"');
                match k {
                    "id" => id = String::from(v),
                    "name" => name = String::from(v),
                    "version" => version = String::from(v),
                    "version_code" => {
                        if let Ok(vc) = v.parse::<u32>() {
                            version_code = vc;
                        }
                    }
                    "icon" => {
                        icon = match v {
                            "sliders" => Icon::Sliders,
                            "monitor" => Icon::Monitor,
                            "stopwatch" => Icon::Stopwatch,
                            _ => Icon::Generic,
                        };
                    }
                    "width_units" => {
                        if let Ok(w) = v.parse::<i32>() {
                            width_units = w;
                        }
                    }
                    "height_units" => {
                        if let Ok(h) = v.parse::<i32>() {
                            height_units = h;
                        }
                    }
                    "publisher_key_id" => publisher_key_id = String::from(v),
                    "payload_hash" => payload_hash = String::from(v),
                    "capabilities" => {
                        for cap in v.split(',') {
                            let c = cap.trim();
                            if !c.is_empty() {
                                capabilities.push(String::from(c));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if id.is_empty() || name.is_empty() {
            return None;
        }

        Some(PackageManifest {
            id,
            name,
            version,
            version_code,
            icon,
            width_units,
            height_units,
            publisher_key_id,
            payload_hash,
            capabilities,
            signature,
            raw_signed_manifest,
        })
    }
}
