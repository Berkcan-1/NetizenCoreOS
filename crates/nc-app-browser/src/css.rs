extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use nc_gfx::rgb;
use crate::dom::DomTree;

pub trait SafeClamp {
    fn sclamp(self, lo: Self, hi: Self) -> Self;
}

impl SafeClamp for i32 {
    fn sclamp(self, lo: i32, hi: i32) -> i32 {
        if hi < lo {
            hi
        } else if self < lo {
            lo
        } else if self > hi {
            hi
        } else {
            self
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
    Adjacent,
    Sibling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrOp {
    Exists,
    Eq,
    Includes,
    Dash,
    Prefix,
    Suffix,
    Substring,
}

#[derive(Clone, Debug)]
pub struct AttrSel {
    pub name: String,
    pub op: AttrOp,
    pub value: String,
    pub ci: bool,
}

#[derive(Clone, Debug)]
pub enum Pseudo {
    Root,
    Empty,
    FirstChild,
    LastChild,
    OnlyChild,
    FirstOfType,
    LastOfType,
    OnlyOfType,
    NthChild(i32, i32),
    NthLastChild(i32, i32),
    NthOfType(i32, i32),
    NthLastOfType(i32, i32),
    Not(Vec<SelectorPart>),
    Is(Vec<SelectorPart>),
    Where(Vec<SelectorPart>),
    Link,
    Disabled,
    Enabled,
    Checked,
    Hover,
    Focus,
    FocusWithin,
    /// Pseudo-elements (`::before`) and pseudo-classes whose state the engine does not model.
    /// A selector containing it never matches.
    Never,
}

/// One compound selector (`div.card[data-x]:first-child`) plus how it relates to the compound on its left.
#[derive(Clone, Debug)]
pub struct SelectorPart {
    pub combinator: Combinator,
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attrs: Vec<AttrSel>,
    pub pseudos: Vec<Pseudo>,
}

#[derive(Clone, Debug)]
pub struct Selector {
    pub chain: Vec<SelectorPart>,
}

fn part_specificity(p: &SelectorPart) -> (u16, u16, u16) {
    let mut ids: u16 = if p.id.is_some() { 1 } else { 0 };
    let mut classes: u16 = (p.classes.len() + p.attrs.len()) as u16;
    let mut tags: u16 = match &p.tag {
        Some(t) if t != "*" => 1,
        _ => 0,
    };
    for ps in &p.pseudos {
        match ps {
            Pseudo::Where(_) => {}
            Pseudo::Not(args) | Pseudo::Is(args) => {
                let mut best = (0u16, 0u16, 0u16);
                for a in args {
                    let s = part_specificity(a);
                    if s > best {
                        best = s;
                    }
                }
                ids = ids.saturating_add(best.0);
                classes = classes.saturating_add(best.1);
                tags = tags.saturating_add(best.2);
            }
            _ => classes = classes.saturating_add(1),
        }
    }
    (ids, classes, tags)
}

impl Selector {
    pub fn specificity(&self) -> (u16, u16, u16) {
        let mut total = (0u16, 0u16, 0u16);
        for part in &self.chain {
            let s = part_specificity(part);
            total.0 = total.0.saturating_add(s.0);
            total.1 = total.1.saturating_add(s.1);
            total.2 = total.2.saturating_add(s.2);
        }
        total
    }

    fn uses_pointer_or_focus_state(&self) -> bool {
        fn part_uses(p: &SelectorPart) -> bool {
            p.pseudos.iter().any(|ps| match ps {
                Pseudo::Hover | Pseudo::Focus | Pseudo::FocusWithin => true,
                Pseudo::Not(a) | Pseudo::Is(a) | Pseudo::Where(a) => a.iter().any(part_uses),
                _ => false,
            })
        }
        self.chain.iter().any(part_uses)
    }

    fn can_never_match(&self) -> bool {
        self.chain.iter().any(|p| p.pseudos.iter().any(|ps| matches!(ps, Pseudo::Never)))
    }
}

#[derive(Clone, Debug)]
pub struct Declaration {
    pub property: String,
    pub value: String,
    pub important: bool,
}

#[derive(Clone, Debug)]
pub struct CssRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
    /// Enclosing `@media` conditions (all must hold). Evaluated against the real viewport.
    pub media: Vec<String>,
    pub active: bool,
}

const MAX_RULES: usize = 1500;
const MAX_CSS_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug)]
pub struct Stylesheet {
    pub rules: Vec<CssRule>,
    pub variables: Vec<(String, String)>,
    viewport_w: i32,
    viewport_h: i32,
    /// True when at least one rule depends on `:hover` / `:focus` state, so the browser
    /// only pays the cost of pointer-triggered relayout for pages that need it.
    pub has_state_rules: bool,
}

impl Stylesheet {
    pub fn new() -> Self {
        Stylesheet {
            rules: Vec::new(),
            variables: Vec::new(),
            viewport_w: 1100,
            viewport_h: 800,
            has_state_rules: false,
        }
    }

    /// Re-evaluates every `@media` condition for the given viewport (CSS px). Cheap when unchanged.
    pub fn set_viewport(&mut self, w: i32, h: i32) {
        if w == self.viewport_w && h == self.viewport_h {
            return;
        }
        self.viewport_w = w;
        self.viewport_h = h;
        for rule in self.rules.iter_mut() {
            rule.active = rule.media.iter().all(|m| media_query_list_matches(m, w, h));
        }
    }

    pub fn append_css(&mut self, css_text: &str) {
        if self.rules.len() >= MAX_RULES {
            return;
        }
        let limit = floor_utf8_boundary(css_text, MAX_CSS_BYTES);
        let cleaned = strip_css_comments(&css_text[..limit]);
        self.parse_blocks(&cleaned, 0, &Vec::new());
    }

