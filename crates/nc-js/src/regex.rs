//! Backtracking regular expression engine (ECMAScript flavoured).
//! Works on `char` slices, so all indices are code-point based.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::Cell;

pub type Caps = Vec<Option<(usize, usize)>>;

const STEP_LIMIT: usize = 4_000_000;

#[derive(Clone)]
enum ClassItem {
    Range(char, char),
    Digit(bool),
    Word(bool),
    Space(bool),
}

enum Node {
    Empty,
    Char(char),
    Any,
    Class(Vec<ClassItem>, bool),
    Start,
    End,
    WordB(bool),
    Group(Box<Node>, Option<usize>),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat { node: Box<Node>, min: usize, max: usize, greedy: bool },
    BackRef(usize),
    NamedRef(String),
    Look { node: Box<Node>, ahead: bool, neg: bool },
}

pub struct Regex {
    root: Node,
    pub ngroups: usize,
    pub names: Vec<(String, usize)>,
    pub source: String,
    pub flags: String,
    icase: bool,
    multiline: bool,
    dotall: bool,
    pub global: bool,
    pub sticky: bool,
}

struct P {
    c: Vec<char>,
    i: usize,
    ngroups: usize,
    names: Vec<(String, usize)>,
    depth: usize,
}

fn is_word(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn is_line_term(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn upper(c: char) -> char {
    c.to_uppercase().next().unwrap_or(c)
}

fn in_range(ch: char, a: char, b: char, icase: bool) -> bool {
    if ch >= a && ch <= b {
        return true;
    }
    if icase {
        let l = lower(ch);
        let u = upper(ch);
        return (l >= a && l <= b) || (u >= a && u <= b);
    }
    false
}

fn class_matches(items: &[ClassItem], neg: bool, ch: char, icase: bool) -> bool {
    let mut m = false;
    for it in items {
        let hit = match it {
            ClassItem::Range(a, b) => in_range(ch, *a, *b, icase),
            ClassItem::Digit(n) => ch.is_ascii_digit() != *n,
            ClassItem::Word(n) => is_word(ch) != *n,
            ClassItem::Space(n) => is_space(ch) != *n,
        };
        if hit {
            m = true;
            break;
        }
    }
    m != neg
}

impl P {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn eat(&mut self, ch: char) -> bool {
        if self.peek() == Some(ch) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn alt(&mut self) -> Result<Node, String> {
        self.depth += 1;
        if self.depth > 200 {
            return Err(String::from("regex nesting too deep"));
        }
        let mut alts = vec![self.concat()?];
        while self.eat('|') {
            alts.push(self.concat()?);
        }
        self.depth -= 1;
        Ok(if alts.len() == 1 { alts.pop().unwrap_or(Node::Empty) } else { Node::Alt(alts) })
    }

    fn concat(&mut self) -> Result<Node, String> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            items.push(self.quantified()?);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap_or(Node::Empty),
            _ => Node::Concat(items),
        })
    }

    fn braces(&mut self) -> Option<(usize, usize)> {
        let save = self.i;
        self.i += 1;
        let mut a = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                a.push(c);
                self.i += 1;
            } else {
                break;
            }
        }
        if a.is_empty() {
            self.i = save;
            return None;
        }
        let min: usize = a.parse().unwrap_or(0);
        let mut max = min;
        if self.eat(',') {
            let mut b = String::new();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    b.push(c);
                    self.i += 1;
                } else {
                    break;
                }
            }
            max = if b.is_empty() { usize::MAX } else { b.parse().unwrap_or(usize::MAX) };
        }
        if !self.eat('}') {
            self.i = save;
            return None;
        }
        Some((min, max))
    }

    fn quantified(&mut self) -> Result<Node, String> {
        let atom = self.atom()?;
        let (min, max) = match self.peek() {
            Some('*') => {
                self.i += 1;
                (0, usize::MAX)
            }
            Some('+') => {
                self.i += 1;
                (1, usize::MAX)
            }
            Some('?') => {
                self.i += 1;
                (0, 1)
            }
            Some('{') => match self.braces() {
                Some(r) => r,
                None => return Ok(atom),
            },
            _ => return Ok(atom),
        };
        let greedy = !self.eat('?');
        Ok(Node::Repeat { node: Box::new(atom), min, max, greedy })
    }

    fn hex(&mut self, n: usize) -> Option<u32> {
        if self.i + n > self.c.len() {
            return None;
        }
        let mut v = 0u32;
        for k in 0..n {
            v = v * 16 + self.c[self.i + k].to_digit(16)?;
        }
        self.i += n;
        Some(v)
    }

    fn char_escape(&mut self) -> Result<char, String> {
        let c = match self.peek() {
            Some(c) => c,
            None => return Err(String::from("\\ at end of pattern")),
        };
        self.i += 1;
        Ok(match c {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\u{b}',
            'f' => '\u{c}',
            '0' => '\0',
            'x' => match self.hex(2) {
                Some(v) => char::from_u32(v).unwrap_or('x'),
                None => 'x',
            },
            'u' => {
                if self.peek() == Some('{') {
                    self.i += 1;
                    let mut v = 0u32;
                    while let Some(d) = self.peek() {
                        self.i += 1;
                        if d == '}' {
                            break;
                        }
                        v = v.wrapping_mul(16).wrapping_add(d.to_digit(16).unwrap_or(0));
                    }
                    char::from_u32(v).unwrap_or('\u{fffd}')
                } else {
                    match self.hex(4) {
                        Some(v) => char::from_u32(v).unwrap_or('\u{fffd}'),
                        None => 'u',
                    }
                }
            }
            'c' => {
                let l = self.peek().unwrap_or('@');
                self.i += 1;
                char::from_u32((l as u32) % 32).unwrap_or('\0')
            }
            other => other,
        })
    }

    fn class_escape(c: char) -> Option<ClassItem> {
        Some(match c {
            'd' => ClassItem::Digit(false),
            'D' => ClassItem::Digit(true),
            'w' => ClassItem::Word(false),
            'W' => ClassItem::Word(true),
            's' => ClassItem::Space(false),
            'S' => ClassItem::Space(true),
            _ => return None,
        })
    }

    fn class(&mut self) -> Result<Node, String> {
        let neg = self.eat('^');
        let mut items: Vec<ClassItem> = Vec::new();
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => return Err(String::from("unterminated character class")),
            };
            if c == ']' {
                self.i += 1;
                break;
            }
            self.i += 1;
            let lo: char;
            if c == '\\' {
                let e = match self.peek() {
                    Some(e) => e,
                    None => return Err(String::from("bad escape in class")),
                };
                if let Some(ci) = Self::class_escape(e) {
                    self.i += 1;
                    items.push(ci);
                    continue;
                }
                if e == 'b' {
                    self.i += 1;
                    lo = '\u{8}';
                } else {
                    lo = self.char_escape()?;
                }
            } else {
                lo = c;
            }
            if self.peek() == Some('-') && self.c.get(self.i + 1).map(|x| *x != ']').unwrap_or(false) {
                self.i += 1;
                let hc = self.peek().unwrap_or(']');
                self.i += 1;
                let hi = if hc == '\\' {
                    let e = self.peek().unwrap_or('\\');
                    if Self::class_escape(e).is_some() {
                        items.push(ClassItem::Range(lo, lo));
                        items.push(ClassItem::Range('-', '-'));
                        continue;
                    }
                    self.char_escape()?
                } else {
                    hc
                };
                if hi < lo {
                    return Err(String::from("range out of order in character class"));
                }
                items.push(ClassItem::Range(lo, hi));
            } else {
                items.push(ClassItem::Range(lo, lo));
            }
        }
        Ok(Node::Class(items, neg))
    }

    fn atom(&mut self) -> Result<Node, String> {
        let c = match self.peek() {
            Some(c) => c,
            None => return Ok(Node::Empty),
        };
        self.i += 1;
        match c {
            '(' => {
                if self.eat('?') {
                    if self.eat(':') {
                        let n = self.alt()?;
                        self.close()?;
                        return Ok(Node::Group(Box::new(n), None));
                    }
                    if self.eat('=') || self.peek() == Some('!') {
                        let neg = self.eat('!');
                        let n = self.alt()?;
                        self.close()?;
                        return Ok(Node::Look { node: Box::new(n), ahead: true, neg });
                    }
                    if self.eat('<') {
                        if self.eat('=') || self.peek() == Some('!') {
                            let neg = self.eat('!');
                            let n = self.alt()?;
                            self.close()?;
                            return Ok(Node::Look { node: Box::new(n), ahead: false, neg });
                        }
                        let mut name = String::new();
                        while let Some(d) = self.peek() {
                            self.i += 1;
                            if d == '>' {
                                break;
                            }
                            name.push(d);
                        }
                        self.ngroups += 1;
                        let idx = self.ngroups;
                        self.names.push((name, idx));
                        let n = self.alt()?;
                        self.close()?;
                        return Ok(Node::Group(Box::new(n), Some(idx)));
                    }
                    return Err(String::from("invalid group"));
                }
                self.ngroups += 1;
                let idx = self.ngroups;
                let n = self.alt()?;
                self.close()?;
                Ok(Node::Group(Box::new(n), Some(idx)))
            }
            '[' => self.class(),
            '.' => Ok(Node::Any),
            '^' => Ok(Node::Start),
            '$' => Ok(Node::End),
            '*' | '+' | '?' => Err(String::from("nothing to repeat")),
            '\\' => {
                let e = match self.peek() {
                    Some(e) => e,
                    None => return Err(String::from("\\ at end of pattern")),
                };
                if let Some(ci) = Self::class_escape(e) {
                    self.i += 1;
                    return Ok(Node::Class(vec![ci], false));
                }
                if e == 'b' {
                    self.i += 1;
                    return Ok(Node::WordB(true));
                }
                if e == 'B' {
                    self.i += 1;
                    return Ok(Node::WordB(false));
                }
                if e.is_ascii_digit() && e != '0' {
                    let mut n = 0usize;
                    while let Some(d) = self.peek() {
                        if let Some(v) = d.to_digit(10) {
                            n = n * 10 + v as usize;
                            self.i += 1;
                        } else {
                            break;
                        }
                    }
                    return Ok(Node::BackRef(n));
                }
                if e == 'k' && self.c.get(self.i + 1) == Some(&'<') {
                    self.i += 2;
                    let mut name = String::new();
                    while let Some(d) = self.peek() {
                        self.i += 1;
                        if d == '>' {
                            break;
                        }
                        name.push(d);
                    }
                    return Ok(Node::NamedRef(name));
                }
                Ok(Node::Char(self.char_escape()?))
            }
            other => Ok(Node::Char(other)),
        }
    }

    fn close(&mut self) -> Result<(), String> {
        if self.eat(')') {
            Ok(())
        } else {
            Err(String::from("missing ')'"))
        }
    }
}

