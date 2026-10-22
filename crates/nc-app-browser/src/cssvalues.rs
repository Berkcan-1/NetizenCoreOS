//! Pure CSS value parsing (no layout, no drawing, no nc-* crates): host-testable.
//!
//! Provides what the old `parse_css_px` could not: every absolute/relative unit, `calc()`, `min()`,
//! `max()`, `clamp()`, `font-weight`, `line-height`, shorthand expansion and gradient colour stops.
//! All internal arithmetic is fixed point (x1000) in i64, so nothing here can panic on overflow.
extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Context needed to resolve relative units. All values are CSS px (integers).
#[derive(Clone, Copy, Debug)]
pub struct LenCtx {
    /// Font size of the element in CSS px (`em`).
    pub font_px: i32,
    /// Root font size in CSS px (`rem`).
    pub root_px: i32,
    /// Base for percentages (usually the containing block width).
    pub percent_base: i32,
    pub viewport_w: i32,
    pub viewport_h: i32,
}

#[derive(Clone, Copy, Debug)]
struct Q {
    /// value x1000 (px for lengths)
    v: i64,
    len: bool,
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
    ctx: &'a LenCtx,
    depth: u8,
}

const MAX_DEPTH: u8 = 12;

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn expr(&mut self) -> Option<Q> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return None;
        }
        let mut acc = self.term()?;
        loop {
            self.ws();
            match self.peek() {
                Some(b'+') => {
                    self.i += 1;
                    let r = self.term()?;
                    if acc.len != r.len {
                        return None;
                    }
                    acc.v = acc.v.saturating_add(r.v);
                }
                Some(b'-') => {
                    self.i += 1;
                    let r = self.term()?;
                    if acc.len != r.len {
                        return None;
                    }
                    acc.v = acc.v.saturating_sub(r.v);
                }
                _ => break,
            }
        }
        self.depth -= 1;
        Some(acc)
    }

    fn term(&mut self) -> Option<Q> {
        let mut acc = self.factor()?;
        loop {
            self.ws();
            match self.peek() {
                Some(b'*') => {
                    self.i += 1;
                    let r = self.factor()?;
                    if acc.len && r.len {
                        return None;
                    }
                    acc = Q { v: acc.v.saturating_mul(r.v) / 1000, len: acc.len || r.len };
                }
                Some(b'/') => {
                    self.i += 1;
                    let r = self.factor()?;
                    if r.len || r.v == 0 {
                        return None;
                    }
                    acc = Q { v: acc.v.saturating_mul(1000) / r.v, len: acc.len };
                }
                _ => break,
            }
        }
        Some(acc)
    }

    fn factor(&mut self) -> Option<Q> {
        self.ws();
        match self.peek()? {
            b'(' => {
                self.i += 1;
                let q = self.expr()?;
                self.ws();
                if self.peek() != Some(b')') {
                    return None;
                }
                self.i += 1;
                Some(q)
            }
            c if c.is_ascii_alphabetic() => self.func(),
            _ => self.dimension(),
        }
    }

    fn func(&mut self) -> Option<Q> {
        let start = self.i;
        while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'-') {
            self.i += 1;
        }
        let name: String = self.s[start..self.i].iter().map(|b| b.to_ascii_lowercase() as char).collect();
        if self.peek() != Some(b'(') {
            return None;
        }
        self.i += 1;
        let mut args: Vec<Q> = Vec::new();
        loop {
            let q = self.expr()?;
            args.push(q);
            self.ws();
            match self.peek()? {
                b',' => {
                    self.i += 1;
                }
                b')' => {
                    self.i += 1;
                    break;
                }
                _ => return None,
            }
        }
        match name.as_str() {
            "calc" | "-webkit-calc" | "-moz-calc" => {
                if args.len() == 1 {
                    Some(args[0])
                } else {
                    None
                }
            }
            "min" | "max" => {
                let first = *args.first()?;
                let mut best = first;
                for a in args.iter() {
                    if a.len != first.len {
                        return None;
                    }
                    if (name == "min" && a.v < best.v) || (name == "max" && a.v > best.v) {
                        best = *a;
                    }
                }
                Some(best)
            }
            "clamp" => {
                if args.len() != 3 || args[0].len != args[1].len || args[1].len != args[2].len {
                    return None;
                }
                let (lo, val, hi) = (args[0].v, args[1].v, args[2].v);
                // CSS: max(lo, min(val, hi)); when lo > hi the lower bound wins.
                let capped = if val > hi { hi } else { val };
                Some(Q { v: if capped < lo { lo } else { capped }, len: args[1].len })
            }
            _ => None,
        }
    }

    fn dimension(&mut self) -> Option<Q> {
        let start = self.i;
        let mut neg = false;
        match self.peek()? {
            b'-' => {
                neg = true;
                self.i += 1;
            }
            b'+' => {
                self.i += 1;
            }
            _ => {}
        }
        let mut int: i64 = 0;
        let mut digits = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                int = int.saturating_mul(10).saturating_add((c - b'0') as i64);
                digits = true;
                self.i += 1;
            } else {
                break;
            }
        }
        let mut frac: i64 = 0;
        let mut frac_digits = 0;
        if self.peek() == Some(b'.') {
            self.i += 1;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    if frac_digits < 3 {
                        frac = frac * 10 + (c - b'0') as i64;
                        frac_digits += 1;
                    }
                    digits = true;
                    self.i += 1;
                } else {
                    break;
                }
            }
        }
        if !digits {
            self.i = start;
            return None;
        }
        while frac_digits < 3 {
            frac *= 10;
            frac_digits += 1;
        }
        let mut num = int.saturating_mul(1000).saturating_add(frac);
        if neg {
            num = -num;
        }
        let ustart = self.i;
        if self.peek() == Some(b'%') {
            self.i += 1;
            return Some(Q { v: num.saturating_mul(self.ctx.percent_base as i64) / 100, len: true });
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() {
                self.i += 1;
            } else {
                break;
            }
        }
        let unit: String = self.s[ustart..self.i].iter().map(|b| b.to_ascii_lowercase() as char).collect();
        let c = self.ctx;
        let px = |mul: i64, div: i64| Q { v: num.saturating_mul(mul) / div, len: true };
        match unit.as_str() {
            "" => Some(Q { v: num, len: false }),
            "px" => Some(px(1, 1)),
            "em" => Some(px(c.font_px as i64, 1)),
            "rem" => Some(px(c.root_px as i64, 1)),
            // ex and ch are approximated as half an em, which is what browsers do without font metrics.
            "ex" | "ch" => Some(px(c.font_px as i64, 2)),
            "vw" | "svw" | "lvw" | "dvw" => Some(px(c.viewport_w as i64, 100)),
            "vh" | "svh" | "lvh" | "dvh" => Some(px(c.viewport_h as i64, 100)),
            "vmin" => Some(px(c.viewport_w.min(c.viewport_h) as i64, 100)),
            "vmax" => Some(px(c.viewport_w.max(c.viewport_h) as i64, 100)),
            "pt" => Some(px(4, 3)),
            "pc" => Some(px(16, 1)),
            "in" => Some(px(96, 1)),
            "cm" => Some(px(3780, 100)),
            "mm" => Some(px(378, 100)),
            "q" => Some(px(945, 1000)),
            _ => None,
        }
    }
}