    fn parse_blocks(&mut self, input: &str, depth_level: usize, conds: &Vec<String>) {
        if depth_level > 4 || self.rules.len() >= MAX_RULES {
            return;
        }
        let bytes = input.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len && self.rules.len() < MAX_RULES {
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= len {
                break;
            }

            if bytes[i] == b'@' {
                let at_start = i;
                while i < len && bytes[i] != b'{' && bytes[i] != b';' {
                    i += 1;
                }
                if i < len && bytes[i] == b';' {
                    i += 1;
                    continue;
                }
                if i < len && bytes[i] == b'{' {
                    let header = safe_slice(input, at_start, i).trim();
                    i += 1;
                    let body_start = i;
                    let mut depth = 1usize;
                    while i < len && depth > 0 {
                        if bytes[i] == b'{' {
                            depth += 1;
                        } else if bytes[i] == b'}' {
                            depth -= 1;
                        }
                        i += 1;
                    }
                    let body_end = i.saturating_sub(1);
                    let lower = header.to_ascii_lowercase();
                    let inner = safe_slice(input, body_start, body_end);
                    if lower.starts_with("@media") {
                        let mut next = conds.clone();
                        next.push(String::from(safe_slice(header, 6, header.len()).trim()));
                        self.parse_blocks(inner, depth_level + 1, &next);
                    } else if lower.starts_with("@supports") {
                        // Evaluated against the properties the engine really applies (supports.rs),
                        // so pages fall back to their older CSS instead of getting a false "yes".
                        let cond = safe_slice(header, 9, header.len());
                        if crate::supports::evaluate(cond) {
                            self.parse_blocks(inner, depth_level + 1, conds);
                        }
                    } else if lower.starts_with("@layer") {
                        // Cascade layers are flattened into source order.
                        self.parse_blocks(inner, depth_level + 1, conds);
                    }
                    // @font-face, @keyframes, @page, @container ... are skipped.
                }
                continue;
            }

            let sel_start = i;
            while i < len && bytes[i] != b'{' {
                i += 1;
            }
            if i >= len {
                break;
            }
            let selector_str = safe_slice(input, sel_start, i).trim();
            i += 1;

            let decl_start = i;
            let mut depth = 1usize;
            while i < len && depth > 0 {
                if bytes[i] == b'{' {
                    depth += 1;
                } else if bytes[i] == b'}' {
                    depth -= 1;
                }
                i += 1;
            }
            let decl_end = i.saturating_sub(1);
            let decl_str = safe_slice(input, decl_start, decl_end);

            let declarations = parse_declarations(decl_str);
            for d in &declarations {
                if d.property.starts_with("--") && self.variables.len() < 256 {
                    if let Some(existing) = self.variables.iter_mut().find(|(k, _)| k == &d.property) {
                        existing.1 = d.value.clone();
                    } else {
                        self.variables.push((d.property.clone(), d.value.clone()));
                    }
                }
            }

            let selectors = parse_selectors(selector_str);
            if !selectors.is_empty() && !declarations.is_empty() {
                if selectors.iter().any(|s| s.uses_pointer_or_focus_state()) {
                    self.has_state_rules = true;
                }
                let active = conds.iter().all(|m| media_query_list_matches(m, self.viewport_w, self.viewport_h));
                self.rules.push(CssRule {
                    selectors,
                    declarations,
                    media: conds.clone(),
                    active,
                });
            }
        }
    }

    pub fn resolve_all_vars(&self, raw: &str) -> String {
        let mut cur = String::from(raw);
        for _ in 0..4 {
            let Some(start) = cur.find("var(") else {
                break;
            };
            let mut depth = 0i32;
            let mut end = None;
            {
                let bytes = cur.as_bytes();
                let mut i = start + 3;
                while i < bytes.len() {
                    if bytes[i] == b'(' {
                        depth += 1;
                    } else if bytes[i] == b')' {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(i);
                            break;
                        }
                    }
                    i += 1;
                }
            }
            let Some(end) = end else {
                break;
            };
            let replacement: String = {
                let inner = &cur[start + 4..end];
                let (name, fallback) = match inner.find(',') {
                    Some(c) => (inner[..c].trim(), Some(inner[c + 1..].trim())),
                    None => (inner.trim(), None),
                };
                match self.variables.iter().find(|(k, _)| k == name) {
                    Some((_, v)) => v.clone(),
                    None => fallback.map(String::from).unwrap_or_default(),
                }
            };
            let mut next = String::with_capacity(cur.len());
            next.push_str(&cur[..start]);
            next.push_str(&replacement);
            next.push_str(&cur[end + 1..]);
            cur = next;
        }
        cur
    }

    pub fn resolve_var<'a>(&'a self, raw: &'a str) -> &'a str {
        let s = raw.trim();
        if let Some(inner) = s.strip_prefix("var(").and_then(|v| v.strip_suffix(')')) {
            let (var_name, fallback) = if let Some(comma) = inner.find(',') {
                (inner[..comma].trim(), Some(inner[comma + 1..].trim()))
            } else {
                (inner.trim(), None)
            };
            if let Some((_, val)) = self.variables.iter().find(|(k, _)| k == var_name) {
                return val.as_str();
            }
            if let Some(fb) = fallback {
                return fb;
            }
        }
        s
    }

    pub fn matching_declarations(&self, tree: &DomTree, node_id: usize) -> Vec<Declaration> {
        let mut matched: Vec<((u16, u16, u16), usize, Declaration)> = Vec::new();
        let mut order = 0usize;

        for rule in &self.rules {
            if !rule.active {
                continue;
            }
            let mut best_spec: Option<(u16, u16, u16)> = None;
            for sel in &rule.selectors {
                if selector_matches(tree, node_id, sel) {
                    let spec = sel.specificity();
                    if best_spec.map_or(true, |b| spec > b) {
                        best_spec = Some(spec);
                    }
                }
            }
            if let Some(spec) = best_spec {
                for decl in &rule.declarations {
                    if decl.property.starts_with("--") {
                        continue;
                    }
                    let priority_spec = if decl.important {
                        (spec.0.saturating_add(1000), spec.1, spec.2)
                    } else {
                        spec
                    };
                    let resolved_val = self.resolve_all_vars(&decl.value);
                    matched.push((
                        priority_spec,
                        order,
                        Declaration {
                            property: decl.property.clone(),
                            value: resolved_val,
                            important: decl.important,
                        },
                    ));
                    order += 1;
                }
            }
        }

        matched.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        matched.into_iter().map(|(_, _, d)| d).collect()
    }
}

