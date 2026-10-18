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

#[derive(Clone, Debug)]
pub struct SelectorPart {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attr_eq: Option<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct Selector {
    pub chain: Vec<SelectorPart>,
}

impl Selector {
    pub fn specificity(&self) -> (u16, u16, u16) {
        let mut ids = 0u16;
        let mut classes = 0u16;
        let mut tags = 0u16;
        for part in &self.chain {
            if part.id.is_some() {
                ids += 1;
            }
            classes += part.classes.len() as u16;
            if part.attr_eq.is_some() {
                classes += 1;
            }
            if let Some(ref t) = part.tag {
                if t != "*" {
                    tags += 1;
                }
            }
        }
        (ids, classes, tags)
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
}

#[derive(Clone, Debug)]
pub struct Stylesheet {
    pub rules: Vec<CssRule>,
    pub variables: Vec<(String, String)>,
}

impl Stylesheet {
    pub fn new() -> Self {
        Stylesheet {
            rules: Vec::new(),
            variables: Vec::new(),
        }
    }

    pub fn append_css(&mut self, css_text: &str) {
        if self.rules.len() >= 600 {
            return;
        }
        let limit = floor_utf8_boundary(css_text, 128 * 1024);
        let cleaned = strip_css_comments(&css_text[..limit]);
        self.parse_blocks(&cleaned, 0);
    }