fn run(raw: &str, ctx: &LenCtx) -> Option<Q> {
    let mut p = P { s: raw.trim().as_bytes(), i: 0, ctx, depth: 0 };
    let q = p.expr()?;
    p.ws();
    if p.i != p.s.len() {
        return None;
    }
    Some(q)
}

fn round_px(v: i64) -> i32 {
    let r = if v >= 0 { (v + 500) / 1000 } else { (v - 500) / 1000 };
    r.clamp(i32::MIN as i64 / 2, i32::MAX as i64 / 2) as i32
}

/// Resolves a CSS length expression to whole px. Unitless numbers are accepted as px (legacy pages).
/// `auto`, `none`, `normal` and anything unparsable return `None`.
pub fn eval_length(raw: &str, ctx: &LenCtx) -> Option<i32> {
    let q = run(raw, ctx)?;
    Some(round_px(q.v))
}

/// Unitless number x1000 (`1.5` -> 1500). Rejects lengths.
pub fn eval_number(raw: &str, ctx: &LenCtx) -> Option<i64> {
    let q = run(raw, ctx)?;
    if q.len {
        None
    } else {
        Some(q.v)
    }
}

/// `font-weight` -> 1..=1000. `parent` is the inherited weight for `bolder` / `lighter`.
pub fn font_weight(raw: &str, parent: i32) -> Option<i32> {
    let v = raw.trim().to_ascii_lowercase();
    match v.as_str() {
        "normal" => Some(400),
        "bold" => Some(700),
        "bolder" => Some(if parent < 400 { 400 } else if parent < 600 { 700 } else { 900 }),
        "lighter" => Some(if parent < 600 { 100 } else if parent < 800 { 400 } else { 700 }),
        "inherit" | "unset" => Some(parent),
        "initial" => Some(400),
        _ => {
            let ctx = LenCtx { font_px: 16, root_px: 16, percent_base: 0, viewport_w: 0, viewport_h: 0 };
            let n = eval_number(&v, &ctx)?;
            Some(((n + 500) / 1000).clamp(1, 1000) as i32)
        }
    }
}

