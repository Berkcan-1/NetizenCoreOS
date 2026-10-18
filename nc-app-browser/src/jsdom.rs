//! Bridge between the nc-js engine and the browser: native DOM primitives,
//! the JavaScript prelude (DOM/BOM written in JS) and the network pump.

extern crate alloc;

use crate::dom::{DomTree, NodeType};
use crate::sel;
use alloc::collections::VecDeque;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use nc_js::{Ctl, Interp, NativeFn, Res, Value};
use nc_net::{global as net_global, HttpState, TcpHttpClient};

pub const PRELUDE: &str = include_str!("prelude.js");
pub const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 NetizenCore/1.0";

pub struct NetReq {
    pub id: u32,
    pub url: String,
}

pub struct JsHost {
    pub tree: Rc<RefCell<DomTree>>,
    pub dirty: Cell<bool>,
    pub location: RefCell<String>,
    pub navigate: RefCell<Option<String>>,
    pub client: RefCell<TcpHttpClient>,
    pub queue: RefCell<VecDeque<NetReq>>,
    pub current: RefCell<Option<NetReq>>,
    pub next_id: Cell<u32>,
    pub completed: Cell<u32>,
    pub errors: RefCell<Vec<String>>,
}

impl JsHost {
    pub fn note_error(&self, msg: String) {
        let mut e = self.errors.borrow_mut();
        if e.len() >= 40 {
            e.remove(0);
        }
        e.push(msg);
    }

    pub fn network_busy(&self) -> bool {
        self.current.borrow().is_some() || !self.queue.borrow().is_empty()
    }
}

pub fn host(it: &Interp) -> Option<Rc<JsHost>> {
    it.host.clone().and_then(|h| h.downcast::<JsHost>().ok())
}

macro_rules! H {
    ($it:expr) => {
        match host($it) {
            Some(h) => h,
            None => return Ok(Value::Undefined),
        }
    };
}

fn aid(a: &[Value], i: usize) -> usize {
    match a.get(i) {
        Some(Value::Num(n)) if *n >= 0.0 => *n as usize,
        _ => usize::MAX,
    }
}

fn astr(it: &mut Interp, a: &[Value], i: usize) -> Res<String> {
    let v = a.get(i).cloned().unwrap_or(Value::Undefined);
    it.to_str(&v)
}

fn valid(h: &JsHost, id: usize) -> bool {
    id < h.tree.borrow().nodes.len()
}

fn ids_array(it: &Interp, ids: &[usize]) -> Value {
    it.new_array(ids.iter().map(|i| Value::Num(*i as f64)).collect())
}

fn client_final_url(c: &TcpHttpClient) -> String {
    let scheme = if c.https { "https" } else { "http" };
    let default_port = if c.https { 443 } else { 80 };
    if c.remote_port != default_port {
        format!("{}://{}:{}{}", scheme, c.host, c.remote_port, c.path)
    } else {
        format!("{}://{}{}", scheme, c.host, c.path)
    }
}

// ---------- __dom natives ----------

fn d_type(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let id = aid(a, 0);
    let tree = h.tree.borrow();
    if id >= tree.nodes.len() {
        return Ok(Value::Undefined);
    }
    if id == tree.root {
        return Ok(Value::Num(9.0));
    }
    Ok(Value::Num(match tree.nodes[id].node_type {
        NodeType::Element { .. } => 1.0,
        NodeType::Text(_) => 3.0,
    }))
}

fn d_tag(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let id = aid(a, 0);
    let tree = h.tree.borrow();
    Ok(Value::str(tree.tag(id).unwrap_or("")))
}

fn d_create(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let tag = astr(it, a, 0)?;
    let id = h.tree.borrow_mut().create_element(&tag);
    Ok(Value::Num(id as f64))
}

fn d_text(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let s = astr(it, a, 0)?;
    let id = h.tree.borrow_mut().create_text(&s);
    Ok(Value::Num(id as f64))
}

fn d_value(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let id = aid(a, 0);
    let tree = h.tree.borrow();
    Ok(match tree.nodes.get(id).map(|n| &n.node_type) {
        Some(NodeType::Text(t)) => Value::str(t),
        _ => Value::str(""),
    })
}

fn d_set_value(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let id = aid(a, 0);
    let s = astr(it, a, 1)?;
    if valid(&h, id) {
        h.tree.borrow_mut().set_text_content(id, &s);
        h.dirty.set(true);
    }
    Ok(Value::Undefined)
}

