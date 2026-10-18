use crate::interp::*;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}

fn def(it: &mut Interp, o: &ObjRef, name: &str, f: NativeFn) {
    let v = it.new_native(f);
    it.set_own(o, name, v);
}

fn setg(it: &mut Interp, name: &str, v: Value) {
    let g = it.global.clone();
    it.set_own(&g, name, v);
}

fn ctor(it: &mut Interp, name: &str, f: NativeFn, proto: &ObjRef) -> ObjRef {
    let c = it.alloc(Kind::Native(f), Some(it.protos.function.clone()));
    it.set_own(&c, "prototype", Value::Obj(proto.clone()));
    it.set_own(proto, "constructor", Value::Obj(c.clone()));
    setg(it, name, Value::Obj(c.clone()));
    c
}

fn num_arg(it: &mut Interp, args: &[Value], i: usize) -> Res<f64> {
    let v = arg(args, i);
    it.to_num(&v)
}

fn str_arg(it: &mut Interp, args: &[Value], i: usize) -> Res<String> {
    let v = arg(args, i);
    it.to_str(&v)
}

fn this_array(it: &mut Interp, this: &Value) -> Res<Vec<Value>> {
    if let Value::Obj(o) = this {
        if let Kind::Array(a) = &o.borrow().kind {
            return Ok(a.clone());
        }
    }
    Err(it.type_err("not an array"))
}

fn with_arr<R>(this: &Value, f: impl FnOnce(&mut Vec<Value>) -> R) -> Option<R> {
    if let Value::Obj(o) = this {
        if let Kind::Array(a) = &mut o.borrow_mut().kind {
            return Some(f(a));
        }
    }
    None
}

fn rel(n: f64, len: usize) -> usize {
    if n.is_nan() {
        return 0;
    }
    let l = len as f64;
    let t = libm::trunc(n);
    let v = if t < 0.0 { (l + t).max(0.0) } else { t.min(l) };
    v as usize
}

fn rel_opt(it: &mut Interp, args: &[Value], i: usize, len: usize, default: usize) -> Res<usize> {
    let v = arg(args, i);
    if matches!(v, Value::Undefined) {
        return Ok(default);
    }
    Ok(rel(it.to_num(&v)?, len))
}