/// Splits `raw` on `sep`, ignoring separators nested in (), [] or quotes.
fn split_top_level(raw: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0usize;
    for (i, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch == '(' || ch == '[' {
            depth += 1;
        } else if ch == ')' || ch == ']' {
            depth -= 1;
        } else if ch == sep && depth == 0 {
            out.push(&raw[start..i]);
            start = i + ch.len_utf8();
        }
    }
    out.push(&raw[start..]);
    out
}

/// `12px`, `40em`, `47.9375em`, `600` -> CSS px (em/rem = 16px).
fn media_len_px(v: &str) -> Option<i32> {
    let v = v.trim();
    let end = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(v.len());
    if end == 0 {
        return None;
    }
    let num: f32 = v[..end].parse::<f32>().ok()?;
    let unit = v[end..].trim();
    let px = if unit.starts_with("em") || unit.starts_with("rem") { num * 16.0 } else { num };
    Some(px as i32)
}

fn media_ratio(v: &str) -> Option<f32> {
    let v = v.trim();
    let end = v.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(v.len());
    if end == 0 {
        return None;
    }
    let num: f32 = v[..end].parse::<f32>().ok()?;
    let unit = v[end..].trim();
    Some(if unit.starts_with("dpi") { num / 96.0 } else { num })
}

fn media_operand(tok: &str, vw: i32, vh: i32) -> Option<i32> {
    match tok.trim() {
        "width" | "device-width" => Some(vw),
        "height" | "device-height" => Some(vh),
        other => media_len_px(other),
    }
}

/// Range syntax: `(width >= 600px)`, `(400px <= width <= 700px)`.
fn media_range_matches(g: &str, vw: i32, vh: i32) -> bool {
    let mut operands: Vec<String> = Vec::new();
    let mut ops: Vec<String> = Vec::new();
    let chars: Vec<char> = g.chars().collect();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '<' || ch == '>' || ch == '=' {
            let mut op = String::new();
            op.push(ch);
            if ch != '=' && i + 1 < chars.len() && chars[i + 1] == '=' {
                op.push('=');
                i += 1;
            }
            operands.push(core::mem::take(&mut cur));
            ops.push(op);
        } else {
            cur.push(ch);
        }
        i += 1;
    }
    operands.push(cur);
    if operands.len() != ops.len() + 1 {
        return true;
    }
    for k in 0..ops.len() {
        let (Some(a), Some(b)) = (media_operand(&operands[k], vw, vh), media_operand(&operands[k + 1], vw, vh)) else {
            return true;
        };
        let ok = match ops[k].as_str() {
            "<" => a < b,
            "<=" => a <= b,
            ">" => a > b,
            ">=" => a >= b,
            "=" => a == b,
            _ => true,
        };
        if !ok {
            return false;
        }
    }
    true
}

fn media_feature_matches(g: &str, vw: i32, vh: i32) -> bool {
    if g.contains('<') || g.contains('>') {
        return media_range_matches(g, vw, vh);
    }
    let (name, val) = match g.split_once(':') {
        Some((n, v)) => (n.trim(), v.trim()),
        None => (g.trim(), ""),
    };
    match name {
        "min-width" | "min-device-width" => media_len_px(val).map_or(true, |n| vw >= n),
        "max-width" | "max-device-width" => media_len_px(val).map_or(true, |n| vw <= n),
        "width" | "device-width" => media_len_px(val).map_or(true, |n| vw == n),
        "min-height" | "min-device-height" => media_len_px(val).map_or(true, |n| vh >= n),
        "max-height" | "max-device-height" => media_len_px(val).map_or(true, |n| vh <= n),
        "height" | "device-height" => media_len_px(val).map_or(true, |n| vh == n),
        "orientation" => match val {
            "portrait" => vh > vw,
            "landscape" => vw >= vh,
            _ => true,
        },
        // The browser renders a light theme without motion or touch input.
        "prefers-color-scheme" => val != "dark",
        "prefers-reduced-motion" => val != "reduce",
        "prefers-contrast" => val != "more" && val != "less" && val != "custom",
        "prefers-reduced-transparency" | "prefers-reduced-data" => val != "reduce",
        "hover" | "any-hover" => val != "none",
        "pointer" | "any-pointer" => val != "coarse" && val != "none",
        "forced-colors" => val != "active",
        "inverted-colors" => val != "inverted",
        "scripting" => val != "none",
        "display-mode" => val == "browser",
        "min-resolution" | "min-device-pixel-ratio" | "-webkit-min-device-pixel-ratio" => {
            media_ratio(val).map_or(true, |r| r <= 1.0)
        }
        "max-resolution" | "max-device-pixel-ratio" | "-webkit-max-device-pixel-ratio" => {
            media_ratio(val).map_or(true, |r| r >= 1.0)
        }
        _ => true,
    }
}

