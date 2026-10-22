//! `@supports` feature-query evaluation against what the engine really applies.
//!
//! The list below is derived from `layout.rs::apply_declaration` (every property with a match arm).
//! `prelude.js` mirrors it for `CSS.supports()`; the host test `supports_list_matches_prelude_js`
//! keeps both copies identical. When a property gains real support, add it here AND in prelude.js.
extern crate alloc;

use alloc::vec::Vec;

pub const SUPPORTED_PROPERTIES: &[&str] = &[
    "align-items", "align-self", "background", "background-color", "background-image", "border",
    "border-bottom", "border-bottom-color", "border-bottom-style", "border-bottom-width",
    "border-color", "border-left", "border-left-color", "border-left-style", "border-left-width",
    "border-radius", "border-right", "border-right-color", "border-right-style",
    "border-right-width", "border-style", "border-top", "border-top-color", "border-top-style",
    "border-top-width", "border-width", "bottom", "box-sizing", "color", "column-gap", "display",
    "flex", "flex-basis", "flex-direction", "flex-flow", "flex-grow", "flex-shrink", "flex-wrap",
    "float", "font", "font-size", "font-style", "font-weight", "gap", "grid-column",
    "grid-column-end", "grid-column-gap", "grid-gap", "grid-row-gap", "grid-template-columns",
    "height", "inset", "justify-content", "left", "letter-spacing", "line-height", "margin",
    "margin-bottom", "margin-left", "margin-right", "margin-top", "max-height", "max-width",
    "min-height", "min-width", "opacity", "overflow", "overflow-x", "overflow-y", "padding",
    "padding-bottom", "padding-left", "padding-right", "padding-top", "position", "right",
    "row-gap", "text-align", "text-decoration", "text-decoration-line", "text-transform", "top",
    "visibility", "white-space", "width", "z-index",
];

/// `display` values the cascade understands (see `apply_declaration`).
const DISPLAY_VALUES: &[&str] = &[
    "none", "block", "inline", "inline-block", "flex", "inline-flex", "grid", "inline-grid", "list-item",
    "flow-root", "table",
];

pub fn property_supported(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    n.starts_with("--") || SUPPORTED_PROPERTIES.iter().any(|p| *p == n)
}

pub fn declaration_supported(prop: &str, value: &str) -> bool {
    let p = prop.trim().to_ascii_lowercase();
    let v = value.trim().to_ascii_lowercase();
    if v.is_empty() || !property_supported(&p) {
        return false;
    }
    if p == "display" {
        return DISPLAY_VALUES.iter().any(|d| *d == v) || v == "initial" || v == "inherit" || v == "unset";
    }
    if p == "position" {
        // `sticky` is parsed but laid out as static, so a feature query must not claim it.
        return matches!(v.as_str(), "static" | "relative" | "absolute" | "fixed" | "initial" | "inherit" | "unset");
    }
    true
}

/// Evaluates the text after `@supports`: `(prop: value)`, `not`, `and`, `or`, nesting.
/// Anything unknown (`selector()`, `font-tech()`, malformed input) is **false** (fail closed).
pub fn evaluate(condition: &str) -> bool {
    eval(condition.trim(), 0)
}

fn eval(cond: &str, depth: usize) -> bool {
    let c = cond.trim();
    if c.is_empty() || depth > 16 {
        return false;
    }
    let lower = c.to_ascii_lowercase();
    if lower.starts_with("not ") || lower.starts_with("not(") {
        return !eval(c[3..].trim(), depth + 1);
    }
    // Top-level `or` binds looser than `and`; mixing without parentheses is invalid CSS, so we only
    // need to split on whichever keyword is present at depth 0.
    let parts_or = split_top(c, " or ");
    if parts_or.len() > 1 {
        return parts_or.iter().any(|p| eval(p, depth + 1));
    }
    let parts_and = split_top(c, " and ");
    if parts_and.len() > 1 {
        return parts_and.iter().all(|p| eval(p, depth + 1));
    }
    if c.starts_with('(') && matching_close(c, 0) == Some(c.len() - 1) {
        let inner = c[1..c.len() - 1].trim();
        if let Some(colon) = top_level_colon(inner) {
            return declaration_supported(&inner[..colon], &inner[colon + 1..]);
        }
        return eval(inner, depth + 1);
    }
    false
}

fn matching_close(s: &str, open_at: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, ch) in s.char_indices().skip_while(|(i, _)| *i < open_at) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn top_level_colon(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn split_top<'a>(s: &'a str, kw: &str) -> Vec<&'a str> {
    let lower = s.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let b = lower.as_bytes();
    let k = kw.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 && i + k.len() <= b.len() && &b[i..i + k.len()] == k {
            out.push(&s[start..i]);
            i += k.len();
            start = i;
            continue;
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown_properties() {
        assert!(evaluate("(display: flex)"));
        assert!(evaluate("(width: 10px)"));
        assert!(evaluate("(--brand: red)"));
        assert!(evaluate("(position: absolute)"));
        assert!(!evaluate("(position: sticky)"));
        assert!(!evaluate("(backdrop-filter: blur(2px))"));
        assert!(!evaluate("(transform: rotate(3deg))"));
        assert!(!evaluate("(display: contents)"));
    }

    #[test]
    fn boolean_operators_and_nesting() {
        assert!(evaluate("(display: grid) and (width: 1px)"));
        assert!(!evaluate("(display: grid) and (position: sticky)"));
        assert!(evaluate("(position: sticky) or (display: flex)"));
        assert!(evaluate("not (position: sticky)"));
        assert!(!evaluate("not (display: flex)"));
        assert!(evaluate("((display: flex))"));
        assert!(evaluate("(display: grid) and (not (position: sticky))"));
    }

    #[test]
    fn unknown_functions_and_garbage_fail_closed() {
        assert!(!evaluate("selector(:has(a))"));
        assert!(!evaluate("font-tech(color-COLRv1)"));
        assert!(!evaluate(""));
        assert!(!evaluate("((("));
    }
}
