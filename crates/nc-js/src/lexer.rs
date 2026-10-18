use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Num(f64),
    Str(String),
    Tpl(Vec<String>, Vec<String>),
    Regex(String, String),
    Ident(String),
    Punct(String),
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub nl: bool,
    pub pos: usize,
}

const PUNCTS: &[&str] = &[
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "=>", "==", "!=",
    "<=", ">=", "&&", "||", "??", "?.", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=",
    "<<", ">>", "**", "{", "}", "(", ")", "[", "]", ";", ",", "<", ">", "+", "-", "*", "/", "%", "&",
    "|", "^", "!", "~", "?", ":", "=", ".",
];

fn is_id_start(c: char) -> bool {
    c == '_' || c == '$' || c.is_alphabetic()
}

fn is_id_part(c: char) -> bool {
    is_id_start(c) || c.is_ascii_digit() || c == '\u{200c}' || c == '\u{200d}'
}

fn starts_at(ch: &[char], i: usize, s: &str) -> bool {
    let mut k = i;
    for c in s.chars() {
        if k >= ch.len() || ch[k] != c {
            return false;
        }
        k += 1;
    }
    true
}

fn regex_allowed(prev: Option<&Tok>) -> bool {
    match prev {
        None => true,
        Some(Tok::Num(_)) | Some(Tok::Str(_)) | Some(Tok::Tpl(_, _)) | Some(Tok::Regex(_, _)) => false,
        Some(Tok::Ident(k)) => matches!(
            k.as_str(),
            "return" | "typeof" | "instanceof" | "in" | "of" | "new" | "delete" | "void" | "throw" | "case" | "do" | "else" | "yield" | "await"
        ),
        Some(Tok::Punct(p)) => !matches!(p.as_str(), ")" | "]" | "}"),
        Some(Tok::Eof) => true,
    }
}

fn read_hex(ch: &[char], i: &mut usize, n: usize) -> Option<u32> {
    if *i + n > ch.len() {
        return None;
    }
    let mut v: u32 = 0;
    for k in 0..n {
        let d = ch[*i + k].to_digit(16)?;
        v = v * 16 + d;
    }
    *i += n;
    Some(v)
}

fn read_escape(ch: &[char], i: &mut usize, out: &mut String) -> Result<(), String> {
    if *i >= ch.len() {
        return Err(String::from("unterminated escape"));
    }
    let c = ch[*i];
    *i += 1;
    match c {
        'n' => out.push('\n'),
        't' => out.push('\t'),
        'r' => out.push('\r'),
        'b' => out.push('\u{8}'),
        'f' => out.push('\u{c}'),
        'v' => out.push('\u{b}'),
        '0' => out.push('\0'),
        'x' => {
            let v = read_hex(ch, i, 2).ok_or_else(|| String::from("bad \\x escape"))?;
            out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
        }
        'u' => {
            let code: u32;
            if *i < ch.len() && ch[*i] == '{' {
                *i += 1;
                let mut v: u32 = 0;
                while *i < ch.len() && ch[*i] != '}' {
                    let d = ch[*i].to_digit(16).ok_or_else(|| String::from("bad \\u escape"))?;
                    v = v.wrapping_mul(16).wrapping_add(d);
                    *i += 1;
                }
                *i += 1;
                code = v;
            } else {
                code = read_hex(ch, i, 4).ok_or_else(|| String::from("bad \\u escape"))?;
            }
            if (0xD800..0xDC00).contains(&code) && starts_at(ch, *i, "\\u") {
                let save = *i;
                *i += 2;
                if let Some(low) = read_hex(ch, i, 4) {
                    if (0xDC00..0xE000).contains(&low) {
                        let full = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                        out.push(char::from_u32(full).unwrap_or('\u{fffd}'));
                        return Ok(());
                    }
                }
                *i = save;
            }
            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
        }
        '\n' => {}
        '\r' => {
            if *i < ch.len() && ch[*i] == '\n' {
                *i += 1;
            }
        }
        other => out.push(other),
    }
    Ok(())
}