fn media_query_matches(q: &str, vw: i32, vh: i32) -> bool {
    let lower = q.trim().to_ascii_lowercase();
    let mut rest: &str = lower.as_str();
    let mut negate = false;
    if let Some(r) = rest.strip_prefix("not ") {
        negate = true;
        rest = r.trim_start();
    } else if let Some(r) = rest.strip_prefix("only ") {
        rest = r.trim_start();
    }
    let mut ok = true;
    if !rest.starts_with('(') && !rest.is_empty() {
        let end = rest.find(|c: char| c.is_whitespace() || c == '(').unwrap_or(rest.len());
        ok = matches!(&rest[..end], "all" | "screen");
        rest = &rest[end..];
    }
    let chars: Vec<char> = rest.chars().collect();
    let mut p = 0usize;
    while p < chars.len() {
        if chars[p] == '(' {
            let g = read_group(&chars, &mut p, '(', ')');
            if !media_feature_matches(g.trim(), vw, vh) {
                ok = false;
            }
        } else {
            p += 1;
        }
    }
    ok != negate
}

/// Evaluates a comma separated media query list (the text after `@media`) for a real viewport.
fn media_query_list_matches(list: &str, vw: i32, vh: i32) -> bool {
    let l = list.trim();
    if l.is_empty() {
        return true;
    }
    split_top_level(l, ',').into_iter().any(|q| media_query_matches(q, vw, vh))
}

pub fn floor_utf8_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn safe_slice(s: &str, start: usize, end: usize) -> &str {
    let a = floor_utf8_boundary(s, start);
    let b = floor_utf8_boundary(s, end).max(a);
    &s[a..b]
}

fn contains_ascii_case(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return false;
    }
    h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else {
            let valid_i = floor_utf8_boundary(css, i);
            if let Some(ch) = css[valid_i..].chars().next() {
                out.push(ch);
                i = valid_i + ch.len_utf8();
            } else {
                break;
            }
        }
    }
    out
}

fn read_ident(c: &[char], i: &mut usize) -> String {
    let mut out = String::new();
    while *i < c.len() {
        let ch = c[*i];
        if ch == '\\' {
            *i += 1;
            if *i >= c.len() {
                break;
            }
            if c[*i].is_ascii_hexdigit() {
                let mut v: u32 = 0;
                let mut cnt = 0;
                while *i < c.len() && cnt < 6 && c[*i].is_ascii_hexdigit() {
                    v = v * 16 + c[*i].to_digit(16).unwrap_or(0);
                    *i += 1;
                    cnt += 1;
                }
                if *i < c.len() && c[*i].is_whitespace() {
                    *i += 1;
                }
                out.push(char::from_u32(v).unwrap_or('\u{FFFD}'));
            } else {
                out.push(c[*i]);
                *i += 1;
            }
        } else if ch.is_alphanumeric() || ch == '-' || ch == '_' || !ch.is_ascii() {
            out.push(ch);
            *i += 1;
        } else {
            break;
        }
    }
    out
}