fn chars_of(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn from_chars(c: &[char]) -> String {
    c.iter().collect()
}

fn find_chars(h: &[char], n: &[char], from: usize) -> Option<usize> {
    if n.is_empty() {
        return if from <= h.len() { Some(from) } else { Some(h.len()) };
    }
    if n.len() > h.len() {
        return None;
    }
    let mut i = from;
    while i + n.len() <= h.len() {
        if &h[i..i + n.len()] == n {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn get_regex(v: &Value) -> Option<Rc<crate::regex::Regex>> {
    if let Value::Obj(o) = v {
        if let Kind::Regex(r) = &o.borrow().kind {
            return Some(r.clone());
        }
    }
    None
}

fn is_regex(v: &Value) -> bool {
    get_regex(v).is_some()
}

fn last_index(it: &Interp, o: &ObjRef) -> usize {
    match it.get_own_chain(o, "lastIndex") {
        Some(Value::Num(n)) if n >= 0.0 => n as usize,
        _ => 0,
    }
}

fn cap_value(s: &[char], c: Option<(usize, usize)>) -> Value {
    match c {
        Some((a, b)) if b <= s.len() && a <= b => Value::string(from_chars(&s[a..b])),
        _ => Value::Undefined,
    }
}

fn caps_to_array(it: &mut Interp, re: &crate::regex::Regex, s: &[char], caps: &crate::regex::Caps) -> Value {
    let items: Vec<Value> = caps.iter().map(|c| cap_value(s, *c)).collect();
    let arr = it.new_array(items);
    if let Value::Obj(o) = &arr {
        let idx = caps[0].map(|c| c.0).unwrap_or(0);
        it.set_own(o, "index", Value::Num(idx as f64));
        it.set_own(o, "input", Value::string(from_chars(s)));
        if re.names.is_empty() {
            it.set_own(o, "groups", Value::Undefined);
        } else {
            let g = it.new_plain();
            for (name, i) in re.names.iter() {
                let v = cap_value(s, caps.get(*i).copied().flatten());
                it.set_own(&g, name, v);
            }
            it.set_own(o, "groups", Value::Obj(g));
        }
    }
    arr
}

fn regex_exec(it: &mut Interp, rv: &Value, s: &str) -> Res<Value> {
    let (o, re) = match (rv, get_regex(rv)) {
        (Value::Obj(o), Some(r)) => (o.clone(), r),
        _ => return Err(it.type_err("receiver is not a RegExp")),
    };
    let chars = chars_of(s);
    let use_last = re.global || re.sticky;
    let start = if use_last { last_index(it, &o) } else { 0 };
    if start > chars.len() {
        it.set_own(&o, "lastIndex", Value::Num(0.0));
        return Ok(Value::Null);
    }
    match re.exec_at(&chars, start) {
        Some(caps) => {
            if use_last {
                let e = caps[0].map(|c| c.1).unwrap_or(0);
                it.set_own(&o, "lastIndex", Value::Num(e as f64));
            }
            Ok(caps_to_array(it, &re, &chars, &caps))
        }
        None => {
            if use_last {
                it.set_own(&o, "lastIndex", Value::Num(0.0));
            }
            Ok(Value::Null)
        }
    }
}

fn to_regex(it: &mut Interp, v: &Value, flags: &str) -> Res<Value> {
    if is_regex(v) {
        return Ok(v.clone());
    }
    let src = match v {
        Value::Undefined => String::new(),
        other => it.to_str(other)?,
    };
    it.regex_obj(&src, flags)
}

fn all_matches(re: &crate::regex::Regex, chars: &[char], global: bool) -> Vec<crate::regex::Caps> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos <= chars.len() {
        match re.exec_at(chars, pos) {
            Some(c) => {
                let (a, b) = c[0].unwrap_or((pos, pos));
                pos = if b == a { b + 1 } else { b };
                out.push(c);
                if !global {
                    break;
                }
            }
            None => break,
        }
        if out.len() > 200_000 {
            break;
        }
    }
    out
}

fn expand_template(tpl: &str, chars: &[char], caps: &crate::regex::Caps, re: Option<&crate::regex::Regex>) -> String {
    let t: Vec<char> = tpl.chars().collect();
    let (ma, mb) = caps[0].unwrap_or((0, 0));
    let mut out = String::new();
    let mut i = 0;
    while i < t.len() {
        if t[i] == '$' && i + 1 < t.len() {
            let n = t[i + 1];
            if n == '$' {
                out.push('$');
                i += 2;
                continue;
            }
            if n == '&' {
                out.push_str(&from_chars(&chars[ma..mb]));
                i += 2;
                continue;
            }
            if n == '`' {
                out.push_str(&from_chars(&chars[..ma]));
                i += 2;
                continue;
            }
            if n == '\'' {
                out.push_str(&from_chars(&chars[mb..]));
                i += 2;
                continue;
            }
            if n.is_ascii_digit() {
                let mut num = n.to_digit(10).unwrap_or(0) as usize;
                let mut used = 2;
                if i + 2 < t.len() && t[i + 2].is_ascii_digit() {
                    let two = num * 10 + t[i + 2].to_digit(10).unwrap_or(0) as usize;
                    if two < caps.len() {
                        num = two;
                        used = 3;
                    }
                }
                if num >= 1 && num < caps.len() {
                    if let Value::Str(s) = cap_value(chars, caps[num]) {
                        out.push_str(&s);
                    }
                    i += used;
                    continue;
                }
            }
            if n == '<' {
                if let Some(r) = re {
                    if let Some(close) = t[i + 2..].iter().position(|c| *c == '>') {
                        let name: String = t[i + 2..i + 2 + close].iter().collect();
                        if let Some((_, gi)) = r.names.iter().find(|(nm, _)| *nm == name) {
                            if let Value::Str(s) = cap_value(chars, caps.get(*gi).copied().flatten()) {
                                out.push_str(&s);
                            }
                        }
                        i += close + 3;
                        continue;
                    }
                }
            }
        }
        out.push(t[i]);
        i += 1;
    }
    out
}

// ---------- sorting ----------

fn sort_le(it: &mut Interp, cmp: &Value, a: &Value, b: &Value) -> Res<bool> {
    if cmp.is_callable() {
        let r = it.call(cmp, &Value::Undefined, &[a.clone(), b.clone()])?;
        let n = it.to_num(&r)?;
        return Ok(n.is_nan() || n <= 0.0);
    }
    if matches!(b, Value::Undefined) {
        return Ok(true);
    }
    if matches!(a, Value::Undefined) {
        return Ok(false);
    }
    let x = it.to_str(a)?;
    let y = it.to_str(b)?;
    Ok(x <= y)
}

fn sort_values(it: &mut Interp, v: Vec<Value>, cmp: &Value) -> Res<Vec<Value>> {
    let mut out: Vec<Value> = Vec::with_capacity(v.len());
    for x in v {
        let mut lo = 0usize;
        let mut hi = out.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            let m = out[mid].clone();
            if sort_le(it, cmp, &m, &x)? {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        out.insert(lo, x);
    }
    Ok(out)
}

fn join_values(it: &mut Interp, items: &[Value], sep: &str) -> Res<String> {
    let mut out = String::new();
    for (i, v) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        if !v.is_nullish() {
            out.push_str(&it.to_str(v)?);
        }
    }
    Ok(out)
}

fn flatten(it: &mut Interp, items: &[Value], depth: usize, out: &mut Vec<Value>) {
    for v in items {
        let inner = if let Value::Obj(o) = v {
            if let Kind::Array(a) = &o.borrow().kind {
                Some(a.clone())
            } else {
                None
            }
        } else {
            None
        };
        match inner {
            Some(a) if depth > 0 => flatten(it, &a, depth - 1, out),
            _ => out.push(v.clone()),
        }
    }
}

// ---------- JSON ----------

fn json_quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn json_ser(it: &mut Interp, v: &Value, ind: &str, cur: &str, out: &mut String, depth: usize) -> Res<bool> {
    if depth > 40 {
        return Err(it.type_err("Converting circular structure to JSON"));
    }
    let mut v = v.clone();
    if let Value::Obj(o) = &v {
        if let Some(f) = it.get_own_chain(o, "toJSON") {
            if f.is_callable() {
                v = it.call(&f, &v, &[])?;
            }
        }
    }
    match &v {
        Value::Undefined => Ok(false),
        Value::Null => {
            out.push_str("null");
            Ok(true)
        }
        Value::Bool(b) => {
            out.push_str(if *b { "true" } else { "false" });
            Ok(true)
        }
        Value::Num(n) => {
            if n.is_finite() {
                out.push_str(&num_to_str(*n));
            } else {
                out.push_str("null");
            }
            Ok(true)
        }
        Value::Str(s) => {
            json_quote(s, out);
            Ok(true)
        }
        Value::Obj(o) => {
            if v.is_callable() {
                return Ok(false);
            }
            let inner = format!("{}{}", cur, ind);
            let arr = if let Kind::Array(a) = &o.borrow().kind { Some(a.clone()) } else { None };
            if let Some(a) = arr {
                if a.is_empty() {
                    out.push_str("[]");
                    return Ok(true);
                }
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    if !ind.is_empty() {
                        out.push('\n');
                        out.push_str(&inner);
                    }
                    if !json_ser(it, x, ind, &inner, out, depth + 1)? {
                        out.push_str("null");
                    }
                }
                if !ind.is_empty() {
                    out.push('\n');
                    out.push_str(cur);
                }
                out.push(']');
                return Ok(true);
            }
            out.push('{');
            let mut first = true;
            for k in it.own_keys(&v) {
                let pv = it.get_own_chain(o, &k).unwrap_or(Value::Undefined);
                let mark = out.len();
                if !first {
                    out.push(',');
                }
                if !ind.is_empty() {
                    out.push('\n');
                    out.push_str(&inner);
                }
                json_quote(&k, out);
                out.push(':');
                if !ind.is_empty() {
                    out.push(' ');
                }
                if json_ser(it, &pv, ind, &inner, out, depth + 1)? {
                    first = false;
                } else {
                    out.truncate(mark);
                }
            }
            if !first && !ind.is_empty() {
                out.push('\n');
                out.push_str(cur);
            }
            out.push('}');
            Ok(true)
        }
    }
}

struct JP {
    c: Vec<char>,
    i: usize,
}

impl JP {
    fn ws(&mut self) {
        while self.i < self.c.len() && self.c[self.i].is_whitespace() {
            self.i += 1;
        }
    }
    fn lit(&mut self, s: &str) -> bool {
        let cs: Vec<char> = s.chars().collect();
        if self.i + cs.len() <= self.c.len() && self.c[self.i..self.i + cs.len()] == cs[..] {
            self.i += cs.len();
            true
        } else {
            false
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            if self.i >= self.c.len() {
                return Err(String::from("Unterminated string in JSON"));
            }
            let ch = self.c[self.i];
            self.i += 1;
            match ch {
                '"' => return Ok(out),
                '\\' => {
                    if self.i >= self.c.len() {
                        return Err(String::from("Bad escape in JSON"));
                    }
                    let e = self.c[self.i];
                    self.i += 1;
                    match e {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => {
                            let mut v = 0u32;
                            for _ in 0..4 {
                                let d = self.c.get(self.i).and_then(|c| c.to_digit(16)).ok_or_else(|| String::from("Bad \\u escape in JSON"))?;
                                v = v * 16 + d;
                                self.i += 1;
                            }
                            out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
                        }
                        other => out.push(other),
                    }
                }
                c => out.push(c),
            }
        }
    }
    fn value(&mut self, it: &mut Interp, depth: usize) -> Result<Value, String> {
        if depth > 60 {
            return Err(String::from("JSON nesting too deep"));
        }
        self.ws();
        if self.i >= self.c.len() {
            return Err(String::from("Unexpected end of JSON input"));
        }
        let ch = self.c[self.i];
        match ch {
            '{' => {
                self.i += 1;
                let o = it.new_plain();
                self.ws();
                if self.i < self.c.len() && self.c[self.i] == '}' {
                    self.i += 1;
                    return Ok(Value::Obj(o));
                }
                loop {
                    self.ws();
                    if self.i >= self.c.len() || self.c[self.i] != '"' {
                        return Err(String::from("Expected string key in JSON"));
                    }
                    let k = self.string()?;
                    self.ws();
                    if !self.lit(":") {
                        return Err(String::from("Expected ':' in JSON"));
                    }
                    let v = self.value(it, depth + 1)?;
                    it.set_own(&o, &k, v);
                    self.ws();
                    if self.lit(",") {
                        continue;
                    }
                    if self.lit("}") {
                        return Ok(Value::Obj(o));
                    }
                    return Err(String::from("Expected ',' or '}' in JSON"));
                }
            }
            '[' => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.i < self.c.len() && self.c[self.i] == ']' {
                    self.i += 1;
                    return Ok(it.new_array(items));
                }
                loop {
                    let v = self.value(it, depth + 1)?;
                    items.push(v);
                    self.ws();
                    if self.lit(",") {
                        continue;
                    }
                    if self.lit("]") {
                        return Ok(it.new_array(items));
                    }
                    return Err(String::from("Expected ',' or ']' in JSON"));
                }
            }
            '"' => Ok(Value::string(self.string()?)),
            't' if self.lit("true") => Ok(Value::Bool(true)),
            'f' if self.lit("false") => Ok(Value::Bool(false)),
            'n' if self.lit("null") => Ok(Value::Null),
            _ => {
                let start = self.i;
                while self.i < self.c.len() && matches!(self.c[self.i], '0'..='9' | '-' | '+' | '.' | 'e' | 'E') {
                    self.i += 1;
                }
                let t: String = self.c[start..self.i].iter().collect();
                match t.parse::<f64>() {
                    Ok(n) if !t.is_empty() => Ok(Value::Num(n)),
                    _ => Err(format!("Unexpected token '{}' in JSON", ch)),
                }
            }
        }
    }
}

// ---------- dates ----------

fn civil(ms: f64) -> (i64, i64, i64, i64, i64, i64, i64, i64) {
    let days = libm::floor(ms / 86_400_000.0) as i64;
    let rem = (ms - (days as f64) * 86_400_000.0) as i64;
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let wd = (days + 4).rem_euclid(7);
    (y, m, d, rem / 3_600_000, (rem / 60_000) % 60, (rem / 1000) % 60, rem % 1000, wd)
}

fn date_val(this: &Value) -> f64 {
    if let Value::Obj(o) = this {
        if let Kind::Date(t) = &o.borrow().kind {
            return *t;
        }
    }
    f64::NAN
}

// ---------- URI ----------

fn uri_encode(s: &str, keep: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || keep.contains(c) {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn uri_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = (b[i + 1] as char).to_digit(16);
            let l = (b[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (h, l) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------- promise helpers ----------

fn promise_of(it: &mut Interp, v: &Value) -> ObjRef {
    if let Value::Obj(o) = v {
        if it.promise_state(o).is_some() {
            return o.clone();
        }
    }
    let p = it.new_promise();
    it.resolve_with(&p, v.clone());
    p
}

fn promise_then(it: &mut Interp, p: &ObjRef, on_ful: Value, on_rej: Value) -> Value {
    let child = it.new_promise();
    it.subscribe(p, Reaction { on_ful, on_rej, child: Some(child.clone()) });
    Value::Obj(child)
}

fn data_obj(d: &[Value], i: usize) -> Option<ObjRef> {
    match d.get(i) {
        Some(Value::Obj(o)) => Some(o.clone()),
        _ => None,
    }
}

fn pr_resolve_fn(it: &mut Interp, d: &[Value], _t: &Value, a: &[Value]) -> Res<Value> {
    if let Some(p) = data_obj(d, 0) {
        it.resolve_with(&p, arg(a, 0));
    }
    Ok(Value::Undefined)
}

fn pr_reject_fn(it: &mut Interp, d: &[Value], _t: &Value, a: &[Value]) -> Res<Value> {
    if let Some(p) = data_obj(d, 0) {
        it.settle(&p, 2, arg(a, 0));
    }
    Ok(Value::Undefined)
}

fn pr_finally_ok(it: &mut Interp, d: &[Value], _t: &Value, a: &[Value]) -> Res<Value> {
    it.call(&d[0], &Value::Undefined, &[])?;
    Ok(arg(a, 0))
}

fn pr_finally_err(it: &mut Interp, d: &[Value], _t: &Value, a: &[Value]) -> Res<Value> {
    it.call(&d[0], &Value::Undefined, &[])?;
    Err(Ctl::Throw(arg(a, 0)))
}

fn pr_all_item(it: &mut Interp, d: &[Value], _t: &Value, a: &[Value]) -> Res<Value> {
    // d: [resultArray, index, counterObj, outerPromise, total]
    let idx = match &d[1] {
        Value::Num(n) => *n as usize,
        _ => 0,
    };
    let total = match &d[4] {
        Value::Num(n) => *n as usize,
        _ => 0,
    };
    with_arr(&d[0], |v| {
        if idx < v.len() {
            v[idx] = arg(a, 0);
        }
    });
    let counter = data_obj(d, 2);
    let outer = data_obj(d, 3);
    if let (Some(c), Some(p)) = (counter, outer) {
        let n = match it.get_own_chain(&c, "n") {
            Some(Value::Num(n)) => n + 1.0,
            _ => 1.0,
        };
        it.set_own(&c, "n", Value::Num(n));
        if n as usize >= total {
            let arr = d[0].clone();
            it.resolve_with(&p, arr);
        }
    }
    Ok(Value::Undefined)
}

fn pr_all_fail(it: &mut Interp, d: &[Value], _t: &Value, a: &[Value]) -> Res<Value> {
    if let Some(p) = data_obj(d, 0) {
        it.settle(&p, 2, arg(a, 0));
    }
    Ok(Value::Undefined)
}

fn pr_race_ok(it: &mut Interp, d: &[Value], _t: &Value, a: &[Value]) -> Res<Value> {
    if let Some(p) = data_obj(d, 0) {
        it.resolve_with(&p, arg(a, 0));
    }
    Ok(Value::Undefined)
}

fn timer_args(it: &mut Interp, args: &[Value], repeat: bool) -> Res<Value> {
    let f = arg(args, 0);
    if !f.is_callable() {
        return Err(it.type_err("timer callback is not a function"));
    }
    let delay = num_arg(it, args, 1).unwrap_or(0.0);
    let extra: Vec<Value> = args.iter().skip(2).cloned().collect();
    let id = it.add_timer(f, delay, repeat, extra);
    Ok(Value::Num(id as f64))
}

fn parse_int(s: &str, radix: u32) -> f64 {
    let t = s.trim();
    let (neg, mut rest) = if let Some(r) = t.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = t.strip_prefix('+') {
        (false, r)
    } else {
        (false, t)
    };
    let mut radix = radix;
    if (radix == 0 || radix == 16) && (rest.starts_with("0x") || rest.starts_with("0X")) {
        rest = &rest[2..];
        radix = 16;
    }
    if radix == 0 {
        radix = 10;
    }
    if !(2..=36).contains(&radix) {
        return f64::NAN;
    }
    let mut acc = 0f64;
    let mut any = false;
    for c in rest.chars() {
        match c.to_digit(radix) {
            Some(d) => {
                acc = acc * radix as f64 + d as f64;
                any = true;
            }
            None => break,
        }
    }
    if !any {
        return f64::NAN;
    }
    if neg {
        -acc
    } else {
        acc
    }
}

fn parse_float(s: &str) -> f64 {
    let t: Vec<char> = s.trim().chars().collect();
    let mut i = 0;
    let mut out = String::new();
    if i < t.len() && (t[i] == '+' || t[i] == '-') {
        out.push(t[i]);
        i += 1;
    }
    let rest: String = t[i..].iter().collect();
    if rest.starts_with("Infinity") {
        return if out == "-" { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    let mut digits = false;
    while i < t.len() && t[i].is_ascii_digit() {
        out.push(t[i]);
        i += 1;
        digits = true;
    }
    if i < t.len() && t[i] == '.' {
        out.push('.');
        i += 1;
        while i < t.len() && t[i].is_ascii_digit() {
            out.push(t[i]);
            i += 1;
            digits = true;
        }
    }
    if !digits {
        return f64::NAN;
    }
    if i < t.len() && (t[i] == 'e' || t[i] == 'E') {
        let mut j = i + 1;
        let mut exp = String::from("e");
        if j < t.len() && (t[j] == '+' || t[j] == '-') {
            exp.push(t[j]);
            j += 1;
        }
        let mut ed = false;
        while j < t.len() && t[j].is_ascii_digit() {
            exp.push(t[j]);
            j += 1;
            ed = true;
        }
        if ed {
            out.push_str(&exp);
        }
    }
    out.parse::<f64>().unwrap_or(f64::NAN)
}

fn this_str(it: &mut Interp, this: &Value) -> Res<String> {
    it.to_str(this)
}

pub fn setup(it: &mut Interp) {
    let g = it.global.clone();
    setg(it, "undefined", Value::Undefined);
    setg(it, "NaN", Value::Num(f64::NAN));
    setg(it, "Infinity", Value::Num(f64::INFINITY));
    for n in ["globalThis", "window", "self", "global"] {
        setg(it, n, Value::Obj(g.clone()));
    }

    setup_object(it);
    setup_function(it);
    setup_errors(it);
    setup_array(it);
    setup_string(it);
    setup_number_math(it);
    setup_json(it);
    setup_collections(it);
    setup_promise_timers(it);
    setup_misc(it);
    setup_proxy(it);
}

fn setup_object(it: &mut Interp) {
    let op = it.protos.object.clone();
    def(it, &op, "hasOwnProperty", |it, this, args| {
        let k = str_arg(it, args, 0)?;
        if let Value::Obj(o) = this {
            if let Kind::Array(a) = &o.borrow().kind {
                if let Some(i) = parse_index(&k) {
                    return Ok(Value::Bool(i < a.len()));
                }
            }
            return Ok(Value::Bool(o.borrow().props.iter().any(|(p, _)| *p == k)));
        }
        Ok(Value::Bool(false))
    });
    def(it, &op, "toString", |_it, _this, _args| Ok(Value::str("[object Object]")));
    def(it, &op, "valueOf", |_it, this, _args| Ok(this.clone()));
    def(it, &op, "isPrototypeOf", |_it, this, args| {
        if let (Value::Obj(p), Value::Obj(o)) = (this, &arg(args, 0)) {
            let mut cur = o.borrow().proto.clone();
            while let Some(c) = cur {
                if Rc::ptr_eq(&c, p) {
                    return Ok(Value::Bool(true));
                }
                cur = c.borrow().proto.clone();
            }
        }
        Ok(Value::Bool(false))
    });
    let oc = ctor(it, "Object", |it, _this, args| {
        let v = arg(args, 0);
        if matches!(v, Value::Obj(_)) {
            Ok(v)
        } else {
            Ok(Value::Obj(it.new_plain()))
        }
    }, &op);
    def(it, &oc, "keys", |it, _t, args| {
        let keys: Vec<Value> = it.own_keys(&arg(args, 0)).into_iter().map(Value::string).collect();
        Ok(it.new_array(keys))
    });
    def(it, &oc, "getOwnPropertyNames", |it, _t, args| {
        let keys: Vec<Value> = it.own_keys(&arg(args, 0)).into_iter().map(Value::string).collect();
        Ok(it.new_array(keys))
    });
    def(it, &oc, "values", |it, _t, args| {
        let o = arg(args, 0);
        let mut out = Vec::new();
        for k in it.own_keys(&o) {
            out.push(it.get_prop(&o, &k)?);
        }
        Ok(it.new_array(out))
    });
    def(it, &oc, "entries", |it, _t, args| {
        let o = arg(args, 0);
        let mut out = Vec::new();
        for k in it.own_keys(&o) {
            let v = it.get_prop(&o, &k)?;
            let pair = it.new_array(vec![Value::string(k), v]);
            out.push(pair);
        }
        Ok(it.new_array(out))
    });
    def(it, &oc, "assign", |it, _t, args| {
        let target = arg(args, 0);
        for src in args.iter().skip(1) {
            for k in it.own_keys(src) {
                let v = it.get_prop(src, &k)?;
                it.set_prop(&target, &k, v)?;
            }
        }
        Ok(target)
    });
    def(it, &oc, "fromEntries", |it, _t, args| {
        let o = it.new_plain();
        for pair in it.iterate(&arg(args, 0))? {
            let k = it.get_prop(&pair, "0")?;
            let v = it.get_prop(&pair, "1")?;
            let ks = it.to_key(&k)?;
            it.set_own(&o, &ks, v);
        }
        Ok(Value::Obj(o))
    });
    def(it, &oc, "create", |it, _t, args| {
        let proto = match arg(args, 0) {
            Value::Obj(p) => Some(p),
            _ => None,
        };
        Ok(Value::Obj(it.alloc(Kind::Plain, proto)))
    });
    def(it, &oc, "getPrototypeOf", |_it, _t, args| {
        let target = arg(args, 0);
        let p = match &target {
            Value::Obj(o) => o.borrow().proto.clone(),
            _ => None,
        };
        Ok(match p {
            Some(p) => Value::Obj(p),
            None => Value::Null,
        })
    });
    def(it, &oc, "setPrototypeOf", |_it, _t, args| {
        if let Value::Obj(o) = arg(args, 0) {
            o.borrow_mut().proto = match arg(args, 1) {
                Value::Obj(p) => Some(p),
                _ => None,
            };
        }
        Ok(arg(args, 0))
    });
    for name in ["freeze", "seal", "preventExtensions"] {
        def(it, &oc, name, |_it, _t, args| Ok(arg(args, 0)));
    }
    for name in ["isFrozen", "isSealed"] {
        def(it, &oc, name, |_it, _t, _args| Ok(Value::Bool(false)));
    }
    def(it, &oc, "hasOwn", |it, _t, args| {
        let k = str_arg(it, args, 1)?;
        Ok(Value::Bool(it.own_keys(&arg(args, 0)).contains(&k)))
    });
    def(it, &oc, "defineProperty", |it, _t, args| {
        let target = arg(args, 0);
        let k = str_arg(it, args, 1)?;
        let desc = arg(args, 2);
        let g = it.get_prop(&desc, "get")?;
        let st = it.get_prop(&desc, "set")?;
        if g.is_callable() || st.is_callable() {
            if let Value::Obj(o) = &target {
                it.define_accessor(o, &k, if g.is_callable() { Some(g) } else { None }, if st.is_callable() { Some(st) } else { None });
            }
        } else {
            let v = it.get_prop(&desc, "value")?;
            it.set_prop(&target, &k, v)?;
        }
        Ok(target)
    });
    def(it, &oc, "defineProperties", |it, _t, args| {
        let target = arg(args, 0);
        let props = arg(args, 1);
        for k in it.own_keys(&props) {
            let desc = it.get_prop(&props, &k)?;
            let v = it.get_prop(&desc, "value")?;
            it.set_prop(&target, &k, v)?;
        }
        Ok(target)
    });
    def(it, &oc, "getOwnPropertyDescriptor", |it, _t, args| {
        let o = arg(args, 0);
        let k = str_arg(it, args, 1)?;
        if !it.own_keys(&o).contains(&k) {
            return Ok(Value::Undefined);
        }
        let v = it.get_prop(&o, &k)?;
        let d = it.new_plain();
        it.set_own(&d, "value", v);
        for f in ["writable", "enumerable", "configurable"] {
            it.set_own(&d, f, Value::Bool(true));
        }
        Ok(Value::Obj(d))
    });
    def(it, &oc, "getOwnPropertySymbols", |it, _t, _a| Ok(it.new_array(Vec::new())));
    def(it, &oc, "is", |_it, _t, args| Ok(Value::Bool(same_value_zero(&arg(args, 0), &arg(args, 1)))));
}

fn setup_function(it: &mut Interp) {
    let fp = it.protos.function.clone();
    ctor(it, "Function", |it, _t, _a| Err(it.type_err("Function constructor is not supported")), &fp);
    def(it, &fp, "call", |it, this, args| {
        let t = arg(args, 0);
        let rest: Vec<Value> = args.iter().skip(1).cloned().collect();
        it.call(this, &t, &rest)
    });
    def(it, &fp, "apply", |it, this, args| {
        let t = arg(args, 0);
        let list = match arg(args, 1) {
            Value::Obj(_) => it.iterate(&arg(args, 1))?,
            _ => Vec::new(),
        };
        it.call(this, &t, &list)
    });
    def(it, &fp, "bind", |it, this, args| {
        let t = arg(args, 0);
        let rest: Vec<Value> = args.iter().skip(1).cloned().collect();
        Ok(Value::Obj(it.alloc(Kind::Bound(this.clone(), t, rest), Some(it.protos.function.clone()))))
    });
    def(it, &fp, "toString", |_it, _t, _a| Ok(Value::str("function () { [native code] }")));
}

fn error_ctor(it: &mut Interp, this: &Value, args: &[Value], kind: &str) -> Res<Value> {
    let o = match this {
        Value::Obj(o) if matches!(o.borrow().kind, Kind::Plain) && it.get_own_chain(o, "@@err").is_some() => o.clone(),
        _ => match it.make_error(kind, "") {
            Value::Obj(o) => o,
            _ => it.new_plain(),
        },
    };
    let msg = match arg(args, 0) {
        Value::Undefined => String::new(),
        v => it.to_str(&v)?,
    };
    it.set_own(&o, "message", Value::str(&msg));
    it.set_own(&o, "stack", Value::string(format!("{}: {}", kind, msg)));
    Ok(Value::Obj(o))
}

fn setup_errors(it: &mut Interp) {
    let ep = it.protos.error.clone();
    it.set_own(&ep, "@@err", Value::Bool(true));
    it.set_own(&ep, "name", Value::str("Error"));
    it.set_own(&ep, "message", Value::str(""));
    def(it, &ep, "toString", |it, this, _a| {
        let n = it.get_prop(this, "name")?;
        let m = it.get_prop(this, "message")?;
        let ns = it.to_str(&n)?;
        let ms = it.to_str(&m)?;
        Ok(Value::string(if ms.is_empty() { ns } else { format!("{}: {}", ns, ms) }))
    });
    ctor(it, "Error", |it, t, a| error_ctor(it, t, a, "Error"), &ep);
    let make = |it: &mut Interp, name: &str, f: NativeFn| {
        let p = it.alloc(Kind::Plain, Some(it.protos.error.clone()));
        it.set_own(&p, "name", Value::str(name));
        ctor(it, name, f, &p);
    };
    make(it, "TypeError", |it, t, a| error_ctor(it, t, a, "TypeError"));
    make(it, "RangeError", |it, t, a| error_ctor(it, t, a, "RangeError"));
    make(it, "SyntaxError", |it, t, a| error_ctor(it, t, a, "SyntaxError"));
    make(it, "ReferenceError", |it, t, a| error_ctor(it, t, a, "ReferenceError"));
    make(it, "EvalError", |it, t, a| error_ctor(it, t, a, "EvalError"));
    make(it, "URIError", |it, t, a| error_ctor(it, t, a, "URIError"));
}

fn setup_array(it: &mut Interp) {
    let ap = it.protos.array.clone();
    let ac = ctor(it, "Array", |it, _t, args| {
        if args.len() == 1 {
            if let Value::Num(n) = &args[0] {
                if *n >= 0.0 && *n < 5_000_000.0 {
                    return Ok(it.new_array(vec![Value::Undefined; *n as usize]));
                }
            }
        }
        Ok(it.new_array(args.to_vec()))
    }, &ap);
    def(it, &ac, "isArray", |_it, _t, args| {
        Ok(Value::Bool(matches!(&arg(args, 0), Value::Obj(o) if matches!(o.borrow().kind, Kind::Array(_)))))
    });
    def(it, &ac, "of", |it, _t, args| Ok(it.new_array(args.to_vec())));
    def(it, &ac, "from", |it, _t, args| {
        let src = arg(args, 0);
        let f = arg(args, 1);
        let mut items = match &src {
            Value::Obj(o) => {
                let is_iter = matches!(o.borrow().kind, Kind::Array(_) | Kind::Set(_) | Kind::Map(_)) || it.get_own_chain(o, "@@iterator").is_some();
                if is_iter {
                    it.iterate(&src)?
                } else {
                    let len = it.get_prop(&src, "length")?;
                    let n = it.to_num(&len)?;
                    let n = if n.is_nan() || n < 0.0 { 0 } else { (n as usize).min(1_000_000) };
                    let mut v = Vec::new();
                    for i in 0..n {
                        v.push(it.get_prop(&src, &i.to_string())?);
                    }
                    v
                }
            }
            Value::Str(_) => it.iterate(&src)?,
            _ => Vec::new(),
        };
        if f.is_callable() {
            for (i, x) in items.iter_mut().enumerate() {
                *x = it.call(&f, &Value::Undefined, &[x.clone(), Value::Num(i as f64)])?;
            }
        }
        Ok(it.new_array(items))
    });

    def(it, &ap, "push", |_it, this, args| Ok(Value::Num(with_arr(this, |a| {
        a.extend_from_slice(args);
        a.len()
    }).unwrap_or(0) as f64)));
    def(it, &ap, "pop", |_it, this, _a| Ok(with_arr(this, |a| a.pop()).flatten().unwrap_or(Value::Undefined)));
    def(it, &ap, "shift", |_it, this, _a| {
        Ok(with_arr(this, |a| if a.is_empty() { None } else { Some(a.remove(0)) }).flatten().unwrap_or(Value::Undefined))
    });
    def(it, &ap, "unshift", |_it, this, args| {
        Ok(Value::Num(with_arr(this, |a| {
            let mut v = args.to_vec();
            v.append(a);
            *a = v;
            a.len()
        }).unwrap_or(0) as f64))
    });
    def(it, &ap, "slice", |it, this, args| {
        let a = this_array(it, this)?;
        let s = rel_opt(it, args, 0, a.len(), 0)?;
        let e = rel_opt(it, args, 1, a.len(), a.len())?;
        Ok(it.new_array(if s < e { a[s..e].to_vec() } else { Vec::new() }))
    });
    def(it, &ap, "splice", |it, this, args| {
        let len = this_array(it, this)?.len();
        let s = rel_opt(it, args, 0, len, 0)?;
        let dc = if args.len() < 2 { len - s } else { (num_arg(it, args, 1)?.max(0.0) as usize).min(len - s) };
        let ins: Vec<Value> = args.iter().skip(2).cloned().collect();
        let removed = with_arr(this, |a| a.splice(s..s + dc, ins).collect::<Vec<Value>>()).unwrap_or_default();
        Ok(it.new_array(removed))
    });
    def(it, &ap, "concat", |it, this, args| {
        let mut out = this_array(it, this)?;
        for a in args {
            let inner = if let Value::Obj(o) = a {
                if let Kind::Array(v) = &o.borrow().kind { Some(v.clone()) } else { None }
            } else {
                None
            };
            match inner {
                Some(v) => out.extend(v),
                None => out.push(a.clone()),
            }
        }
        Ok(it.new_array(out))
    });
    def(it, &ap, "indexOf", |it, this, args| {
        let a = this_array(it, this)?;
        let t = arg(args, 0);
        let from = rel_opt(it, args, 1, a.len(), 0)?;
        for (i, x) in a.iter().enumerate().skip(from) {
            if strict_eq(x, &t) {
                return Ok(Value::Num(i as f64));
            }
        }
        Ok(Value::Num(-1.0))
    });
    def(it, &ap, "lastIndexOf", |it, this, args| {
        let a = this_array(it, this)?;
        let t = arg(args, 0);
        for (i, x) in a.iter().enumerate().rev() {
            if strict_eq(x, &t) {
                return Ok(Value::Num(i as f64));
            }
        }
        Ok(Value::Num(-1.0))
    });
    def(it, &ap, "includes", |it, this, args| {
        let a = this_array(it, this)?;
        let t = arg(args, 0);
        Ok(Value::Bool(a.iter().any(|x| same_value_zero(x, &t))))
    });
    def(it, &ap, "join", |it, this, args| {
        let a = this_array(it, this)?;
        let sep = match arg(args, 0) {
            Value::Undefined => String::from(","),
            v => it.to_str(&v)?,
        };
        Ok(Value::string(join_values(it, &a, &sep)?))
    });
    def(it, &ap, "toString", |it, this, _a| {
        let a = this_array(it, this)?;
        Ok(Value::string(join_values(it, &a, ",")?))
    });
    def(it, &ap, "reverse", |_it, this, _a| {
        with_arr(this, |a| a.reverse());
        Ok(this.clone())
    });
    def(it, &ap, "sort", |it, this, args| {
        let a = this_array(it, this)?;
        let sorted = sort_values(it, a, &arg(args, 0))?;
        with_arr(this, |a| *a = sorted);
        Ok(this.clone())
    });
    def(it, &ap, "at", |it, this, args| {
        let a = this_array(it, this)?;
        let n = num_arg(it, args, 0)?;
        let i = if n < 0.0 { a.len() as f64 + n } else { n };
        Ok(if i >= 0.0 { a.get(i as usize).cloned().unwrap_or(Value::Undefined) } else { Value::Undefined })
    });
    def(it, &ap, "fill", |it, this, args| {
        let len = this_array(it, this)?.len();
        let s = rel_opt(it, args, 1, len, 0)?;
        let e = rel_opt(it, args, 2, len, len)?;
        let v = arg(args, 0);
        with_arr(this, |a| {
            for i in s..e.min(a.len()) {
                a[i] = v.clone();
            }
        });
        Ok(this.clone())
    });
    def(it, &ap, "flat", |it, this, args| {
        let a = this_array(it, this)?;
        let d = match arg(args, 0) {
            Value::Undefined => 1,
            v => it.to_num(&v)?.max(0.0) as usize,
        };
        let mut out = Vec::new();
        flatten(it, &a, d, &mut out);
        Ok(it.new_array(out))
    });
    def(it, &ap, "forEach", |it, this, args| {
        let a = this_array(it, this)?;
        let f = arg(args, 0);
        for (i, x) in a.iter().enumerate() {
            it.call(&f, &arg(args, 1), &[x.clone(), Value::Num(i as f64), this.clone()])?;
        }
        Ok(Value::Undefined)
    });
    def(it, &ap, "map", |it, this, args| {
        let a = this_array(it, this)?;
        let f = arg(args, 0);
        let mut out = Vec::with_capacity(a.len());
        for (i, x) in a.iter().enumerate() {
            out.push(it.call(&f, &arg(args, 1), &[x.clone(), Value::Num(i as f64), this.clone()])?);
        }
        Ok(it.new_array(out))
    });
    def(it, &ap, "flatMap", |it, this, args| {
        let a = this_array(it, this)?;
        let f = arg(args, 0);
        let mut out = Vec::new();
        for (i, x) in a.iter().enumerate() {
            let r = it.call(&f, &Value::Undefined, &[x.clone(), Value::Num(i as f64)])?;
            flatten(it, &[r], 1, &mut out);
        }
        Ok(it.new_array(out))
    });
    def(it, &ap, "filter", |it, this, args| {
        let a = this_array(it, this)?;
        let f = arg(args, 0);
        let mut out = Vec::new();
        for (i, x) in a.iter().enumerate() {
            if it.call(&f, &arg(args, 1), &[x.clone(), Value::Num(i as f64), this.clone()])?.truthy() {
                out.push(x.clone());
            }
        }
        Ok(it.new_array(out))
    });
    def(it, &ap, "find", |it, this, args| {
        let a = this_array(it, this)?;
        let f = arg(args, 0);
        for (i, x) in a.iter().enumerate() {
            if it.call(&f, &Value::Undefined, &[x.clone(), Value::Num(i as f64), this.clone()])?.truthy() {
                return Ok(x.clone());
            }
        }
        Ok(Value::Undefined)
    });
    def(it, &ap, "findIndex", |it, this, args| {
        let a = this_array(it, this)?;
        let f = arg(args, 0);
        for (i, x) in a.iter().enumerate() {
            if it.call(&f, &Value::Undefined, &[x.clone(), Value::Num(i as f64), this.clone()])?.truthy() {
                return Ok(Value::Num(i as f64));
            }
        }
        Ok(Value::Num(-1.0))
    });
    def(it, &ap, "some", |it, this, args| {
        let a = this_array(it, this)?;
        let f = arg(args, 0);
        for (i, x) in a.iter().enumerate() {
            if it.call(&f, &Value::Undefined, &[x.clone(), Value::Num(i as f64), this.clone()])?.truthy() {
                return Ok(Value::Bool(true));
            }
        }
        Ok(Value::Bool(false))
    });
    def(it, &ap, "every", |it, this, args| {
        let a = this_array(it, this)?;
        let f = arg(args, 0);
        for (i, x) in a.iter().enumerate() {
            if !it.call(&f, &Value::Undefined, &[x.clone(), Value::Num(i as f64), this.clone()])?.truthy() {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    });
    def(it, &ap, "reduce", |it, this, args| {
        let a = this_array(it, this)?;
        let f = arg(args, 0);
        let mut iter = a.iter().enumerate();
        let mut acc = if args.len() > 1 {
            args[1].clone()
        } else {
            match iter.next() {
                Some((_, x)) => x.clone(),
                None => return Err(it.type_err("Reduce of empty array with no initial value")),
            }
        };
        for (i, x) in iter {
            acc = it.call(&f, &Value::Undefined, &[acc, x.clone(), Value::Num(i as f64), this.clone()])?;
        }
        Ok(acc)
    });
    def(it, &ap, "reduceRight", |it, this, args| {
        let a = this_array(it, this)?;
        let f = arg(args, 0);
        let mut iter = a.iter().enumerate().rev();
        let mut acc = if args.len() > 1 {
            args[1].clone()
        } else {
            match iter.next() {
                Some((_, x)) => x.clone(),
                None => return Err(it.type_err("Reduce of empty array with no initial value")),
            }
        };
        for (i, x) in iter {
            acc = it.call(&f, &Value::Undefined, &[acc, x.clone(), Value::Num(i as f64), this.clone()])?;
        }
        Ok(acc)
    });
    def(it, &ap, "keys", |it, this, _a| {
        let n = this_array(it, this)?.len();
        Ok(it.new_array((0..n).map(|i| Value::Num(i as f64)).collect()))
    });
    def(it, &ap, "entries", |it, this, _a| {
        let a = this_array(it, this)?;
        let mut out = Vec::new();
        for (i, x) in a.into_iter().enumerate() {
            let p = it.new_array(vec![Value::Num(i as f64), x]);
            out.push(p);
        }
        Ok(it.new_array(out))
    });
    def(it, &ap, "values", |it, this, _a| {
        let a = this_array(it, this)?;
        Ok(it.new_array(a))
    });
}

fn setup_string(it: &mut Interp) {
    let sp = it.protos.string.clone();
    let sc = ctor(it, "String", |it, _t, args| {
        if args.is_empty() {
            return Ok(Value::str(""));
        }
        Ok(Value::string(it.to_str(&args[0])?))
    }, &sp);
    def(it, &sc, "fromCharCode", |it, _t, args| {
        let mut s = String::new();
        for i in 0..args.len() {
            let n = num_arg(it, args, i)? as u32;
            s.push(char::from_u32(n).unwrap_or('\u{fffd}'));
        }
        Ok(Value::string(s))
    });
    def(it, &sc, "fromCodePoint", |it, _t, args| {
        let mut s = String::new();
        for i in 0..args.len() {
            let n = num_arg(it, args, i)? as u32;
            s.push(char::from_u32(n).unwrap_or('\u{fffd}'));
        }
        Ok(Value::string(s))
    });
    def(it, &sp, "toString", |it, this, _a| Ok(Value::string(this_str(it, this)?)));
    def(it, &sp, "valueOf", |it, this, _a| Ok(Value::string(this_str(it, this)?)));
    def(it, &sp, "charAt", |it, this, args| {
        let c = chars_of(&this_str(it, this)?);
        let i = num_arg(it, args, 0).unwrap_or(0.0);
        let i = if i.is_nan() { 0.0 } else { i };
        Ok(if i >= 0.0 { c.get(i as usize).map(|x| Value::string(x.to_string())).unwrap_or(Value::str("")) } else { Value::str("") })
    });
    def(it, &sp, "at", |it, this, args| {
        let c = chars_of(&this_str(it, this)?);
        let n = num_arg(it, args, 0)?;
        let i = if n < 0.0 { c.len() as f64 + n } else { n };
        Ok(if i >= 0.0 { c.get(i as usize).map(|x| Value::string(x.to_string())).unwrap_or(Value::Undefined) } else { Value::Undefined })
    });
    def(it, &sp, "charCodeAt", |it, this, args| {
        let c = chars_of(&this_str(it, this)?);
        let i = num_arg(it, args, 0).unwrap_or(0.0);
        let i = if i.is_nan() { 0.0 } else { i };
        Ok(if i >= 0.0 { c.get(i as usize).map(|x| Value::Num(*x as u32 as f64)).unwrap_or(Value::Num(f64::NAN)) } else { Value::Num(f64::NAN) })
    });
    def(it, &sp, "codePointAt", |it, this, args| {
        let c = chars_of(&this_str(it, this)?);
        let i = num_arg(it, args, 0).unwrap_or(0.0);
        Ok(if i >= 0.0 { c.get(i as usize).map(|x| Value::Num(*x as u32 as f64)).unwrap_or(Value::Undefined) } else { Value::Undefined })
    });
    def(it, &sp, "indexOf", |it, this, args| {
        let h = chars_of(&this_str(it, this)?);
        let n = chars_of(&str_arg(it, args, 0)?);
        let from = rel_opt(it, args, 1, h.len(), 0)?;
        Ok(Value::Num(find_chars(&h, &n, from).map(|x| x as f64).unwrap_or(-1.0)))
    });
    def(it, &sp, "lastIndexOf", |it, this, args| {
        let h = chars_of(&this_str(it, this)?);
        let n = chars_of(&str_arg(it, args, 0)?);
        let mut last: Option<usize> = None;
        let mut from = 0;
        while let Some(i) = find_chars(&h, &n, from) {
            last = Some(i);
            from = i + 1;
            if from > h.len() {
                break;
            }
        }
        Ok(Value::Num(last.map(|x| x as f64).unwrap_or(-1.0)))
    });
    def(it, &sp, "includes", |it, this, args| {
        let h = chars_of(&this_str(it, this)?);
        let n = chars_of(&str_arg(it, args, 0)?);
        Ok(Value::Bool(find_chars(&h, &n, 0).is_some()))
    });
    def(it, &sp, "startsWith", |it, this, args| {
        let s = this_str(it, this)?;
        let n = str_arg(it, args, 0)?;
        let from = rel_opt(it, args, 1, s.chars().count(), 0)?;
        let rest: String = s.chars().skip(from).collect();
        Ok(Value::Bool(rest.starts_with(n.as_str())))
    });
    def(it, &sp, "endsWith", |it, this, args| {
        let s = this_str(it, this)?;
        let n = str_arg(it, args, 0)?;
        Ok(Value::Bool(s.ends_with(n.as_str())))
    });
    def(it, &sp, "slice", |it, this, args| {
        let c = chars_of(&this_str(it, this)?);
        let s = rel_opt(it, args, 0, c.len(), 0)?;
        let e = rel_opt(it, args, 1, c.len(), c.len())?;
        Ok(Value::string(if s < e { from_chars(&c[s..e]) } else { String::new() }))
    });
    def(it, &sp, "substring", |it, this, args| {
        let c = chars_of(&this_str(it, this)?);
        let clamp = |n: f64| -> usize {
            if n.is_nan() || n < 0.0 {
                0
            } else {
                (n as usize).min(c.len())
            }
        };
        let a = clamp(num_arg(it, args, 0).unwrap_or(0.0));
        let b = match arg(args, 1) {
            Value::Undefined => c.len(),
            v => clamp(it.to_num(&v)?),
        };
        let (s, e) = if a <= b { (a, b) } else { (b, a) };
        Ok(Value::string(from_chars(&c[s..e])))
    });
    def(it, &sp, "substr", |it, this, args| {
        let c = chars_of(&this_str(it, this)?);
        let s = rel_opt(it, args, 0, c.len(), 0)?;
        let n = match arg(args, 1) {
            Value::Undefined => c.len() - s,
            v => (it.to_num(&v)?.max(0.0) as usize).min(c.len() - s),
        };
        Ok(Value::string(from_chars(&c[s..s + n])))
    });
    def(it, &sp, "toUpperCase", |it, this, _a| Ok(Value::string(this_str(it, this)?.to_uppercase())));
    def(it, &sp, "toLowerCase", |it, this, _a| Ok(Value::string(this_str(it, this)?.to_lowercase())));
    def(it, &sp, "toLocaleUpperCase", |it, this, _a| Ok(Value::string(this_str(it, this)?.to_uppercase())));
    def(it, &sp, "toLocaleLowerCase", |it, this, _a| Ok(Value::string(this_str(it, this)?.to_lowercase())));
    def(it, &sp, "trim", |it, this, _a| Ok(Value::string(String::from(this_str(it, this)?.trim()))));
    def(it, &sp, "trimStart", |it, this, _a| Ok(Value::string(String::from(this_str(it, this)?.trim_start()))));
    def(it, &sp, "trimEnd", |it, this, _a| Ok(Value::string(String::from(this_str(it, this)?.trim_end()))));
    def(it, &sp, "concat", |it, this, args| {
        let mut s = this_str(it, this)?;
        for i in 0..args.len() {
            s.push_str(&str_arg(it, args, i)?);
        }
        Ok(Value::string(s))
    });
    def(it, &sp, "repeat", |it, this, args| {
        let s = this_str(it, this)?;
        let n = num_arg(it, args, 0)?;
        if n < 0.0 || n > 1_000_000.0 {
            return Err(it.throw_err("RangeError", "Invalid count value"));
        }
        Ok(Value::string(s.repeat(n as usize)))
    });
    def(it, &sp, "padStart", |it, this, args| {
        let s = chars_of(&this_str(it, this)?);
        let n = num_arg(it, args, 0)?.max(0.0) as usize;
        let pad = match arg(args, 1) {
            Value::Undefined => vec![' '],
            v => chars_of(&it.to_str(&v)?),
        };
        if pad.is_empty() || n <= s.len() {
            return Ok(Value::string(from_chars(&s)));
        }
        let mut out: Vec<char> = Vec::new();
        for i in 0..(n - s.len()) {
            out.push(pad[i % pad.len()]);
        }
        out.extend(s);
        Ok(Value::string(from_chars(&out)))
    });
    def(it, &sp, "padEnd", |it, this, args| {
        let s = chars_of(&this_str(it, this)?);
        let n = num_arg(it, args, 0)?.max(0.0) as usize;
        let pad = match arg(args, 1) {
            Value::Undefined => vec![' '],
            v => chars_of(&it.to_str(&v)?),
        };
        if pad.is_empty() || n <= s.len() {
            return Ok(Value::string(from_chars(&s)));
        }
        let mut out = s.clone();
        for i in 0..(n - s.len()) {
            out.push(pad[i % pad.len()]);
        }
        Ok(Value::string(from_chars(&out)))
    });
    def(it, &sp, "split", |it, this, args| {
        let s = this_str(it, this)?;
        let sep = arg(args, 0);
        let limit = match arg(args, 1) {
            Value::Undefined => usize::MAX,
            v => it.to_num(&v)?.max(0.0) as usize,
        };
        let mut parts: Vec<Value> = Vec::new();
        if matches!(sep, Value::Undefined) {
            parts.push(Value::string(s));
        } else if let Some(re) = get_regex(&sep) {
            let chars = chars_of(&s);
            if chars.is_empty() {
                if re.exec_at(&chars, 0).is_none() {
                    parts.push(Value::str(""));
                }
            } else {
                let mut last = 0usize;
                let mut pos = 0usize;
                while pos < chars.len() {
                    match re.exec_at(&chars, pos) {
                        Some(c) => {
                            let (a, b) = c[0].unwrap_or((pos, pos));
                            if a >= chars.len() {
                                break;
                            }
                            if b == a && a == last {
                                pos = a + 1;
                                continue;
                            }
                            parts.push(Value::string(from_chars(&chars[last..a])));
                            for g in c.iter().skip(1) {
                                parts.push(cap_value(&chars, *g));
                            }
                            last = b;
                            pos = if b == a { b + 1 } else { b };
                        }
                        None => break,
                    }
                }
                parts.push(Value::string(from_chars(&chars[last..])));
            }
        } else {
            let sp = it.to_str(&sep)?;
            if sp.is_empty() {
                for c in s.chars() {
                    parts.push(Value::string(c.to_string()));
                }
            } else {
                for p in s.split(sp.as_str()) {
                    parts.push(Value::str(p));
                }
            }
        }
        parts.truncate(limit);
        Ok(it.new_array(parts))
    });
    def(it, &sp, "replace", |it, this, args| str_replace(it, this, args, false));
    def(it, &sp, "replaceAll", |it, this, args| str_replace(it, this, args, true));
    def(it, &sp, "localeCompare", |it, this, args| {
        let a = this_str(it, this)?;
        let b = str_arg(it, args, 0)?;
        Ok(Value::Num(if a < b { -1.0 } else if a > b { 1.0 } else { 0.0 }))
    });
    def(it, &sp, "normalize", |it, this, _a| Ok(Value::string(this_str(it, this)?)));
    def(it, &sp, "match", |it, this, args| {
        let s = this_str(it, this)?;
        let rv = to_regex(it, &arg(args, 0), "")?;
        let re = match get_regex(&rv) {
            Some(r) => r,
            None => return Ok(Value::Null),
        };
        if !re.global {
            return regex_exec(it, &rv, &s);
        }
        let chars = chars_of(&s);
        let found = all_matches(&re, &chars, true);
        if let Value::Obj(o) = &rv {
            it.set_own(o, "lastIndex", Value::Num(0.0));
        }
        if found.is_empty() {
            return Ok(Value::Null);
        }
        let out: Vec<Value> = found.iter().map(|c| cap_value(&chars, c[0])).collect();
        Ok(it.new_array(out))
    });
    def(it, &sp, "matchAll", |it, this, args| {
        let s = this_str(it, this)?;
        let rv = to_regex(it, &arg(args, 0), "g")?;
        let re = match get_regex(&rv) {
            Some(r) => r,
            None => return Ok(it.new_array(Vec::new())),
        };
        let chars = chars_of(&s);
        let found = all_matches(&re, &chars, true);
        let mut out = Vec::new();
        for c in found.iter() {
            out.push(caps_to_array(it, &re, &chars, c));
        }
        Ok(it.new_array(out))
    });
    def(it, &sp, "search", |it, this, args| {
        let s = this_str(it, this)?;
        let rv = to_regex(it, &arg(args, 0), "")?;
        let chars = chars_of(&s);
        Ok(Value::Num(match get_regex(&rv).and_then(|r| r.exec_at(&chars, 0)) {
            Some(c) => c[0].map(|x| x.0 as f64).unwrap_or(-1.0),
            None => -1.0,
        }))
    });

    let rp = it.protos.regexp.clone();
    ctor(it, "RegExp", |it, _this, args| {
        let p = arg(args, 0);
        let (src, flags0) = match get_regex(&p) {
            Some(r) => (r.source.clone(), r.flags.clone()),
            None => (
                match &p {
                    Value::Undefined => String::new(),
                    v => it.to_str(v)?,
                },
                String::new(),
            ),
        };
        let flags = match arg(args, 1) {
            Value::Undefined => flags0,
            v => it.to_str(&v)?,
        };
        it.regex_obj(&src, &flags)
    }, &rp);
    def(it, &rp, "exec", |it, this, args| {
        let s = str_arg(it, args, 0)?;
        regex_exec(it, this, &s)
    });
    def(it, &rp, "test", |it, this, args| {
        let s = str_arg(it, args, 0)?;
        let r = regex_exec(it, this, &s)?;
        Ok(Value::Bool(!matches!(r, Value::Null)))
    });
    def(it, &rp, "toString", |it, this, _a| {
        let s = it.get_prop(this, "source")?;
        let f = it.get_prop(this, "flags")?;
        Ok(Value::string(format!("/{}/{}", it.to_str(&s)?, it.to_str(&f)?)))
    });
}

fn str_replace(it: &mut Interp, this: &Value, args: &[Value], all: bool) -> Res<Value> {
    let s = this_str(it, this)?;
    let chars = chars_of(&s);
    let pat = arg(args, 0);
    let rep = arg(args, 1);
    let rep_str = if rep.is_callable() { String::new() } else { it.to_str(&rep)? };
    let rx = get_regex(&pat);
    let mut matches: Vec<crate::regex::Caps> = Vec::new();
    match &rx {
        Some(re) => matches = all_matches(re, &chars, re.global || all),
        None => {
            let nd = chars_of(&it.to_str(&pat)?);
            let mut from = 0usize;
            while let Some(i) = find_chars(&chars, &nd, from) {
                matches.push(vec![Some((i, i + nd.len()))]);
                if !all {
                    break;
                }
                from = if nd.is_empty() { i + 1 } else { i + nd.len() };
                if from > chars.len() {
                    break;
                }
            }
        }
    }
    if let (Some(_), Value::Obj(o)) = (&rx, &pat) {
        it.set_own(o, "lastIndex", Value::Num(0.0));
    }
    let mut out = String::new();
    let mut last = 0usize;
    for caps in matches.iter() {
        let (a, b) = caps[0].unwrap_or((0, 0));
        if a < last {
            continue;
        }
        out.push_str(&from_chars(&chars[last..a]));
        if rep.is_callable() {
            let mut cargs: Vec<Value> = caps.iter().map(|c| cap_value(&chars, *c)).collect();
            if cargs.is_empty() {
                cargs.push(Value::str(""));
            }
            cargs.push(Value::Num(a as f64));
            cargs.push(Value::string(s.clone()));
            let r = it.call(&rep, &Value::Undefined, &cargs)?;
            out.push_str(&it.to_str(&r)?);
        } else {
            out.push_str(&expand_template(&rep_str, &chars, caps, rx.as_deref()));
        }
        last = b;
    }
    out.push_str(&from_chars(&chars[last.min(chars.len())..]));
    Ok(Value::string(out))
}

fn setup_number_math(it: &mut Interp) {
    let np = it.protos.number.clone();
    let nc = ctor(it, "Number", |it, _t, args| {
        if args.is_empty() {
            return Ok(Value::Num(0.0));
        }
        Ok(Value::Num(it.to_num(&args[0])?))
    }, &np);
    it.set_own(&nc, "MAX_SAFE_INTEGER", Value::Num(9007199254740991.0));
    it.set_own(&nc, "MIN_SAFE_INTEGER", Value::Num(-9007199254740991.0));
    it.set_own(&nc, "EPSILON", Value::Num(f64::EPSILON));
    it.set_own(&nc, "MAX_VALUE", Value::Num(f64::MAX));
    it.set_own(&nc, "POSITIVE_INFINITY", Value::Num(f64::INFINITY));
    it.set_own(&nc, "NEGATIVE_INFINITY", Value::Num(f64::NEG_INFINITY));
    it.set_own(&nc, "NaN", Value::Num(f64::NAN));
    def(it, &nc, "isInteger", |_it, _t, args| Ok(Value::Bool(matches!(arg(args, 0), Value::Num(n) if n.is_finite() && n == libm::floor(n)))));
    def(it, &nc, "isFinite", |_it, _t, args| Ok(Value::Bool(matches!(arg(args, 0), Value::Num(n) if n.is_finite()))));
    def(it, &nc, "isNaN", |_it, _t, args| Ok(Value::Bool(matches!(arg(args, 0), Value::Num(n) if n.is_nan()))));
    def(it, &nc, "isSafeInteger", |_it, _t, args| Ok(Value::Bool(matches!(arg(args, 0), Value::Num(n) if n.is_finite() && n == libm::floor(n) && libm::fabs(n) <= 9007199254740991.0))));
    def(it, &nc, "parseFloat", |it, _t, args| Ok(Value::Num(parse_float(&str_arg(it, args, 0)?))));
    def(it, &nc, "parseInt", |it, _t, args| {
        let r = num_arg(it, args, 1).unwrap_or(0.0);
        Ok(Value::Num(parse_int(&str_arg(it, args, 0)?, if r.is_nan() { 0 } else { r as u32 })))
    });
    def(it, &np, "toString", |it, this, args| {
        let n = it.to_num(this)?;
        let radix = match arg(args, 0) {
            Value::Undefined => 10,
            v => it.to_num(&v)? as u32,
        };
        if radix == 10 || !n.is_finite() || !(2..=36).contains(&radix) {
            return Ok(Value::string(num_to_str(n)));
        }
        let neg = n < 0.0;
        let mut int = libm::floor(libm::fabs(n));
        let mut frac = libm::fabs(n) - int;
        let mut digits: Vec<char> = Vec::new();
        if int == 0.0 {
            digits.push('0');
        }
        while int > 0.0 {
            let d = libm::fmod(int, radix as f64) as u32;
            digits.push(core::char::from_digit(d, radix).unwrap_or('0'));
            int = libm::floor(int / radix as f64);
        }
        digits.reverse();
        let mut s: String = digits.into_iter().collect();
        if frac > 0.0 {
            s.push('.');
            for _ in 0..20 {
                frac *= radix as f64;
                let d = libm::floor(frac) as u32;
                s.push(core::char::from_digit(d, radix).unwrap_or('0'));
                frac -= libm::floor(frac);
                if frac <= 0.0 {
                    break;
                }
            }
        }
        if neg {
            s.insert(0, '-');
        }
        Ok(Value::string(s))
    });
    def(it, &np, "toFixed", |it, this, args| {
        let n = it.to_num(this)?;
        let d = num_arg(it, args, 0).unwrap_or(0.0);
        let d = if d.is_nan() { 0 } else { (d as usize).min(100) };
        if !n.is_finite() {
            return Ok(Value::string(num_to_str(n)));
        }
        let mut s = format!("{:.*}", d, n);
        if s.starts_with("-0") && s.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') {
            s.remove(0);
        }
        Ok(Value::string(s))
    });
    def(it, &np, "valueOf", |it, this, _a| Ok(Value::Num(it.to_num(this)?)));
    def(it, &np, "toLocaleString", |it, this, _a| Ok(Value::string(num_to_str(it.to_num(this)?))));

    let bp = it.protos.boolean.clone();
    ctor(it, "Boolean", |_it, _t, args| Ok(Value::Bool(arg(args, 0).truthy())), &bp);
    def(it, &bp, "toString", |it, this, _a| Ok(Value::string(it.to_str(this)?)));
    def(it, &bp, "valueOf", |_it, this, _a| Ok(this.clone()));

    setg_fn(it, "parseInt", |it, _t, args| {
        let r = num_arg(it, args, 1).unwrap_or(0.0);
        Ok(Value::Num(parse_int(&str_arg(it, args, 0)?, if r.is_nan() { 0 } else { r as u32 })))
    });
    setg_fn(it, "parseFloat", |it, _t, args| Ok(Value::Num(parse_float(&str_arg(it, args, 0)?))));
    setg_fn(it, "isNaN", |it, _t, args| Ok(Value::Bool(num_arg(it, args, 0)?.is_nan())));
    setg_fn(it, "isFinite", |it, _t, args| Ok(Value::Bool(num_arg(it, args, 0)?.is_finite())));

    let m = it.new_plain();
    setg(it, "Math", Value::Obj(m.clone()));
    it.set_own(&m, "PI", Value::Num(core::f64::consts::PI));
    it.set_own(&m, "E", Value::Num(core::f64::consts::E));
    it.set_own(&m, "LN2", Value::Num(core::f64::consts::LN_2));
    it.set_own(&m, "LN10", Value::Num(core::f64::consts::LN_10));
    it.set_own(&m, "SQRT2", Value::Num(core::f64::consts::SQRT_2));
    macro_rules! m1 {
        ($name:expr, $f:expr) => {
            def(it, &m, $name, |it, _t, args| {
                let x = num_arg(it, args, 0)?;
                let f: fn(f64) -> f64 = $f;
                Ok(Value::Num(f(x)))
            });
        };
    }
    m1!("abs", libm::fabs);
    m1!("floor", libm::floor);
    m1!("ceil", libm::ceil);
    m1!("trunc", libm::trunc);
    m1!("sqrt", libm::sqrt);
    m1!("cbrt", libm::cbrt);
    m1!("exp", libm::exp);
    m1!("log", libm::log);
    m1!("log2", libm::log2);
    m1!("log10", libm::log10);
    m1!("sin", libm::sin);
    m1!("cos", libm::cos);
    m1!("tan", libm::tan);
    m1!("asin", libm::asin);
    m1!("acos", libm::acos);
    m1!("atan", libm::atan);
    m1!("round", |x| libm::floor(x + 0.5));
    m1!("sign", |x| if x.is_nan() || x == 0.0 { x } else if x > 0.0 { 1.0 } else { -1.0 });
    def(it, &m, "pow", |it, _t, args| Ok(Value::Num(libm::pow(num_arg(it, args, 0)?, num_arg(it, args, 1)?))));
    def(it, &m, "atan2", |it, _t, args| Ok(Value::Num(libm::atan2(num_arg(it, args, 0)?, num_arg(it, args, 1)?))));
    def(it, &m, "hypot", |it, _t, args| {
        let mut s = 0.0;
        for i in 0..args.len() {
            let x = num_arg(it, args, i)?;
            s += x * x;
        }
        Ok(Value::Num(libm::sqrt(s)))
    });
    def(it, &m, "max", |it, _t, args| {
        let mut r = f64::NEG_INFINITY;
        for i in 0..args.len() {
            let x = num_arg(it, args, i)?;
            if x.is_nan() {
                return Ok(Value::Num(f64::NAN));
            }
            if x > r {
                r = x;
            }
        }
        Ok(Value::Num(r))
    });
    def(it, &m, "min", |it, _t, args| {
        let mut r = f64::INFINITY;
        for i in 0..args.len() {
            let x = num_arg(it, args, i)?;
            if x.is_nan() {
                return Ok(Value::Num(f64::NAN));
            }
            if x < r {
                r = x;
            }
        }
        Ok(Value::Num(r))
    });
    def(it, &m, "random", |it, _t, _a| {
        let mut x = it.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        it.rng = x;
        Ok(Value::Num((x >> 11) as f64 / 9007199254740992.0))
    });
}

fn setg_fn(it: &mut Interp, name: &str, f: NativeFn) {
    let v = it.new_native(f);
    setg(it, name, v);
}

fn setup_json(it: &mut Interp) {
    let j = it.new_plain();
    setg(it, "JSON", Value::Obj(j.clone()));
    def(it, &j, "stringify", |it, _t, args| {
        let ind = match arg(args, 2) {
            Value::Num(n) => " ".repeat((n.max(0.0) as usize).min(10)),
            Value::Str(s) => String::from(&*s),
            _ => String::new(),
        };
        let mut out = String::new();
        if json_ser(it, &arg(args, 0), &ind, "", &mut out, 0)? {
            Ok(Value::string(out))
        } else {
            Ok(Value::Undefined)
        }
    });
    def(it, &j, "parse", |it, _t, args| {
        let s = str_arg(it, args, 0)?;
        let mut p = JP { c: s.chars().collect(), i: 0 };
        let r = p.value(it, 0);
        match r {
            Ok(v) => {
                p.ws();
                if p.i < p.c.len() {
                    return Err(it.throw_err("SyntaxError", "Unexpected trailing characters in JSON"));
                }
                Ok(v)
            }
            Err(m) => Err(it.throw_err("SyntaxError", &m)),
        }
    });
}

fn map_find(m: &[(Value, Value)], k: &Value) -> Option<usize> {
    m.iter().position(|(x, _)| same_value_zero(x, k))
}

fn setup_collections(it: &mut Interp) {
    let mp = it.protos.map.clone();
    ctor(it, "Map", |it, this, args| {
        let o = match this {
            Value::Obj(o) => o.clone(),
            _ => return Err(it.type_err("Constructor Map requires 'new'")),
        };
        let mut entries: Vec<(Value, Value)> = Vec::new();
        if !arg(args, 0).is_nullish() {
            for pair in it.iterate(&arg(args, 0))? {
                let k = it.get_prop(&pair, "0")?;
                let v = it.get_prop(&pair, "1")?;
                match map_find(&entries, &k) {
                    Some(i) => entries[i].1 = v,
                    None => entries.push((k, v)),
                }
            }
        }
        o.borrow_mut().kind = Kind::Map(entries);
        Ok(Value::Obj(o))
    }, &mp);
    def(it, &mp, "get", |_it, this, args| {
        if let Value::Obj(o) = this {
            if let Kind::Map(m) = &o.borrow().kind {
                return Ok(map_find(m, &arg(args, 0)).map(|i| m[i].1.clone()).unwrap_or(Value::Undefined));
            }
        }
        Ok(Value::Undefined)
    });
    def(it, &mp, "set", |_it, this, args| {
        if let Value::Obj(o) = this {
            if let Kind::Map(m) = &mut o.borrow_mut().kind {
                let k = arg(args, 0);
                match map_find(m, &k) {
                    Some(i) => m[i].1 = arg(args, 1),
                    None => m.push((k, arg(args, 1))),
                }
            }
        }
        Ok(this.clone())
    });
    def(it, &mp, "has", |_it, this, args| {
        if let Value::Obj(o) = this {
            if let Kind::Map(m) = &o.borrow().kind {
                return Ok(Value::Bool(map_find(m, &arg(args, 0)).is_some()));
            }
        }
        Ok(Value::Bool(false))
    });
    def(it, &mp, "delete", |_it, this, args| {
        if let Value::Obj(o) = this {
            if let Kind::Map(m) = &mut o.borrow_mut().kind {
                if let Some(i) = map_find(m, &arg(args, 0)) {
                    m.remove(i);
                    return Ok(Value::Bool(true));
                }
            }
        }
        Ok(Value::Bool(false))
    });
    def(it, &mp, "clear", |_it, this, _a| {
        if let Value::Obj(o) = this {
            if let Kind::Map(m) = &mut o.borrow_mut().kind {
                m.clear();
            }
        }
        Ok(Value::Undefined)
    });
    def(it, &mp, "forEach", |it, this, args| {
        let entries = if let Value::Obj(o) = this {
            if let Kind::Map(m) = &o.borrow().kind { m.clone() } else { Vec::new() }
        } else {
            Vec::new()
        };
        for (k, v) in entries {
            it.call(&arg(args, 0), &Value::Undefined, &[v, k, this.clone()])?;
        }
        Ok(Value::Undefined)
    });
    def(it, &mp, "keys", |it, this, _a| {
        let items = it.iterate(this)?;
        let mut out = Vec::new();
        for p in items {
            out.push(it.get_prop(&p, "0")?);
        }
        Ok(it.new_array(out))
    });
    def(it, &mp, "values", |it, this, _a| {
        let items = it.iterate(this)?;
        let mut out = Vec::new();
        for p in items {
            out.push(it.get_prop(&p, "1")?);
        }
        Ok(it.new_array(out))
    });
    def(it, &mp, "entries", |it, this, _a| {
        let items = it.iterate(this)?;
        Ok(it.new_array(items))
    });

    let sp = it.protos.set.clone();
    ctor(it, "Set", |it, this, args| {
        let o = match this {
            Value::Obj(o) => o.clone(),
            _ => return Err(it.type_err("Constructor Set requires 'new'")),
        };
        let mut items: Vec<Value> = Vec::new();
        if !arg(args, 0).is_nullish() {
            for v in it.iterate(&arg(args, 0))? {
                if !items.iter().any(|x| same_value_zero(x, &v)) {
                    items.push(v);
                }
            }
        }
        o.borrow_mut().kind = Kind::Set(items);
        Ok(Value::Obj(o))
    }, &sp);
    def(it, &sp, "add", |_it, this, args| {
        if let Value::Obj(o) = this {
            if let Kind::Set(s) = &mut o.borrow_mut().kind {
                let v = arg(args, 0);
                if !s.iter().any(|x| same_value_zero(x, &v)) {
                    s.push(v);
                }
            }
        }
        Ok(this.clone())
    });
    def(it, &sp, "has", |_it, this, args| {
        if let Value::Obj(o) = this {
            if let Kind::Set(s) = &o.borrow().kind {
                let v = arg(args, 0);
                return Ok(Value::Bool(s.iter().any(|x| same_value_zero(x, &v))));
            }
        }
        Ok(Value::Bool(false))
    });
    def(it, &sp, "delete", |_it, this, args| {
        if let Value::Obj(o) = this {
            if let Kind::Set(s) = &mut o.borrow_mut().kind {
                let v = arg(args, 0);
                if let Some(i) = s.iter().position(|x| same_value_zero(x, &v)) {
                    s.remove(i);
                    return Ok(Value::Bool(true));
                }
            }
        }
        Ok(Value::Bool(false))
    });
    def(it, &sp, "clear", |_it, this, _a| {
        if let Value::Obj(o) = this {
            if let Kind::Set(s) = &mut o.borrow_mut().kind {
                s.clear();
            }
        }
        Ok(Value::Undefined)
    });
    def(it, &sp, "forEach", |it, this, args| {
        let items = it.iterate(this)?;
        for v in items {
            it.call(&arg(args, 0), &Value::Undefined, &[v.clone(), v, this.clone()])?;
        }
        Ok(Value::Undefined)
    });
    def(it, &sp, "values", |it, this, _a| {
        let items = it.iterate(this)?;
        Ok(it.new_array(items))
    });
    def(it, &sp, "keys", |it, this, _a| {
        let items = it.iterate(this)?;
        Ok(it.new_array(items))
    });
}

fn setup_promise_timers(it: &mut Interp) {
    let pp = it.protos.promise.clone();
    let pc = ctor(it, "Promise", |it, this, args| {
        let o = match this {
            Value::Obj(o) => o.clone(),
            _ => return Err(it.type_err("Promise constructor cannot be invoked without 'new'")),
        };
        let exec = arg(args, 0);
        if !exec.is_callable() {
            return Err(it.type_err("Promise resolver is not a function"));
        }
        o.borrow_mut().kind = Kind::Promise(PromData { state: 0, value: Value::Undefined, reactions: Vec::new() });
        let res = it.new_native_data(pr_resolve_fn, vec![Value::Obj(o.clone())]);
        let rej = it.new_native_data(pr_reject_fn, vec![Value::Obj(o.clone())]);
        if let Err(Ctl::Throw(e)) = it.call(&exec, &Value::Undefined, &[res, rej]) {
            it.settle(&o, 2, e);
        }
        Ok(Value::Obj(o))
    }, &pp);
    def(it, &pp, "then", |it, this, args| {
        match this {
            Value::Obj(p) if it.promise_state(p).is_some() => Ok(promise_then(it, p, arg(args, 0), arg(args, 1))),
            _ => Err(it.type_err("not a promise")),
        }
    });
    def(it, &pp, "catch", |it, this, args| {
        match this {
            Value::Obj(p) if it.promise_state(p).is_some() => Ok(promise_then(it, p, Value::Undefined, arg(args, 0))),
            _ => Err(it.type_err("not a promise")),
        }
    });
    def(it, &pp, "finally", |it, this, args| {
        match this {
            Value::Obj(p) if it.promise_state(p).is_some() => {
                let f = arg(args, 0);
                let ok = it.new_native_data(pr_finally_ok, vec![f.clone()]);
                let err = it.new_native_data(pr_finally_err, vec![f]);
                Ok(promise_then(it, p, ok, err))
            }
            _ => Err(it.type_err("not a promise")),
        }
    });
    def(it, &pc, "resolve", |it, _t, args| Ok(Value::Obj(promise_of(it, &arg(args, 0)))));
    def(it, &pc, "reject", |it, _t, args| {
        let p = it.new_promise();
        it.settle(&p, 2, arg(args, 0));
        Ok(Value::Obj(p))
    });
    def(it, &pc, "all", |it, _t, args| {
        let items = it.iterate(&arg(args, 0))?;
        let outer = it.new_promise();
        let total = items.len();
        if total == 0 {
            let arr = it.new_array(Vec::new());
            it.resolve_with(&outer, arr);
            return Ok(Value::Obj(outer));
        }
        let result = it.new_array(vec![Value::Undefined; total]);
        let counter = it.new_plain();
        for (i, item) in items.iter().enumerate() {
            let p = promise_of(it, item);
            let ok = it.new_native_data(
                pr_all_item,
                vec![result.clone(), Value::Num(i as f64), Value::Obj(counter.clone()), Value::Obj(outer.clone()), Value::Num(total as f64)],
            );
            let fail = it.new_native_data(pr_all_fail, vec![Value::Obj(outer.clone())]);
            promise_then(it, &p, ok, fail);
        }
        Ok(Value::Obj(outer))
    });
    def(it, &pc, "race", |it, _t, args| {
        let items = it.iterate(&arg(args, 0))?;
        let outer = it.new_promise();
        for item in items.iter() {
            let p = promise_of(it, item);
            let ok = it.new_native_data(pr_race_ok, vec![Value::Obj(outer.clone())]);
            let fail = it.new_native_data(pr_all_fail, vec![Value::Obj(outer.clone())]);
            promise_then(it, &p, ok, fail);
        }
        Ok(Value::Obj(outer))
    });

    setg_fn(it, "setTimeout", |it, _t, args| timer_args(it, args, false));
    setg_fn(it, "setInterval", |it, _t, args| timer_args(it, args, true));
    setg_fn(it, "setImmediate", |it, _t, args| {
        let mut a = args.to_vec();
        if a.len() < 2 {
            a.push(Value::Num(0.0));
        } else {
            a[1] = Value::Num(0.0);
        }
        timer_args(it, &a, false)
    });
    for name in ["clearTimeout", "clearInterval", "cancelAnimationFrame"] {
        setg_fn(it, name, |it, _t, args| {
            let n = num_arg(it, args, 0)?;
            if n.is_finite() && n >= 0.0 {
                it.clear_timer(n as u32);
            }
            Ok(Value::Undefined)
        });
    }
    setg_fn(it, "requestAnimationFrame", |it, _t, args| {
        let a = vec![arg(args, 0), Value::Num(16.0)];
        timer_args(it, &a, false)
    });
    setg_fn(it, "queueMicrotask", |it, _t, args| {
        let f = arg(args, 0);
        it.microtasks.push_back(Job::Call(f, Vec::new()));
        Ok(Value::Undefined)
    });
}

fn log_fn(it: &mut Interp, _t: &Value, args: &[Value]) -> Res<Value> {
    let mut parts = Vec::new();
    for a in args {
        parts.push(it.inspect(a, 0));
    }
    it.logs.push(parts.join(" "));
    if it.logs.len() > 500 {
        it.logs.remove(0);
    }
    Ok(Value::Undefined)
}

fn setup_misc(it: &mut Interp) {
    let c = it.new_plain();
    setg(it, "console", Value::Obj(c.clone()));
    for name in ["log", "info", "warn", "error", "debug", "trace"] {
        def(it, &c, name, log_fn);
    }

    let sym = it.new_native(|it, _t, args| {
        let d = match arg(args, 0) {
            Value::Undefined => String::new(),
            v => it.to_str(&v)?,
        };
        it.next_timer_id += 1;
        Ok(Value::string(format!("@@sym{}:{}", it.next_timer_id, d)))
    });
    if let Value::Obj(so) = &sym {
        it.set_own(so, "iterator", Value::str("@@iterator"));
        it.set_own(so, "asyncIterator", Value::str("@@asyncIterator"));
        it.set_own(so, "toPrimitive", Value::str("@@toPrimitive"));
        it.set_own(so, "toStringTag", Value::str("@@toStringTag"));
        it.set_own(so, "hasInstance", Value::str("@@hasInstance"));
        def(it, so, "for", |it, _t, args| Ok(Value::string(format!("@@for:{}", str_arg(it, args, 0)?))));
    }
    setg(it, "Symbol", sym);

    let dp = it.protos.date.clone();
    let dc = ctor(it, "Date", |it, this, args| {
        let now = it.now_ms;
        let t = match args.len() {
            0 => now,
            _ => match &args[0] {
                Value::Num(n) => *n,
                Value::Obj(_) => date_val(&args[0]),
                _ => f64::NAN,
            },
        };
        match this {
            Value::Obj(o) => {
                o.borrow_mut().kind = Kind::Date(t);
                Ok(this.clone())
            }
            _ => Ok(Value::string(format!("{}", t))),
        }
    }, &dp);
    def(it, &dc, "now", |it, _t, _a| Ok(Value::Num(it.now_ms)));
    def(it, &dp, "getTime", |_it, this, _a| Ok(Value::Num(date_val(this))));
    def(it, &dp, "valueOf", |_it, this, _a| Ok(Value::Num(date_val(this))));
    def(it, &dp, "getTimezoneOffset", |_it, _t, _a| Ok(Value::Num(0.0)));
    macro_rules! dget {
        ($name:expr, $idx:tt) => {
            def(it, &dp, $name, |_it, this, _a| {
                let t = date_val(this);
                if t.is_nan() {
                    return Ok(Value::Num(f64::NAN));
                }
                let c = civil(t);
                Ok(Value::Num(c.$idx as f64))
            });
        };
    }
    dget!("getFullYear", 0);
    dget!("getUTCFullYear", 0);
    dget!("getDate", 2);
    dget!("getUTCDate", 2);
    dget!("getHours", 3);
    dget!("getUTCHours", 3);
    dget!("getMinutes", 4);
    dget!("getUTCMinutes", 4);
    dget!("getSeconds", 5);
    dget!("getUTCSeconds", 5);
    dget!("getMilliseconds", 6);
    dget!("getDay", 7);
    dget!("getUTCDay", 7);
    def(it, &dp, "getMonth", |_it, this, _a| {
        let t = date_val(this);
        Ok(Value::Num(if t.is_nan() { f64::NAN } else { (civil(t).1 - 1) as f64 }))
    });
    def(it, &dp, "toISOString", |it, this, _a| {
        let t = date_val(this);
        if t.is_nan() {
            return Err(it.throw_err("RangeError", "Invalid time value"));
        }
        let c = civil(t);
        Ok(Value::string(format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", c.0, c.1, c.2, c.3, c.4, c.5, c.6)))
    });
    def(it, &dp, "toJSON", |it, this, _a| {
        let t = date_val(this);
        if t.is_nan() {
            return Ok(Value::Null);
        }
        let c = civil(t);
        let _ = it;
        Ok(Value::string(format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", c.0, c.1, c.2, c.3, c.4, c.5, c.6)))
    });

    setg_fn(it, "encodeURIComponent", |it, _t, args| Ok(Value::string(uri_encode(&str_arg(it, args, 0)?, "-_.!~*'()"))));
    setg_fn(it, "encodeURI", |it, _t, args| Ok(Value::string(uri_encode(&str_arg(it, args, 0)?, "-_.!~*'();/?:@&=+$,#"))));
    setg_fn(it, "decodeURIComponent", |it, _t, args| Ok(Value::string(uri_decode(&str_arg(it, args, 0)?))));
    setg_fn(it, "decodeURI", |it, _t, args| Ok(Value::string(uri_decode(&str_arg(it, args, 0)?))));
}

fn setup_proxy(it: &mut Interp) {
    let pp = it.new_plain();
    ctor(it, "Proxy", |it, this, args| {
        let target = arg(args, 0);
        let handler = arg(args, 1);
        let o = match this {
            Value::Obj(o) => o.clone(),
            _ => return Err(it.type_err("Constructor Proxy requires 'new'")),
        };
        if !matches!(target, Value::Obj(_)) || !matches!(handler, Value::Obj(_)) {
            return Err(it.type_err("Cannot create proxy with a non-object as target or handler"));
        }
        let proto = if let Value::Obj(t) = &target { t.borrow().proto.clone() } else { None };
        {
            let mut b = o.borrow_mut();
            b.kind = Kind::Proxy(target, handler);
            b.proto = proto;
        }
        Ok(Value::Obj(o))
    }, &pp);
}
