use super::*;
use std::vec::Vec;

fn ink(text: &str, cap: i32) -> (i32, i32, i32, i32, u32) {
    let style = Style::new(cap);
    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    let mut total: u32 = 0;
    draw_text(text, 0, 0, &style, &mut |x, y, c| {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        total += c as u32;
    });
    (min_x, max_x, min_y, max_y, total)
}

#[test]
fn every_entry_draws_within_bounds() {
    for entry in data::ENTRIES.iter() {
        for cap in [7, 10, 16, 40] {
            let mut count = 0;
            let style = Style::new(cap);
            raster::draw_glyph(entry, 0, 0, cap, style.stroke_sixteenths, &mut |_, _, _| count += 1);
            if entry.part_count > 0 {
                assert!(count > 0, "glyph {:#X} produced no pixels at {}", entry.code, cap);
            }
        }
    }
}

#[test]
fn entries_are_sorted_and_unique() {
    let codes: Vec<u32> = data::ENTRIES.iter().map(|entry| entry.code).collect();
    for pair in codes.windows(2) {
        assert!(pair[0] < pair[1]);
    }
}

#[test]
fn width_grows_with_text() {
    let style = Style::new(12);
    assert_eq!(text_width("", &style), 0);
    assert!(text_width("ab", &style) > text_width("a", &style));
    assert!(text_width("Hesap Makinesi", &style) > 60);
}

#[test]
fn turkish_and_other_scripts_are_covered() {
    for ch in "çğıöşüÇĞİÖŞÜ₺ąćłñßÅØáéíóúΑΩαωЖжЯя₽€".chars() {
        assert!(data::find(ch as u32).is_some(), "missing {}", ch);
    }
}

#[test]
fn decomposed_text_matches_precomposed() {
    let style = Style::new(12);
    assert_eq!(text_width("e\u{0301}", &style), text_width("é", &style));
}

#[test]
fn capital_dotted_i_rises_above_cap_line() {
    let plain = ink("I", 12);
    let dotted = ink("İ", 12);
    assert!(dotted.2 < plain.2);
}

#[test]
fn baseline_and_cap_line_are_crisp() {
    let (_, _, min_y, max_y, _) = ink("H", 12);
    assert!(min_y >= -1 && max_y <= 12);
}
