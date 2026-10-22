//! Ana makinede (gercek PC / QEMU konugu / VirtualBox konugu icindeki Linux dahil) calisan test koprusu.
//! `cargo test` ile: nc-app-browser/host-tests klasorunden calistirin.
//!
//! Gercek kaynak dosyalar `#[path]` ile dogrudan dahil edilir; kopya yoktur.
extern crate alloc;

/// `css.rs` nc-gfx'e bagli oldugu icin sadece html.rs'in kullandigi yardimci burada aynalanir.
/// (Govde css.rs::floor_utf8_boundary ile birebir aynidir.)
pub mod css {
    pub fn floor_utf8_boundary(s: &str, index: usize) -> usize {
        let mut i = index.min(s.len());
        while i > 0 && !s.is_char_boundary(i) {
            i -= 1;
        }
        i
    }
}

/// nc-font'a bagli `font_cov.rs`'in ana makine aynasi. nc-font README'sindeki repertuvari taklit eder;
/// GERCEK kapsam testi `nc-font/src/tests.rs::has_glyph_matches_the_drawn_repertoire`'dir.
pub mod font_cov {
    pub fn has_glyph(ch: char) -> bool {
        let c = ch as u32;
        matches!(c,
            0xA0..=0x17F      // Latin-1 + Latin Extended-A (Turkce dahil)
            | 0x300..=0x36F   // birlestirici aksanlar
            | 0x391..=0x3C9   // Yunanca
            | 0x400..=0x4FF   // Kiril
            | 0x200B..=0x200F // sifir genislik
            | 0x20AC | 0x20BA | 0x2713)
    }
}

#[path = "../../src/dom.rs"]
pub mod dom;
#[path = "../../src/html.rs"]
pub mod html;
#[path = "../../src/url.rs"]
pub mod url;
pub mod gif_fixtures;
#[path = "../../src/gif.rs"]
pub mod gif;
#[path = "../../src/cssvalues.rs"]
pub mod cssvalues;
#[path = "../../src/supports.rs"]
pub mod supports;

#[cfg(test)]
mod sync_tests {
    /// `CSS.supports()` in prelude.js must expose exactly the properties css/layout really apply.
    #[test]
    fn supports_list_matches_prelude_js() {
        let js = include_str!("../../src/prelude.js");
        let line = js
            .lines()
            .find(|l| l.contains("@@css-supported"))
            .expect("prelude.js must contain the @@css-supported marker line");
        let start = line.find('\'').expect("quoted list") + 1;
        let end = line[start..].find('\'').expect("closing quote") + start;
        let mut js_list: Vec<&str> = line[start..end].split(' ').filter(|s| !s.is_empty()).collect();
        let mut rs_list: Vec<&str> = crate::supports::SUPPORTED_PROPERTIES.to_vec();
        js_list.sort();
        rs_list.sort();
        assert_eq!(js_list, rs_list);
    }
}