fn d_parent(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let id = aid(a, 0);
    let tree = h.tree.borrow();
    Ok(Value::Num(match tree.nodes.get(id).and_then(|n| n.parent) {
        Some(p) => p as f64,
        None => -1.0,
    }))
}

fn d_children(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let id = aid(a, 0);
    let kids: Vec<usize> = h.tree.borrow().nodes.get(id).map(|n| n.children.clone()).unwrap_or_default();
    Ok(ids_array(it, &kids))
}

fn d_append(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let ok = h.tree.borrow_mut().append_child(aid(a, 0), aid(a, 1));
    h.dirty.set(true);
    Ok(Value::Bool(ok))
}

fn d_insert_before(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let r = aid(a, 2);
    let before = if r == usize::MAX { None } else { Some(r) };
    let ok = h.tree.borrow_mut().insert_before(aid(a, 0), aid(a, 1), before);
    h.dirty.set(true);
    Ok(Value::Bool(ok))
}

fn d_remove(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    h.tree.borrow_mut().detach(aid(a, 0));
    h.dirty.set(true);
    Ok(Value::Undefined)
}

fn d_get_attr(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let id = aid(a, 0);
    let name = astr(it, a, 1)?;
    let tree = h.tree.borrow();
    Ok(match tree.attr(id, &name) {
        Some(v) => Value::str(v),
        None => Value::Null,
    })
}

fn d_set_attr(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let id = aid(a, 0);
    let name = astr(it, a, 1)?;
    let val = astr(it, a, 2)?;
    h.tree.borrow_mut().set_attr(id, &name, &val);
    h.dirty.set(true);
    Ok(Value::Undefined)
}

fn d_remove_attr(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let id = aid(a, 0);
    let name = astr(it, a, 1)?;
    h.tree.borrow_mut().remove_attr(id, &name);
    h.dirty.set(true);
    Ok(Value::Undefined)
}

fn d_attr_names(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let names = h.tree.borrow().attr_names(aid(a, 0));
    Ok(it.new_array(names.into_iter().map(Value::string).collect()))
}

fn d_text_content(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let s = h.tree.borrow().raw_text(aid(a, 0));
    Ok(Value::string(s))
}

fn d_set_text_content(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let s = astr(it, a, 1)?;
    h.tree.borrow_mut().set_text_content(aid(a, 0), &s);
    h.dirty.set(true);
    Ok(Value::Undefined)
}

fn d_inner_html(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let s = h.tree.borrow().serialize(aid(a, 0), false);
    Ok(Value::string(s))
}

fn d_outer_html(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let s = h.tree.borrow().serialize(aid(a, 0), true);
    Ok(Value::string(s))
}

fn d_set_inner_html(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let s = astr(it, a, 1)?;
    h.tree.borrow_mut().set_inner_html(aid(a, 0), &s);
    h.dirty.set(true);
    Ok(Value::Undefined)
}

fn d_clone(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let deep = a.get(1).map(|v| v.truthy()).unwrap_or(false);
    let id = h.tree.borrow_mut().clone_node(aid(a, 0), deep);
    Ok(Value::Num(id as f64))
}

fn d_query(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let root = aid(a, 0);
    let text = astr(it, a, 1)?;
    let all = a.get(2).map(|v| v.truthy()).unwrap_or(false);
    let list = match sel::parse(&text) {
        Some(l) => l,
        None => return Err(it.throw_err("SyntaxError", &format!("'{}' is not a valid selector", text))),
    };
    let ids = {
        let tree = h.tree.borrow();
        if root >= tree.nodes.len() {
            Vec::new()
        } else {
            sel::query_all(&tree, root, &list, if all { 20_000 } else { 1 })
        }
    };
    Ok(ids_array(it, &ids))
}

fn d_matches(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let text = astr(it, a, 1)?;
    let list = match sel::parse(&text) {
        Some(l) => l,
        None => return Err(it.throw_err("SyntaxError", &format!("'{}' is not a valid selector", text))),
    };
    let tree = h.tree.borrow();
    let id = aid(a, 0);
    Ok(Value::Bool(id < tree.nodes.len() && sel::matches(&tree, id, &list)))
}

fn d_by_id(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let want = astr(it, a, 0)?;
    let tree = h.tree.borrow();
    for n in tree.nodes.iter() {
        if n.id != tree.root && tree.attr(n.id, "id") == Some(want.as_str()) && tree.is_ancestor(tree.root, n.id) {
            return Ok(Value::Num(n.id as f64));
        }
    }
    Ok(Value::Num(-1.0))
}