fn read_template(ch: &[char], i: &mut usize) -> Result<(Vec<String>, Vec<String>), String> {
    let mut strs: Vec<String> = Vec::new();
    let mut exprs: Vec<String> = Vec::new();
    let mut cur = String::new();
    loop {
        if *i >= ch.len() {
            return Err(String::from("unterminated template literal"));
        }
        let c = ch[*i];
        if c == '`' {
            *i += 1;
            strs.push(cur);
            return Ok((strs, exprs));
        }
        if c == '\\' {
            *i += 1;
            read_escape(ch, i, &mut cur)?;
            continue;
        }
        if c == '$' && *i + 1 < ch.len() && ch[*i + 1] == '{' {
            *i += 2;
            let start = *i;
            let mut depth = 1;
            while *i < ch.len() {
                let d = ch[*i];
                if d == '{' {
                    depth += 1;
                } else if d == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                } else if d == '\'' || d == '"' {
                    let q = d;
                    *i += 1;
                    while *i < ch.len() && ch[*i] != q {
                        if ch[*i] == '\\' {
                            *i += 1;
                        }
                        *i += 1;
                    }
                } else if d == '`' {
                    *i += 1;
                    read_template(ch, i)?;
                    continue;
                }
                *i += 1;
            }
            if depth != 0 {
                return Err(String::from("unterminated template expression"));
            }
            let src: String = ch[start..*i].iter().collect();
            *i += 1;
            strs.push(cur);
            cur = String::new();
            exprs.push(src);
            continue;
        }
        cur.push(c);
        *i += 1;
    }
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let ch: Vec<char> = src.chars().collect();
    let len = ch.len();
    let mut i = 0usize;
    let mut out: Vec<Token> = Vec::new();
    let mut nl = false;

    if starts_at(&ch, 0, "#!") {
        while i < len && ch[i] != '\n' {
            i += 1;
        }
    }

    while i < len {
        let c = ch[i];
        if c == '\n' || c == '\r' || c == '\u{2028}' || c == '\u{2029}' {
            nl = true;
            i += 1;
            continue;
        }
        if c.is_whitespace() || c == '\u{feff}' {
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < len && ch[i + 1] == '/' {
            while i < len && ch[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < len && ch[i + 1] == '*' {
            i += 2;
            while i + 1 < len && !(ch[i] == '*' && ch[i + 1] == '/') {
                if ch[i] == '\n' {
                    nl = true;
                }
                i += 1;
            }
            i += 2;
            continue;
        }

        let tok_start = i;
        let tok: Tok;
        if is_id_start(c) {
            let start = i;
            while i < len && is_id_part(ch[i]) {
                i += 1;
            }
            tok = Tok::Ident(ch[start..i].iter().collect());
        } else if c.is_ascii_digit() || (c == '.' && i + 1 < len && ch[i + 1].is_ascii_digit()) {
            if c == '0' && i + 1 < len && matches!(ch[i + 1], 'x' | 'X' | 'b' | 'B' | 'o' | 'O') {
                let radix: u32 = match ch[i + 1] {
                    'x' | 'X' => 16,
                    'b' | 'B' => 2,
                    _ => 8,
                };
                i += 2;
                let mut acc = 0f64;
                while i < len && (ch[i].is_ascii_alphanumeric() || ch[i] == '_') {
                    if ch[i] != '_' {
                        match ch[i].to_digit(radix) {
                            Some(d) => acc = acc * radix as f64 + d as f64,
                            None => break,
                        }
                    }
                    i += 1;
                }
                if i < len && ch[i] == 'n' {
                    i += 1;
                }
                tok = Tok::Num(acc);
            } else {
                let mut text = String::new();
                while i < len && (ch[i].is_ascii_digit() || ch[i] == '_') {
                    if ch[i] != '_' {
                        text.push(ch[i]);
                    }
                    i += 1;
                }
                if i < len && ch[i] == '.' {
                    text.push('.');
                    i += 1;
                    while i < len && (ch[i].is_ascii_digit() || ch[i] == '_') {
                        if ch[i] != '_' {
                            text.push(ch[i]);
                        }
                        i += 1;
                    }
                }
                if i < len && (ch[i] == 'e' || ch[i] == 'E') {
                    let mut j = i + 1;
                    if j < len && (ch[j] == '+' || ch[j] == '-') {
                        j += 1;
                    }
                    if j < len && ch[j].is_ascii_digit() {
                        text.push('e');
                        i += 1;
                        if ch[i] == '+' || ch[i] == '-' {
                            text.push(ch[i]);
                            i += 1;
                        }
                        while i < len && ch[i].is_ascii_digit() {
                            text.push(ch[i]);
                            i += 1;
                        }
                    }
                }
                if i < len && ch[i] == 'n' {
                    i += 1;
                }
                if text.starts_with('.') {
                    text.insert(0, '0');
                }
                if text.ends_with('.') {
                    text.push('0');
                }
                tok = Tok::Num(text.parse::<f64>().unwrap_or(f64::NAN));
            }
        } else if c == '"' || c == '\'' {
            let q = c;
            i += 1;
            let mut s = String::new();
            loop {
                if i >= len {
                    return Err(String::from("unterminated string literal"));
                }
                let d = ch[i];
                if d == q {
                    i += 1;
                    break;
                }
                if d == '\\' {
                    i += 1;
                    read_escape(&ch, &mut i, &mut s)?;
                    continue;
                }
                s.push(d);
                i += 1;
            }
            tok = Tok::Str(s);
        } else if c == '`' {
            i += 1;
            let (strs, exprs) = read_template(&ch, &mut i)?;
            tok = Tok::Tpl(strs, exprs);
        } else if c == '/' && regex_allowed(out.last().map(|t| &t.tok)) {
            i += 1;
            let mut body = String::new();
            let mut in_class = false;
            loop {
                if i >= len || ch[i] == '\n' {
                    return Err(String::from("unterminated regex literal"));
                }
                let d = ch[i];
                if d == '\\' {
                    body.push(d);
                    i += 1;
                    if i < len {
                        body.push(ch[i]);
                        i += 1;
                    }
                    continue;
                }
                if d == '[' {
                    in_class = true;
                } else if d == ']' {
                    in_class = false;
                } else if d == '/' && !in_class {
                    i += 1;
                    break;
                }
                body.push(d);
                i += 1;
            }
            let mut flags = String::new();
            while i < len && ch[i].is_ascii_alphabetic() {
                flags.push(ch[i]);
                i += 1;
            }
            tok = Tok::Regex(body, flags);
        } else {
            let mut found: Option<&str> = None;
            for p in PUNCTS.iter() {
                if starts_at(&ch, i, p) {
                    if *p == "?." && i + 2 < len && ch[i + 2].is_ascii_digit() {
                        continue;
                    }
                    found = Some(*p);
                    break;
                }
            }
            match found {
                Some(p) => {
                    i += p.chars().count();
                    tok = Tok::Punct(String::from(p));
                }
                None => {
                    if c == '@' || c == '#' {
                        i += 1;
                        let start = i;
                        while i < len && is_id_part(ch[i]) {
                            i += 1;
                        }
                        let name: String = ch[start..i].iter().collect();
                        let mut full = String::new();
                        full.push(c);
                        full.push_str(&name);
                        tok = Tok::Ident(full);
                    } else {
                        return Err(alloc::format!("unexpected character '{}' at char {}", c, i));
                    }
                }
            }
        }
        out.push(Token { tok, nl, pos: tok_start });
        nl = false;
    }
    out.push(Token { tok: Tok::Eof, nl, pos: len });
    Ok(out)
}