/// `line-height` as a multiplier x100 of the element's font size (`1.5` -> 150).
/// `normal` (and anything unparsable) returns `None`, meaning "use the engine default".
pub fn line_height_x100(raw: &str, ctx: &LenCtx) -> Option<i32> {
    let v = raw.trim().to_ascii_lowercase();
    if v == "normal" || v.is_empty() {
        return None;
    }
    if let Some(pct) = v.strip_suffix('%') {
        // Percentages are relative to the element's own font size, not the containing block.
        let n = eval_number(pct, ctx)?;
        return Some(((n + 5) / 10).clamp(50, 600) as i32);
    }
    if let Some(n) = eval_number(&v, ctx) {
        return Some(((n + 5) / 10).clamp(50, 600) as i32);
    }
    let px = eval_length(&v, ctx)?;
    if ctx.font_px <= 0 {
        return None;
    }
    Some((px * 100 / ctx.font_px).clamp(50, 600))
}

/// Splits on `sep` outside parentheses.
pub fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == sep && depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// Whitespace-separated tokens, keeping `fn(...)` groups intact.
pub fn tokens(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start: Option<usize> = None;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => {
                depth += 1;
                if start.is_none() {
                    start = Some(i);
                }
            }
            ')' => depth -= 1,
            c if c.is_whitespace() && depth <= 0 => {
                if let Some(st) = start.take() {
                    out.push(&s[st..i]);
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(i);
                }
            }
        }
    }
    if let Some(st) = start {
        out.push(&s[st..]);
    }
    out
}

/// 1-4 value box shorthand -> (top, right, bottom, left).
pub fn box4(val: &str) -> Option<[String; 4]> {
    let t = tokens(val);
    let g = |i: usize| String::from(t[i]);
    match t.len() {
        1 => Some([g(0), g(0), g(0), g(0)]),
        2 => Some([g(0), g(1), g(0), g(1)]),
        3 => Some([g(0), g(1), g(2), g(1)]),
        4 => Some([g(0), g(1), g(2), g(3)]),
        _ => None,
    }
}

fn is_border_style(t: &str) -> bool {
    matches!(
        t,
        "none" | "hidden" | "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "inset" | "outset"
    )
}

