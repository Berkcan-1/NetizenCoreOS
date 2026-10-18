use crate::lexer::{tokenize, Tok, Token};
use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    Var,
    Let,
    Const,
}

pub enum PropKey {
    Name(String),
    Computed(Expr),
}

pub struct PatElem {
    pub pat: Pattern,
    pub default: Option<Expr>,
    pub rest: bool,
}

pub type Param = PatElem;

pub enum Pattern {
    Ident(String),
    Array(Vec<Option<PatElem>>),
    Object(Vec<(PropKey, PatElem)>, Option<String>),
}

pub struct FuncDef {
    pub name: Option<String>,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub is_arrow: bool,
    pub is_async: bool,
    pub is_generator: bool,
    pub is_ctor: bool,
    pub is_derived: bool,
}

pub enum MemberKind {
    Method(Rc<FuncDef>),
    Getter(Rc<FuncDef>),
    Setter(Rc<FuncDef>),
    Field(Option<Expr>),
    StaticBlock(Vec<Stmt>),
}

pub struct ClassMember {
    pub key: PropKey,
    pub is_static: bool,
    pub kind: MemberKind,
}

pub struct ClassDef {
    pub name: Option<String>,
    pub extends: Option<Expr>,
    pub ctor: Rc<FuncDef>,
    pub members: Vec<ClassMember>,
}

pub enum PropDef {
    KeyVal(PropKey, Expr),
    Getter(PropKey, Rc<FuncDef>),
    Setter(PropKey, Rc<FuncDef>),
    Spread(Expr),
}

pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
    This,
    Hole,
    Ident(String),
    Tpl(Vec<String>, Vec<Expr>),
    Regex(String, String),
    Array(Vec<Expr>),
    Object(Vec<PropDef>),
    Func(Rc<FuncDef>),
    Unary(String, Box<Expr>),
    Update { op: String, prefix: bool, target: Box<Expr> },
    Binary(String, Box<Expr>, Box<Expr>),
    Logical(String, Box<Expr>, Box<Expr>),
    Assign { op: String, target: Box<Expr>, value: Box<Expr> },
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Call { callee: Box<Expr>, args: Vec<Expr>, optional: bool },
    New { callee: Box<Expr>, args: Vec<Expr> },
    Member { obj: Box<Expr>, prop: String, optional: bool },
    Index { obj: Box<Expr>, idx: Box<Expr>, optional: bool },
    Seq(Vec<Expr>),
    Spread(Box<Expr>),
    Await(Box<Expr>),
    Class(Rc<ClassDef>),
    SuperCall(Vec<Expr>),
    SuperMember(Box<Expr>),
    TaggedTpl { tag: Box<Expr>, strs: Vec<String>, exprs: Vec<Expr> },
    Yield { arg: Option<Box<Expr>>, delegate: bool },
}

pub enum ForTarget {
    Pat(Pattern),
    Expr(Expr),
}

pub enum Stmt {
    Empty,
    Expr(Expr),
    Var(VarKind, Vec<(Pattern, Option<Expr>)>),
    FuncDecl(Rc<FuncDef>),
    Return(Option<Expr>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    Block(Vec<Stmt>),
    For { init: Option<Box<Stmt>>, test: Option<Expr>, update: Option<Expr>, body: Box<Stmt> },
    ForIn { kind: Option<VarKind>, target: ForTarget, obj: Expr, body: Box<Stmt>, is_of: bool },
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    Break(Option<String>),
    Continue(Option<String>),
    Throw(Expr),
    Try { block: Vec<Stmt>, param: Option<Pattern>, handler: Option<Vec<Stmt>>, finalizer: Option<Vec<Stmt>> },
    Switch(Expr, Vec<(Option<Expr>, Vec<Stmt>)>),
    Labeled(String, Box<Stmt>),
    ClassDecl(Rc<ClassDef>),
}

pub fn pattern_names(p: &Pattern, out: &mut Vec<String>) {
    match p {
        Pattern::Ident(n) => out.push(n.clone()),
        Pattern::Array(items) => {
            for it in items.iter().flatten() {
                pattern_names(&it.pat, out);
            }
        }
        Pattern::Object(items, rest) => {
            for (_, el) in items.iter() {
                pattern_names(&el.pat, out);
            }
            if let Some(r) = rest {
                out.push(r.clone());
            }
        }
    }
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    no_in: bool,
    depth: usize,
    in_gen: bool,
}

const MAX_PARSE_DEPTH: usize = 120;

fn is_reserved(s: &str) -> bool {
    matches!(
        s,
        "function" | "return" | "if" | "else" | "for" | "while" | "do" | "break" | "continue" | "new" | "delete" | "typeof" | "void" | "in" | "instanceof" | "this" | "null" | "true" | "false" | "throw" | "try" | "catch" | "finally" | "switch" | "case" | "default" | "class" | "const" | "var" | "with"
    )
}

fn binop_prec(p: &str) -> Option<u8> {
    Some(match p {
        "??" => 1,
        "||" => 2,
        "&&" => 3,
        "|" => 4,
        "^" => 5,
        "&" => 6,
        "==" | "!=" | "===" | "!==" => 7,
        "<" | ">" | "<=" | ">=" => 8,
        "<<" | ">>" | ">>>" => 9,
        "+" | "-" => 10,
        "*" | "/" | "%" => 11,
        "**" => 12,
        _ => return None,
    })
}

fn is_assign_op(p: &str) -> bool {
    matches!(
        p,
        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "**=" | "<<=" | ">>=" | ">>>=" | "&=" | "|=" | "^=" | "&&=" | "||=" | "??="
    )
}

pub fn parse_program(src: &str) -> Result<Vec<Stmt>, String> {
    let toks = tokenize(src)?;
    let mut p = Parser { toks, pos: 0, no_in: false, depth: 0, in_gen: false };
    let mut out = Vec::new();
    while !p.at_eof() {
        match p.statement() {
            Ok(s) => out.push(s),
            Err(e) => return Err(add_context(e, src)),
        }
    }
    Ok(out)
}

fn add_context(e: String, src: &str) -> String {
    if let Some(i) = e.rfind("[pos=") {
        let num: String = e[i + 5..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(p) = num.parse::<usize>() {
            let start = p.saturating_sub(70);
            let snippet: String = src.chars().skip(start).take(140).map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
            return format!("{} at char {} near: ...{}...", e[..i].trim_end(), p, snippet);
        }
    }
    e
}

pub fn parse_expression_source(src: &str) -> Result<Expr, String> {
    let toks = tokenize(src)?;
    let mut p = Parser { toks, pos: 0, no_in: false, depth: 0, in_gen: false };
    p.expression()
}

impl Parser {
    fn peek(&self) -> &Tok {
        let i = if self.pos < self.toks.len() { self.pos } else { self.toks.len() - 1 };
        &self.toks[i].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        let i = self.pos + n;
        let i = if i < self.toks.len() { i } else { self.toks.len() - 1 };
        &self.toks[i].tok
    }

    fn nl_before(&self) -> bool {
        let i = if self.pos < self.toks.len() { self.pos } else { self.toks.len() - 1 };
        self.toks[i].nl
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), Tok::Eof)
    }

