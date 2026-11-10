//! CSS selector engine used by `querySelector`, `matches`, `closest`.
//! Supports type/universal/#id/.class/[attr ops], combinators (descendant, >, +, ~),
//! selector lists and the common structural pseudo-classes.

extern crate alloc;

use crate::dom::{DomTree, NodeType};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone)]
enum AttrOp {
    Exists,
    Eq,
    Prefix,
    Suffix,
    Contains,
    Word,
    Dash,
}

#[derive(Clone)]
struct AttrSel {
    name: String,
    op: AttrOp,
    val: String,
}

#[derive(Clone)]
enum Pseudo {
    FirstChild,
    LastChild,
    OnlyChild,
    Nth(i32, i32),
    Not(Vec<Compound>),
    Checked,
    Disabled,
    Enabled,
    Empty,
    Root,
    Link,
    Never,
}

#[derive(Clone, Default)]
struct Compound {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attrs: Vec<AttrSel>,
    pseudos: Vec<Pseudo>,
}

#[derive(Clone, Copy, PartialEq)]
enum Comb {
    Descendant,
    Child,
    Adjacent,
    Sibling,
}

#[derive(Clone)]
struct Complex {
    parts: Vec<Compound>,
    combs: Vec<Comb>,
}

#[derive(Clone)]
pub struct SelectorList(Vec<Complex>);

fn is_ident(c: char) -> bool {
    c == '-' || c == '_' || c.is_alphanumeric() || (c as u32) > 127
}

struct SP {
    c: Vec<char>,
    i: usize,
}

impl SP {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            if is_ident(ch) {
                s.push(ch);
                self.i += 1;
            } else if ch == '\\' && self.i + 1 < self.c.len() {
                s.push(self.c[self.i + 1]);
                self.i += 2;
            } else {
                break;
            }
        }
        s
    }

    fn skip_ws(&mut self) -> bool {
        let mut any = false;
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.i += 1;
                any = true;
            } else {
                break;
            }
        }
        any
    }

    fn compound(&mut self) -> Option<Compound> {
        let mut cp = Compound::default();
        let mut any = false;
        loop {
            match self.peek() {
                Some('*') => {
                    self.i += 1;
                    any = true;
                }
                Some('#') => {
                    self.i += 1;
                    cp.id = Some(self.ident());
                    any = true;
                }
                Some('.') => {
                    self.i += 1;
                    cp.classes.push(self.ident());
                    any = true;
                }
                Some('[') => {
                    self.i += 1;
                    self.skip_ws();
                    let name = self.ident();
                    self.skip_ws();
                    let mut op = AttrOp::Exists;
                    let mut val = String::new();
                    if self.peek() != Some(']') {
                        let first = self.peek()?;
                        op = match first {
                            '=' => {
                                self.i += 1;
                                AttrOp::Eq
                            }
                            '^' | '$' | '*' | '~' | '|' => {
                                self.i += 1;
                                if self.peek() != Some('=') {
                                    return None;
                                }
                                self.i += 1;
                                match first {
                                    '^' => AttrOp::Prefix,
                                    '$' => AttrOp::Suffix,
                                    '*' => AttrOp::Contains,
                                    '~' => AttrOp::Word,
                                    _ => AttrOp::Dash,
                                }
                            }
                            _ => return None,
                        };
                        self.skip_ws();
                        match self.peek() {
                            Some(q) if q == '"' || q == '\'' => {
                                self.i += 1;
                                while let Some(ch) = self.peek() {
                                    self.i += 1;
                                    if ch == q {
                                        break;
                                    }
                                    val.push(ch);
                                }
                            }
                            _ => {
                                while let Some(ch) = self.peek() {
                                    if ch == ']' || ch.is_whitespace() {
                                        break;
                                    }
                                    val.push(ch);
                                    self.i += 1;
                                }
                            }
                        }
                        self.skip_ws();
                        if matches!(self.peek(), Some('i') | Some('s')) {
                            self.i += 1;
                            self.skip_ws();
                        }
                    }
                    if self.peek() != Some(']') {
                        return None;
                    }
                    self.i += 1;
                    cp.attrs.push(AttrSel { name, op, val });
                    any = true;
                }
                Some(':') => {
                    self.i += 1;
                    if self.peek() == Some(':') {
                        self.i += 1;
                        self.ident();
                        cp.pseudos.push(Pseudo::Never);
                        any = true;
                        continue;
                    }
                    let name = self.ident().to_ascii_lowercase();
                    let mut arg = String::new();
                    if self.peek() == Some('(') {
                        self.i += 1;
                        let mut depth = 1;
                        while let Some(ch) = self.peek() {
                            self.i += 1;
                            if ch == '(' {
                                depth += 1;
                            } else if ch == ')' {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            arg.push(ch);
                        }
                    }
                    let p = match name.as_str() {
                        "first-child" | "first-of-type" => Pseudo::FirstChild,
                        "last-child" | "last-of-type" => Pseudo::LastChild,
                        "only-child" | "only-of-type" => Pseudo::OnlyChild,
                        "nth-child" | "nth-of-type" => {
                            let (a, b) = parse_nth(&arg);
                            Pseudo::Nth(a, b)
                        }
                        "nth-last-child" | "nth-last-of-type" => Pseudo::Never,
                        "not" => {
                            let list = parse(&arg)?;
                            let mut simple = Vec::new();
                            for cx in list.0 {
                                if cx.parts.len() != 1 {
                                    return None;
                                }
                                simple.push(cx.parts.into_iter().next()?);
                            }
                            Pseudo::Not(simple)
                        }
                        "checked" => Pseudo::Checked,
                        "disabled" => Pseudo::Disabled,
                        "enabled" => Pseudo::Enabled,
                        "empty" => Pseudo::Empty,
                        "root" => Pseudo::Root,
                        "link" | "any-link" => Pseudo::Link,
                        "is" | "where" => {
                            let _ = Box::new(0u8);
                            Pseudo::Never
                        }
                        _ => Pseudo::Never,
                    };
                    cp.pseudos.push(p);
                    any = true;
                }
                Some(ch) if is_ident(ch) && !any_tag(&cp, any) => {
                    cp.tag = Some(self.ident().to_ascii_lowercase());
                    any = true;
                }
                _ => break,
            }
        }
        if any {
            Some(cp)
        } else {
            None
        }
    }
}