/// Reads a balanced `open ... close` group starting at `c[*i] == open`; returns the inner text.
fn read_group(c: &[char], i: &mut usize, open: char, close: char) -> String {
    let mut depth = 0i32;
    let mut out = String::new();
    let mut quote: Option<char> = None;
    while *i < c.len() {
        let ch = c[*i];
        *i += 1;
        if let Some(q) = quote {
            out.push(ch);
            if ch == '\\' && *i < c.len() {
                out.push(c[*i]);
                *i += 1;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            out.push(ch);
            continue;
        }
        if ch == open {
            depth += 1;
            if depth == 1 {
                continue;
            }
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        out.push(ch);
    }
    out
}

fn parse_attr(inner: &str) -> Option<AttrSel> {
    let s = inner.trim();
    let Some(p) = s.find('=') else {
        if s.is_empty() {
            return None;
        }
        return Some(AttrSel { name: s.to_ascii_lowercase(), op: AttrOp::Exists, value: String::new(), ci: false });
    };
    let mut name = &s[..p];
    let mut op = AttrOp::Eq;
    if let Some(&last) = name.as_bytes().last() {
        let mapped = match last {
            b'~' => Some(AttrOp::Includes),
            b'|' => Some(AttrOp::Dash),
            b'^' => Some(AttrOp::Prefix),
            b'$' => Some(AttrOp::Suffix),
            b'*' => Some(AttrOp::Substring),
            _ => None,
        };
        if let Some(o) = mapped {
            op = o;
            name = &name[..name.len() - 1];
        }
    }
    let mut val = s[p + 1..].trim();
    let mut ci = false;
    if let Some(stripped) = val.strip_suffix(" i").or_else(|| val.strip_suffix(" I")) {
        val = stripped.trim();
        ci = true;
    }
    let val = val.trim_matches(|c: char| c == '"' || c == '\'');
    let name = name.trim().to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    // HTML enumerated attributes are matched case-insensitively.
    if matches!(name.as_str(), "type" | "rel" | "dir" | "method" | "lang" | "align" | "target" | "role") {
        ci = true;
    }
    Some(AttrSel { name, op, value: String::from(val), ci })
}

fn parse_nth(arg: &str) -> Option<(i32, i32)> {
    let mut s = arg.trim().to_ascii_lowercase();
    if let Some(p) = s.find(" of ") {
        s.truncate(p);
    }
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s == "odd" {
        return Some((2, 1));
    }
    if s == "even" {
        return Some((2, 0));
    }
    if let Some(npos) = s.find('n') {
        let a_str = &s[..npos];
        let b_str = &s[npos + 1..];
        let a: i32 = match a_str {
            "" | "+" => 1,
            "-" => -1,
            t => t.parse::<i32>().ok()?,
        };
        let b: i32 = if b_str.is_empty() { 0 } else { b_str.trim_start_matches('+').parse::<i32>().ok()? };
        Some((a, b))
    } else {
        s.trim_start_matches('+').parse::<i32>().ok().map(|b| (0, b))
    }
}

fn parse_pseudo(name: &str, arg: Option<&str>) -> Pseudo {
    match name {
        "root" => Pseudo::Root,
        "empty" => Pseudo::Empty,
        "first-child" => Pseudo::FirstChild,
        "last-child" => Pseudo::LastChild,
        "only-child" => Pseudo::OnlyChild,
        "first-of-type" => Pseudo::FirstOfType,
        "last-of-type" => Pseudo::LastOfType,
        "only-of-type" => Pseudo::OnlyOfType,
        "nth-child" | "nth-last-child" | "nth-of-type" | "nth-last-of-type" => match arg.and_then(parse_nth) {
            Some((a, b)) => match name {
                "nth-child" => Pseudo::NthChild(a, b),
                "nth-last-child" => Pseudo::NthLastChild(a, b),
                "nth-of-type" => Pseudo::NthOfType(a, b),
                _ => Pseudo::NthLastOfType(a, b),
            },
            None => Pseudo::Never,
        },
        "not" | "is" | "where" | "matches" | "-webkit-any" | "-moz-any" => {
            let mut parts: Vec<SelectorPart> = Vec::new();
            for item in split_top_level(arg.unwrap_or(""), ',') {
                let chars: Vec<char> = item.trim().chars().collect();
                let mut k = 0usize;
                match parse_compound(&chars, &mut k) {
                    Some(p) if k >= chars.len() => parts.push(p),
                    _ => return Pseudo::Never,
                }
            }
            if parts.is_empty() {
                Pseudo::Never
            } else if name == "not" {
                Pseudo::Not(parts)
            } else if name == "where" {
                Pseudo::Where(parts)
            } else {
                Pseudo::Is(parts)
            }
        }
        "link" | "any-link" => Pseudo::Link,
        "disabled" => Pseudo::Disabled,
        "enabled" => Pseudo::Enabled,
        "checked" => Pseudo::Checked,
        "hover" => Pseudo::Hover,
        "focus" | "focus-visible" => Pseudo::Focus,
        "focus-within" => Pseudo::FocusWithin,
        _ => Pseudo::Never,
    }
}

fn parse_compound(c: &[char], i: &mut usize) -> Option<SelectorPart> {
    let mut part = SelectorPart {
        combinator: Combinator::Descendant,
        tag: None,
        id: None,
        classes: Vec::new(),
        attrs: Vec::new(),
        pseudos: Vec::new(),
    };
    let start = *i;
    while *i < c.len() {
        let ch = c[*i];
        match ch {
            '*' => {
                *i += 1;
                part.tag = Some(String::from("*"));
            }
            '#' => {
                *i += 1;
                let n = read_ident(c, i);
                if n.is_empty() {
                    return None;
                }
                part.id = Some(n);
            }
            '.' => {
                *i += 1;
                let n = read_ident(c, i);
                if n.is_empty() {
                    return None;
                }
                part.classes.push(n);
            }
            '[' => {
                let inner = read_group(c, i, '[', ']');
                part.attrs.push(parse_attr(&inner)?);
            }
            ':' => {
                *i += 1;
                let mut element = false;
                if *i < c.len() && c[*i] == ':' {
                    *i += 1;
                    element = true;
                }
                let name = read_ident(c, i).to_ascii_lowercase();
                let arg = if *i < c.len() && c[*i] == '(' { Some(read_group(c, i, '(', ')')) } else { None };
                if element {
                    part.pseudos.push(Pseudo::Never);
                } else {
                    part.pseudos.push(parse_pseudo(&name, arg.as_deref()));
                }
            }
            ' ' | '\t' | '\n' | '\r' | '>' | '+' | '~' | ',' => break,
            _ => {
                if *i == start && (ch.is_alphabetic() || ch == '_' || ch == '-' || ch == '\\' || !ch.is_ascii()) {
                    let n = read_ident(c, i);
                    if n.is_empty() {
                        return None;
                    }
                    part.tag = Some(n.to_ascii_lowercase());
                } else {
                    return None;
                }
            }
        }
    }
    if *i == start {
        return None;
    }
    Some(part)
}

fn parse_complex_selector(raw: &str) -> Option<Selector> {
    let c: Vec<char> = raw.trim().chars().collect();
    let mut i = 0usize;
    let mut chain: Vec<SelectorPart> = Vec::new();
    let mut pending: Option<Combinator> = None;
    while i < c.len() {
        let mut saw_ws = false;
        while i < c.len() && c[i].is_whitespace() {
            i += 1;
            saw_ws = true;
        }
        if i >= c.len() {
            break;
        }
        match c[i] {
            '>' => {
                pending = Some(Combinator::Child);
                i += 1;
                continue;
            }
            '+' => {
                pending = Some(Combinator::Adjacent);
                i += 1;
                continue;
            }
            '~' => {
                pending = Some(Combinator::Sibling);
                i += 1;
                continue;
            }
            _ => {}
        }
        if saw_ws && pending.is_none() && !chain.is_empty() {
            pending = Some(Combinator::Descendant);
        }
        let mut part = parse_compound(&c, &mut i)?;
        part.combinator = pending.take().unwrap_or(Combinator::Descendant);
        chain.push(part);
    }
    if chain.is_empty() {
        None
    } else {
        Some(Selector { chain })
    }
}

/// Parses a selector list. Selectors the engine cannot represent (pseudo-elements such as `::before`,
/// unknown syntax) are dropped; they never match instead of matching the wrong element.
fn parse_selectors(raw: &str) -> Vec<Selector> {
    let mut list = Vec::new();
    for group in split_top_level(raw, ',') {
        let g = group.trim();
        if g.is_empty() {
            continue;
        }
        if let Some(sel) = parse_complex_selector(g) {
            if !sel.can_never_match() {
                list.push(sel);
            }
        }
    }
    list
}

pub fn parse_declarations(raw: &str) -> Vec<Declaration> {
    let mut decls = Vec::new();
    for item in raw.split(';') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(colon) = trimmed.find(':') {
            let prop = trimmed[..colon].trim().to_ascii_lowercase();
            let mut val = trimmed[colon + 1..].trim();
            let mut important = false;
            if let Some(imp_pos) = val.as_bytes().windows(10).position(|w| w.eq_ignore_ascii_case(b"!important")) {
                important = true;
                val = safe_slice(val, 0, imp_pos).trim();
            }
            if !prop.is_empty() && !val.is_empty() {
                decls.push(Declaration {
                    property: prop,
                    value: String::from(val),
                    important,
                });
            }
        }
    }
    decls
}

fn bytes_eq(a: &[u8], b: &[u8], ci: bool) -> bool {
    if ci {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

fn attr_matches(tree: &DomTree, node_id: usize, a: &AttrSel) -> bool {
    let Some(v) = tree.attr(node_id, &a.name) else {
        return false;
    };
    let vb = v.as_bytes();
    let pb = a.value.as_bytes();
    let n = pb.len();
    match a.op {
        AttrOp::Exists => true,
        AttrOp::Eq => bytes_eq(vb, pb, a.ci),
        AttrOp::Includes => {
            n > 0
                && !a.value.contains(char::is_whitespace)
                && v.split_whitespace().any(|t| bytes_eq(t.as_bytes(), pb, a.ci))
        }
        AttrOp::Dash => bytes_eq(vb, pb, a.ci) || (vb.len() > n && bytes_eq(&vb[..n], pb, a.ci) && vb[n] == b'-'),
        AttrOp::Prefix => n > 0 && vb.len() >= n && bytes_eq(&vb[..n], pb, a.ci),
        AttrOp::Suffix => n > 0 && vb.len() >= n && bytes_eq(&vb[vb.len() - n..], pb, a.ci),
        AttrOp::Substring => n > 0 && vb.len() >= n && vb.windows(n).any(|w| bytes_eq(w, pb, a.ci)),
    }
}

/// (1-based position among element siblings, number of element siblings), optionally restricted to the same tag.
fn sibling_pos(tree: &DomTree, node_id: usize, same_type: bool) -> (i32, i32) {
    let Some(parent) = tree.nodes.get(node_id).and_then(|n| n.parent) else {
        return (1, 1);
    };
    let my_tag = tree.tag(node_id).unwrap_or("");
    let mut total = 0i32;
    let mut found = 0i32;
    for &c in &tree.nodes[parent].children {
        let Some(t) = tree.tag(c) else {
            continue;
        };
        if same_type && !t.eq_ignore_ascii_case(my_tag) {
            continue;
        }
        total += 1;
        if c == node_id {
            found = total;
        }
    }
    (found, total)
}

fn nth_matches(a: i32, b: i32, idx: i32) -> bool {
    if idx <= 0 {
        return false;
    }
    if a == 0 {
        idx == b
    } else {
        let d = idx - b;
        d % a == 0 && d / a >= 0
    }
}

fn self_or_ancestor_is(tree: &DomTree, target: Option<usize>, node_id: usize) -> bool {
    let mut cur = target;
    let mut guard = 0;
    while let Some(t) = cur {
        if t == node_id {
            return true;
        }
        guard += 1;
        if guard > 256 {
            return false;
        }
        cur = tree.nodes.get(t).and_then(|n| n.parent);
    }
    false
}

fn pseudo_matches(tree: &DomTree, node_id: usize, ps: &Pseudo) -> bool {
    match ps {
        Pseudo::Root => tree.tag(node_id) == Some("html"),
        Pseudo::Empty => tree.nodes.get(node_id).map_or(false, |n| n.children.is_empty()),
        Pseudo::FirstChild => sibling_pos(tree, node_id, false).0 == 1,
        Pseudo::LastChild => {
            let (i, t) = sibling_pos(tree, node_id, false);
            i == t
        }
        Pseudo::OnlyChild => sibling_pos(tree, node_id, false).1 == 1,
        Pseudo::FirstOfType => sibling_pos(tree, node_id, true).0 == 1,
        Pseudo::LastOfType => {
            let (i, t) = sibling_pos(tree, node_id, true);
            i == t
        }
        Pseudo::OnlyOfType => sibling_pos(tree, node_id, true).1 == 1,
        Pseudo::NthChild(a, b) => nth_matches(*a, *b, sibling_pos(tree, node_id, false).0),
        Pseudo::NthLastChild(a, b) => {
            let (i, t) = sibling_pos(tree, node_id, false);
            nth_matches(*a, *b, t - i + 1)
        }
        Pseudo::NthOfType(a, b) => nth_matches(*a, *b, sibling_pos(tree, node_id, true).0),
        Pseudo::NthLastOfType(a, b) => {
            let (i, t) = sibling_pos(tree, node_id, true);
            nth_matches(*a, *b, t - i + 1)
        }
        Pseudo::Not(args) => args.iter().all(|a| !part_matches_node(tree, node_id, a)),
        Pseudo::Is(args) | Pseudo::Where(args) => args.iter().any(|a| part_matches_node(tree, node_id, a)),
        Pseudo::Link => matches!(tree.tag(node_id), Some("a") | Some("area")) && tree.attr(node_id, "href").is_some(),
        Pseudo::Disabled => tree.attr(node_id, "disabled").is_some(),
        Pseudo::Enabled => {
            matches!(tree.tag(node_id), Some("input") | Some("button") | Some("select") | Some("textarea"))
                && tree.attr(node_id, "disabled").is_none()
        }
        Pseudo::Checked => tree.attr(node_id, "checked").is_some() || tree.attr(node_id, "selected").is_some(),
        Pseudo::Hover => self_or_ancestor_is(tree, tree.hover_node, node_id),
        Pseudo::Focus => tree.focus_node == Some(node_id),
        Pseudo::FocusWithin => self_or_ancestor_is(tree, tree.focus_node, node_id),
        Pseudo::Never => false,
    }
}

fn part_matches_node(tree: &DomTree, node_id: usize, part: &SelectorPart) -> bool {
    let Some(node_tag) = tree.tag(node_id) else {
        return false;
    };
    if let Some(ref req_tag) = part.tag {
        if req_tag != "*" && !req_tag.eq_ignore_ascii_case(node_tag) {
            return false;
        }
    }
    if let Some(ref req_id) = part.id {
        match tree.id_attr(node_id) {
            Some(actual_id) if actual_id.eq_ignore_ascii_case(req_id) => {}
            _ => return false,
        }
    }
    for req_class in &part.classes {
        if !tree.has_class(node_id, req_class) {
            return false;
        }
    }
    for a in &part.attrs {
        if !attr_matches(tree, node_id, a) {
            return false;
        }
    }
    for ps in &part.pseudos {
        if !pseudo_matches(tree, node_id, ps) {
            return false;
        }
    }
    true
}

fn prev_element_sibling(tree: &DomTree, node_id: usize) -> Option<usize> {
    let parent = tree.nodes.get(node_id).and_then(|n| n.parent)?;
    let mut prev: Option<usize> = None;
    for &c in &tree.nodes[parent].children {
        if c == node_id {
            return prev;
        }
        if tree.tag(c).is_some() {
            prev = Some(c);
        }
    }
    None
}

/// Right-to-left match with real combinator semantics (`>` direct child, `+` adjacent, `~` general sibling,
/// whitespace any ancestor). `idx` is the chain position `node_id` must satisfy.
fn match_chain(tree: &DomTree, node_id: usize, chain: &[SelectorPart], idx: usize, budget: &mut u32) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    if !part_matches_node(tree, node_id, &chain[idx]) {
        return false;
    }
    if idx == 0 {
        return true;
    }
    match chain[idx].combinator {
        Combinator::Child => match tree.nodes.get(node_id).and_then(|n| n.parent) {
            Some(p) => match_chain(tree, p, chain, idx - 1, budget),
            None => false,
        },
        Combinator::Descendant => {
            let mut cur = tree.nodes.get(node_id).and_then(|n| n.parent);
            while let Some(p) = cur {
                if match_chain(tree, p, chain, idx - 1, budget) {
                    return true;
                }
                cur = tree.nodes.get(p).and_then(|n| n.parent);
            }
            false
        }
        Combinator::Adjacent => match prev_element_sibling(tree, node_id) {
            Some(s) => match_chain(tree, s, chain, idx - 1, budget),
            None => false,
        },
        Combinator::Sibling => {
            let mut cur = prev_element_sibling(tree, node_id);
            while let Some(s) = cur {
                if match_chain(tree, s, chain, idx - 1, budget) {
                    return true;
                }
                cur = prev_element_sibling(tree, s);
            }
            false
        }
    }
}

fn selector_matches(tree: &DomTree, node_id: usize, selector: &Selector) -> bool {
    if selector.chain.is_empty() {
        return false;
    }
    let last = selector.chain.len() - 1;
    // Cheap rejection first: the right-most compound must match the node itself.
    if !part_matches_node(tree, node_id, &selector.chain[last]) {
        return false;
    }
    let mut budget: u32 = 4000;
    match_chain(tree, node_id, &selector.chain, last, &mut budget)
}

pub fn parse_css_color(raw: &str) -> Option<u32> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() || s == "transparent" || s == "none" || s == "inherit" || s == "initial" || s == "currentcolor" {
        return None;
    }

    if s.starts_with("var(") {
        if let Some(comma) = s.find(',') {
            let fallback = s[comma + 1..].trim_end_matches(')').trim();
            if let Some(c) = parse_css_color(fallback) {
                return Some(c);
            }
        }
        if s.contains("text-primary") || s.contains("white") {
            return Some(rgb(241, 245, 249));
        }
        if s.contains("text-secondary") || s.contains("muted") {
            return Some(rgb(148, 163, 184));
        }
        if s.contains("base-background") || s.contains("dark") {
            return Some(rgb(15, 23, 42));
        }
        return None;
    }

    if let Some(hex) = s.strip_prefix('#') {
        if !hex.is_ascii() {
            return None;
        }
        let hex = hex.split_whitespace().next().unwrap_or(hex);
        if hex.len() == 3 || hex.len() == 4 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            return Some(rgb(r * 17, g * 17, b * 17));
        } else if hex.len() >= 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(rgb(r, g, b));
        }
    }

    if let Some(inner) = s.strip_prefix("rgb(").or_else(|| s.strip_prefix("rgba(")) {
        let inner = inner.trim_end_matches(')');
        let mut nums = Vec::new();
        for part in inner.split(|c: char| c == ',' || c == '/' || c.is_whitespace()) {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            if let Ok(v) = p.parse::<u16>() {
                nums.push(v.min(255) as u8);
            }
        }
        if nums.len() >= 3 {
            if nums.len() >= 4 && nums[3] == 0 {
                return None;
            }
            return Some(rgb(nums[0], nums[1], nums[2]));
        }
    }

    match s.as_str() {
        "black" => Some(rgb(15, 23, 42)),
        "white" | "#fff" | "#ffffff" => Some(rgb(255, 255, 255)),
        "red" | "crimson" => Some(rgb(220, 38, 38)),
        "darkred" | "maroon" => Some(rgb(153, 27, 27)),
        "green" | "forestgreen" => Some(rgb(22, 163, 74)),
        "limegreen" | "lime" => Some(rgb(34, 197, 94)),
        "blue" | "royalblue" => Some(rgb(37, 99, 235)),
        "navy" | "darkblue" => Some(rgb(30, 58, 138)),
        "dodgerblue" | "deepskyblue" => Some(rgb(14, 165, 233)),
        "cyan" | "aqua" | "teal" => Some(rgb(13, 148, 136)),
        "yellow" | "gold" => Some(rgb(234, 179, 8)),
        "orange" | "darkorange" | "coral" | "tomato" => Some(rgb(234, 88, 12)),
        "purple" | "violet" | "indigo" | "blueviolet" => Some(rgb(124, 58, 237)),
        "pink" | "hotpink" | "magenta" => Some(rgb(219, 39, 119)),
        "gray" | "grey" | "slategray" => Some(rgb(100, 116, 139)),
        "darkgray" | "darkgrey" | "dimgray" => Some(rgb(71, 85, 105)),
        "lightgray" | "lightgrey" | "silver" => Some(rgb(203, 213, 225)),
        "whitesmoke" | "snow" | "ivory" | "aliceblue" | "ghostwhite" => Some(rgb(248, 250, 252)),
        "beige" | "linen" | "cornsilk" => Some(rgb(254, 243, 199)),
        _ => {
            for token in s.split_whitespace() {
                if token.starts_with('#') || token.starts_with("rgb") {
                    if let Some(c) = parse_css_color(token) {
                        return Some(c);
                    }
                }
            }
            None
        }
    }
}