fn d_connected(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let tree = h.tree.borrow();
    let id = aid(a, 0);
    Ok(Value::Bool(id < tree.nodes.len() && tree.is_ancestor(tree.root, id)))
}

// ---------- __host natives ----------

fn h_location(it: &mut Interp, _t: &Value, _a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let s = h.location.borrow().clone();
    Ok(Value::string(s))
}

fn h_navigate(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let url = astr(it, a, 0)?;
    *h.navigate.borrow_mut() = Some(url);
    Ok(Value::Undefined)
}

fn h_time(it: &mut Interp, _t: &Value, _a: &[Value]) -> Res<Value> {
    Ok(Value::Num(it.now_ms))
}

fn h_user_agent(_it: &mut Interp, _t: &Value, _a: &[Value]) -> Res<Value> {
    Ok(Value::str(USER_AGENT))
}

fn h_error(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let v = a.get(0).cloned().unwrap_or(Value::Undefined);
    let msg = it.describe_error(&v);
    h.note_error(msg);
    Ok(Value::Undefined)
}

fn h_net(it: &mut Interp, _t: &Value, a: &[Value]) -> Res<Value> {
    let h = H!(it);
    let url = astr(it, a, 0)?;
    let id = h.next_id.get();
    h.next_id.set(id + 1);
    h.queue.borrow_mut().push_back(NetReq { id, url });
    Ok(Value::Num(id as f64))
}

fn register(it: &mut Interp, name: &str, fns: &[(&str, NativeFn)]) {
    let o = it.new_plain();
    for (n, f) in fns.iter() {
        let v = it.new_native(*f);
        it.set_own(&o, n, v);
    }
    it.set_global(name, Value::Obj(o));
}

/// Creates a fresh JS runtime bound to `tree`, loads the DOM/BOM prelude.
pub fn create_runtime(tree: Rc<RefCell<DomTree>>, url: &str) -> (Interp, Rc<JsHost>) {
    let mut it = Interp::new();
    it.max_depth = 60;
    let host = Rc::new(JsHost {
        tree,
        dirty: Cell::new(false),
        location: RefCell::new(String::from(url)),
        navigate: RefCell::new(None),
        client: RefCell::new(TcpHttpClient::new()),
        queue: RefCell::new(VecDeque::new()),
        current: RefCell::new(None),
        next_id: Cell::new(1),
        completed: Cell::new(0),
        errors: RefCell::new(Vec::new()),
    });
    let any: Rc<dyn core::any::Any> = host.clone();
    it.host = Some(any);
    it.pump = Some(pump_blocking);
    register(
        &mut it,
        "__dom",
        &[
            ("type", d_type as NativeFn),
            ("tag", d_tag as NativeFn),
            ("create", d_create as NativeFn),
            ("text", d_text as NativeFn),
            ("value", d_value as NativeFn),
            ("setValue", d_set_value as NativeFn),
            ("parent", d_parent as NativeFn),
            ("children", d_children as NativeFn),
            ("append", d_append as NativeFn),
            ("insertBefore", d_insert_before as NativeFn),
            ("remove", d_remove as NativeFn),
            ("getAttr", d_get_attr as NativeFn),
            ("setAttr", d_set_attr as NativeFn),
            ("removeAttr", d_remove_attr as NativeFn),
            ("attrNames", d_attr_names as NativeFn),
            ("textContent", d_text_content as NativeFn),
            ("setTextContent", d_set_text_content as NativeFn),
            ("innerHTML", d_inner_html as NativeFn),
            ("outerHTML", d_outer_html as NativeFn),
            ("setInnerHTML", d_set_inner_html as NativeFn),
            ("clone", d_clone as NativeFn),
            ("query", d_query as NativeFn),
            ("matches", d_matches as NativeFn),
            ("byId", d_by_id as NativeFn),
            ("connected", d_connected as NativeFn),
        ],
    );
    register(
        &mut it,
        "__host",
        &[
            ("location", h_location as NativeFn),
            ("navigate", h_navigate as NativeFn),
            ("time", h_time as NativeFn),
            ("userAgent", h_user_agent as NativeFn),
            ("error", h_error as NativeFn),
            ("net", h_net as NativeFn),
        ],
    );
    if let Err(e) = it.run(PRELUDE) {
        host.note_error(format!("prelude: {}", e));
    }
    if let Ok(v) = it.run("__prelude_errors.join('; ')") {
        if let Ok(s) = it.to_str(&v) {
            if !s.is_empty() {
                host.note_error(format!("prelude sections: {}", s));
            }
        }
    }
    (it, host)
}

