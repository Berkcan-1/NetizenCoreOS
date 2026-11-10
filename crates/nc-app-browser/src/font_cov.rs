//! Single seam between the HTML layer and the built-in font's repertoire.
//! Kept separate so `html.rs` stays free of `nc-font` and can be unit-tested on a host machine.

/// `true` when the built-in font can draw `ch` (or composes / intentionally hides it).
pub fn has_glyph(ch: char) -> bool {
    nc_font::has_glyph(ch)
}