    fn advance(&mut self) -> Tok {
        let t = self.peek().clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn is_p(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Punct(p) if p == s)
    }

    fn is_p_at(&self, n: usize, s: &str) -> bool {
        matches!(self.peek_at(n), Tok::Punct(p) if p == s)
    }

    fn is_kw(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Ident(p) if p == s)
    }

    fn eat_p(&mut self, s: &str) -> bool {
        if self.is_p(s) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, s: &str) -> bool {
        if self.is_kw(s) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn err<T>(&self, msg: &str) -> Result<T, String> {
        let i = if self.pos < self.toks.len() { self.pos } else { self.toks.len() - 1 };
        Err(format!("SyntaxError: {} (token {:?}) [pos={}]", msg, self.peek(), self.toks[i].pos))
    }

    fn expect_p(&mut self, s: &str) -> Result<(), String> {
        if self.eat_p(s) {
            Ok(())
        } else {
            self.err(&format!("expected '{}'", s))
        }
    }

    fn semi(&mut self) -> Result<(), String> {
        if self.eat_p(";") {
            return Ok(());
        }
        if self.is_p("}") || self.at_eof() || self.nl_before() {
            return Ok(());
        }
        self.err("expected ';'")
    }

    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            return self.err("nesting too deep");
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn ident_name(&mut self) -> Result<String, String> {
        match self.advance() {
            Tok::Ident(s) => Ok(s),
            _ => {
                self.pos = self.pos.saturating_sub(1);
                self.err("expected identifier")
            }
        }
    }

    fn binding_ident(&mut self) -> Result<String, String> {
        match self.peek().clone() {
            Tok::Ident(s) if !is_reserved(&s) => {
                self.advance();
                Ok(s)
            }
            _ => self.err("expected binding identifier"),
        }
    }

    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect_p("{")?;
        self.enter()?;
        let mut out = Vec::new();
        while !self.is_p("}") {
            if self.at_eof() {
                return self.err("unexpected end of input in block");
            }
            out.push(self.statement()?);
        }
        self.advance();
        self.leave();
        Ok(out)
    }

    fn statement(&mut self) -> Result<Stmt, String> {
        self.enter()?;
        let r = self.statement_inner();
        self.leave();
        r
    }

    fn statement_inner(&mut self) -> Result<Stmt, String> {
        let t = self.peek().clone();
        match t {
            Tok::Punct(p) if p == "{" => Ok(Stmt::Block(self.block()?)),
            Tok::Punct(p) if p == ";" => {
                self.advance();
                Ok(Stmt::Empty)
            }
            Tok::Ident(k) => match k.as_str() {
                "var" => {
                    let s = self.var_decl(VarKind::Var)?;
                    self.semi()?;
                    Ok(s)
                }
                "const" => {
                    let s = self.var_decl(VarKind::Const)?;
                    self.semi()?;
                    Ok(s)
                }
                "let" if matches!(self.peek_at(1), Tok::Ident(_)) || self.is_p_at(1, "[") || self.is_p_at(1, "{") => {
                    let s = self.var_decl(VarKind::Let)?;
                    self.semi()?;
                    Ok(s)
                }
                "function" => {
                    self.advance();
                    self.func_decl(false)
                }
                "async" if matches!(self.peek_at(1), Tok::Ident(f) if f == "function") => {
                    self.advance();
                    self.advance();
                    self.func_decl(true)
                }
                "debugger" => {
                    self.advance();
                    self.semi()?;
                    Ok(Stmt::Empty)
                }
                "class" => {
                    self.advance();
                    let d = self.class_def()?;
                    if d.name.is_none() {
                        return self.err("class declaration requires a name");
                    }
                    Ok(Stmt::ClassDecl(d))
                }
                "if" => {
                    self.advance();
                    self.expect_p("(")?;
                    let c = self.expression()?;
                    self.expect_p(")")?;
                    let a = self.statement()?;
                    let b = if self.eat_kw("else") { Some(Box::new(self.statement()?)) } else { None };
                    Ok(Stmt::If(c, Box::new(a), b))
                }
                "for" => self.for_stmt(),
                "while" => {
                    self.advance();
                    self.expect_p("(")?;
                    let c = self.expression()?;
                    self.expect_p(")")?;
                    let b = self.statement()?;
                    Ok(Stmt::While(c, Box::new(b)))
                }
                "do" => {
                    self.advance();
                    let b = self.statement()?;
                    if !self.eat_kw("while") {
                        return self.err("expected 'while'");
                    }
                    self.expect_p("(")?;
                    let c = self.expression()?;
                    self.expect_p(")")?;
                    self.eat_p(";");
                    Ok(Stmt::DoWhile(Box::new(b), c))
                }
                "return" => {
                    self.advance();
                    let v = if self.is_p(";") || self.is_p("}") || self.at_eof() || self.nl_before() {
                        None
                    } else {
                        Some(self.expression()?)
                    };
                    self.semi()?;
                    Ok(Stmt::Return(v))
                }
                "break" | "continue" => {
                    self.advance();
                    let mut label = None;
                    if !self.nl_before() {
                        if let Tok::Ident(l) = self.peek().clone() {
                            if !is_reserved(&l) {
                                self.advance();
                                label = Some(l);
                            }
                        }
                    }
                    self.semi()?;
                    if k == "break" {
                        Ok(Stmt::Break(label))
                    } else {
                        Ok(Stmt::Continue(label))
                    }
                }
                "throw" => {
                    self.advance();
                    let e = self.expression()?;
                    self.semi()?;
                    Ok(Stmt::Throw(e))
                }
                "try" => {
                    self.advance();
                    let block = self.block()?;
                    let mut param = None;
                    let mut handler = None;
                    let mut finalizer = None;
                    if self.eat_kw("catch") {
                        if self.eat_p("(") {
                            param = Some(self.binding_pattern()?);
                            self.expect_p(")")?;
                        }
                        handler = Some(self.block()?);
                    }
                    if self.eat_kw("finally") {
                        finalizer = Some(self.block()?);
                    }
                    if handler.is_none() && finalizer.is_none() {
                        return self.err("try without catch or finally");
                    }
                    Ok(Stmt::Try { block, param, handler, finalizer })
                }
                "switch" => {
                    self.advance();
                    self.expect_p("(")?;
                    let d = self.expression()?;
                    self.expect_p(")")?;
                    self.expect_p("{")?;
                    let mut cases: Vec<(Option<Expr>, Vec<Stmt>)> = Vec::new();
                    while !self.is_p("}") {
                        let test = if self.eat_kw("case") {
                            Some(self.expression()?)
                        } else if self.eat_kw("default") {
                            None
                        } else {
                            return self.err("expected case or default");
                        };
                        self.expect_p(":")?;
                        let mut body = Vec::new();
                        while !self.is_p("}") && !self.is_kw("case") && !self.is_kw("default") {
                            if self.at_eof() {
                                return self.err("unexpected end of input in switch");
                            }
                            body.push(self.statement()?);
                        }
                        cases.push((test, body));
                    }
                    self.advance();
                    Ok(Stmt::Switch(d, cases))
                }
                _ => {
                    if self.is_p_at(1, ":") && !is_reserved(&k) {
                        self.advance();
                        self.advance();
                        let inner = self.statement()?;
                        return Ok(Stmt::Labeled(k.clone(), Box::new(inner)));
                    }
                    let e = self.expression()?;
                    self.semi()?;
                    Ok(Stmt::Expr(e))
                }
            },
            _ => {
                let e = self.expression()?;
                self.semi()?;
                Ok(Stmt::Expr(e))
            }
        }
    }

    fn var_decl(&mut self, kind: VarKind) -> Result<Stmt, String> {
        self.advance();
        let mut decls: Vec<(Pattern, Option<Expr>)> = Vec::new();
        loop {
            let pat = self.binding_pattern()?;
            let init = if self.eat_p("=") { Some(self.assign_expr()?) } else { None };
            decls.push((pat, init));
            if !self.eat_p(",") {
                break;
            }
        }
        Ok(Stmt::Var(kind, decls))
    }

    fn func_decl(&mut self, is_async: bool) -> Result<Stmt, String> {
        let is_gen = self.eat_p("*");
        let name = self.binding_ident()?;
        let def = self.function_ex(Some(name), is_async, is_gen, false, false)?;
        Ok(Stmt::FuncDecl(def))
    }

    fn for_stmt(&mut self) -> Result<Stmt, String> {
        self.advance();
        self.eat_kw("await");
        self.expect_p("(")?;
        let mut init: Option<Box<Stmt>> = None;
        if self.is_p(";") {
        } else {
            let kind = if self.is_kw("var") {
                Some(VarKind::Var)
            } else if self.is_kw("const") {
                Some(VarKind::Const)
            } else if self.is_kw("let") && (matches!(self.peek_at(1), Tok::Ident(_)) || self.is_p_at(1, "[") || self.is_p_at(1, "{")) {
                Some(VarKind::Let)
            } else {
                None
            };
            let saved = self.no_in;
            self.no_in = true;
            if let Some(k) = kind {
                self.advance();
                let pat = self.binding_pattern()?;
                if self.is_kw("of") || self.is_kw("in") {
                    let is_of = self.is_kw("of");
                    self.advance();
                    self.no_in = saved;
                    let obj = if is_of { self.assign_expr()? } else { self.expression()? };
                    self.expect_p(")")?;
                    let body = self.statement()?;
                    return Ok(Stmt::ForIn { kind: Some(k), target: ForTarget::Pat(pat), obj, body: Box::new(body), is_of });
                }
                let mut decls: Vec<(Pattern, Option<Expr>)> = Vec::new();
                let first_init = if self.eat_p("=") { Some(self.assign_expr()?) } else { None };
                decls.push((pat, first_init));
                while self.eat_p(",") {
                    let p2 = self.binding_pattern()?;
                    let i2 = if self.eat_p("=") { Some(self.assign_expr()?) } else { None };
                    decls.push((p2, i2));
                }
                init = Some(Box::new(Stmt::Var(k, decls)));
            } else {
                let e = self.expression()?;
                if self.is_kw("of") || self.is_kw("in") {
                    let is_of = self.is_kw("of");
                    self.advance();
                    self.no_in = saved;
                    let obj = if is_of { self.assign_expr()? } else { self.expression()? };
                    self.expect_p(")")?;
                    let body = self.statement()?;
                    return Ok(Stmt::ForIn { kind: None, target: ForTarget::Expr(e), obj, body: Box::new(body), is_of });
                }
                init = Some(Box::new(Stmt::Expr(e)));
            }
            self.no_in = saved;
        }
        self.expect_p(";")?;
        let test = if self.is_p(";") { None } else { Some(self.expression()?) };
        self.expect_p(";")?;
        let update = if self.is_p(")") { None } else { Some(self.expression()?) };
        self.expect_p(")")?;
        let body = self.statement()?;
        Ok(Stmt::For { init, test, update, body: Box::new(body) })
    }

    fn prop_key(&mut self) -> Result<PropKey, String> {
        match self.advance() {
            Tok::Ident(s) => Ok(PropKey::Name(s)),
            Tok::Str(s) => Ok(PropKey::Name(s)),
            Tok::Num(n) => Ok(PropKey::Name(crate::interp::num_to_str(n))),
            Tok::Punct(p) if p == "[" => {
                let saved = self.no_in;
                self.no_in = false;
                let e = self.assign_expr()?;
                self.no_in = saved;
                self.expect_p("]")?;
                Ok(PropKey::Computed(e))
            }
            _ => {
                self.pos = self.pos.saturating_sub(1);
                self.err("expected property name")
            }
        }
    }

    fn binding_pattern(&mut self) -> Result<Pattern, String> {
        self.enter()?;
        let r = self.binding_pattern_inner();
        self.leave();
        r
    }

    fn pat_elem(&mut self) -> Result<PatElem, String> {
        let pat = self.binding_pattern()?;
        let default = if self.eat_p("=") { Some(self.assign_expr()?) } else { None };
        Ok(PatElem { pat, default, rest: false })
    }

    fn binding_pattern_inner(&mut self) -> Result<Pattern, String> {
        if self.is_p("[") {
            self.advance();
            let mut items: Vec<Option<PatElem>> = Vec::new();
            loop {
                if self.eat_p("]") {
                    break;
                }
                if self.is_p(",") {
                    self.advance();
                    items.push(None);
                    continue;
                }
                if self.eat_p("...") {
                    let pat = self.binding_pattern()?;
                    items.push(Some(PatElem { pat, default: None, rest: true }));
                    self.eat_p(",");
                    continue;
                }
                let el = self.pat_elem()?;
                items.push(Some(el));
                if !self.is_p("]") {
                    self.expect_p(",")?;
                }
            }
            return Ok(Pattern::Array(items));
        }
        if self.is_p("{") {
            self.advance();
            let mut items: Vec<(PropKey, PatElem)> = Vec::new();
            let mut rest: Option<String> = None;
            loop {
                if self.eat_p("}") {
                    break;
                }
                if self.eat_p("...") {
                    rest = Some(self.binding_ident()?);
                    self.eat_p(",");
                    continue;
                }
                let key = self.prop_key()?;
                if self.eat_p(":") {
                    let el = self.pat_elem()?;
                    items.push((key, el));
                } else {
                    let name = match &key {
                        PropKey::Name(n) => n.clone(),
                        _ => return self.err("invalid shorthand pattern"),
                    };
                    let default = if self.eat_p("=") { Some(self.assign_expr()?) } else { None };
                    items.push((key, PatElem { pat: Pattern::Ident(name), default, rest: false }));
                }
                if !self.is_p("}") {
                    self.expect_p(",")?;
                }
            }
            return Ok(Pattern::Object(items, rest));
        }
        let name = self.binding_ident()?;
        Ok(Pattern::Ident(name))
    }

    fn params(&mut self) -> Result<Vec<Param>, String> {
        self.expect_p("(")?;
        let mut out = Vec::new();
        loop {
            if self.eat_p(")") {
                break;
            }
            if self.eat_p("...") {
                let pat = self.binding_pattern()?;
                out.push(PatElem { pat, default: None, rest: true });
                self.eat_p(",");
                continue;
            }
            out.push(self.pat_elem()?);
            if !self.is_p(")") {
                self.expect_p(",")?;
            }
        }
        Ok(out)
    }

    fn function_rest(&mut self, name: Option<String>, is_async: bool) -> Result<Rc<FuncDef>, String> {
        self.function_ex(name, is_async, false, false, false)
    }

    fn function_ex(&mut self, name: Option<String>, is_async: bool, is_gen: bool, is_ctor: bool, is_derived: bool) -> Result<Rc<FuncDef>, String> {
        let saved = self.no_in;
        let saved_gen = self.in_gen;
        self.no_in = false;
        self.in_gen = is_gen;
        let params = self.params()?;
        let body = self.block()?;
        self.no_in = saved;
        self.in_gen = saved_gen;
        Ok(Rc::new(FuncDef { name, params, body, is_arrow: false, is_async, is_generator: is_gen, is_ctor, is_derived }))
    }

    fn class_def(&mut self) -> Result<Rc<ClassDef>, String> {
        let name = match self.peek().clone() {
            Tok::Ident(n) if n != "extends" && !is_reserved(&n) => {
                self.advance();
                Some(n)
            }
            _ => None,
        };
        let extends = if self.eat_kw("extends") { Some(self.lhs()?) } else { None };
        let derived = extends.is_some();
        self.expect_p("{")?;
        let mut ctor: Option<Rc<FuncDef>> = None;
        let mut members: Vec<ClassMember> = Vec::new();
        loop {
            if self.eat_p("}") {
                break;
            }
            if self.eat_p(";") {
                continue;
            }
            if self.at_eof() {
                return self.err("unexpected end of input in class body");
            }
            let mut is_static = false;
            let ends = |p: &Parser| p.is_p_at(1, "(") || p.is_p_at(1, "=") || p.is_p_at(1, ";") || p.is_p_at(1, "}");
            if self.is_kw("static") && !ends(self) {
                self.advance();
                is_static = true;
                if self.is_p("{") {
                    let saved_gen = self.in_gen;
                    self.in_gen = false;
                    let body = self.block()?;
                    self.in_gen = saved_gen;
                    members.push(ClassMember { key: PropKey::Name(String::new()), is_static: true, kind: MemberKind::StaticBlock(body) });
                    continue;
                }
            }
            let mut is_async = false;
            let mut accessor = 0u8;
            if let Tok::Ident(k) = self.peek().clone() {
                if !ends(self) {
                    if k == "async" {
                        self.advance();
                        is_async = true;
                    } else if k == "get" {
                        self.advance();
                        accessor = 1;
                    } else if k == "set" {
                        self.advance();
                        accessor = 2;
                    }
                }
            }
            let is_gen = self.eat_p("*");
            let key = self.prop_key()?;
            if self.is_p("(") {
                let is_ctor = !is_static && accessor == 0 && matches!(&key, PropKey::Name(n) if n == "constructor");
                let f = self.function_ex(None, is_async, is_gen, is_ctor, is_ctor && derived)?;
                if is_ctor {
                    ctor = Some(f);
                } else if accessor == 1 {
                    members.push(ClassMember { key, is_static, kind: MemberKind::Getter(f) });
                } else if accessor == 2 {
                    members.push(ClassMember { key, is_static, kind: MemberKind::Setter(f) });
                } else {
                    members.push(ClassMember { key, is_static, kind: MemberKind::Method(f) });
                }
            } else {
                let init = if self.eat_p("=") {
                    let saved = self.no_in;
                    self.no_in = false;
                    let e = self.assign_expr()?;
                    self.no_in = saved;
                    Some(e)
                } else {
                    None
                };
                self.semi()?;
                members.push(ClassMember { key, is_static, kind: MemberKind::Field(init) });
            }
        }
        let ctor = match ctor {
            Some(c) => c,
            None => {
                let (params, body) = if derived {
                    (
                        vec![PatElem { pat: Pattern::Ident(String::from("args")), default: None, rest: true }],
                        vec![Stmt::Expr(Expr::SuperCall(vec![Expr::Spread(Box::new(Expr::Ident(String::from("args"))))]))],
                    )
                } else {
                    (Vec::new(), Vec::new())
                };
                Rc::new(FuncDef { name: None, params, body, is_arrow: false, is_async: false, is_generator: false, is_ctor: true, is_derived: derived })
            }
        };
        Ok(Rc::new(ClassDef { name, extends, ctor, members }))
    }

    pub fn expression(&mut self) -> Result<Expr, String> {
        let first = self.assign_expr()?;
        if self.is_p(",") {
            let mut items = vec![first];
            while self.eat_p(",") {
                items.push(self.assign_expr()?);
            }
            return Ok(Expr::Seq(items));
        }
        Ok(first)
    }

    fn arrow_after_paren(&self, off: usize) -> bool {
        let mut depth = 0i32;
        let mut i = self.pos + off;
        while i < self.toks.len() {
            match &self.toks[i].tok {
                Tok::Punct(p) if p == "(" => depth += 1,
                Tok::Punct(p) if p == ")" => {
                    depth -= 1;
                    if depth == 0 {
                        return match self.toks.get(i + 1) {
                            Some(t) => matches!(&t.tok, Tok::Punct(q) if q == "=>") && !t.nl,
                            None => false,
                        };
                    }
                }
                Tok::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn try_arrow(&mut self) -> Result<Option<Expr>, String> {
        let is_async = self.is_kw("async")
            && !self.toks.get(self.pos + 1).map(|t| t.nl).unwrap_or(false)
            && ((matches!(self.peek_at(1), Tok::Ident(_)) && self.is_p_at(2, "=>"))
                || (self.is_p_at(1, "(") && self.arrow_after_paren(1)));
        let off = if is_async { 1 } else { 0 };
        let single = matches!(self.peek_at(off), Tok::Ident(n) if !is_reserved(n)) && self.is_p_at(off + 1, "=>");
        let paren = self.is_p_at(off, "(") && self.arrow_after_paren(off);
        if !single && !paren {
            return Ok(None);
        }
        if is_async {
            self.advance();
        }
        let params: Vec<Param> = if single {
            let n = self.binding_ident()?;
            vec![PatElem { pat: Pattern::Ident(n), default: None, rest: false }]
        } else {
            let saved = self.no_in;
            self.no_in = false;
            let p = self.params()?;
            self.no_in = saved;
            p
        };
        self.expect_p("=>")?;
        let body: Vec<Stmt> = if self.is_p("{") {
            let saved = self.no_in;
            self.no_in = false;
            let b = self.block()?;
            self.no_in = saved;
            b
        } else {
            let e = self.assign_expr()?;
            vec![Stmt::Return(Some(e))]
        };
        Ok(Some(Expr::Func(Rc::new(FuncDef { name: None, params, body, is_arrow: true, is_async, is_generator: false, is_ctor: false, is_derived: false }))))
    }

    fn assign_expr(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let r = self.assign_inner();
        self.leave();
        r
    }

    fn assign_inner(&mut self) -> Result<Expr, String> {
        if self.in_gen && self.is_kw("yield") {
            self.advance();
            let delegate = self.eat_p("*");
            let ends = self.nl_before() || self.at_eof() || self.is_p(")") || self.is_p("]") || self.is_p("}") || self.is_p(",") || self.is_p(";") || self.is_p(":");
            let arg = if ends && !delegate { None } else { Some(Box::new(self.assign_expr()?)) };
            return Ok(Expr::Yield { arg, delegate });
        }
        if let Some(f) = self.try_arrow()? {
            return Ok(f);
        }
        let left = self.conditional()?;
        if let Tok::Punct(p) = self.peek().clone() {
            if is_assign_op(&p) {
                match &left {
                    Expr::Ident(_) | Expr::Member { .. } | Expr::Index { .. } => {}
                    _ => return self.err("invalid assignment target"),
                }
                self.advance();
                let right = self.assign_expr()?;
                return Ok(Expr::Assign { op: p, target: Box::new(left), value: Box::new(right) });
            }
        }
        Ok(left)
    }

    fn conditional(&mut self) -> Result<Expr, String> {
        let c = self.binary(0)?;
        if self.is_p("?") {
            self.advance();
            let saved = self.no_in;
            self.no_in = false;
            let a = self.assign_expr()?;
            self.no_in = saved;
            self.expect_p(":")?;
            let b = self.assign_expr()?;
            return Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }

    fn binary(&mut self, min: u8) -> Result<Expr, String> {
        let mut left = self.unary()?;
        loop {
            let (op, prec) = match self.peek() {
                Tok::Punct(p) => match binop_prec(p) {
                    Some(pr) => (p.clone(), pr),
                    None => break,
                },
                Tok::Ident(k) if k == "instanceof" || (k == "in" && !self.no_in) => (k.clone(), 8u8),
                _ => break,
            };
            if prec < min {
                break;
            }
            self.advance();
            let right = if op == "**" { self.binary(prec)? } else { self.binary(prec + 1)? };
            left = if op == "&&" || op == "||" || op == "??" {
                Expr::Logical(op, Box::new(left), Box::new(right))
            } else {
                Expr::Binary(op, Box::new(left), Box::new(right))
            };
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let r = self.unary_inner();
        self.leave();
        r
    }

    fn unary_inner(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Tok::Punct(p) if matches!(p.as_str(), "!" | "-" | "+" | "~") => {
                self.advance();
                let e = self.unary()?;
                Ok(Expr::Unary(p, Box::new(e)))
            }
            Tok::Punct(p) if p == "++" || p == "--" => {
                self.advance();
                let t = self.unary()?;
                Ok(Expr::Update { op: p, prefix: true, target: Box::new(t) })
            }
            Tok::Ident(k) if matches!(k.as_str(), "typeof" | "void" | "delete") => {
                self.advance();
                let e = self.unary()?;
                Ok(Expr::Unary(k, Box::new(e)))
            }
            Tok::Ident(k) if k == "await" => {
                self.advance();
                let e = self.unary()?;
                Ok(Expr::Await(Box::new(e)))
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let e = self.lhs()?;
        if (self.is_p("++") || self.is_p("--")) && !self.nl_before() {
            if let Tok::Punct(op) = self.advance() {
                return Ok(Expr::Update { op, prefix: false, target: Box::new(e) });
            }
        }
        Ok(e)
    }

    fn arguments(&mut self) -> Result<Vec<Expr>, String> {
        self.expect_p("(")?;
        let saved = self.no_in;
        self.no_in = false;
        let mut args = Vec::new();
        loop {
            if self.eat_p(")") {
                break;
            }
            if self.eat_p("...") {
                let e = self.assign_expr()?;
                args.push(Expr::Spread(Box::new(e)));
            } else {
                args.push(self.assign_expr()?);
            }
            if !self.is_p(")") {
                self.expect_p(",")?;
            }
        }
        self.no_in = saved;
        Ok(args)
    }

    fn lhs(&mut self) -> Result<Expr, String> {
        let mut e = if self.is_kw("new") { self.new_expr()? } else { self.primary()? };
        loop {
            if self.is_p(".") {
                self.advance();
                let name = self.ident_name()?;
                e = Expr::Member { obj: Box::new(e), prop: name, optional: false };
            } else if self.is_p("?.") {
                self.advance();
                if self.is_p("(") {
                    let args = self.arguments()?;
                    e = Expr::Call { callee: Box::new(e), args, optional: true };
                } else if self.is_p("[") {
                    self.advance();
                    let saved = self.no_in;
                    self.no_in = false;
                    let idx = self.expression()?;
                    self.no_in = saved;
                    self.expect_p("]")?;
                    e = Expr::Index { obj: Box::new(e), idx: Box::new(idx), optional: true };
                } else {
                    let name = self.ident_name()?;
                    e = Expr::Member { obj: Box::new(e), prop: name, optional: true };
                }
            } else if self.is_p("[") {
                self.advance();
                let saved = self.no_in;
                self.no_in = false;
                let idx = self.expression()?;
                self.no_in = saved;
                self.expect_p("]")?;
                e = Expr::Index { obj: Box::new(e), idx: Box::new(idx), optional: false };
            } else if self.is_p("(") {
                let args = self.arguments()?;
                e = Expr::Call { callee: Box::new(e), args, optional: false };
            } else if matches!(self.peek(), Tok::Tpl(_, _)) {
                if let Tok::Tpl(strs, srcs) = self.advance() {
                    let mut exprs = Vec::new();
                    for src in srcs.iter() {
                        exprs.push(parse_expression_source(src)?);
                    }
                    e = Expr::TaggedTpl { tag: Box::new(e), strs, exprs };
                }
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn new_expr(&mut self) -> Result<Expr, String> {
        self.advance();
        if self.is_p(".") {
            return self.err("new.target is not supported");
        }
        let mut callee = if self.is_kw("new") { self.new_expr()? } else { self.primary()? };
        loop {
            if self.is_p(".") {
                self.advance();
                let name = self.ident_name()?;
                callee = Expr::Member { obj: Box::new(callee), prop: name, optional: false };
            } else if self.is_p("[") {
                self.advance();
                let idx = self.expression()?;
                self.expect_p("]")?;
                callee = Expr::Index { obj: Box::new(callee), idx: Box::new(idx), optional: false };
            } else {
                break;
            }
        }
        let args = if self.is_p("(") { self.arguments()? } else { Vec::new() };
        Ok(Expr::New { callee: Box::new(callee), args })
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let t = self.advance();
        match t {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Regex(b, f) => Ok(Expr::Regex(b, f)),
            Tok::Tpl(strs, srcs) => {
                let mut exprs = Vec::new();
                for s in srcs.iter() {
                    exprs.push(parse_expression_source(s)?);
                }
                Ok(Expr::Tpl(strs, exprs))
            }
            Tok::Ident(name) => match name.as_str() {
                "function" => {
                    let is_gen = self.eat_p("*");
                    let fname = match self.peek().clone() {
                        Tok::Ident(n) if !is_reserved(&n) => {
                            self.advance();
                            Some(n)
                        }
                        _ => None,
                    };
                    Ok(Expr::Func(self.function_ex(fname, false, is_gen, false, false)?))
                }
                "async" if self.is_kw("function") => {
                    self.advance();
                    let is_gen = self.eat_p("*");
                    let fname = match self.peek().clone() {
                        Tok::Ident(n) if !is_reserved(&n) => {
                            self.advance();
                            Some(n)
                        }
                        _ => None,
                    };
                    Ok(Expr::Func(self.function_ex(fname, true, is_gen, false, false)?))
                }
                "true" => Ok(Expr::Bool(true)),
                "false" => Ok(Expr::Bool(false)),
                "null" => Ok(Expr::Null),
                "this" => Ok(Expr::This),
                "class" => Ok(Expr::Class(self.class_def()?)),
                "super" => {
                    if self.is_p("(") {
                        let args = self.arguments()?;
                        Ok(Expr::SuperCall(args))
                    } else if self.eat_p(".") {
                        let n = self.ident_name()?;
                        Ok(Expr::SuperMember(Box::new(Expr::Str(n))))
                    } else if self.eat_p("[") {
                        let e = self.expression()?;
                        self.expect_p("]")?;
                        Ok(Expr::SuperMember(Box::new(e)))
                    } else {
                        self.err("'super' keyword unexpected here")
                    }
                }
                _ => {
                    if is_reserved(&name) {
                        self.pos = self.pos.saturating_sub(1);
                        return self.err("unexpected keyword");
                    }
                    Ok(Expr::Ident(name))
                }
            },
            Tok::Punct(p) => match p.as_str() {
                "(" => {
                    let saved = self.no_in;
                    self.no_in = false;
                    let e = self.expression()?;
                    self.no_in = saved;
                    self.expect_p(")")?;
                    Ok(e)
                }
                "[" => {
                    let saved = self.no_in;
                    self.no_in = false;
                    let mut items = Vec::new();
                    loop {
                        if self.eat_p("]") {
                            break;
                        }
                        if self.is_p(",") {
                            self.advance();
                            items.push(Expr::Hole);
                            continue;
                        }
                        if self.eat_p("...") {
                            let e = self.assign_expr()?;
                            items.push(Expr::Spread(Box::new(e)));
                        } else {
                            items.push(self.assign_expr()?);
                        }
                        if !self.is_p("]") {
                            self.expect_p(",")?;
                        }
                    }
                    self.no_in = saved;
                    Ok(Expr::Array(items))
                }
                "{" => self.object_literal(),
                _ => {
                    self.pos = self.pos.saturating_sub(1);
                    self.err("unexpected token")
                }
            },
            Tok::Eof => self.err("unexpected end of input"),
        }
    }

    fn object_literal(&mut self) -> Result<Expr, String> {
        let saved = self.no_in;
        self.no_in = false;
        let mut props: Vec<PropDef> = Vec::new();
        loop {
            if self.eat_p("}") {
                break;
            }
            if self.eat_p("...") {
                let e = self.assign_expr()?;
                props.push(PropDef::Spread(e));
            } else {
                let mut is_async = false;
                let mut accessor = 0u8;
                if let Tok::Ident(k) = self.peek().clone() {
                    let next_is_key = !(self.is_p_at(1, ":") || self.is_p_at(1, "(") || self.is_p_at(1, ",") || self.is_p_at(1, "}") || self.is_p_at(1, "="));
                    if next_is_key {
                        if k == "async" {
                            self.advance();
                            is_async = true;
                        } else if k == "get" {
                            self.advance();
                            accessor = 1;
                        } else if k == "set" {
                            self.advance();
                            accessor = 2;
                        }
                    }
                }
                let is_gen = self.eat_p("*");
                let key = self.prop_key()?;
                if self.is_p("(") {
                    let saved2 = self.no_in;
                    let f = self.function_ex(None, is_async, is_gen, false, false)?;
                    self.no_in = saved2;
                    match accessor {
                        1 => props.push(PropDef::Getter(key, f)),
                        2 => props.push(PropDef::Setter(key, f)),
                        _ => props.push(PropDef::KeyVal(key, Expr::Func(f))),
                    }
                } else if self.eat_p(":") {
                    let v = self.assign_expr()?;
                    props.push(PropDef::KeyVal(key, v));
                } else {
                    match &key {
                        PropKey::Name(n) => {
                            let v = Expr::Ident(n.clone());
                            props.push(PropDef::KeyVal(key, v));
                        }
                        _ => return self.err("invalid shorthand property"),
                    }
                }
            }
            if !self.is_p("}") {
                self.expect_p(",")?;
            }
        }
        self.no_in = saved;
        Ok(Expr::Object(props))
    }
}