fn any_tag(cp: &Compound, any: bool) -> bool {
    any || cp.tag.is_some()
}

fn parse_nth(arg: &str) -> (i32, i32) {
    let t: String = arg.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
    match t.as_str() {
        "odd" => return (2, 1),
        "even" => return (2, 0),
        _ => {}
    }
    if let Some(npos) = t.find('n') {
        let a_str = &t[..npos];
        let a = match a_str {
            "" | "+" => 1,
            "-" => -1,
            other => other.parse::<i32>().unwrap_or(0),
        };
        let b = t[npos + 1..].trim_start_matches('+').parse::<i32>().unwrap_or(0);
        (a, b)
    } else {
        (0, t.parse::<i32>().unwrap_or(0))
    }
}

fn split_top_level(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for ch in text.chars() {
        match quote {
            Some(q) => {
                cur.push(ch);
                if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => {
                    quote = Some(ch);
                    cur.push(ch);
                }
                '(' | '[' => {
                    depth += 1;
                    cur.push(ch);
                }
                ')' | ']' => {
                    depth -= 1;
                    cur.push(ch);
                }
                ',' if depth == 0 => {
                    out.push(core::mem::take(&mut cur));
                }
                c => cur.push(c),
            },
        }
    }
    out.push(cur);
    out
}

pub fn parse(text: &str) -> Option<SelectorList> {
    let mut list = Vec::new();
    for part in split_top_level(text) {
        let t = part.trim();
        if t.is_empty() {
            return None;
        }
        let mut p = SP { c: t.chars().collect(), i: 0 };
        let mut parts = Vec::new();
        let mut combs: Vec<Comb> = Vec::new();
        parts.push(p.compound()?);
        loop {
            let had_ws = p.skip_ws();
            match p.peek() {
                None => break,
                Some('>') => {
                    p.i += 1;
                    p.skip_ws();
                    combs.push(Comb::Child);
                }
                Some('+') => {
                    p.i += 1;
                    p.skip_ws();
                    combs.push(Comb::Adjacent);
                }
                Some('~') => {
                    p.i += 1;
                    p.skip_ws();
                    combs.push(Comb::Sibling);
                }
                Some(_) => {
                    if !had_ws {
                        return None;
                    }
                    combs.push(Comb::Descendant);
                }
            }
            parts.push(p.compound()?);
        }
        list.push(Complex { parts, combs });
    }
    Some(SelectorList(list))
}

fn parent_element(tree: &DomTree, id: usize) -> Option<usize> {
    let p = tree.nodes.get(id)?.parent?;
    if p == tree.root {
        None
    } else {
        Some(p)
    }
}

fn element_siblings(tree: &DomTree, id: usize) -> Vec<usize> {
    match tree.nodes.get(id).and_then(|n| n.parent) {
        Some(p) => tree.nodes[p].children.iter().copied().filter(|c| matches!(tree.nodes[*c].node_type, NodeType::Element { .. })).collect(),
        None => alloc::vec![id],
    }
}

fn nth_matches(a: i32, b: i32, pos: i32) -> bool {
    if a == 0 {
        return pos == b;
    }
    let d = pos - b;
    d % a == 0 && d / a >= 0
}