fn is_width_token(t: &str) -> bool {
    matches!(t, "thin" | "medium" | "thick")
        || t.starts_with(|c: char| c.is_ascii_digit() || c == '.' || c == '+' || c == '-')
        || t.starts_with("calc(")
}

fn is_image_token(t: &str) -> bool {
    t.starts_with("url(")
        || t.starts_with("linear-gradient(")
        || t.starts_with("radial-gradient(")
        || t.starts_with("conic-gradient(")
        || t.starts_with("repeating-linear-gradient(")
        || t.starts_with("repeating-radial-gradient(")
        || t.starts_with("image-set(")
}

/// Expands shorthands the layout does not understand into longhands it does.
/// Returns `None` when `prop` is not one of the shorthands handled here.
pub fn expand(prop: &str, val: &str) -> Option<Vec<(String, String)>> {
    let v = val.trim();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |p: &str, x: &str| out.push((String::from(p), String::from(x)));
    match prop {
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            let side = prop.strip_prefix("border-").unwrap_or("");
            let name = |part: &str| if side.is_empty() { format!("border-{}", part) } else { format!("border-{}-{}", side, part) };
            let (mut w, mut s, mut c) = (None, None, None);
            for t in tokens(v) {
                let l = t.to_ascii_lowercase();
                if is_border_style(&l) {
                    s = Some(l);
                } else if is_width_token(&l) {
                    w = Some(l);
                } else {
                    c = Some(String::from(t));
                }
            }
            // CSS initial values: width medium, style none, colour currentcolor.
            push(&name("width"), w.as_deref().unwrap_or("medium"));
            push(&name("style"), s.as_deref().unwrap_or("none"));
            if let Some(c) = c {
                push(&name("color"), &c);
            }
        }
        "overflow" => {
            let t = tokens(v);
            let x = t.first().copied().unwrap_or("visible");
            let y = t.get(1).copied().unwrap_or(x);
            push("overflow-x", x);
            push("overflow-y", y);
        }
        "inset" => {
            let b = box4(v)?;
            push("top", &b[0]);
            push("right", &b[1]);
            push("bottom", &b[2]);
            push("left", &b[3]);
        }
        "text-decoration" => {
            let (mut line, mut color) = (String::new(), None);
            for t in tokens(v) {
                let l = t.to_ascii_lowercase();
                match l.as_str() {
                    "none" | "underline" | "overline" | "line-through" | "blink" => {
                        if !line.is_empty() {
                            line.push(' ');
                        }
                        line.push_str(&l);
                    }
                    "solid" | "double" | "dotted" | "dashed" | "wavy" => {}
                    _ => color = Some(String::from(t)),
                }
            }
            push("text-decoration-line", if line.is_empty() { "none" } else { &line });
            if let Some(c) = color {
                push("text-decoration-color", &c);
            }
        }
        "list-style" => {
            for t in tokens(v) {
                let l = t.to_ascii_lowercase();
                if !matches!(l.as_str(), "inside" | "outside") && !l.starts_with("url(") {
                    push("list-style-type", &l);
                    break;
                }
            }
        }
        "background" => {
            for t in tokens(v) {
                let l = t.to_ascii_lowercase();
                if is_image_token(&l) {
                    push("background-image", t);
                } else if l.starts_with(|c: char| c.is_ascii_digit() || c == '.' || c == '-')
                    || matches!(
                        l.as_str(),
                        "repeat" | "no-repeat" | "repeat-x" | "repeat-y" | "space" | "round" | "center" | "top"
                            | "bottom" | "left" | "right" | "cover" | "contain" | "fixed" | "scroll" | "local"
                            | "border-box" | "padding-box" | "content-box" | "auto" | "none" | "/"
                    )
                    || l.starts_with("calc(")
                {
                    // position / size / repeat / attachment: not modelled
                } else {
                    push("background-color", t);
                }
            }
        }
        "font" => {
            let (mut style, mut weight) = (None, None);
            let toks = tokens(v);
            let mut i = 0;
            while i < toks.len() {
                let l = toks[i].to_ascii_lowercase();
                match l.as_str() {
                    "italic" | "oblique" => style = Some(l),
                    "normal" | "small-caps" | "condensed" | "expanded" => {}
                    "bold" | "bolder" | "lighter" => weight = Some(l),
                    _ => {
                        // A bare integer can only be a weight (a font size always carries a unit).
                        let is_num = !l.contains('/') && l.parse::<u32>().is_ok();
                        if is_num {
                            weight = Some(l);
                        } else {
                            // size[/line-height] then family
                            let mut parts = toks[i].splitn(2, '/');
                            let size = parts.next().unwrap_or("");
                            push("font-size", size);
                            if let Some(lh) = parts.next() {
                                push("line-height", lh);
                            }
                            break;
                        }
                    }
                }
                i += 1;
            }
            push("font-style", style.as_deref().unwrap_or("normal"));
            push("font-weight", weight.as_deref().unwrap_or("normal"));
        }
        _ => return None,
    }
    Some(out)
}