/// CSS `font-size` -> layout font units. Layout fonts are measured by cap height, so a
/// 16px CSS font maps to 12 units (x0.75); em/rem/% are relative to the parent in those units.
pub fn parse_css_font_size(raw: &str, parent_font: i32) -> Option<i32> {
    let first = raw.split_whitespace().next()?.trim().to_ascii_lowercase();
    match first.as_str() {
        "xx-small" => return Some(8),
        "x-small" => return Some(9),
        "small" => return Some(10),
        "medium" => return Some(12),
        "large" => return Some(14),
        "x-large" => return Some(18),
        "xx-large" => return Some(22),
        "smaller" => return Some(parent_font * 5 / 6),
        "larger" => return Some(parent_font * 6 / 5),
        "inherit" | "initial" | "unset" => return Some(parent_font),
        _ => {}
    }
    if let Some(num) = first.strip_suffix("px") {
        return parse_float_i32(num, 75).map(|v| v / 100);
    }
    if let Some(num) = first.strip_suffix("pt") {
        return parse_float_i32(num, 100).map(|v| v / 100);
    }
    if let Some(num) = first.strip_suffix("rem") {
        return parse_float_i32(num, 12);
    }
    if let Some(num) = first.strip_suffix("em") {
        return parse_float_i32(num, parent_font);
    }
    if let Some(num) = first.strip_suffix('%') {
        return parse_float_i32(num, parent_font).map(|v| v / 100);
    }
    parse_float_i32(&first, 75).map(|v| v / 100)
}