pub fn compile(pattern: &str, flags: &str) -> Result<Regex, String> {
    for f in flags.chars() {
        if !matches!(f, 'g' | 'i' | 'm' | 's' | 'u' | 'y' | 'd' | 'v') {
            return Err(format!("Invalid flags supplied to RegExp constructor '{}'", flags));
        }
    }
    let mut p = P { c: pattern.chars().collect(), i: 0, ngroups: 0, names: Vec::new(), depth: 0 };
    let root = p.alt()?;
    if p.i < p.c.len() {
        return Err(String::from("unmatched ')'"));
    }
    Ok(Regex {
        root,
        ngroups: p.ngroups,
        names: p.names,
        source: String::from(pattern),
        flags: String::from(flags),
        icase: flags.contains('i'),
        multiline: flags.contains('m'),
        dotall: flags.contains('s'),
        global: flags.contains('g'),
        sticky: flags.contains('y'),
    })
}

struct M<'a> {
    re: &'a Regex,
    s: &'a [char],
    steps: Cell<usize>,
    depth: Cell<usize>,
}

type K<'k> = &'k mut dyn FnMut(usize, &mut Caps) -> bool;

impl<'a> M<'a> {
    fn over(&self) -> bool {
        let n = self.steps.get() + 1;
        self.steps.set(n);
        n > STEP_LIMIT
    }

