#![no_std]

extern crate alloc;

use alloc::string::String;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

pub const LANGUAGE_COUNT: usize = 11;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Language {
    Turkish = 0,
    English = 1,
    French = 2,
    German = 3,
    Spanish = 4,
    Portuguese = 5,
    Russian = 6,
    Chinese = 7,
    Japanese = 8,
    Korean = 9,
    Hindi = 10,
}

impl Language {
    pub const ALL: [Language; LANGUAGE_COUNT] = [
        Language::English,
        Language::Turkish,
        Language::French,
        Language::German,
        Language::Spanish,
        Language::Portuguese,
        Language::Russian,
        Language::Chinese,
        Language::Japanese,
        Language::Korean,
        Language::Hindi,
    ];

    pub const fn from_u8(value: u8) -> Language {
        match value {
            1 => Language::English,
            2 => Language::French,
            3 => Language::German,
            4 => Language::Spanish,
            5 => Language::Portuguese,
            6 => Language::Russian,
            7 => Language::Chinese,
            8 => Language::Japanese,
            9 => Language::Korean,
            10 => Language::Hindi,
            _ => Language::Turkish,
        }
    }

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn code(self) -> &'static str {
        match self {
            Language::Turkish => "tr",
            Language::English => "en",
            Language::French => "fr",
            Language::German => "de",
            Language::Spanish => "es",
            Language::Portuguese => "pt",
            Language::Russian => "ru",
            Language::Chinese => "zh",
            Language::Japanese => "ja",
            Language::Korean => "ko",
            Language::Hindi => "hi",
        }
    }

    pub fn from_code(code: &str) -> Option<Language> {
        let mut i = 0;
        while i < LANGUAGE_COUNT {
            if Language::ALL[i].code() == code {
                return Some(Language::ALL[i]);
            }
            i += 1;
        }
        None
    }

    pub fn native_name(self) -> &'static str {
        TEXTS[self as usize][TextId::NativeName as usize]
    }

    pub const fn script_hint(self) -> u8 {
        match self {
            Language::Japanese => 1,
            Language::Korean => 2,
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum KeyboardLayout {
    TurkishQ = 0,
    UsQwerty = 1,
}

impl KeyboardLayout {
    pub const fn from_u8(value: u8) -> KeyboardLayout {
        match value {
            1 => KeyboardLayout::UsQwerty,
            _ => KeyboardLayout::TurkishQ,
        }
    }

    pub const fn label(self) -> TextId {
        match self {
            KeyboardLayout::TurkishQ => TextId::KbdNameTrQ,
            KeyboardLayout::UsQwerty => TextId::KbdNameUs,
        }
    }
}

pub fn tr(lang: Language, id: TextId) -> &'static str {
    TEXTS[lang as usize][id as usize]
}

pub fn trf(lang: Language, id: TextId, args: &[&str]) -> String {
    let template = tr(lang, id);
    let bytes = template.as_bytes();
    let mut out = String::with_capacity(template.len() + 16);
    let mut i = 0;
    while i < bytes.len() {
        if i + 2 < bytes.len()
            && bytes[i] == b'{'
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2] == b'}'
        {
            let n = (bytes[i + 1] - b'0') as usize;
            if let Some(arg) = args.get(n) {
                out.push_str(arg);
            }
            i += 3;
        } else if let Some(ch) = template[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}