fn match_compound(tree: &DomTree, id: usize, c: &Compound) -> bool {
    let tag = match tree.nodes.get(id).map(|n| &n.node_type) {
        Some(NodeType::Element { tag, .. }) => tag,
        _ => return false,
    };
    if id == tree.root {
        return false;
    }
    if let Some(t) = &c.tag {
        if !t.eq_ignore_ascii_case(tag) {
            return false;
        }
    }
    if let Some(want) = &c.id {
        if tree.attr(id, "id") != Some(want.as_str()) {
            return false;
        }
    }
    for cl in &c.classes {
        if !tree.has_class(id, cl) {
            return false;
        }
    }
    for a in &c.attrs {
        let v = match tree.attr(id, &a.name) {
            Some(v) => v,
            None => return false,
        };
        let ok = match a.op {
            AttrOp::Exists => true,
            AttrOp::Eq => v == a.val,
            AttrOp::Prefix => !a.val.is_empty() && v.starts_with(a.val.as_str()),
            AttrOp::Suffix => !a.val.is_empty() && v.ends_with(a.val.as_str()),
            AttrOp::Contains => !a.val.is_empty() && v.contains(a.val.as_str()),
            AttrOp::Word => v.split_whitespace().any(|w| w == a.val),
            AttrOp::Dash => v == a.val || (v.starts_with(a.val.as_str()) && v[a.val.len()..].starts_with('-')),
        };
        if !ok {
            return false;
        }
    }
    for p in &c.pseudos {
        let ok = match p {
            Pseudo::Never => false,
            Pseudo::FirstChild => element_siblings(tree, id).first() == Some(&id),
            Pseudo::LastChild => element_siblings(tree, id).last() == Some(&id),
            Pseudo::OnlyChild => element_siblings(tree, id).len() == 1,
            Pseudo::Nth(a, b) => {
                let sibs = element_siblings(tree, id);
                let pos = sibs.iter().position(|s| *s == id).map(|x| x as i32 + 1).unwrap_or(0);
                nth_matches(*a, *b, pos)
            }
            Pseudo::Not(list) => !list.iter().any(|cc| match_compound(tree, id, cc)),
            Pseudo::Checked => tree.attr(id, "checked").is_some() || tree.attr(id, "selected").is_some(),
            Pseudo::Disabled => tree.attr(id, "disabled").is_some(),
            Pseudo::Enabled => tree.attr(id, "disabled").is_none(),
            Pseudo::Empty => tree.nodes[id].children.is_empty(),
            Pseudo::Root => tree.nodes[id].parent == Some(tree.root) && tag == "html",
            Pseudo::Link => tag == "a" && tree.attr(id, "href").is_some(),
        };
        if !ok {
            return false;
        }
    }
    true
}

fn match_from(tree: &DomTree, id: usize, cx: &Complex, idx: usize, depth: usize) -> bool {
    if depth > 64 || !match_compound(tree, id, &cx.parts[idx]) {
        return false;
    }
    if idx == 0 {
        return true;
    }
    match cx.combs[idx - 1] {
        Comb::Child => match parent_element(tree, id) {
            Some(p) => match_from(tree, p, cx, idx - 1, depth + 1),
            None => false,
        },
        Comb::Descendant => {
            let mut cur = parent_element(tree, id);
            let mut guard = 0;
            while let Some(p) = cur {
                if match_from(tree, p, cx, idx - 1, depth + 1) {
                    return true;
                }
                cur = parent_element(tree, p);
                guard += 1;
                if guard > 512 {
                    break;
                }
            }
            false
        }
        Comb::Adjacent => {
            let sibs = element_siblings(tree, id);
            match sibs.iter().position(|s| *s == id) {
                Some(pos) if pos > 0 => match_from(tree, sibs[pos - 1], cx, idx - 1, depth + 1),
                _ => false,
            }
        }
        Comb::Sibling => {
            let sibs = element_siblings(tree, id);
            match sibs.iter().position(|s| *s == id) {
                Some(pos) => sibs[..pos].iter().any(|s| match_from(tree, *s, cx, idx - 1, depth + 1)),
                None => false,
            }
        }
    }
}

pub fn matches(tree: &DomTree, id: usize, list: &SelectorList) -> bool {
    list.0.iter().any(|cx| match_from(tree, id, cx, cx.parts.len() - 1, 0))
}

/// Descendants of `root` (excluding root) that match, in document order.
pub fn query_all(tree: &DomTree, root: usize, list: &SelectorList, limit: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    if let Some(n) = tree.nodes.get(root) {
        for &c in n.children.iter().rev() {
            stack.push(c);
        }
    }
    while let Some(id) = stack.pop() {
        if out.len() >= limit {
            break;
        }
        if matches(tree, id, list) {
            out.push(id);
        }
        for &c in tree.nodes[id].children.iter().rev() {
            stack.push(c);
        }
    }
    out
}
