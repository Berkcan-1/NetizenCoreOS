use crate::parser::*;
use crate::regex::Regex;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

pub const MAX_DEPTH: usize = 90;

pub type ObjRef = Rc<RefCell<Obj>>;
pub type ScopeRef = Rc<RefCell<Scope>>;
pub type NativeFn = fn(&mut Interp, &Value, &[Value]) -> Res<Value>;
pub type NativeDataFn = fn(&mut Interp, &[Value], &Value, &[Value]) -> Res<Value>;
pub type Res<T> = Result<T, Ctl>;

#[derive(Clone)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    Obj(ObjRef),
}

pub enum Ctl {
    Throw(Value),
    Break(Option<String>),
    Continue(Option<String>),
    Return(Value),
}

pub struct Reaction {
    pub on_ful: Value,
    pub on_rej: Value,
    pub child: Option<ObjRef>,
}

pub struct PromData {
    pub state: u8,
    pub value: Value,
    pub reactions: Vec<Reaction>,
}

pub struct Closure {
    pub def: Rc<FuncDef>,
    pub scope: ScopeRef,
    pub home: Option<ObjRef>,
    pub class: Option<(Rc<ClassDef>, ScopeRef)>,
}

pub enum Kind {
    Plain,
    Array(Vec<Value>),
    Closure(Closure),
    Native(NativeFn),
    NativeData(NativeDataFn, Vec<Value>),
    Bound(Value, Value, Vec<Value>),
    Regex(Rc<Regex>),
    Date(f64),
    Promise(PromData),
    Map(Vec<(Value, Value)>),
    Set(Vec<Value>),
    Proxy(Value, Value),
}

pub struct Obj {
    pub kind: Kind,
    pub props: Vec<(String, Value)>,
    pub accessors: Vec<(String, Option<Value>, Option<Value>)>,
    pub proto: Option<ObjRef>,
}

pub struct Scope {
    pub vars: Vec<(String, Value)>,
    pub parent: Option<ScopeRef>,
    pub this_val: Option<Value>,
}

pub struct Protos {
    pub object: ObjRef,
    pub function: ObjRef,
    pub array: ObjRef,
    pub string: ObjRef,
    pub number: ObjRef,
    pub boolean: ObjRef,
    pub error: ObjRef,
    pub regexp: ObjRef,
    pub promise: ObjRef,
    pub date: ObjRef,
    pub map: ObjRef,
    pub set: ObjRef,
}

pub enum Job {
    React(Reaction, u8, Value),
    Call(Value, Vec<Value>),
}

pub struct Timer {
    pub id: u32,
    pub due: f64,
    pub interval: Option<f64>,
    pub f: Value,
    pub args: Vec<Value>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BindMode {
    Declare,
    Var,
}

enum LoopAct {
    Next,
    Stop,
    Pass(Ctl),
}

fn loop_act(c: Ctl, label: Option<&str>) -> LoopAct {
    match c {
        Ctl::Break(None) => LoopAct::Stop,
        Ctl::Break(Some(l)) => {
            if Some(l.as_str()) == label {
                LoopAct::Stop
            } else {
                LoopAct::Pass(Ctl::Break(Some(l)))
            }
        }
        Ctl::Continue(None) => LoopAct::Next,
        Ctl::Continue(Some(l)) => {
            if Some(l.as_str()) == label {
                LoopAct::Next
            } else {
                LoopAct::Pass(Ctl::Continue(Some(l)))
            }
        }
        other => LoopAct::Pass(other),
    }
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }
    pub fn string(s: String) -> Value {
        Value::Str(Rc::from(s))
    }
    pub fn is_nullish(&self) -> bool {
        matches!(self, Value::Undefined | Value::Null)
    }
    pub fn truthy(&self) -> bool {
        match self {
            Value::Undefined | Value::Null => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::Obj(_) => true,
        }
    }
    pub fn is_callable(&self) -> bool {
        match self {
            Value::Obj(o) => matches!(
                o.borrow().kind,
                Kind::Closure(_) | Kind::Native(_) | Kind::NativeData(_, _) | Kind::Bound(_, _, _)
            ),
            _ => false,
        }
    }
}

pub fn num_to_str(n: f64) -> String {
    if n.is_nan() {
        return String::from("NaN");
    }
    if n.is_infinite() {
        return if n > 0.0 { String::from("Infinity") } else { String::from("-Infinity") };
    }
    if n == 0.0 {
        return String::from("0");
    }
    if n == libm::floor(n) && libm::fabs(n) < 9.0e15 {
        return format!("{}", n as i64);
    }
    format!("{}", n)
}

pub fn str_to_num(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).map(|v| v as f64).unwrap_or(f64::NAN);
    }
    if t == "Infinity" || t == "+Infinity" {
        return f64::INFINITY;
    }
    if t == "-Infinity" {
        return f64::NEG_INFINITY;
    }
    if t.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-')) {
        t.parse::<f64>().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

pub fn to_i32(n: f64) -> i32 {
    if !n.is_finite() {
        return 0;
    }
    let t = libm::trunc(n);
    let mut m = libm::fmod(t, 4294967296.0);
    if m < 0.0 {
        m += 4294967296.0;
    }
    (m as u32) as i32
}

pub fn parse_index(key: &str) -> Option<usize> {
    if key.is_empty() || key.len() > 9 {
        return None;
    }
    if key.len() > 1 && key.starts_with('0') {
        return None;
    }
    if !key.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    key.parse::<usize>().ok()
}

pub fn new_scope(parent: Option<ScopeRef>) -> ScopeRef {
    Rc::new(RefCell::new(Scope { vars: Vec::new(), parent, this_val: None }))
}

