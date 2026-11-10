use std::collections::HashSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

const ORDER: [&str; 11] = ["tr", "en", "fr", "de", "es", "pt", "ru", "zh", "ja", "ko", "hi"];

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse(raw: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(pos) = line.find('=') {
            let key = line[..pos].trim().to_string();
            let value = unescape(line[pos + 1..].trim());
            out.push((key, value));
        }
    }
    out
}

fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric())
}

fn placeholders(value: &str) -> Vec<char> {
    let bytes = value.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1].is_ascii_digit() && bytes[i + 2] == b'}' {
            found.push(bytes[i + 1] as char);
            i += 3;
        } else {
            i += 1;
        }
    }
    found.sort();
    found
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=build.rs");

    let mut tables: Vec<Vec<(String, String)>> = Vec::new();
    for code in ORDER.iter() {
        let path = manifest.join("langs").join(format!("{}.lang", code));
        println!("cargo:rerun-if-changed={}", path.display());
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("nc-i18n: missing language file {}", path.display()));
        tables.push(parse(&raw));
    }

    let en_index = ORDER.iter().position(|c| *c == "en").unwrap();
    let master: Vec<(String, String)> = tables[en_index].clone();
    let mut seen = HashSet::new();
    for (key, _) in master.iter() {
        if !valid_key(key) {
            panic!("nc-i18n: invalid key name '{}' in en.lang", key);
        }
        if !seen.insert(key.clone()) {
            panic!("nc-i18n: duplicate key '{}' in en.lang", key);
        }
    }

    let mut code = String::new();
    code.push_str("#[derive(Clone, Copy, PartialEq, Eq, Debug)]\n#[repr(usize)]\npub enum TextId {\n");
    for (index, (key, _)) in master.iter().enumerate() {
        let _ = writeln!(code, "    {} = {},", key, index);
    }
    code.push_str("}\n\n");
    let _ = writeln!(code, "pub const TEXT_COUNT: usize = {};\n", master.len());
    let _ = writeln!(
        code,
        "pub static TEXTS: [[&str; {}]; {}] = [",
        master.len(),
        ORDER.len()
    );

    for (lang_index, lang_code) in ORDER.iter().enumerate() {
        let table = &tables[lang_index];
        let mut known = HashSet::new();
        for (key, _) in table.iter() {
            if !seen.contains(key) {
                println!("cargo:warning=nc-i18n: {}.lang has unknown key '{}'", lang_code, key);
            }
            if !known.insert(key.clone()) {
                println!("cargo:warning=nc-i18n: {}.lang repeats key '{}'", lang_code, key);
            }
        }
        let mut missing = 0usize;
        code.push_str("    [\n");
        for (key, english) in master.iter() {
            let value = table.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            let chosen = match value {
                Some(v) if !v.is_empty() => {
                    if placeholders(&v) != placeholders(english) {
                        println!(
                            "cargo:warning=nc-i18n: {}.lang key '{}' has different placeholders than en.lang",
                            lang_code, key
                        );
                    }
                    v
                }
                _ => {
                    missing += 1;
                    english.clone()
                }
            };
            let _ = writeln!(code, "        {:?},", chosen);
        }
        code.push_str("    ],\n");
        if missing > 0 {
            println!(
                "cargo:warning=nc-i18n: {}.lang is missing {} keys (English is used for them)",
                lang_code, missing
            );
        }
    }
    code.push_str("];\n");

    fs::write(out_dir.join("generated.rs"), code).unwrap();
}