    fn eq(&self, a: char, b: char) -> bool {
        a == b || (self.re.icase && lower(a) == lower(b))
    }

    fn single(&self, node: &Node, ch: char) -> bool {
        match node {
            Node::Char(c) => self.eq(ch, *c),
            Node::Any => self.re.dotall || !is_line_term(ch),
            Node::Class(items, neg) => class_matches(items, *neg, ch, self.re.icase),
            _ => false,
        }
    }

    fn m(&self, node: &Node, i: usize, caps: &mut Caps, k: K) -> bool {
        let d = self.depth.get();
        if d > 2500 {
            return false;
        }
        self.depth.set(d + 1);
        let r = self.m_inner(node, i, caps, k);
        self.depth.set(d);
        r
    }

    fn m_inner(&self, node: &Node, i: usize, caps: &mut Caps, k: K) -> bool {
        if self.over() {
            return false;
        }
        let len = self.s.len();
        match node {
            Node::Empty => k(i, caps),
            Node::Char(_) | Node::Any | Node::Class(_, _) => i < len && self.single(node, self.s[i]) && k(i + 1, caps),
            Node::Start => (i == 0 || (self.re.multiline && is_line_term(self.s[i - 1]))) && k(i, caps),
            Node::End => (i == len || (self.re.multiline && is_line_term(self.s[i]))) && k(i, caps),
            Node::WordB(want) => {
                let a = i > 0 && is_word(self.s[i - 1]);
                let b = i < len && is_word(self.s[i]);
                ((a != b) == *want) && k(i, caps)
            }
            Node::Group(n, idx) => match idx {
                None => self.m(n, i, caps, k),
                Some(g) => {
                    let g = *g;
                    let old = caps[g];
                    let r = self.m(n, i, caps, &mut |e, c2| {
                        let prev = c2[g];
                        c2[g] = Some((i, e));
                        if k(e, c2) {
                            true
                        } else {
                            c2[g] = prev;
                            false
                        }
                    });
                    if !r {
                        caps[g] = old;
                    }
                    r
                }
            },
            Node::Concat(items) => self.seq(items, 0, i, caps, k),
            Node::Alt(alts) => {
                for a in alts {
                    if self.m(a, i, caps, k) {
                        return true;
                    }
                }
                false
            }
            Node::Repeat { node: inner, min, max, greedy } => {
                if matches!(**inner, Node::Char(_) | Node::Any | Node::Class(_, _)) {
                    let mut n = 0usize;
                    while n < *max && i + n < len && self.single(inner, self.s[i + n]) {
                        n += 1;
                        if self.over() {
                            return false;
                        }
                    }
                    if n < *min {
                        return false;
                    }
                    if *greedy {
                        let mut c = n;
                        loop {
                            if k(i + c, caps) {
                                return true;
                            }
                            if c == *min {
                                return false;
                            }
                            c -= 1;
                        }
                    } else {
                        let mut c = *min;
                        loop {
                            if k(i + c, caps) {
                                return true;
                            }
                            if c == n {
                                return false;
                            }
                            c += 1;
                        }
                    }
                } else {
                    self.rep(inner, *min, *max, *greedy, 0, i, caps, k)
                }
            }
            Node::BackRef(n) => self.backref(*n, i, caps, k),
            Node::NamedRef(name) => {
                let idx = self.re.names.iter().find(|(nm, _)| nm == name).map(|(_, x)| *x).unwrap_or(0);
                self.backref(idx, i, caps, k)
            }
            Node::Look { node: inner, ahead, neg } => {
                let mut c2 = caps.clone();
                let found = if *ahead {
                    self.m(inner, i, &mut c2, &mut |_, _| true)
                } else {
                    let mut f = false;
                    let mut j = i as isize;
                    while j >= 0 {
                        let mut c3 = caps.clone();
                        if self.m(inner, j as usize, &mut c3, &mut |e, _| e == i) {
                            c2 = c3;
                            f = true;
                            break;
                        }
                        j -= 1;
                    }
                    f
                };
                if *neg {
                    if found {
                        false
                    } else {
                        k(i, caps)
                    }
                } else if found {
                    *caps = c2;
                    k(i, caps)
                } else {
                    false
                }
            }
        }
    }