pub fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Obj(x), Value::Obj(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

pub fn same_value_zero(a: &Value, b: &Value) -> bool {
    if let (Value::Num(x), Value::Num(y)) = (a, b) {
        if x.is_nan() && y.is_nan() {
            return true;
        }
    }
    strict_eq(a, b)
}

pub struct Interp {
    pub global: ObjRef,
    pub global_scope: ScopeRef,
    pub protos: Protos,
    pub logs: Vec<String>,
    pub microtasks: VecDeque<Job>,
    pub timers: Vec<Timer>,
    pub next_timer_id: u32,
    pub now_ms: f64,
    pub rng: u64,
    pub depth: usize,
    pub max_depth: usize,
    pub last: Value,
    pub yield_stack: Vec<Vec<Value>>,
    pub regex_cache: Vec<(String, Rc<Regex>)>,
    /// Opaque host data (the browser stores its DOM bridge here).
    pub host: Option<Rc<dyn core::any::Any>>,
    /// Called by `await` when a promise is still pending and no timer can run.
    /// Returns true if it made (or may still make) progress, false if nothing is pending.
    pub pump: Option<fn(&mut Interp) -> bool>,
    /// `new.target` hand-over from `construct` / `super()` to the closure that is about to run.
    pub pending_nt: Option<Value>,
}

impl Interp {
    pub fn new() -> Interp {
        let mk = |proto: Option<ObjRef>| Rc::new(RefCell::new(Obj { kind: Kind::Plain, props: Vec::new(), accessors: Vec::new(), proto }));
        let object = mk(None);
        let child = |p: &ObjRef| mk(Some(p.clone()));
        let protos = Protos {
            function: child(&object),
            array: child(&object),
            string: child(&object),
            number: child(&object),
            boolean: child(&object),
            error: child(&object),
            regexp: child(&object),
            promise: child(&object),
            date: child(&object),
            map: child(&object),
            set: child(&object),
            object: object.clone(),
        };
        let global = child(&object);
        let global_scope = new_scope(None);
        global_scope.borrow_mut().this_val = Some(Value::Obj(global.clone()));
        let mut it = Interp {
            global,
            global_scope,
            protos,
            logs: Vec::new(),
            microtasks: VecDeque::new(),
            timers: Vec::new(),
            next_timer_id: 1,
            now_ms: 0.0,
            rng: 0x2545F4914F6CDD1D,
            depth: 0,
            max_depth: MAX_DEPTH,
            last: Value::Undefined,
            yield_stack: Vec::new(),
            regex_cache: Vec::new(),
            host: None,
            pump: None,
            pending_nt: None,
        };
        crate::builtins::setup(&mut it);
        it
    }

    // ---------- allocation helpers ----------

    pub fn alloc(&self, kind: Kind, proto: Option<ObjRef>) -> ObjRef {
        Rc::new(RefCell::new(Obj { kind, props: Vec::new(), accessors: Vec::new(), proto }))
    }

    pub fn new_plain(&self) -> ObjRef {
        self.alloc(Kind::Plain, Some(self.protos.object.clone()))
    }

    pub fn new_array(&self, items: Vec<Value>) -> Value {
        Value::Obj(self.alloc(Kind::Array(items), Some(self.protos.array.clone())))
    }

    pub fn new_native(&self, f: NativeFn) -> Value {
        Value::Obj(self.alloc(Kind::Native(f), Some(self.protos.function.clone())))
    }

    pub fn new_native_data(&self, f: NativeDataFn, data: Vec<Value>) -> Value {
        Value::Obj(self.alloc(Kind::NativeData(f, data), Some(self.protos.function.clone())))
    }

    pub fn set_own(&self, o: &ObjRef, key: &str, val: Value) {
        let mut b = o.borrow_mut();
        for e in b.props.iter_mut() {
            if e.0 == key {
                e.1 = val;
                return;
            }
        }
        b.props.push((String::from(key), val));
    }

    pub fn define_accessor(&self, o: &ObjRef, key: &str, getter: Option<Value>, setter: Option<Value>) {
        let mut b = o.borrow_mut();
        for e in b.accessors.iter_mut() {
            if e.0 == key {
                if getter.is_some() {
                    e.1 = getter;
                }
                if setter.is_some() {
                    e.2 = setter;
                }
                return;
            }
        }
        b.accessors.push((String::from(key), getter, setter));
    }

    pub fn regex_obj(&mut self, src: &str, flags: &str) -> Res<Value> {
        let key = format!("{}/{}", flags, src);
        let cached = self.regex_cache.iter().find(|(k, _)| *k == key).map(|(_, r)| r.clone());
        let re = match cached {
            Some(r) => r,
            None => match crate::regex::compile(src, flags) {
                Ok(r) => {
                    let rc = Rc::new(r);
                    if self.regex_cache.len() >= 64 {
                        self.regex_cache.remove(0);
                    }
                    self.regex_cache.push((key, rc.clone()));
                    rc
                }
                Err(m) => return Err(self.throw_err("SyntaxError", &format!("Invalid regular expression: /{}/: {}", src, m))),
            },
        };
        let o = self.alloc(Kind::Regex(re), Some(self.protos.regexp.clone()));
        self.set_own(&o, "lastIndex", Value::Num(0.0));
        Ok(Value::Obj(o))
    }

    pub fn make_error(&self, kind: &str, msg: &str) -> Value {
        let proto = match self.get_own_chain(&self.global, kind) {
            Some(Value::Obj(ctor)) => match self.get_own_chain(&ctor, "prototype") {
                Some(Value::Obj(p)) => p,
                _ => self.protos.error.clone(),
            },
            _ => self.protos.error.clone(),
        };
        let o = self.alloc(Kind::Plain, Some(proto));
        self.set_own(&o, "message", Value::str(msg));
        self.set_own(&o, "name", Value::str(kind));
        Value::Obj(o)
    }

    pub fn throw_err(&self, kind: &str, msg: &str) -> Ctl {
        Ctl::Throw(self.make_error(kind, msg))
    }

    pub fn type_err(&self, msg: &str) -> Ctl {
        self.throw_err("TypeError", msg)
    }

    // ---------- scope handling ----------

    pub fn get_own_chain(&self, o: &ObjRef, key: &str) -> Option<Value> {
        let mut cur: Option<ObjRef> = Some(o.clone());
        while let Some(c) = cur.take() {
            let next = {
                let b = c.borrow();
                for (k, v) in b.props.iter() {
                    if k == key {
                        return Some(v.clone());
                    }
                }
                b.proto.clone()
            };
            cur = next;
        }
        None
    }

    pub fn lookup(&self, scope: &ScopeRef, name: &str) -> Option<Value> {
        let mut cur = scope.clone();
        loop {
            let next = {
                let s = cur.borrow();
                for (k, v) in s.vars.iter() {
                    if k == name {
                        return Some(v.clone());
                    }
                }
                s.parent.clone()
            };
            match next {
                Some(p) => cur = p,
                None => break,
            }
        }
        self.get_own_chain(&self.global, name)
    }

    pub fn lookup_this(&self, scope: &ScopeRef) -> Value {
        let mut cur = scope.clone();
        loop {
            let next = {
                let s = cur.borrow();
                if let Some(t) = &s.this_val {
                    return t.clone();
                }
                s.parent.clone()
            };
            match next {
                Some(p) => cur = p,
                None => break,
            }
        }
        Value::Undefined
    }

    pub fn assign_var(&mut self, scope: &ScopeRef, name: &str, val: Value) {
        let mut cur = scope.clone();
        loop {
            let next = {
                let mut s = cur.borrow_mut();
                for e in s.vars.iter_mut() {
                    if e.0 == name {
                        e.1 = val;
                        return;
                    }
                }
                s.parent.clone()
            };
            match next {
                Some(p) => cur = p,
                None => break,
            }
        }
        let g = self.global.clone();
        self.set_own(&g, name, val);
    }

    pub fn declare_var(&mut self, scope: &ScopeRef, name: &str, val: Value) {
        let is_root = scope.borrow().parent.is_none();
        if is_root {
            let g = self.global.clone();
            self.set_own(&g, name, val);
            return;
        }
        let mut s = scope.borrow_mut();
        for e in s.vars.iter_mut() {
            if e.0 == name {
                e.1 = val;
                return;
            }
        }
        s.vars.push((String::from(name), val));
    }

    fn declared_here(&self, scope: &ScopeRef, name: &str) -> bool {
        let s = scope.borrow();
        if s.parent.is_none() {
            return self.global.borrow().props.iter().any(|(k, _)| k == name);
        }
        s.vars.iter().any(|(k, _)| k == name)
    }

    // ---------- program entry ----------

    pub fn run(&mut self, src: &str) -> Result<Value, String> {
        let prog = parse_program(src)?;
        let scope = self.global_scope.clone();
        self.hoist_vars(&prog, &scope);
        self.hoist_funcs(&prog, &scope);
        self.last = Value::Undefined;
        self.depth = 0;
        let r = self.exec_list(&prog, &scope);
        if let Err(Ctl::Throw(v)) = r {
            return Err(self.describe_error(&v));
        }
        self.run_microtasks();
        Ok(self.last.clone())
    }

    /// Short human description of a value for diagnostics: `undefined`, `string`, `Headers`, `Array`...
    pub fn describe_value_kind(&mut self, v: &Value) -> String {
        if let Value::Obj(o) = v {
            if let Some(ctor) = self.get_own_chain(o, "constructor") {
                if let Value::Obj(c) = &ctor {
                    if let Some(n) = self.get_own_chain(c, "name") {
                        if let Ok(ns) = self.to_str(&n) {
                            if !ns.is_empty() {
                                return ns;
                            }
                        }
                    }
                }
            }
            return String::from("object");
        }
        String::from(self.typeof_str(v))
    }

    pub fn describe_error(&mut self, v: &Value) -> String {
        if let Value::Obj(o) = v {
            let name = self.get_own_chain(o, "name");
            let msg = self.get_own_chain(o, "message");
            if let (Some(n), Some(m)) = (name, msg) {
                let ns = self.to_str(&n).unwrap_or_default();
                let ms = self.to_str(&m).unwrap_or_default();
                return format!("{}: {}", ns, ms);
            }
        }
        match self.to_str(v) {
            Ok(s) => format!("Uncaught {}", s),
            Err(_) => String::from("Uncaught exception"),
        }
    }

    // ---------- hoisting ----------

    fn hoist_vars(&mut self, stmts: &[Stmt], scope: &ScopeRef) {
        for s in stmts {
            self.hoist_stmt(s, scope);
        }
    }

    fn hoist_names(&mut self, p: &Pattern, scope: &ScopeRef) {
        let mut names = Vec::new();
        pattern_names(p, &mut names);
        for n in names {
            if !self.declared_here(scope, &n) {
                self.declare_var(scope, &n, Value::Undefined);
            }
        }
    }

    fn hoist_stmt(&mut self, s: &Stmt, scope: &ScopeRef) {
        match s {
            Stmt::Var(VarKind::Var, decls) => {
                for (p, _) in decls {
                    self.hoist_names(p, scope);
                }
            }
            Stmt::If(_, a, b) => {
                self.hoist_stmt(a, scope);
                if let Some(b) = b {
                    self.hoist_stmt(b, scope);
                }
            }
            Stmt::Block(v) => self.hoist_vars(v, scope),
            Stmt::For { init, body, .. } => {
                if let Some(i) = init {
                    self.hoist_stmt(i, scope);
                }
                self.hoist_stmt(body, scope);
            }
            Stmt::ForIn { kind, target, body, .. } => {
                if let (Some(VarKind::Var), ForTarget::Pat(p)) = (kind, target) {
                    self.hoist_names(p, scope);
                }
                self.hoist_stmt(body, scope);
            }
            Stmt::While(_, b) => self.hoist_stmt(b, scope),
            Stmt::DoWhile(b, _) => self.hoist_stmt(b, scope),
            Stmt::Try { block, handler, finalizer, .. } => {
                self.hoist_vars(block, scope);
                if let Some(h) = handler {
                    self.hoist_vars(h, scope);
                }
                if let Some(f) = finalizer {
                    self.hoist_vars(f, scope);
                }
            }
            Stmt::Switch(_, cases) => {
                for (_, body) in cases {
                    self.hoist_vars(body, scope);
                }
            }
            Stmt::Labeled(_, b) => self.hoist_stmt(b, scope),
            _ => {}
        }
    }

    fn hoist_funcs(&mut self, stmts: &[Stmt], scope: &ScopeRef) {
        for s in stmts {
            if let Stmt::FuncDecl(def) = s {
                let f = self.make_closure(def, scope);
                let name = def.name.clone().unwrap_or_default();
                self.declare_var(scope, &name, f);
            }
        }
    }

    pub fn make_closure(&self, def: &Rc<FuncDef>, scope: &ScopeRef) -> Value {
        Value::Obj(self.alloc(Kind::Closure(Closure { def: def.clone(), scope: scope.clone(), home: None, class: None }), Some(self.protos.function.clone())))
    }

    // ---------- statements ----------

    fn exec_list(&mut self, stmts: &[Stmt], scope: &ScopeRef) -> Res<()> {
        for s in stmts {
            self.exec_stmt(s, scope, None)?;
        }
        Ok(())
    }

    fn exec_stmt(&mut self, s: &Stmt, scope: &ScopeRef, label: Option<&str>) -> Res<()> {
        match s {
            Stmt::Empty => Ok(()),
            Stmt::Expr(e) => {
                let v = self.eval_expr(e, scope)?;
                self.last = v;
                Ok(())
            }
            Stmt::Var(kind, decls) => {
                for (pat, init) in decls {
                    match init {
                        Some(e) => {
                            let v = self.eval_expr(e, scope)?;
                            let mode = if *kind == VarKind::Var { BindMode::Var } else { BindMode::Declare };
                            self.bind_pattern(pat, v, scope, mode)?;
                        }
                        None => {
                            if *kind != VarKind::Var {
                                self.bind_pattern(pat, Value::Undefined, scope, BindMode::Declare)?;
                            }
                        }
                    }
                }
                Ok(())
            }
            Stmt::FuncDecl(_) => Ok(()),
            Stmt::Return(e) => {
                let v = match e {
                    Some(e) => self.eval_expr(e, scope)?,
                    None => Value::Undefined,
                };
                Err(Ctl::Return(v))
            }
            Stmt::If(c, a, b) => {
                if self.eval_expr(c, scope)?.truthy() {
                    self.exec_stmt(a, scope, None)
                } else if let Some(b) = b {
                    self.exec_stmt(b, scope, None)
                } else {
                    Ok(())
                }
            }
            Stmt::Block(v) => {
                let inner = new_scope(Some(scope.clone()));
                self.hoist_funcs(v, &inner);
                self.exec_list(v, &inner)
            }
            Stmt::While(test, body) => {
                loop {
                    if !self.eval_expr(test, scope)?.truthy() {
                        break;
                    }
                    if let Err(c) = self.exec_stmt(body, scope, None) {
                        match loop_act(c, label) {
                            LoopAct::Stop => break,
                            LoopAct::Next => {}
                            LoopAct::Pass(c) => return Err(c),
                        }
                    }
                }
                Ok(())
            }
            Stmt::DoWhile(body, test) => {
                loop {
                    if let Err(c) = self.exec_stmt(body, scope, None) {
                        match loop_act(c, label) {
                            LoopAct::Stop => break,
                            LoopAct::Next => {}
                            LoopAct::Pass(c) => return Err(c),
                        }
                    }
                    if !self.eval_expr(test, scope)?.truthy() {
                        break;
                    }
                }
                Ok(())
            }
            Stmt::For { init, test, update, body } => {
                let loop_scope = new_scope(Some(scope.clone()));
                let per_iter = matches!(init.as_deref(), Some(Stmt::Var(k, _)) if *k != VarKind::Var);
                if let Some(i) = init {
                    self.exec_stmt(i, &loop_scope, None)?;
                }
                let mut cur = loop_scope;
                loop {
                    if let Some(t) = test {
                        if !self.eval_expr(t, &cur)?.truthy() {
                            break;
                        }
                    }
                    if let Err(c) = self.exec_stmt(body, &cur, None) {
                        match loop_act(c, label) {
                            LoopAct::Stop => break,
                            LoopAct::Next => {}
                            LoopAct::Pass(c) => return Err(c),
                        }
                    }
                    if per_iter {
                        let vars = cur.borrow().vars.clone();
                        cur = Rc::new(RefCell::new(Scope { vars, parent: Some(scope.clone()), this_val: None }));
                    }
                    if let Some(u) = update {
                        self.eval_expr(u, &cur)?;
                    }
                }
                Ok(())
            }
            Stmt::ForIn { kind, target, obj, body, is_of } => {
                let o = self.eval_expr(obj, scope)?;
                let items: Vec<Value> = if *is_of {
                    self.iterate(&o)?
                } else {
                    self.own_keys(&o).into_iter().map(Value::string).collect()
                };
                let mode = if matches!(kind, Some(VarKind::Var)) { BindMode::Var } else { BindMode::Declare };
                for item in items {
                    let iter_scope = new_scope(Some(scope.clone()));
                    match target {
                        ForTarget::Pat(p) => self.bind_pattern(p, item, &iter_scope, mode)?,
                        ForTarget::Expr(e) => self.assign_to(e, item, &iter_scope)?,
                    }
                    if let Err(c) = self.exec_stmt(body, &iter_scope, None) {
                        match loop_act(c, label) {
                            LoopAct::Stop => break,
                            LoopAct::Next => {}
                            LoopAct::Pass(c) => return Err(c),
                        }
                    }
                }
                Ok(())
            }
            Stmt::Break(l) => Err(Ctl::Break(l.clone())),
            Stmt::Continue(l) => Err(Ctl::Continue(l.clone())),
            Stmt::Throw(e) => {
                let v = self.eval_expr(e, scope)?;
                Err(Ctl::Throw(v))
            }
            Stmt::Try { block, param, handler, finalizer } => {
                let bscope = new_scope(Some(scope.clone()));
                self.hoist_funcs(block, &bscope);
                let mut result = self.exec_list(block, &bscope);
                if let Err(Ctl::Throw(exc)) = result {
                    match handler {
                        Some(h) => {
                            let hscope = new_scope(Some(scope.clone()));
                            let mut ok = Ok(());
                            if let Some(p) = param {
                                ok = self.bind_pattern(p, exc, &hscope, BindMode::Declare);
                            }
                            result = match ok {
                                Ok(()) => {
                                    self.hoist_funcs(h, &hscope);
                                    self.exec_list(h, &hscope)
                                }
                                Err(e) => Err(e),
                            };
                        }
                        None => result = Err(Ctl::Throw(exc)),
                    }
                }
                if let Some(f) = finalizer {
                    let fscope = new_scope(Some(scope.clone()));
                    self.hoist_funcs(f, &fscope);
                    self.exec_list(f, &fscope)?;
                }
                result
            }
            Stmt::Switch(disc, cases) => {
                let d = self.eval_expr(disc, scope)?;
                let sw = new_scope(Some(scope.clone()));
                let mut matched: Option<usize> = None;
                for (i, (t, _)) in cases.iter().enumerate() {
                    if let Some(t) = t {
                        let tv = self.eval_expr(t, &sw)?;
                        if strict_eq(&d, &tv) {
                            matched = Some(i);
                            break;
                        }
                    }
                }
                if matched.is_none() {
                    matched = cases.iter().position(|(t, _)| t.is_none());
                }
                if let Some(start) = matched {
                    for (_, body) in &cases[start..] {
                        for st in body {
                            match self.exec_stmt(st, &sw, None) {
                                Ok(()) => {}
                                Err(Ctl::Break(None)) => return Ok(()),
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }
                Ok(())
            }
            Stmt::ClassDecl(cd) => {
                let c = self.eval_class(cd, scope)?;
                let name = cd.name.clone().unwrap_or_default();
                self.declare_var(scope, &name, c);
                Ok(())
            }
            Stmt::Labeled(l, inner) => match self.exec_stmt(inner, scope, Some(l.as_str())) {
                Err(Ctl::Break(Some(b))) if b == *l => Ok(()),
                other => other,
            },
        }
    }

    // ---------- patterns ----------

    fn bind_name(&mut self, name: &str, val: Value, scope: &ScopeRef, mode: BindMode) {
        match mode {
            BindMode::Declare => self.declare_var(scope, name, val),
            BindMode::Var => {
                if self.lookup(scope, name).is_some() {
                    self.assign_var(scope, name, val);
                } else {
                    self.declare_var(scope, name, val);
                }
            }
        }
    }

    pub fn bind_pattern(&mut self, pat: &Pattern, val: Value, scope: &ScopeRef, mode: BindMode) -> Res<()> {
        match pat {
            Pattern::Ident(n) => {
                self.bind_name(n, val, scope, mode);
                Ok(())
            }
            Pattern::Array(items) => {
                let list = self.iterate(&val)?;
                for (i, it) in items.iter().enumerate() {
                    if let Some(el) = it {
                        if el.rest {
                            let rest: Vec<Value> = list.iter().skip(i).cloned().collect();
                            let arr = self.new_array(rest);
                            self.bind_pattern(&el.pat, arr, scope, mode)?;
                        } else {
                            let mut v = list.get(i).cloned().unwrap_or(Value::Undefined);
                            if matches!(v, Value::Undefined) {
                                if let Some(d) = &el.default {
                                    v = self.eval_expr(d, scope)?;
                                }
                            }
                            self.bind_pattern(&el.pat, v, scope, mode)?;
                        }
                    }
                }
                Ok(())
            }
            Pattern::Object(items, rest) => {
                if val.is_nullish() {
                    return Err(self.type_err("Cannot destructure undefined or null"));
                }
                let mut used: Vec<String> = Vec::new();
                for (key, el) in items {
                    let k = match key {
                        PropKey::Name(n) => n.clone(),
                        PropKey::Computed(e) => {
                            let kv = self.eval_expr(e, scope)?;
                            self.to_key(&kv)?
                        }
                    };
                    let mut v = self.get_prop(&val, &k)?;
                    used.push(k);
                    if matches!(v, Value::Undefined) {
                        if let Some(d) = &el.default {
                            v = self.eval_expr(d, scope)?;
                        }
                    }
                    self.bind_pattern(&el.pat, v, scope, mode)?;
                }
                if let Some(r) = rest {
                    let o = self.new_plain();
                    for k in self.own_keys(&val) {
                        if !used.contains(&k) {
                            let v = self.get_prop(&val, &k)?;
                            self.set_own(&o, &k, v);
                        }
                    }
                    self.bind_name(r, Value::Obj(o), scope, mode);
                }
                Ok(())
            }
        }
    }

    // ---------- property access ----------

    pub fn ensure_prototype(&mut self, o: &ObjRef) {
        let need = {
            let b = o.borrow();
            matches!(&b.kind, Kind::Closure(c) if !c.def.is_arrow) && !b.props.iter().any(|(k, _)| k == "prototype")
        };
        if need {
            let p = self.new_plain();
            self.set_own(&p, "constructor", Value::Obj(o.clone()));
            self.set_own(o, "prototype", Value::Obj(p));
        }
    }

    pub fn get_prop(&mut self, base: &Value, key: &str) -> Res<Value> {
        match base {
            Value::Obj(o) => {
                let proxy = if let Kind::Proxy(t, h) = &o.borrow().kind { Some((t.clone(), h.clone())) } else { None };
                if let Some((t, h)) = proxy {
                    let trap = self.get_prop(&h, "get")?;
                    return if trap.is_callable() {
                        self.call(&trap, &h, &[t, Value::str(key), base.clone()])
                    } else {
                        self.get_prop(&t, key)
                    };
                }
                {
                    let b = o.borrow();
                    match &b.kind {
                        Kind::Array(v) => {
                            if key == "length" {
                                return Ok(Value::Num(v.len() as f64));
                            }
                            if let Some(i) = parse_index(key) {
                                return Ok(v.get(i).cloned().unwrap_or(Value::Undefined));
                            }
                        }
                        Kind::Map(m) => {
                            if key == "size" {
                                return Ok(Value::Num(m.len() as f64));
                            }
                        }
                        Kind::Set(s) => {
                            if key == "size" {
                                return Ok(Value::Num(s.len() as f64));
                            }
                        }
                        Kind::Regex(r) => match key {
                            "source" => return Ok(Value::str(&r.source)),
                            "flags" => return Ok(Value::str(&r.flags)),
                            "global" => return Ok(Value::Bool(r.global)),
                            "sticky" => return Ok(Value::Bool(r.sticky)),
                            "ignoreCase" => return Ok(Value::Bool(r.flags.contains('i'))),
                            "multiline" => return Ok(Value::Bool(r.flags.contains('m'))),
                            "dotAll" => return Ok(Value::Bool(r.flags.contains('s'))),
                            "unicode" => return Ok(Value::Bool(r.flags.contains('u'))),
                            _ => {}
                        },
                        _ => {}
                    }
                }
                if key == "prototype" {
                    self.ensure_prototype(o);
                }
                let mut cur: Option<ObjRef> = Some(o.clone());
                while let Some(c) = cur.take() {
                    let (found, getter, next) = {
                        let b = c.borrow();
                        let mut found: Option<Value> = None;
                        for (k, v) in b.props.iter() {
                            if k == key {
                                found = Some(v.clone());
                                break;
                            }
                        }
                        let mut getter: Option<Option<Value>> = None;
                        if found.is_none() {
                            for (k, g, _) in b.accessors.iter() {
                                if k == key {
                                    getter = Some(g.clone());
                                    break;
                                }
                            }
                        }
                        (found, getter, b.proto.clone())
                    };
                    if let Some(v) = found {
                        return Ok(v);
                    }
                    if let Some(g) = getter {
                        return match g {
                            Some(gf) => self.call(&gf, base, &[]),
                            None => Ok(Value::Undefined),
                        };
                    }
                    cur = next;
                }
                Ok(Value::Undefined)
            }
            Value::Str(s) => {
                if key == "length" {
                    return Ok(Value::Num(s.chars().count() as f64));
                }
                if let Some(i) = parse_index(key) {
                    return Ok(match s.chars().nth(i) {
                        Some(c) => Value::string(c.to_string()),
                        None => Value::Undefined,
                    });
                }
                Ok(self.get_own_chain(&self.protos.string, key).unwrap_or(Value::Undefined))
            }
            Value::Num(_) => Ok(self.get_own_chain(&self.protos.number, key).unwrap_or(Value::Undefined)),
            Value::Bool(_) => Ok(self.get_own_chain(&self.protos.boolean, key).unwrap_or(Value::Undefined)),
            Value::Undefined => Err(self.type_err(&format!("Cannot read properties of undefined (reading '{}')", key))),
            Value::Null => Err(self.type_err(&format!("Cannot read properties of null (reading '{}')", key))),
        }
    }

    pub fn set_prop(&mut self, base: &Value, key: &str, val: Value) -> Res<()> {
        match base {
            Value::Obj(o) => {
                let proxy = if let Kind::Proxy(t, h) = &o.borrow().kind { Some((t.clone(), h.clone())) } else { None };
                if let Some((t, h)) = proxy {
                    let trap = self.get_prop(&h, "set")?;
                    if trap.is_callable() {
                        self.call(&trap, &h, &[t, Value::str(key), val, base.clone()])?;
                        return Ok(());
                    }
                    return self.set_prop(&t, key, val);
                }
                {
                    let mut b = o.borrow_mut();
                    if let Kind::Array(v) = &mut b.kind {
                        if key == "length" {
                            let n = match &val {
                                Value::Num(n) if *n >= 0.0 => *n as usize,
                                _ => 0,
                            };
                            if n > 5_000_000 {
                                drop(b);
                                return Err(self.throw_err("RangeError", "Invalid array length"));
                            }
                            v.resize(n, Value::Undefined);
                            return Ok(());
                        }
                        if let Some(i) = parse_index(key) {
                            if i > 5_000_000 {
                                drop(b);
                                return Err(self.throw_err("RangeError", "Array index too large"));
                            }
                            if i >= v.len() {
                                v.resize(i + 1, Value::Undefined);
                            }
                            v[i] = val;
                            return Ok(());
                        }
                    }
                }
                let mut cur: Option<ObjRef> = Some(o.clone());
                while let Some(c) = cur.take() {
                    let (has_data, setter, next) = {
                        let b = c.borrow();
                        let has_data = b.props.iter().any(|(k, _)| k == key);
                        let mut setter: Option<Option<Value>> = None;
                        if !has_data {
                            for (k, _, st) in b.accessors.iter() {
                                if k == key {
                                    setter = Some(st.clone());
                                    break;
                                }
                            }
                        }
                        (has_data, setter, b.proto.clone())
                    };
                    if has_data {
                        break;
                    }
                    if let Some(st) = setter {
                        if let Some(sf) = st {
                            self.call(&sf, base, &[val])?;
                        }
                        return Ok(());
                    }
                    cur = next;
                }
                self.set_own(o, key, val);
                Ok(())
            }
            Value::Undefined | Value::Null => Err(self.type_err(&format!("Cannot set properties of undefined (setting '{}')", key))),
            _ => Ok(()),
        }
    }

    pub fn to_key(&mut self, v: &Value) -> Res<String> {
        match v {
            Value::Num(n) => Ok(num_to_str(*n)),
            Value::Str(s) => Ok(String::from(&**s)),
            other => self.to_str(other),
        }
    }

    pub fn own_keys(&self, v: &Value) -> Vec<String> {
        let mut out = Vec::new();
        if let Value::Obj(o) = v {
            if let Kind::Proxy(t, _) = &o.borrow().kind {
                return self.own_keys(t);
            }
        }
        match v {
            Value::Obj(o) => {
                let b = o.borrow();
                if let Kind::Array(a) = &b.kind {
                    for i in 0..a.len() {
                        out.push(i.to_string());
                    }
                }
                for (k, _) in b.props.iter() {
                    if !k.starts_with("@@") && !k.starts_with('#') {
                        if matches!(b.kind, Kind::Regex(_)) && k == "lastIndex" {
                            continue;
                        }
                        out.push(k.clone());
                    }
                }
                for (k, _, _) in b.accessors.iter() {
                    if !k.starts_with('#') && !out.contains(k) {
                        out.push(k.clone());
                    }
                }
            }
            Value::Str(s) => {
                for i in 0..s.chars().count() {
                    out.push(i.to_string());
                }
            }
            _ => {}
        }
        out
    }

    // ---------- conversions ----------

    pub fn to_str(&mut self, v: &Value) -> Res<String> {
        match v {
            Value::Undefined => Ok(String::from("undefined")),
            Value::Null => Ok(String::from("null")),
            Value::Bool(b) => Ok(String::from(if *b { "true" } else { "false" })),
            Value::Num(n) => Ok(num_to_str(*n)),
            Value::Str(s) => Ok(String::from(&**s)),
            Value::Obj(o) => {
                let f = self.get_own_chain(o, "toString");
                match f {
                    Some(f) if f.is_callable() => {
                        let r = self.call(&f, v, &[])?;
                        match r {
                            Value::Obj(_) => Ok(String::from("[object Object]")),
                            other => self.to_str(&other),
                        }
                    }
                    _ => Ok(String::from("[object Object]")),
                }
            }
        }
    }

    pub fn to_num(&mut self, v: &Value) -> Res<f64> {
        match v {
            Value::Undefined => Ok(f64::NAN),
            Value::Null => Ok(0.0),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::Num(n) => Ok(*n),
            Value::Str(s) => Ok(str_to_num(s)),
            Value::Obj(o) => {
                if let Kind::Date(t) = &o.borrow().kind {
                    return Ok(*t);
                }
                let s = self.to_str(v)?;
                Ok(str_to_num(&s))
            }
        }
    }

    fn to_prim(&mut self, v: &Value) -> Res<Value> {
        match v {
            Value::Obj(o) => {
                if let Kind::Date(t) = &o.borrow().kind {
                    return Ok(Value::Num(*t));
                }
                let s = self.to_str(v)?;
                Ok(Value::string(s))
            }
            other => Ok(other.clone()),
        }
    }

    pub fn typeof_str(&self, v: &Value) -> &'static str {
        match v {
            Value::Undefined => "undefined",
            Value::Null => "object",
            Value::Bool(_) => "boolean",
            Value::Num(_) => "number",
            Value::Str(_) => "string",
            Value::Obj(_) => {
                if v.is_callable() {
                    "function"
                } else {
                    "object"
                }
            }
        }
    }

    pub fn loose_eq(&mut self, a: &Value, b: &Value) -> Res<bool> {
        if a.is_nullish() && b.is_nullish() {
            return Ok(true);
        }
        if a.is_nullish() || b.is_nullish() {
            return Ok(false);
        }
        match (a, b) {
            (Value::Num(_), Value::Str(s)) => Ok(strict_eq(a, &Value::Num(str_to_num(s)))),
            (Value::Str(s), Value::Num(_)) => Ok(strict_eq(&Value::Num(str_to_num(s)), b)),
            (Value::Bool(x), _) => {
                let n = Value::Num(if *x { 1.0 } else { 0.0 });
                self.loose_eq(&n, b)
            }
            (_, Value::Bool(y)) => {
                let n = Value::Num(if *y { 1.0 } else { 0.0 });
                self.loose_eq(a, &n)
            }
            (Value::Obj(_), Value::Obj(_)) => Ok(strict_eq(a, b)),
            (Value::Obj(_), _) => {
                let p = self.to_prim(a)?;
                self.loose_eq(&p, b)
            }
            (_, Value::Obj(_)) => {
                let p = self.to_prim(b)?;
                self.loose_eq(a, &p)
            }
            _ => Ok(strict_eq(a, b)),
        }
    }

    pub fn iterate(&mut self, v: &Value) -> Res<Vec<Value>> {
        match v {
            Value::Str(s) => Ok(s.chars().map(|c| Value::string(c.to_string())).collect()),
            Value::Obj(o) => {
                {
                    let b = o.borrow();
                    match &b.kind {
                        Kind::Array(a) => return Ok(a.clone()),
                        Kind::Set(s) => return Ok(s.clone()),
                        Kind::Map(m) => {
                            let pairs: Vec<(Value, Value)> = m.clone();
                            drop(b);
                            return Ok(pairs.into_iter().map(|(k, v)| self.new_array(vec![k, v])).collect());
                        }
                        _ => {}
                    }
                }
                let itf = self.get_own_chain(o, "@@iterator");
                if let Some(f) = itf {
                    if f.is_callable() {
                        let iter = self.call(&f, v, &[])?;
                        let next = self.get_prop(&iter, "next")?;
                        let mut out = Vec::new();
                        for _ in 0..200_000 {
                            let r = self.call(&next, &iter, &[])?;
                            if self.get_prop(&r, "done")?.truthy() {
                                return Ok(out);
                            }
                            out.push(self.get_prop(&r, "value")?);
                        }
                        return Ok(out);
                    }
                }
                Err(self.type_err("object is not iterable"))
            }
            _ => Err(self.type_err("value is not iterable")),
        }
    }

    // ---------- calls ----------

    pub fn call(&mut self, f: &Value, this: &Value, args: &[Value]) -> Res<Value> {
        enum Callee {
            Closure(Rc<FuncDef>, ScopeRef, Option<ObjRef>, Option<(Rc<ClassDef>, ScopeRef)>),
            Native(NativeFn),
            NativeData(NativeDataFn, Vec<Value>),
            Bound(Value, Value, Vec<Value>),
            No,
        }
        let callee = match f {
            Value::Obj(o) => match &o.borrow().kind {
                Kind::Closure(c) => Callee::Closure(c.def.clone(), c.scope.clone(), c.home.clone(), c.class.clone()),
                Kind::Native(n) => Callee::Native(*n),
                Kind::NativeData(n, d) => Callee::NativeData(*n, d.clone()),
                Kind::Bound(t, th, a) => Callee::Bound(t.clone(), th.clone(), a.clone()),
                _ => Callee::No,
            },
            _ => Callee::No,
        };
        match callee {
            Callee::Closure(def, scope, home, class) => {
                if self.depth >= self.max_depth {
                    return Err(self.throw_err("RangeError", "Maximum call stack size exceeded"));
                }
                self.depth += 1;
                let r = self.call_closure(&def, &scope, &home, &class, f, this, args);
                self.depth -= 1;
                r
            }
            Callee::Native(n) => n(self, this, args),
            Callee::NativeData(n, d) => n(self, &d, this, args),
            Callee::Bound(t, th, mut a) => {
                a.extend_from_slice(args);
                self.call(&t, &th, &a)
            }
            Callee::No => Err(self.type_err("value is not a function")),
        }
    }

    fn call_closure(
        &mut self,
        def: &Rc<FuncDef>,
        scope: &ScopeRef,
        home: &Option<ObjRef>,
        class: &Option<(Rc<ClassDef>, ScopeRef)>,
        fval: &Value,
        this: &Value,
        args: &[Value],
    ) -> Res<Value> {
        let fscope = new_scope(Some(scope.clone()));
        if def.is_generator {
            self.yield_stack.push(Vec::new());
        }
        if let Some(h) = home {
            fscope.borrow_mut().vars.push((String::from("@@home"), Value::Obj(h.clone())));
        }
        if def.is_ctor {
            fscope.borrow_mut().vars.push((String::from("@@func"), fval.clone()));
        }
        if !def.is_arrow {
            let nt = self.pending_nt.take().unwrap_or(Value::Undefined);
            fscope.borrow_mut().vars.push((String::from("@@nt"), nt));
            fscope.borrow_mut().this_val = Some(this.clone());
            let arr = self.new_array(args.to_vec());
            fscope.borrow_mut().vars.push((String::from("arguments"), arr));
        }
        let mut bind_err: Option<Ctl> = None;
        for (i, p) in def.params.iter().enumerate() {
            let r: Res<()> = if p.rest {
                let rest: Vec<Value> = args.iter().skip(i).cloned().collect();
                let arr = self.new_array(rest);
                self.bind_pattern(&p.pat, arr, &fscope, BindMode::Declare)
            } else {
                let mut v = args.get(i).cloned().unwrap_or(Value::Undefined);
                let mut res = Ok(());
                if matches!(v, Value::Undefined) {
                    if let Some(d) = &p.default {
                        match self.eval_expr(d, &fscope) {
                            Ok(dv) => v = dv,
                            Err(e) => res = Err(e),
                        }
                    }
                }
                if res.is_ok() {
                    self.bind_pattern(&p.pat, v, &fscope, BindMode::Declare)
                } else {
                    res
                }
            };
            if let Err(e) = r {
                bind_err = Some(e);
                break;
            }
        }
        let result: Res<Value> = match bind_err {
            Some(e) => Err(e),
            None => {
                let mut pre: Res<()> = Ok(());
                if def.is_ctor && !def.is_derived {
                    if let Some((cd, cs)) = class {
                        pre = self.init_fields(cd, cs, this);
                    }
                }
                match pre {
                    Err(e) => Err(e),
                    Ok(()) => {
                        self.hoist_vars(&def.body, &fscope);
                        self.hoist_funcs(&def.body, &fscope);
                        match self.exec_list(&def.body, &fscope) {
                            Ok(()) => Ok(Value::Undefined),
                            Err(Ctl::Return(v)) => Ok(v),
                            Err(Ctl::Throw(v)) => Err(Ctl::Throw(v)),
                            Err(_) => Ok(Value::Undefined),
                        }
                    }
                }
            }
        };
        if def.is_generator {
            let buf = self.yield_stack.pop().unwrap_or_default();
            return match result {
                Ok(v) => Ok(self.make_generator(buf, v)),
                Err(Ctl::Throw(e)) => Err(Ctl::Throw(e)),
                Err(_) => Ok(self.make_generator(buf, Value::Undefined)),
            };
        }
        if def.is_ctor {
            if let Ok(Value::Undefined) = &result {
                return Ok(self.lookup_this(&fscope));
            }
        }
        if def.is_async {
            let p = self.new_promise();
            match result {
                Ok(v) => self.resolve_with(&p, v),
                Err(Ctl::Throw(e)) => self.settle(&p, 2, e),
                Err(_) => {}
            }
            return Ok(Value::Obj(p));
        }
        result
    }

    pub fn construct(&mut self, f: &Value, args: &[Value]) -> Res<Value> {
        if !f.is_callable() {
            return Err(self.type_err("value is not a constructor"));
        }
        if let Value::Obj(fo) = f {
            if let Kind::Closure(c) = &fo.borrow().kind {
                if c.def.is_arrow {
                    return Err(self.type_err("arrow function is not a constructor"));
                }
            }
        }
        let proto = match self.get_prop(f, "prototype")? {
            Value::Obj(p) => p,
            _ => self.protos.object.clone(),
        };
        let this = self.alloc(Kind::Plain, Some(proto));
        // Only a script closure observes new.target; a native constructor may call back into script
        // (e.g. a Promise executor) and must not leak it.
        let is_closure = matches!(&f, Value::Obj(fo) if matches!(fo.borrow().kind, Kind::Closure(_)));
        self.pending_nt = if is_closure { Some(f.clone()) } else { None };
        let r = self.call(f, &Value::Obj(this.clone()), args);
        self.pending_nt = None;
        let r = r?;
        match r {
            Value::Obj(_) => Ok(r),
            _ => Ok(Value::Obj(this)),
        }
    }

    // ---------- expressions ----------

    fn eval_args(&mut self, args: &[Expr], scope: &ScopeRef) -> Res<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Expr::Spread(inner) => {
                    let v = self.eval_expr(inner, scope)?;
                    let items = self.iterate(&v)?;
                    out.extend(items);
                }
                other => out.push(self.eval_expr(other, scope)?),
            }
        }
        Ok(out)
    }

    fn eval_chain_or_expr(&mut self, e: &Expr, scope: &ScopeRef) -> Res<Option<Value>> {
        match e {
            Expr::Member { .. } | Expr::Index { .. } | Expr::Call { .. } => self.eval_chain(e, scope),
            _ => Ok(Some(self.eval_expr(e, scope)?)),
        }
    }

    fn eval_chain(&mut self, e: &Expr, scope: &ScopeRef) -> Res<Option<Value>> {
        match e {
            Expr::Member { obj, prop, optional } => {
                let base = match self.eval_chain_or_expr(obj, scope)? {
                    Some(v) => v,
                    None => return Ok(None),
                };
                if *optional && base.is_nullish() {
                    return Ok(None);
                }
                Ok(Some(self.get_prop(&base, prop)?))
            }
            Expr::Index { obj, idx, optional } => {
                let base = match self.eval_chain_or_expr(obj, scope)? {
                    Some(v) => v,
                    None => return Ok(None),
                };
                if *optional && base.is_nullish() {
                    return Ok(None);
                }
                let iv = self.eval_expr(idx, scope)?;
                if let (Value::Obj(o), Value::Num(n)) = (&base, &iv) {
                    if let Kind::Array(a) = &o.borrow().kind {
                        if *n >= 0.0 && *n == libm::floor(*n) {
                            return Ok(Some(a.get(*n as usize).cloned().unwrap_or(Value::Undefined)));
                        }
                    }
                }
                let k = self.to_key(&iv)?;
                Ok(Some(self.get_prop(&base, &k)?))
            }
            Expr::Call { callee, args, optional } => {
                let (func, this) = match &**callee {
                    Expr::Member { obj, prop, optional: mo } => {
                        let base = match self.eval_chain_or_expr(obj, scope)? {
                            Some(v) => v,
                            None => return Ok(None),
                        };
                        if *mo && base.is_nullish() {
                            return Ok(None);
                        }
                        let f = self.get_prop(&base, prop)?;
                        (f, base)
                    }
                    Expr::Index { obj, idx, optional: mo } => {
                        let base = match self.eval_chain_or_expr(obj, scope)? {
                            Some(v) => v,
                            None => return Ok(None),
                        };
                        if *mo && base.is_nullish() {
                            return Ok(None);
                        }
                        let iv = self.eval_expr(idx, scope)?;
                        let k = self.to_key(&iv)?;
                        let f = self.get_prop(&base, &k)?;
                        (f, base)
                    }
                    Expr::SuperMember(pe) => {
                        let base = self.super_base(scope)?;
                        let kv = self.eval_expr(pe, scope)?;
                        let k = self.to_key(&kv)?;
                        let f = self.get_prop(&base, &k)?;
                        (f, self.lookup_this(scope))
                    }
                    other => {
                        let f = match self.eval_chain_or_expr(other, scope)? {
                            Some(v) => v,
                            None => return Ok(None),
                        };
                        (f, Value::Undefined)
                    }
                };
                if *optional && func.is_nullish() {
                    return Ok(None);
                }
                let argv = self.eval_args(args, scope)?;
                if !func.is_callable() {
                    // Name the receiver's constructor and what the property really is, so a missing
                    // builtin / DOM method can be identified from one log line.
                    let recv = self.describe_value_kind(&this);
                    let got = self.describe_value_kind(&func);
                    return Err(self.type_err(&format!(
                        "{} is not a function [receiver: {}, value: {}]",
                        describe_callee(callee),
                        recv,
                        got
                    )));
                }
                Ok(Some(self.call(&func, &this, &argv)?))
            }
            _ => Ok(Some(self.eval_expr(e, scope)?)),
        }
    }

    fn assign_to(&mut self, target: &Expr, val: Value, scope: &ScopeRef) -> Res<()> {
        match target {
            Expr::Ident(n) => {
                self.assign_var(scope, n, val);
                Ok(())
            }
            Expr::Member { obj, prop, .. } => {
                let base = self.eval_expr(obj, scope)?;
                self.set_prop(&base, prop, val)
            }
            Expr::Index { obj, idx, .. } => {
                let base = self.eval_expr(obj, scope)?;
                let iv = self.eval_expr(idx, scope)?;
                let k = self.to_key(&iv)?;
                self.set_prop(&base, &k, val)
            }
            _ => Err(self.throw_err("SyntaxError", "Invalid assignment target")),
        }
    }

    pub fn binary_op(&mut self, op: &str, a: &Value, b: &Value) -> Res<Value> {
        match op {
            "+" => {
                if let (Value::Num(x), Value::Num(y)) = (a, b) {
                    return Ok(Value::Num(x + y));
                }
                let pa = self.to_prim(a)?;
                let pb = self.to_prim(b)?;
                if matches!(pa, Value::Str(_)) || matches!(pb, Value::Str(_)) {
                    let mut s = self.to_str(&pa)?;
                    s.push_str(&self.to_str(&pb)?);
                    Ok(Value::string(s))
                } else {
                    Ok(Value::Num(self.to_num(&pa)? + self.to_num(&pb)?))
                }
            }
            "-" | "*" | "/" | "%" | "**" => {
                let x = self.to_num(a)?;
                let y = self.to_num(b)?;
                Ok(Value::Num(match op {
                    "-" => x - y,
                    "*" => x * y,
                    "/" => x / y,
                    "%" => {
                        if y == 0.0 || x.is_infinite() {
                            f64::NAN
                        } else if y.is_infinite() {
                            x
                        } else {
                            libm::fmod(x, y)
                        }
                    }
                    _ => libm::pow(x, y),
                }))
            }
            "==" => Ok(Value::Bool(self.loose_eq(a, b)?)),
            "!=" => Ok(Value::Bool(!self.loose_eq(a, b)?)),
            "===" => Ok(Value::Bool(strict_eq(a, b))),
            "!==" => Ok(Value::Bool(!strict_eq(a, b))),
            "<" | ">" | "<=" | ">=" => {
                let pa = self.to_prim(a)?;
                let pb = self.to_prim(b)?;
                if let (Value::Str(x), Value::Str(y)) = (&pa, &pb) {
                    let r = match op {
                        "<" => x < y,
                        ">" => x > y,
                        "<=" => x <= y,
                        _ => x >= y,
                    };
                    return Ok(Value::Bool(r));
                }
                let x = self.to_num(&pa)?;
                let y = self.to_num(&pb)?;
                Ok(Value::Bool(match op {
                    "<" => x < y,
                    ">" => x > y,
                    "<=" => x <= y,
                    _ => x >= y,
                }))
            }
            "&" | "|" | "^" | "<<" | ">>" | ">>>" => {
                let x = to_i32(self.to_num(a)?);
                let y = to_i32(self.to_num(b)?);
                let r: f64 = match op {
                    "&" => (x & y) as f64,
                    "|" => (x | y) as f64,
                    "^" => (x ^ y) as f64,
                    "<<" => x.wrapping_shl((y & 31) as u32) as f64,
                    ">>" => (x >> (y & 31)) as f64,
                    _ => ((x as u32) >> ((y & 31) as u32)) as f64,
                };
                Ok(Value::Num(r))
            }
            "instanceof" => {
                if !b.is_callable() {
                    return Err(self.type_err("Right-hand side of 'instanceof' is not callable"));
                }
                let target = match self.get_prop(b, "prototype")? {
                    Value::Obj(p) => p,
                    _ => return Ok(Value::Bool(false)),
                };
                if let Value::Obj(o) = a {
                    let mut cur = o.borrow().proto.clone();
                    while let Some(p) = cur {
                        if Rc::ptr_eq(&p, &target) {
                            return Ok(Value::Bool(true));
                        }
                        cur = p.borrow().proto.clone();
                    }
                }
                Ok(Value::Bool(false))
            }
            "in" => {
                let k = self.to_key(a)?;
                match b {
                    Value::Obj(o) => {
                        let proxy = if let Kind::Proxy(t, h) = &o.borrow().kind { Some((t.clone(), h.clone())) } else { None };
                        if let Some((t, h)) = proxy {
                            let trap = self.get_prop(&h, "has")?;
                            if trap.is_callable() {
                                let r = self.call(&trap, &h, &[t, Value::string(k)])?;
                                return Ok(Value::Bool(r.truthy()));
                            }
                            return self.binary_op("in", a, &t);
                        }
                        if let Kind::Array(arr) = &o.borrow().kind {
                            if let Some(i) = parse_index(&k) {
                                return Ok(Value::Bool(i < arr.len()));
                            }
                            if k == "length" {
                                return Ok(Value::Bool(true));
                            }
                        }
                        Ok(Value::Bool(self.get_own_chain(o, &k).is_some()))
                    }
                    _ => Err(self.type_err("Cannot use 'in' operator on a non-object")),
                }
            }
            _ => Err(self.throw_err("SyntaxError", &format!("unknown operator {}", op))),
        }
    }

    pub fn eval_expr(&mut self, e: &Expr, scope: &ScopeRef) -> Res<Value> {
        match e {
            Expr::Num(n) => Ok(Value::Num(*n)),
            Expr::Str(s) => Ok(Value::str(s)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Null => Ok(Value::Null),
            Expr::Hole => Ok(Value::Undefined),
            Expr::This => Ok(self.lookup_this(scope)),
            Expr::NewTarget => Ok(self.lookup(scope, "@@nt").unwrap_or(Value::Undefined)),
            Expr::Ident(n) => match self.lookup(scope, n) {
                Some(v) => Ok(v),
                None => Err(self.throw_err("ReferenceError", &format!("{} is not defined", n))),
            },
            Expr::Tpl(strs, exprs) => {
                let mut out = String::new();
                for (i, s) in strs.iter().enumerate() {
                    out.push_str(s);
                    if i < exprs.len() {
                        let v = self.eval_expr(&exprs[i], scope)?;
                        out.push_str(&self.to_str(&v)?);
                    }
                }
                Ok(Value::string(out))
            }
            Expr::Regex(b, f) => self.regex_obj(b, f),
            Expr::Array(items) => {
                let v = self.eval_args(items, scope)?;
                Ok(self.new_array(v))
            }
            Expr::Object(props) => {
                let o = self.new_plain();
                for p in props {
                    match p {
                        PropDef::KeyVal(k, ve) => {
                            let key = match k {
                                PropKey::Name(n) => n.clone(),
                                PropKey::Computed(ce) => {
                                    let kv = self.eval_expr(ce, scope)?;
                                    self.to_key(&kv)?
                                }
                            };
                            let v = self.eval_expr(ve, scope)?;
                            self.set_own(&o, &key, v);
                        }
                        PropDef::Getter(k, f) => {
                            let key = self.eval_prop_key(k, scope)?;
                            let c = self.make_closure(f, scope);
                            self.define_accessor(&o, &key, Some(c), None);
                        }
                        PropDef::Setter(k, f) => {
                            let key = self.eval_prop_key(k, scope)?;
                            let c = self.make_closure(f, scope);
                            self.define_accessor(&o, &key, None, Some(c));
                        }
                        PropDef::Spread(se) => {
                            let sv = self.eval_expr(se, scope)?;
                            for k in self.own_keys(&sv) {
                                let v = self.get_prop(&sv, &k)?;
                                self.set_own(&o, &k, v);
                            }
                        }
                    }
                }
                Ok(Value::Obj(o))
            }
            Expr::Func(def) => {
                if let (Some(name), false) = (&def.name, def.is_arrow) {
                    let fscope = new_scope(Some(scope.clone()));
                    let f = self.make_closure(def, &fscope);
                    fscope.borrow_mut().vars.push((name.clone(), f.clone()));
                    Ok(f)
                } else {
                    Ok(self.make_closure(def, scope))
                }
            }
            Expr::Unary(op, inner) => match op.as_str() {
                "typeof" => {
                    if let Expr::Ident(n) = &**inner {
                        return Ok(match self.lookup(scope, n) {
                            Some(v) => Value::str(self.typeof_str(&v)),
                            None => Value::str("undefined"),
                        });
                    }
                    let v = self.eval_expr(inner, scope)?;
                    Ok(Value::str(self.typeof_str(&v)))
                }
                "delete" => {
                    match &**inner {
                        Expr::Member { obj, prop, .. } => {
                            let base = self.eval_expr(obj, scope)?;
                            self.delete_prop(&base, prop);
                        }
                        Expr::Index { obj, idx, .. } => {
                            let base = self.eval_expr(obj, scope)?;
                            let iv = self.eval_expr(idx, scope)?;
                            let k = self.to_key(&iv)?;
                            self.delete_prop(&base, &k);
                        }
                        _ => {}
                    }
                    Ok(Value::Bool(true))
                }
                "void" => {
                    self.eval_expr(inner, scope)?;
                    Ok(Value::Undefined)
                }
                "!" => Ok(Value::Bool(!self.eval_expr(inner, scope)?.truthy())),
                "-" => {
                    let v = self.eval_expr(inner, scope)?;
                    Ok(Value::Num(-self.to_num(&v)?))
                }
                "+" => {
                    let v = self.eval_expr(inner, scope)?;
                    Ok(Value::Num(self.to_num(&v)?))
                }
                _ => {
                    let v = self.eval_expr(inner, scope)?;
                    Ok(Value::Num(!to_i32(self.to_num(&v)?) as f64))
                }
            },
            Expr::Update { op, prefix, target } => {
                let old = self.eval_expr(target, scope)?;
                let n = self.to_num(&old)?;
                let new = if op == "++" { n + 1.0 } else { n - 1.0 };
                self.assign_to(target, Value::Num(new), scope)?;
                Ok(Value::Num(if *prefix { new } else { n }))
            }
            Expr::Binary(op, l, r) => {
                let a = self.eval_expr(l, scope)?;
                let b = self.eval_expr(r, scope)?;
                self.binary_op(op, &a, &b)
            }
            Expr::Logical(op, l, r) => {
                let a = self.eval_expr(l, scope)?;
                let take_right = match op.as_str() {
                    "&&" => a.truthy(),
                    "||" => !a.truthy(),
                    _ => a.is_nullish(),
                };
                if take_right {
                    self.eval_expr(r, scope)
                } else {
                    Ok(a)
                }
            }
            Expr::DestructAssign { pat, value } => {
                let v = self.eval_expr(value, scope)?;
                // `Var` mode assigns to an existing binding and only declares when none is visible.
                self.bind_pattern(pat, v.clone(), scope, BindMode::Var)?;
                Ok(v)
            }
            Expr::Assign { op, target, value } => {
                if op == "=" {
                    let v = self.eval_expr(value, scope)?;
                    self.assign_to(target, v.clone(), scope)?;
                    return Ok(v);
                }
                let cur = self.eval_expr(target, scope)?;
                if op == "&&=" || op == "||=" || op == "??=" {
                    let assign = match op.as_str() {
                        "&&=" => cur.truthy(),
                        "||=" => !cur.truthy(),
                        _ => cur.is_nullish(),
                    };
                    if !assign {
                        return Ok(cur);
                    }
                    let v = self.eval_expr(value, scope)?;
                    self.assign_to(target, v.clone(), scope)?;
                    return Ok(v);
                }
                let rhs = self.eval_expr(value, scope)?;
                let bop = &op[..op.len() - 1];
                let v = self.binary_op(bop, &cur, &rhs)?;
                self.assign_to(target, v.clone(), scope)?;
                Ok(v)
            }
            Expr::Cond(c, a, b) => {
                if self.eval_expr(c, scope)?.truthy() {
                    self.eval_expr(a, scope)
                } else {
                    self.eval_expr(b, scope)
                }
            }
            Expr::Member { .. } | Expr::Index { .. } | Expr::Call { .. } => Ok(self.eval_chain(e, scope)?.unwrap_or(Value::Undefined)),
            Expr::New { callee, args } => {
                let f = self.eval_expr(callee, scope)?;
                let argv = self.eval_args(args, scope)?;
                self.construct(&f, &argv)
            }
            Expr::Seq(items) => {
                let mut last = Value::Undefined;
                for it in items {
                    last = self.eval_expr(it, scope)?;
                }
                Ok(last)
            }
            Expr::Spread(_) => Err(self.throw_err("SyntaxError", "Unexpected spread")),
            Expr::Await(inner) => {
                let v = self.eval_expr(inner, scope)?;
                self.await_value(v)
            }
            Expr::Class(cd) => self.eval_class(cd, scope),
            Expr::SuperMember(pe) => {
                let base = self.super_base(scope)?;
                let kv = self.eval_expr(pe, scope)?;
                let k = self.to_key(&kv)?;
                self.get_prop(&base, &k)
            }
            Expr::SuperCall(args) => {
                let func = self.lookup(scope, "@@func").unwrap_or(Value::Undefined);
                let parent = match &func {
                    Value::Obj(f) => f.borrow().proto.clone(),
                    _ => None,
                };
                let parent = match parent {
                    Some(p) => Value::Obj(p),
                    None => return Err(self.throw_err("SyntaxError", "'super' keyword unexpected here")),
                };
                let argv = self.eval_args(args, scope)?;
                if !parent.is_callable() {
                    return Err(self.type_err("Super constructor is not a constructor"));
                }
                let this = self.lookup_this(scope);
                let parent_is_closure = matches!(&parent, Value::Obj(po) if matches!(po.borrow().kind, Kind::Closure(_)));
                self.pending_nt = if parent_is_closure { self.lookup(scope, "@@nt") } else { None };
                let r = self.call(&parent, &this, &argv);
                self.pending_nt = None;
                let r = r?;
                let new_this = match &r {
                    Value::Obj(_) if !strict_eq(&r, &this) => r.clone(),
                    _ => this,
                };
                self.set_this(scope, new_this.clone());
                let class = if let Value::Obj(f) = &func {
                    match &f.borrow().kind {
                        Kind::Closure(c) => c.class.clone(),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some((cd, cs)) = class {
                    self.init_fields(&cd, &cs, &new_this)?;
                }
                Ok(new_this)
            }
            Expr::TaggedTpl { tag, strs, exprs } => {
                let (f, this) = match &**tag {
                    Expr::Member { obj, prop, .. } => {
                        let b = self.eval_expr(obj, scope)?;
                        let f = self.get_prop(&b, prop)?;
                        (f, b)
                    }
                    other => (self.eval_expr(other, scope)?, Value::Undefined),
                };
                let sv: Vec<Value> = strs.iter().map(|x| Value::str(x)).collect();
                let sa = self.new_array(sv.clone());
                let raw = self.new_array(sv);
                if let Value::Obj(o) = &sa {
                    self.set_own(o, "raw", raw);
                }
                let mut argv = vec![sa];
                for e in exprs {
                    argv.push(self.eval_expr(e, scope)?);
                }
                self.call(&f, &this, &argv)
            }
            Expr::Yield { arg, delegate } => {
                let v = match arg {
                    Some(a) => self.eval_expr(a, scope)?,
                    None => Value::Undefined,
                };
                let items = if *delegate { self.iterate(&v)? } else { vec![v] };
                let too_many = match self.yield_stack.last_mut() {
                    Some(buf) => {
                        buf.extend(items);
                        buf.len() > 100_000
                    }
                    None => return Err(self.throw_err("SyntaxError", "yield outside generator")),
                };
                if too_many {
                    return Err(self.throw_err("RangeError", "generator produced too many values (generators are eager in this engine)"));
                }
                Ok(Value::Undefined)
            }
        }
    }

    fn eval_prop_key(&mut self, k: &PropKey, scope: &ScopeRef) -> Res<String> {
        match k {
            PropKey::Name(n) => Ok(n.clone()),
            PropKey::Computed(e) => {
                let v = self.eval_expr(e, scope)?;
                self.to_key(&v)
            }
        }
    }

    fn super_base(&mut self, scope: &ScopeRef) -> Res<Value> {
        match self.lookup(scope, "@@home") {
            Some(Value::Obj(h)) => {
                let p = h.borrow().proto.clone();
                Ok(match p {
                    Some(p) => Value::Obj(p),
                    None => Value::Undefined,
                })
            }
            _ => Err(self.throw_err("SyntaxError", "'super' keyword unexpected here")),
        }
    }

    fn set_this(&mut self, scope: &ScopeRef, val: Value) {
        let mut cur = scope.clone();
        loop {
            let next = {
                let mut s = cur.borrow_mut();
                if s.this_val.is_some() {
                    s.this_val = Some(val);
                    return;
                }
                s.parent.clone()
            };
            match next {
                Some(p) => cur = p,
                None => return,
            }
        }
    }

    fn init_fields(&mut self, cd: &Rc<ClassDef>, cscope: &ScopeRef, this: &Value) -> Res<()> {
        if let Value::Obj(o) = this {
            for m in cd.members.iter() {
                if m.is_static {
                    continue;
                }
                if let MemberKind::Field(init) = &m.kind {
                    let fs = new_scope(Some(cscope.clone()));
                    fs.borrow_mut().this_val = Some(this.clone());
                    let key = self.eval_prop_key(&m.key, &fs)?;
                    let v = match init {
                        Some(e) => self.eval_expr(e, &fs)?,
                        None => Value::Undefined,
                    };
                    self.set_own(o, &key, v);
                }
            }
        }
        Ok(())
    }

    fn eval_class(&mut self, cd: &Rc<ClassDef>, scope: &ScopeRef) -> Res<Value> {
        let cscope = new_scope(Some(scope.clone()));
        let parent = match &cd.extends {
            Some(e) => Some(self.eval_expr(e, scope)?),
            None => None,
        };
        let (proto_parent, ctor_parent): (Option<ObjRef>, ObjRef) = match &parent {
            None => (Some(self.protos.object.clone()), self.protos.function.clone()),
            Some(Value::Null) => (None, self.protos.function.clone()),
            Some(p) if p.is_callable() => {
                let pp = match self.get_prop(p, "prototype")? {
                    Value::Obj(o) => Some(o),
                    _ => None,
                };
                let cp = match p {
                    Value::Obj(po) => po.clone(),
                    _ => self.protos.function.clone(),
                };
                (pp, cp)
            }
            Some(_) => return Err(self.type_err("Class extends value is not a constructor or null")),
        };
        let proto = self.alloc(Kind::Plain, proto_parent);
        let ctor_obj = self.alloc(
            Kind::Closure(Closure { def: cd.ctor.clone(), scope: cscope.clone(), home: Some(proto.clone()), class: Some((cd.clone(), cscope.clone())) }),
            Some(ctor_parent),
        );
        let ctor_val = Value::Obj(ctor_obj.clone());
        self.set_own(&ctor_obj, "prototype", Value::Obj(proto.clone()));
        self.set_own(&proto, "constructor", ctor_val.clone());
        if let Some(n) = &cd.name {
            cscope.borrow_mut().vars.push((n.clone(), ctor_val.clone()));
        }
        for m in cd.members.iter() {
            let target = if m.is_static { ctor_obj.clone() } else { proto.clone() };
            match &m.kind {
                MemberKind::Method(f) | MemberKind::Getter(f) | MemberKind::Setter(f) => {
                    let key = self.eval_prop_key(&m.key, &cscope)?;
                    let c = Value::Obj(self.alloc(
                        Kind::Closure(Closure { def: f.clone(), scope: cscope.clone(), home: Some(target.clone()), class: None }),
                        Some(self.protos.function.clone()),
                    ));
                    match &m.kind {
                        MemberKind::Getter(_) => self.define_accessor(&target, &key, Some(c), None),
                        MemberKind::Setter(_) => self.define_accessor(&target, &key, None, Some(c)),
                        _ => self.set_own(&target, &key, c),
                    }
                }
                MemberKind::Field(init) => {
                    if m.is_static {
                        let fs = new_scope(Some(cscope.clone()));
                        fs.borrow_mut().this_val = Some(ctor_val.clone());
                        let key = self.eval_prop_key(&m.key, &fs)?;
                        let v = match init {
                            Some(e) => self.eval_expr(e, &fs)?,
                            None => Value::Undefined,
                        };
                        self.set_own(&ctor_obj, &key, v);
                    }
                }
                MemberKind::StaticBlock(body) => {
                    let fs = new_scope(Some(cscope.clone()));
                    fs.borrow_mut().this_val = Some(ctor_val.clone());
                    self.hoist_vars(body, &fs);
                    self.hoist_funcs(body, &fs);
                    match self.exec_list(body, &fs) {
                        Ok(()) | Err(Ctl::Return(_)) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
        }
        Ok(ctor_val)
    }

    pub fn make_generator(&mut self, buf: Vec<Value>, ret: Value) -> Value {
        let o = self.new_plain();
        let arr = self.new_array(buf);
        self.set_own(&o, "@@buf", arr);
        self.set_own(&o, "@@i", Value::Num(0.0));
        self.set_own(&o, "@@ret", ret);
        let next = self.new_native(gen_next);
        self.set_own(&o, "next", next);
        let rf = self.new_native(gen_return);
        self.set_own(&o, "return", rf);
        let itf = self.new_native(|_it, this, _a| Ok(this.clone()));
        self.set_own(&o, "@@iterator", itf);
        Value::Obj(o)
    }

    pub fn delete_prop(&mut self, base: &Value, key: &str) {
        if let Value::Obj(o) = base {
            let target = if let Kind::Proxy(t, _) = &o.borrow().kind { Some(t.clone()) } else { None };
            if let Some(t) = target {
                self.delete_prop(&t, key);
                return;
            }
            let mut b = o.borrow_mut();
            if let Kind::Array(a) = &mut b.kind {
                if let Some(i) = parse_index(key) {
                    if i < a.len() {
                        a[i] = Value::Undefined;
                    }
                    return;
                }
            }
            b.props.retain(|(k, _)| k != key);
        }
    }

    // ---------- promises, jobs, timers ----------

    pub fn new_promise(&self) -> ObjRef {
        self.alloc(Kind::Promise(PromData { state: 0, value: Value::Undefined, reactions: Vec::new() }), Some(self.protos.promise.clone()))
    }

    pub fn promise_state(&self, p: &ObjRef) -> Option<(u8, Value)> {
        match &p.borrow().kind {
            Kind::Promise(d) => Some((d.state, d.value.clone())),
            _ => None,
        }
    }

    pub fn settle(&mut self, p: &ObjRef, state: u8, value: Value) {
        let reactions = {
            let mut b = p.borrow_mut();
            match &mut b.kind {
                Kind::Promise(d) if d.state == 0 => {
                    d.state = state;
                    d.value = value.clone();
                    core::mem::take(&mut d.reactions)
                }
                _ => return,
            }
        };
        for r in reactions {
            self.microtasks.push_back(Job::React(r, state, value.clone()));
        }
    }

    pub fn resolve_with(&mut self, p: &ObjRef, v: Value) {
        if let Value::Obj(vo) = &v {
            if Rc::ptr_eq(vo, p) {
                let e = self.make_error("TypeError", "Chaining cycle detected for promise");
                self.settle(p, 2, e);
                return;
            }
            if self.promise_state(vo).is_some() {
                self.subscribe(vo, Reaction { on_ful: Value::Undefined, on_rej: Value::Undefined, child: Some(p.clone()) });
                return;
            }
        }
        self.settle(p, 1, v);
    }

    pub fn subscribe(&mut self, p: &ObjRef, r: Reaction) {
        let settled = {
            let b = p.borrow();
            match &b.kind {
                Kind::Promise(d) => {
                    if d.state == 0 {
                        None
                    } else {
                        Some((d.state, d.value.clone()))
                    }
                }
                _ => None,
            }
        };
        match settled {
            Some((st, val)) => self.microtasks.push_back(Job::React(r, st, val)),
            None => {
                if let Kind::Promise(d) = &mut p.borrow_mut().kind {
                    d.reactions.push(r);
                }
            }
        }
    }

    fn run_reaction(&mut self, r: Reaction, state: u8, value: Value) {
        let handler = if state == 1 { r.on_ful.clone() } else { r.on_rej.clone() };
        if handler.is_callable() {
            let res = self.call(&handler, &Value::Undefined, &[value]);
            if let Some(child) = &r.child {
                match res {
                    Ok(v) => self.resolve_with(child, v),
                    Err(Ctl::Throw(e)) => self.settle(child, 2, e),
                    Err(_) => {}
                }
            }
        } else if let Some(child) = &r.child {
            if state == 1 {
                self.settle(child, 1, value);
            } else {
                self.settle(child, 2, value);
            }
        }
    }

    pub fn run_microtasks(&mut self) {
        let mut guard = 0;
        while let Some(job) = self.microtasks.pop_front() {
            guard += 1;
            if guard > 100_000 {
                break;
            }
            match job {
                Job::React(r, st, v) => self.run_reaction(r, st, v),
                Job::Call(f, args) => {
                    let _ = self.call(&f, &Value::Undefined, &args);
                }
            }
        }
    }

    pub fn add_timer(&mut self, f: Value, delay: f64, repeat: bool, args: Vec<Value>) -> u32 {
        let id = self.next_timer_id;
        self.next_timer_id += 1;
        let d = if delay.is_nan() || delay < 0.0 { 0.0 } else { delay };
        self.timers.push(Timer { id, due: self.now_ms + d, interval: if repeat { Some(if d < 1.0 { 1.0 } else { d }) } else { None }, f, args });
        id
    }

    pub fn clear_timer(&mut self, id: u32) {
        self.timers.retain(|t| t.id != id);
    }

    fn pop_due_timer(&mut self, limit: f64) -> Option<Timer> {
        let mut best: Option<usize> = None;
        for (i, t) in self.timers.iter().enumerate() {
            if t.due <= limit {
                match best {
                    Some(b) if self.timers[b].due <= t.due => {}
                    _ => best = Some(i),
                }
            }
        }
        let i = best?;
        let t = self.timers.remove(i);
        if let Some(iv) = t.interval {
            self.timers.push(Timer { id: t.id, due: t.due + iv, interval: t.interval, f: t.f.clone(), args: t.args.clone() });
        }
        Some(t)
    }

    pub fn run_timers(&mut self, now_ms: f64, max_jobs: usize) -> usize {
        if now_ms > self.now_ms {
            self.now_ms = now_ms;
        }
        let mut n = 0;
        while n < max_jobs {
            let lim = self.now_ms;
            match self.pop_due_timer(lim) {
                Some(t) => {
                    let _ = self.call(&t.f, &Value::Undefined, &t.args);
                    self.run_microtasks();
                    n += 1;
                }
                None => break,
            }
        }
        n
    }

    fn run_next_timer(&mut self) -> bool {
        let mut earliest: Option<f64> = None;
        for t in self.timers.iter() {
            earliest = Some(match earliest {
                Some(e) if e <= t.due => e,
                _ => t.due,
            });
        }
        match earliest {
            Some(d) => {
                if d > self.now_ms {
                    self.now_ms = d;
                }
                self.run_timers(d, 1) > 0
            }
            None => false,
        }
    }

    pub fn call_global(&mut self, name: &str, args: &[Value]) -> Res<Value> {
        let f = self.get_own_chain(&self.global, name).unwrap_or(Value::Undefined);
        if f.is_callable() {
            self.call(&f, &Value::Undefined, args)
        } else {
            Ok(Value::Undefined)
        }
    }

    pub fn set_global(&mut self, name: &str, v: Value) {
        let g = self.global.clone();
        self.set_own(&g, name, v);
    }

    pub fn await_value(&mut self, v: Value) -> Res<Value> {
        let prom = match &v {
            Value::Obj(o) if self.promise_state(o).is_some() => o.clone(),
            _ => return Ok(v),
        };
        let mut spins = 0;
        loop {
            self.run_microtasks();
            if let Some((st, val)) = self.promise_state(&prom) {
                if st == 1 {
                    return Ok(val);
                }
                if st == 2 {
                    return Err(Ctl::Throw(val));
                }
            }
            spins += 1;
            let limit = if self.pump.is_some() { 5_000_000 } else { 10_000 };
            if spins > limit {
                return Err(self.throw_err("Error", "await: promise never settled"));
            }
            if self.run_next_timer() {
                continue;
            }
            match self.pump {
                Some(pump) => {
                    if !pump(self) {
                        self.run_microtasks();
                        if let Some((st, val)) = self.promise_state(&prom) {
                            if st == 1 {
                                return Ok(val);
                            }
                            if st == 2 {
                                return Err(Ctl::Throw(val));
                            }
                        }
                        return Err(self.throw_err("Error", "await: promise never settled"));
                    }
                }
                None => return Err(self.throw_err("Error", "await: promise never settled")),
            }
        }
    }

    pub fn inspect(&mut self, v: &Value, depth: usize) -> String {
        match v {
            Value::Str(s) => {
                if depth == 0 {
                    String::from(&**s)
                } else {
                    format!("'{}'", s)
                }
            }
            Value::Obj(o) => {
                if v.is_callable() {
                    return String::from("[Function]");
                }
                if depth > 2 {
                    return String::from("[Object]");
                }
                let arr = if let Kind::Array(a) = &o.borrow().kind { Some(a.clone()) } else { None };
                if let Some(a) = arr {
                    let parts: Vec<String> = a.iter().map(|x| self.inspect(x, depth + 1)).collect();
                    return format!("[ {} ]", parts.join(", "));
                }
                let is_err = self.get_own_chain(o, "message").is_some() && self.get_own_chain(o, "name").is_some();
                if is_err {
                    return self.describe_error(v);
                }
                let keys = self.own_keys(v);
                let mut parts = Vec::new();
                for k in keys {
                    let pv = self.get_own_chain(o, &k).unwrap_or(Value::Undefined);
                    parts.push(format!("{}: {}", k, self.inspect(&pv, depth + 1)));
                }
                if parts.is_empty() {
                    String::from("{}")
                } else {
                    format!("{{ {} }}", parts.join(", "))
                }
            }
            other => self.to_str(other).unwrap_or_default(),
        }
    }
}

/// Source-like name of a callee for error messages (`a.b.c`, `x[...]`, `f(...)`, `new X`, ...).
/// "expression" alone told us nothing when a bundled script failed 2 MB into a minified file.
fn describe_callee(e: &Expr) -> String {
    match e {
        Expr::Ident(n) => n.clone(),
        Expr::Member { obj, prop, .. } => format!("{}.{}", describe_callee(obj), prop),
        Expr::Index { obj, .. } => format!("{}[...]", describe_callee(obj)),
        Expr::Call { callee, .. } => format!("{}(...)", describe_callee(callee)),
        Expr::New { callee, .. } => format!("new {}", describe_callee(callee)),
        Expr::This => String::from("this"),
        Expr::Str(_) => String::from("<string>"),
        Expr::Num(_) => String::from("<number>"),
        Expr::Array(_) => String::from("[...]"),
        Expr::Object(_) => String::from("{...}"),
        Expr::Func(_) => String::from("(function)"),
        Expr::Class(_) => String::from("(class)"),
        Expr::Tpl(..) => String::from("<template>"),
        Expr::Logical(..) | Expr::Cond(..) | Expr::Seq(_) => String::from("(...)"),
        _ => String::from("expression"),
    }
}

#[allow(dead_code)]
fn _unused(_: Box<u8>) {}

fn gen_next(it: &mut Interp, this: &Value, _a: &[Value]) -> Res<Value> {
    let o = match this {
        Value::Obj(o) => o.clone(),
        _ => return Err(it.type_err("not a generator")),
    };
    let i = match it.get_own_chain(&o, "@@i") {
        Some(Value::Num(n)) => n as usize,
        _ => 0,
    };
    let buf = it.get_own_chain(&o, "@@buf").unwrap_or(Value::Undefined);
    let (len, item) = match &buf {
        Value::Obj(b) => match &b.borrow().kind {
            Kind::Array(a) => (a.len(), a.get(i).cloned()),
            _ => (0, None),
        },
        _ => (0, None),
    };
    let res = it.new_plain();
    if i < len {
        it.set_own(&o, "@@i", Value::Num((i + 1) as f64));
        it.set_own(&res, "value", item.unwrap_or(Value::Undefined));
        it.set_own(&res, "done", Value::Bool(false));
    } else {
        let v = if i == len { it.get_own_chain(&o, "@@ret").unwrap_or(Value::Undefined) } else { Value::Undefined };
        it.set_own(&o, "@@i", Value::Num((len + 1) as f64));
        it.set_own(&res, "value", v);
        it.set_own(&res, "done", Value::Bool(true));
    }
    Ok(Value::Obj(res))
}

fn gen_return(it: &mut Interp, this: &Value, a: &[Value]) -> Res<Value> {
    if let Value::Obj(o) = this {
        it.set_own(o, "@@i", Value::Num(f64::MAX / 4.0));
    }
    let res = it.new_plain();
    it.set_own(&res, "value", a.get(0).cloned().unwrap_or(Value::Undefined));
    it.set_own(&res, "done", Value::Bool(true));
    Ok(Value::Obj(res))
}