pub fn parse_css_px(raw: &str, base_font: i32, container_width: i32) -> Option<i32> {
    let first = raw.split_whitespace().next()?.trim().to_ascii_lowercase();
    if first == "auto" || first == "none" || first == "0" {
        return Some(0);
    }
    if let Some(num) = first.strip_suffix("px") {
        return parse_float_i32(num, 100).map(|v| v / 100);
    }
    if let Some(num) = first.strip_suffix("rem") {
        return parse_float_i32(num, 16);
    }
    if let Some(num) = first.strip_suffix("em") {
        return parse_float_i32(num, base_font);
    }
    if let Some(num) = first.strip_suffix("pt") {
        return parse_float_i32(num, 133).map(|v| v / 100);
    }
    if let Some(num_str) = first.strip_suffix("vw").or_else(|| first.strip_suffix('%')) {
        return parse_float_i32(num_str, container_width).map(|v| v / 100);
    }
    parse_float_i32(&first, 100).map(|v| v / 100)
}

pub fn parse_float_i32(s: &str, scale: i32) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (sign, rest) = if let Some(r) = s.strip_prefix('-') {
        (-1, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (1, r)
    } else {
        (1, s)
    };

    let (int_part, frac_part) = if let Some(dot) = rest.find('.') {
        (&rest[..dot], &rest[dot + 1..])
    } else {
        (rest, "")
    };

    let mut int_val: i32 = 0;
    let mut has_digits = false;
    for b in int_part.bytes() {
        if b.is_ascii_digit() {
            int_val = int_val.saturating_mul(10).saturating_add((b - b'0') as i32);
            has_digits = true;
        } else {
            return None;
        }
    }

    let mut frac_val: i32 = 0;
    let mut frac_div: i32 = 1;
    for b in frac_part.bytes().take(2) {
        if b.is_ascii_digit() {
            frac_val = frac_val * 10 + (b - b'0') as i32;
            frac_div *= 10;
            has_digits = true;
        } else {
            break;
        }
    }

    if !has_digits {
        return None;
    }

    let scaled = int_val.saturating_mul(scale).saturating_add((frac_val * scale) / frac_div);
    Some(sign * scaled)
}