    fn backref(&self, n: usize, i: usize, caps: &mut Caps, k: K) -> bool {
        if n >= caps.len() {
            return k(i, caps);
        }
        match caps[n] {
            None => k(i, caps),
            Some((a, b)) => {
                let l = b - a;
                if i + l > self.s.len() {
                    return false;
                }
                for x in 0..l {
                    if !self.eq(self.s[a + x], self.s[i + x]) {
                        return false;
                    }
                }
                k(i + l, caps)
            }
        }
    }

    fn seq(&self, items: &[Node], idx: usize, i: usize, caps: &mut Caps, k: K) -> bool {
        if idx == items.len() {
            return k(i, caps);
        }
        self.m(&items[idx], i, caps, &mut |e, c2| self.seq(items, idx + 1, e, c2, k))
    }

    fn rep(&self, node: &Node, min: usize, max: usize, greedy: bool, count: usize, i: usize, caps: &mut Caps, k: K) -> bool {
        if self.over() {
            return false;
        }
        if count < min {
            return self.m(node, i, caps, &mut |e, c2| self.rep(node, min, max, greedy, count + 1, e, c2, k));
        }
        if greedy {
            if count < max {
                let r = self.m(node, i, caps, &mut |e, c2| {
                    if e == i {
                        return false;
                    }
                    self.rep(node, min, max, greedy, count + 1, e, c2, k)
                });
                if r {
                    return true;
                }
            }
            k(i, caps)
        } else {
            if k(i, caps) {
                return true;
            }
            if count < max {
                return self.m(node, i, caps, &mut |e, c2| {
                    if e == i {
                        return false;
                    }
                    self.rep(node, min, max, greedy, count + 1, e, c2, k)
                });
            }
            false
        }
    }
}

impl Regex {
    /// Searches for the first match starting at or after `start`
    /// (only exactly at `start` for sticky regexes).
    pub fn exec_at(&self, s: &[char], start: usize) -> Option<Caps> {
        let m = M { re: self, s, steps: Cell::new(0), depth: Cell::new(0) };
        let mut st = start;
        while st <= s.len() {
            let mut caps: Caps = vec![None; self.ngroups + 1];
            let mut end_pos = 0usize;
            let ok = m.m(&self.root, st, &mut caps, &mut |e, c2| {
                end_pos = e;
                c2[0] = Some((st, e));
                true
            });
            if ok {
                caps[0] = Some((st, end_pos));
                return Some(caps);
            }
            if self.sticky || m.steps.get() > STEP_LIMIT {
                return None;
            }
            st += 1;
        }
        None
    }

    pub fn group_name(&self, idx: usize) -> Option<&str> {
        self.names.iter().find(|(_, i)| *i == idx).map(|(n, _)| n.as_str())
    }
}