    fn parse_blocks(&mut self, input: &str, depth_level: usize) {
        if depth_level > 3 || self.rules.len() >= 600 {
            return;
        }
        let bytes = input.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len && self.rules.len() < 600 {
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
                    if media_applies(header) {
                        self.parse_blocks(safe_slice(input, body_start, body_end), depth_level + 1);
                    }
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
                if d.property.starts_with("--") && self.variables.len() < 128 {
                    if let Some(existing) = self.variables.iter_mut().find(|(k, _)| k == &d.property) {
                        existing.1 = d.value.clone();
                    } else {
                        self.variables.push((d.property.clone(), d.value.clone()));
                    }
                }
            }

            let selectors = parse_selectors(selector_str);
            if !selectors.is_empty() && !declarations.is_empty() {
                self.rules.push(CssRule {
                    selectors,
                    declarations,
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

fn media_feature_px(h: &str, key: &str) -> Option<i32> {
    let pos = h.find(key)?;
    let rest = h[pos + key.len()..].trim_start().strip_prefix(':')?.trim_start();
    let mut n: i32 = 0;
    let mut digits = 0usize;
    for b in rest.bytes() {
        if b.is_ascii_digit() {
            n = n.saturating_mul(10).saturating_add((b - b'0') as i32);
            digits += 1;
        } else {
            break;
        }
    }
    if digits == 0 {
        return None;
    }
    let unit = &rest[digits..];
    if unit.starts_with("em") || unit.starts_with("rem") {
        n = n.saturating_mul(16);
    }
    Some(n)
}

fn media_applies(header: &str) -> bool {
    let h = header.to_ascii_lowercase();
    if !h.starts_with("@media") {
        return false;
    }
    let blocked = [
        "print", "dark", "reduce", "coarse", "portrait", "hover:none", "hover: none",
        "forced-colors", "speech", "pointer:none", "pointer: none", " not ",
    ];
    for word in blocked {
        if h.contains(word) {
            return false;
        }
    }
    if let Some(v) = media_feature_px(&h, "max-width") {
        if v < 1100 {
            return false;
        }
    }
    if let Some(v) = media_feature_px(&h, "max-device-width") {
        if v < 1100 {
            return false;
        }
    }
    if let Some(v) = media_feature_px(&h, "min-width") {
        if v > 1100 {
            return false;
        }
    }
    true
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

fn parse_selectors(raw: &str) -> Vec<Selector> {
    let mut list = Vec::new();
    for group in raw.split(',') {
        let g = group.trim();
        if g.is_empty() {
            continue;
        }
        if g.contains(":hover")
            || g.contains(":focus")
            || g.contains(":active")
            || g.contains("::before")
            || g.contains("::after")
            || g.contains(":before")
            || g.contains(":after")
        {
            continue;
        }
        let normalized = g.replace('>', " > ");
        let mut chain = Vec::new();
        for token in normalized.split_whitespace() {
            if token == ">" || token == "+" || token == "~" {
                continue;
            }
            let clean = if token == ":root" {
                "html"
            } else {
                token.split(':').next().unwrap_or(token)
            };
            if clean.is_empty() {
                continue;
            }
            if let Some(part) = parse_selector_part(clean) {
                chain.push(part);
            }
        }
        if !chain.is_empty() {
            list.push(Selector { chain });
        }
    }
    list
}

fn parse_selector_part(token: &str) -> Option<SelectorPart> {
    let mut tag = None;
    let mut id = None;
    let mut classes = Vec::new();
    let mut attr_eq = None;

    let (base_token, attr_part) = if let Some(bracket) = token.find('[') {
        (&token[..bracket], Some(&token[bracket + 1..]))
    } else {
        (token, None)
    };

    if let Some(attr_raw) = attr_part {
        let inner = attr_raw.trim_end_matches(']');
        if let Some((k, v)) = inner.split_once('=') {
            let clean_k = k.trim().trim_end_matches('~').trim_end_matches('*').trim_end_matches('^').to_ascii_lowercase();
            let clean_v = v.trim().trim_matches('"').trim_matches('\'').to_ascii_lowercase();
            attr_eq = Some((clean_k, clean_v));
        }
    }

    let mut mode = 't';
    let mut buf = String::new();

    let flush = |mode: char, buf: &mut String, tag: &mut Option<String>, id: &mut Option<String>, classes: &mut Vec<String>| {
        if buf.is_empty() {
            return;
        }
        match mode {
            't' => *tag = Some(buf.to_ascii_lowercase()),
            '#' => *id = Some(buf.clone()),
            '.' => classes.push(buf.clone()),
            _ => {}
        }
        buf.clear();
    };

    for ch in base_token.chars() {
        if ch == '#' || ch == '.' {
            flush(mode, &mut buf, &mut tag, &mut id, &mut classes);
            mode = ch;
        } else {
            buf.push(ch);
        }
    }
    flush(mode, &mut buf, &mut tag, &mut id, &mut classes);

    if tag.is_none() && id.is_none() && classes.is_empty() && attr_eq.is_none() {
        None
    } else {
        Some(SelectorPart {
            tag,
            id,
            classes,
            attr_eq,
        })
    }
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
    if let Some((ref ak, ref av)) = part.attr_eq {
        match tree.attr(node_id, ak) {
            Some(actual_v) if actual_v.eq_ignore_ascii_case(av) => {}
            _ => return false,
        }
    }
    true
}

fn selector_matches(tree: &DomTree, node_id: usize, selector: &Selector) -> bool {
    if selector.chain.is_empty() {
        return false;
    }
    let last_idx = selector.chain.len() - 1;
    if !part_matches_node(tree, node_id, &selector.chain[last_idx]) {
        return false;
    }
    if last_idx == 0 {
        return true;
    }

    let mut chain_idx = last_idx - 1;
    let mut curr = tree.nodes.get(node_id).and_then(|n| n.parent);
    while let Some(parent_id) = curr {
        if part_matches_node(tree, parent_id, &selector.chain[chain_idx]) {
            if chain_idx == 0 {
                return true;
            }
            chain_idx -= 1;
        }
        curr = tree.nodes.get(parent_id).and_then(|n| n.parent);
    }
    false
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

pub fn parse_css_px(raw: &str, base_font: i32, container_width: i32) -> Option<i32> {
    let first = raw.split_whitespace().next()?.trim().to_ascii_lowercase();
    if first == "auto" || first == "none" || first == "0" {
        return Some(0);
    }
    if let Some(num) = first.strip_suffix("px") {
        return parse_float_i32(num, 100).map(|v| v / 100);
    }
    if let Some(num) = first.strip_suffix("rem") {
        return parse_float_i32(num, 14);
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

fn parse_float_i32(s: &str, scale: i32) -> Option<i32> {
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