/// Colour stops of a gradient as raw colour strings, in order (positions and hints removed).
pub fn gradient_colors(val: &str) -> Vec<String> {
    let v = val.trim();
    let open = match v.find('(') {
        Some(o) => o,
        None => return Vec::new(),
    };
    if !v[..open].to_ascii_lowercase().contains("gradient") {
        return Vec::new();
    }
    let inner = match v[open + 1..].rfind(')') {
        Some(c) => &v[open + 1..open + 1 + c],
        None => &v[open + 1..],
    };
    let mut out = Vec::new();
    for (i, arg) in split_top(inner, ',').iter().enumerate() {
        let a = arg.trim();
        let l = a.to_ascii_lowercase();
        if i == 0
            && (l.starts_with("to ")
                || l.ends_with("deg")
                || l.ends_with("turn")
                || l.ends_with("rad")
                || l.ends_with("grad")
                || l.starts_with("circle")
                || l.starts_with("ellipse")
                || l.starts_with("closest")
                || l.starts_with("farthest")
                || l.starts_with("from ")
                || l.starts_with("in "))
        {
            continue;
        }
        let t = tokens(a);
        if let Some(first) = t.first() {
            // A lone length/percentage is a colour hint, not a stop.
            if first.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
                continue;
            }
            out.push(String::from(*first));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> LenCtx {
        LenCtx { font_px: 20, root_px: 16, percent_base: 800, viewport_w: 1000, viewport_h: 600 }
    }

    #[test]
    fn units() {
        let c = ctx();
        assert_eq!(eval_length("10px", &c), Some(10));
        assert_eq!(eval_length("2em", &c), Some(40));
        assert_eq!(eval_length("1.5rem", &c), Some(24));
        assert_eq!(eval_length("50%", &c), Some(400));
        assert_eq!(eval_length("10vw", &c), Some(100));
        assert_eq!(eval_length("50vh", &c), Some(300));
        assert_eq!(eval_length("10vmin", &c), Some(60));
        assert_eq!(eval_length("1in", &c), Some(96));
        assert_eq!(eval_length("12pt", &c), Some(16));
        assert_eq!(eval_length("-4px", &c), Some(-4));
        assert_eq!(eval_length("auto", &c), None);
        assert_eq!(eval_length("10qq", &c), None);
    }

    #[test]
    fn calc_min_max_clamp() {
        let c = ctx();
        assert_eq!(eval_length("calc(100% - 20px)", &c), Some(780));
        assert_eq!(eval_length("calc(2 * (10px + 5px))", &c), Some(30));
        assert_eq!(eval_length("calc(100px / 4)", &c), Some(25));
        assert_eq!(eval_length("min(50%, 300px)", &c), Some(300));
        assert_eq!(eval_length("max(10px, 2em)", &c), Some(40));
        assert_eq!(eval_length("clamp(100px, 50vw, 400px)", &c), Some(400));
        assert_eq!(eval_length("clamp(100px, 5vw, 400px)", &c), Some(100));
        assert_eq!(eval_length("calc(10px + 5)", &c), None);
        assert_eq!(eval_length("calc(10px / 0)", &c), None);
        assert_eq!(eval_length("calc(((((((((((((1px)))))))))))))", &c), None);
    }

    #[test]
    fn numbers_weights_line_heights() {
        let c = ctx();
        assert_eq!(eval_number("1.5", &c), Some(1500));
        assert_eq!(eval_number("2px", &c), None);
        assert_eq!(font_weight("bold", 400), Some(700));
        assert_eq!(font_weight("600", 400), Some(600));
        assert_eq!(font_weight("bolder", 400), Some(700));
        assert_eq!(font_weight("lighter", 700), Some(400));
        assert_eq!(line_height_x100("1.5", &c), Some(150));
        assert_eq!(line_height_x100("30px", &c), Some(150));
        assert_eq!(line_height_x100("150%", &c), Some(150));
        assert_eq!(line_height_x100("normal", &c), None);
    }

    #[test]
    fn shorthands() {
        let b = expand("border", "1px solid #ccc").unwrap();
        assert!(b.contains(&(String::from("border-width"), String::from("1px"))));
        assert!(b.contains(&(String::from("border-style"), String::from("solid"))));
        assert!(b.contains(&(String::from("border-color"), String::from("#ccc"))));
        let bb = expand("border-bottom", "2px dashed red").unwrap();
        assert!(bb.contains(&(String::from("border-bottom-width"), String::from("2px"))));
        let none = expand("border", "none").unwrap();
        assert!(none.contains(&(String::from("border-style"), String::from("none"))));
        let o = expand("overflow", "hidden auto").unwrap();
        assert_eq!(o[0], (String::from("overflow-x"), String::from("hidden")));
        assert_eq!(o[1], (String::from("overflow-y"), String::from("auto")));
        let i = expand("inset", "0 10px").unwrap();
        assert_eq!(i[1], (String::from("right"), String::from("10px")));
        let f = expand("font", "italic bold 14px/1.4 Arial, sans-serif").unwrap();
        assert!(f.contains(&(String::from("font-size"), String::from("14px"))));
        assert!(f.contains(&(String::from("line-height"), String::from("1.4"))));
        assert!(f.contains(&(String::from("font-weight"), String::from("bold"))));
        assert!(f.contains(&(String::from("font-style"), String::from("italic"))));
        let bg = expand("background", "#fff url(a.png) no-repeat center / cover").unwrap();
        assert!(bg.contains(&(String::from("background-color"), String::from("#fff"))));
        assert!(bg.contains(&(String::from("background-image"), String::from("url(a.png)"))));
        let td = expand("text-decoration", "underline red").unwrap();
        assert!(td.contains(&(String::from("text-decoration-line"), String::from("underline"))));
        assert!(expand("margin", "1px").is_none());
    }

    #[test]
    fn gradients() {
        let g = gradient_colors("linear-gradient(to right, #fff 0%, rgb(1, 2, 3) 50%, #000)");
        assert_eq!(g, alloc::vec![String::from("#fff"), String::from("rgb(1, 2, 3)"), String::from("#000")]);
        let g2 = gradient_colors("linear-gradient(45deg, red, blue)");
        assert_eq!(g2.len(), 2);
        assert!(gradient_colors("url(a.png)").is_empty());
    }

    #[test]
    fn tokenizer_keeps_functions_together() {
        assert_eq!(tokens("1px solid rgba(0, 0, 0, .5)"), alloc::vec!["1px", "solid", "rgba(0, 0, 0, .5)"]);
        assert_eq!(split_top("a, b(c, d), e", ','), alloc::vec!["a", " b(c, d)", " e"]);
    }
}