// ---------- network pump ----------

/// Advances the JS network queue by one poll. Returns true while work remains.
pub fn pump_step(it: &mut Interp) -> bool {
    let h = match host(it) {
        Some(h) => h,
        None => return false,
    };
    let mut finished: Option<(NetReq, bool, u16, String, String)> = None;
    {
        let mut client = h.client.borrow_mut();
        let mut cur = h.current.borrow_mut();
        if cur.is_none() {
            let next = h.queue.borrow_mut().pop_front();
            if let Some(r) = next {
                client.navigate(&r.url, net_global());
                *cur = Some(r);
            }
        }
        if cur.is_some() {
            client.poll(net_global());
            let done = matches!(client.state, HttpState::Complete | HttpState::Failed);
            if done {
                let ok = client.state == HttpState::Complete;
                let status = client.status_code;
                let body = if ok { client.html_body.clone() } else { String::new() };
                let url = client_final_url(&client);
                if let Some(r) = cur.take() {
                    finished = Some((r, ok, status, body, url));
                }
            }
        }
    }
    if let Some((r, ok, status, body, url)) = finished {
        h.completed.set(h.completed.get().wrapping_add(1));
        let args = [Value::Num(r.id as f64), Value::Bool(ok), Value::Num(status as f64), Value::string(body), Value::string(url)];
        run_js_call(it, "__net_done", &args);
        h.dirty.set(true);
    }
    h.network_busy()
}

/// Used by `await`: spins the network until one request completes (or none are pending).
pub fn pump_blocking(it: &mut Interp) -> bool {
    let h = match host(it) {
        Some(h) => h,
        None => return false,
    };
    h.client.borrow_mut().tick_every = 2500;
    let start = h.completed.get();
    let mut busy = h.network_busy();
    let mut n = 0;
    while busy && n < 300_000 {
        busy = pump_step(it);
        if h.completed.get() != start {
            break;
        }
        n += 1;
    }
    h.client.borrow_mut().tick_every = 1;
    busy
}

/// Calls a global JS function, drains microtasks and records uncaught errors.
pub fn run_js_call(it: &mut Interp, name: &str, args: &[Value]) -> Value {
    let r = it.call_global(name, args);
    it.run_microtasks();
    match r {
        Ok(v) => v,
        Err(Ctl::Throw(e)) => {
            let msg = it.describe_error(&e);
            if let Some(h) = host(it) {
                h.note_error(msg);
            }
            Value::Undefined
        }
        Err(_) => Value::Undefined,
    }
}

/// Runs a script; errors are recorded, never propagated.
pub fn run_script(it: &mut Interp, label: &str, code: &str) {
    if let Err(e) = it.run(code) {
        if let Some(h) = host(it) {
            h.note_error(format!("{}: {}", label, e));
        }
    }
}

pub struct ScriptItem {
    pub node: usize,
    pub src: Option<String>,
    pub code: String,
}

fn script_type_ok(tree: &DomTree, id: usize) -> bool {
    match tree.attr(id, "type") {
        None => true,
        Some(t) => {
            let t = t.trim().to_ascii_lowercase();
            t.is_empty() || t == "text/javascript" || t == "application/javascript" || t == "text/ecmascript" || t == "application/ecmascript" || t == "text/jsx"
        }
    }
}

/// Attached `<script>` elements in document order that have not been handled yet.
pub fn pending_scripts(tree: &DomTree, done: &[usize]) -> Vec<ScriptItem> {
    let mut out = Vec::new();
    let mut stack: Vec<usize> = alloc::vec![tree.root];
    while let Some(id) = stack.pop() {
        if let Some(n) = tree.nodes.get(id) {
            for &c in n.children.iter().rev() {
                stack.push(c);
            }
            if tree.tag(id) == Some("script") && !done.contains(&id) && script_type_ok(tree, id) {
                let src = tree.attr(id, "src").map(String::from).filter(|s| !s.trim().is_empty());
                let code = if src.is_some() { String::new() } else { tree.raw_text(id) };
                if src.is_some() || !code.trim().is_empty() {
                    out.push(ScriptItem { node: id, src, code });
                }
            }
        }
    }
    out
}
