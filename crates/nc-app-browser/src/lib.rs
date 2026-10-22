#![no_std]

extern crate alloc;

mod cookies;
mod css;
mod cssvalues;
mod dom;
mod font_cov;
mod gif;
mod html;
mod image;
mod jsdom;
mod layout;
mod sel;
mod supports;
mod url;

use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use nc_app::theme::SIDEBAR;
use nc_app::{App, AppInfo, Context, Icon, Input, KeyCode, Update};
use nc_font::{text_width, Style};
use nc_gfx::{rgb, Canvas, Rect};
use nc_js::{Interp, Value};
use nc_net::{global as net_global, HttpState, TcpHttpClient};
use nc_ui::{
    draw_accent_button, draw_badge, draw_button, draw_input_box, draw_panel, draw_text_color,
    text_style, ACCENT_BG, PANEL_BORDER, PANEL_FILL, TEXT_DIM, TEXT_MUTED, TEXT_PRIMARY, WHITE,
};

use crate::cookies::CookieJar;
use crate::css::Stylesheet;
use crate::dom::{DomTree, NodeType};
use crate::html::{sanitize_text, HtmlParser, MAX_HTML_BYTES};
use crate::image::DecodedImage;
use crate::jsdom::{create_runtime, pending_scripts, pump_step, run_js_call, run_script, JsHost};
use crate::layout::{
    page_text_style, set_viewport, LayoutBox, LayoutEngine, WidgetKind, DECO_LINE_THROUGH, DECO_OVERLINE,
    DECO_UNDERLINE,
};
use crate::url::{build_form_url, normalize_input_url, resolve_url};

const MAX_SUGGESTIONS: usize = 6;
const SUGGEST_DEBOUNCE_TICKS: u32 = 12;
const MAX_CSS_TRIED: usize = 40;
const MAX_IMG_TRIED: usize = 60;
const MAX_SCRIPTS: usize = 80;
const IMG_MAX_SIDE: u32 = 640;
const MAX_REFRESH_HOPS: u8 = 3;
const MAX_HISTORY: usize = 64;
const LAYOUT_MIN_TICKS: u32 = 4;

const HOME_HTML: &str = "<html><head><title>NetizenCore Browser</title></head>\
<body style=\"background-color:#f8fafc\">\
<div style=\"text-align:center;padding:24px\">\
<h1 style=\"color:#0f172a\">NETIZENCORE</h1>\
<p style=\"color:#475569\">Search the web or type an address above.</p>\
<form action=\"https://html.duckduckgo.com/html/\">\
<input type=\"text\" name=\"q\" placeholder=\"Search the web\">\
<input type=\"submit\" value=\"SEARCH\">\
</form>\
<p style=\"color:#475569\">Quick links</p>\
<p><a href=\"https://en.wikipedia.org/\">Wikipedia</a></p>\
<p><a href=\"https://www.google.com/\">Google</a></p>\
<p><a href=\"http://info.cern.ch/\">info.cern.ch</a></p>\
<p><a href=\"https://example.com/\">example.com</a></p>\
<p style=\"color:#64748b\">Tip: view-source:URL shows page source, about:debug lists script errors, about:compat tests engine features</p>\
</div></body></html>";

#[derive(Clone, PartialEq, Eq)]
enum Active {
    None,
    Main,
    Css,
    Script(usize),
    Img(String),
    Suggest,
}

#[derive(Clone)]
enum HitKind {
    Node(Option<String>),
    Input { seq: usize, action: String, name: String },
    Submit { action: String, name: String, target: Option<usize> },
}

#[derive(Clone)]
struct Hit {
    rect: Rect,
    node: usize,
    kind: HitKind,
}

pub struct BrowserApp {
    info: AppInfo,
    url_input: String,
    url_focused: bool,
    client: TcpHttpClient,
    active: Active,
    current_url: String,
    history: Vec<String>,
    hist_pos: usize,
    tree: Rc<RefCell<DomTree>>,
    cookies: Rc<RefCell<CookieJar>>,
    stylesheet: Stylesheet,
    css_external: Vec<String>,
    images: Vec<(String, DecodedImage)>,
    css_queue: Vec<String>,
    img_queue: Vec<(String, String)>,
    css_tried: Vec<String>,
    img_tried: Vec<String>,
    img_skipped: Vec<(String, &'static str)>,
    img_total: usize,
    img_done: usize,
    layout: LayoutEngine,
    hits: Vec<Hit>,
    layout_dirty: bool,
    layout_built: bool,
    last_layout_w: i32,
    last_layout_tick: u32,
    focused_input: Option<usize>,
    scroll_y: i32,
    page_title: String,
    refresh_hops: u8,
    hover_back: bool,
    hover_fwd: bool,
    hover_reload: bool,
    hover_go: bool,
    suggestions: Vec<String>,
    suggest_want: Option<String>,
    suggest_tick: u32,
    tick: u32,
    js: Option<Interp>,
    jshost: Option<Rc<JsHost>>,
    scripts_done: Vec<usize>,
    js_ready: bool,
    js_now: f64,
    style_sig: usize,
    view_source: bool,
    last_status: u16,
    page_bytes: usize,
    pending_nav: Option<String>,
}

fn inside(r: Rect, x: i32, y: i32) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

fn empty_layout() -> LayoutEngine {
    let tree = DomTree::new();
    let sheet = Stylesheet::new();
    let mut imgs: Vec<(String, DecodedImage)> = Vec::new();
    LayoutEngine::build(&tree, &sheet, &mut imgs, 320)
}

/// Self-test page (`about:compat`): see compat_probe.html.
const COMPAT_HTML: &str = include_str!("compat_probe.html");

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// Why an image URL is *not* fetched, or `None` when it should be. Formats the decoder in
/// `image.rs` cannot read yet are skipped on purpose (no wasted download), but every skip is
/// recorded and shown on `about:debug` instead of silently producing an empty box.
fn image_skip_reason(src: &str) -> Option<&'static str> {
    if src.is_empty() {
        return Some("empty src");
    }
    // data: URIs are decoded inline by the layout engine; nothing to fetch and nothing to report.
    if src.starts_with("data:") {
        return Some("");
    }
    let lower = src.to_ascii_lowercase();
    let path = match lower.find('?') {
        Some(q) => &lower[..q],
        None => &lower[..],
    };
    if path.ends_with(".webp") {
        Some("WebP decoder not implemented")
    } else if path.ends_with(".avif") {
        Some("AVIF decoder not implemented")
    } else if path.ends_with(".ico") {
        Some("ICO decoder not implemented")
    } else {
        None
    }
}

fn collect_hits(b: &LayoutBox, out: &mut Vec<Hit>, depth: usize) {
    if depth > 60 {
        return;
    }
    match &b.widget {
        WidgetKind::TextInput { input_id, form_action, input_name, .. } => {
            out.push(Hit {
                rect: b.rect,
                node: b.node_id,
                kind: HitKind::Input { seq: *input_id, action: form_action.clone(), name: input_name.clone() },
            });
        }
        WidgetKind::SubmitButton { form_action, input_name, target_input_id, .. } => {
            out.push(Hit {
                rect: b.rect,
                node: b.node_id,
                kind: HitKind::Submit { action: form_action.clone(), name: input_name.clone(), target: *target_input_id },
            });
        }
        _ => {
            if b.rect.width > 0 && b.rect.height > 0 {
                out.push(Hit { rect: b.rect, node: b.node_id, kind: HitKind::Node(b.link_url.clone()) });
            }
        }
    }
    for c in &b.children {
        collect_hits(c, out, depth + 1);
    }
}

fn view_source_page(body: &str, url: &str) -> String {
    let mut s = alloc::format!(
        "<html><head><title>SOURCE</title></head><body style=\"background-color:#ffffff\"><div style=\"padding:12px\"><h2>SOURCE {}</h2>",
        html_escape(url)
    );
    for (i, line) in body.lines().enumerate() {
        if i >= 400 {
            s.push_str("<p>... truncated</p>");
            break;
        }
        let mut l = String::new();
        for ch in line.chars().take(220) {
            l.push(ch);
        }
        s.push_str(&alloc::format!("<p style=\"font-size:12px\">{}</p>", html_escape(&l)));
    }
    s.push_str("</div></body></html>");
    s
}

fn fit_tail(text: &str, st: &Style, max_w: i32) -> String {
    if text_width(text, st) <= max_w {
        return String::from(text);
    }
    let idxs: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    for &start in idxs.iter() {
        let slice = &text[start..];
        if text_width(slice, st) <= max_w {
            return String::from(slice);
        }
    }
    String::new()
}

fn fit_head(text: &str, st: &Style, max_w: i32) -> String {
    if text_width(text, st) <= max_w {
        return String::from(text);
    }
    let mut end_list: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    end_list.push(text.len());
    let mut best = 0usize;
    for &e in end_list.iter() {
        if text_width(&text[..e], st) <= max_w {
            best = e;
        } else {
            break;
        }
    }
    String::from(&text[..best])
}


fn looks_like_query(q: &str) -> bool {
    let t = q.trim();
    if t.chars().count() < 2 || t.contains("://") || t.starts_with("about:") {
        return false;
    }
    t.contains(' ') || !t.contains('.')
}

fn parse_suggestions(body: &str) -> Vec<String> {
    let c: Vec<char> = body.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < c.len() {
        match c[i] {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth <= 1 && !out.is_empty() {
                    break;
                }
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                while i < c.len() && c[i] != '"' {
                    if c[i] == '\\' && i + 1 < c.len() {
                        i += 1;
                        match c[i] {
                            'n' | 't' => s.push(' '),
                            'u' if i + 4 < c.len() => {
                                let hex: String = c[i + 1..i + 5].iter().collect();
                                if let Ok(v) = u32::from_str_radix(&hex, 16) {
                                    s.push(char::from_u32(v).unwrap_or(' '));
                                }
                                i += 4;
                            }
                            other => s.push(other),
                        }
                    } else {
                        s.push(c[i]);
                    }
                    i += 1;
                }
                if depth == 2 && out.len() < MAX_SUGGESTIONS {
                    let clean = sanitize_text(&s);
                    if !clean.trim().is_empty() {
                        out.push(clean);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}


impl BrowserApp {
    fn suggestion_rect(area: Rect, unit: i32, index: usize) -> Rect {
        let url = Self::url_box_rect(area, unit);
        let row_h = unit * 2;
        Rect::new(url.x, url.y + url.height + 2 + (index as i32) * row_h, url.width, row_h)
    }

    fn note_typed(&mut self) {
        self.suggest_tick = self.tick;
        self.suggestions.clear();
        self.suggest_want = Some(self.url_input.clone());
    }

    fn maybe_start_suggest(&mut self) {
        if self.active != Active::None || !self.url_focused {
            return;
        }
        let q = match &self.suggest_want {
            Some(q) => q.clone(),
            None => return,
        };
        if self.tick.wrapping_sub(self.suggest_tick) < SUGGEST_DEBOUNCE_TICKS {
            return;
        }
        self.suggest_want = None;
        if !looks_like_query(&q) {
            return;
        }
        let url = build_form_url(
            "https://suggestqueries.google.com/",
            "/complete/search?client=firefox&hl=tr",
            "q",
            q.trim(),
        );
        self.active = Active::Suggest;
        self.nav(&url);
    }
}


impl BrowserApp {
    pub fn new() -> Self {
        let mut app = BrowserApp {
            info: AppInfo::new("nc.browser", "BROWSER", "6.0.0", Icon::Generic, 48, 30),
            url_input: String::from("about:home"),
            url_focused: false,
            client: TcpHttpClient::new(),
            active: Active::None,
            current_url: String::from("about:home"),
            history: Vec::new(),
            hist_pos: 0,
            tree: Rc::new(RefCell::new(DomTree::new())),
            cookies: Rc::new(RefCell::new(CookieJar::new())),
            stylesheet: Stylesheet::new(),
            css_external: Vec::new(),
            images: Vec::new(),
            css_queue: Vec::new(),
            img_queue: Vec::new(),
            css_tried: Vec::new(),
            img_tried: Vec::new(),
            img_skipped: Vec::new(),
            img_total: 0,
            img_done: 0,
            layout: empty_layout(),
            hits: Vec::new(),
            layout_dirty: true,
            layout_built: false,
            last_layout_w: 0,
            last_layout_tick: 0,
            focused_input: None,
            scroll_y: 0,
            page_title: String::new(),
            refresh_hops: 0,
            hover_back: false,
            hover_fwd: false,
            hover_reload: false,
            hover_go: false,
            suggestions: Vec::new(),
            suggest_want: None,
            suggest_tick: 0,
            tick: 0,
            js: None,
            jshost: None,
            scripts_done: Vec::new(),
            js_ready: false,
            js_now: 0.0,
            style_sig: 0,
            view_source: false,
            last_status: 0,
            page_bytes: 0,
            pending_nav: None,
        };
        app.history.push(String::from("about:home"));
        app.show_html(HOME_HTML, "about:home");
        app
    }

    fn reset_page_state(&mut self) {
        self.css_queue.clear();
        self.img_queue.clear();
        self.css_tried.clear();
        self.img_tried.clear();
        self.img_skipped.clear();
        self.css_external.clear();
        self.img_total = 0;
        self.img_done = 0;
        self.images.clear();
        self.focused_input = None;
        self.scroll_y = 0;
        self.hits.clear();
        self.layout_dirty = true;
        self.js = None;
        self.jshost = None;
        self.scripts_done.clear();
        self.js_ready = false;
        self.js_now = 0.0;
        self.pending_nav = None;
    }

    fn style_signature(&self) -> usize {
        self.tree.borrow().collect_style_texts().iter().map(|s| s.len() + 1).sum()
    }

    fn rebuild_stylesheet(&mut self) {
        let mut sheet = Stylesheet::new();
        for s in self.tree.borrow().collect_style_texts() {
            sheet.append_css(&s);
        }
        for c in self.css_external.iter() {
            sheet.append_css(c);
        }
        self.stylesheet = sheet;
        self.style_sig = self.style_signature();
    }

    fn show_html(&mut self, html_text: &str, shown_url: &str) {
        self.reset_page_state();
        let parsed = HtmlParser::parse(html_text);
        self.page_title = parsed.title().unwrap_or_else(|| String::from(shown_url));
        self.tree = Rc::new(RefCell::new(parsed));
        self.rebuild_stylesheet();
        self.current_url = String::from(shown_url);
        self.url_input = String::from(shown_url);
        self.active = Active::None;
    }

    fn show_error(&mut self, url: &str, reason: &str) {
        let page = alloc::format!(
            "<html><head><title>PAGE ERROR</title></head><body style=\"background-color:#f8fafc\">\
<div style=\"padding:24px\"><h1 style=\"color:#b91c1c\">PAGE COULD NOT BE LOADED</h1>\
<p style=\"color:#334155\">{}</p>\
<p style=\"color:#64748b\">{}</p>\
<p><a href=\"about:home\">Go to home page</a></p></div></body></html>",
            html_escape(reason),
            html_escape(url)
        );
        self.show_html(&page, url);
        self.active = Active::None;
    }

    fn debug_page(&self) -> String {
        let mut s = String::from("<html><head><title>JS DEBUG</title></head><body style=\"background-color:#f8fafc\"><div style=\"padding:16px\"><h1>JS DEBUG</h1>");
        s.push_str(&alloc::format!(
            "<p>URL: {}</p><p>HTTP status: {} - body bytes: {}{}</p><p>scripts handled: {} - JS runtime: {}</p>",
            html_escape(&self.current_url),
            self.last_status,
            self.page_bytes,
            if self.page_bytes >= MAX_HTML_BYTES { " (TRUNCATED)" } else { "" },
            self.scripts_done.len(),
            if self.js.is_some() { "on" } else { "off" }
        ));
        {
            let t = self.tree.borrow();
            for d in t.diagnostics.iter() {
                s.push_str(&alloc::format!("<p style=\"color:#b45309\">PARSER: {}</p>", html_escape(d)));
            }
            for (src, why) in self.img_skipped.iter().take(40) {
                s.push_str(&alloc::format!("<p style=\"color:#b45309\">IMAGE SKIPPED: {} - {}</p>", html_escape(why), html_escape(src)));
            }
            if self.img_skipped.len() > 40 {
                s.push_str(&alloc::format!("<p style=\"color:#b45309\">... and {} more skipped images</p>", self.img_skipped.len() - 40));
            }
            let text = t.text_content(t.root);
            let snippet: String = text.split_whitespace().collect::<Vec<&str>>().join(" ").chars().take(260).collect();
            s.push_str(&alloc::format!("<p>page text: {}</p>", html_escape(&snippet)));
        }
        if let Some(h) = &self.jshost {
            for e in h.errors.borrow().iter() {
                s.push_str(&alloc::format!("<p style=\"color:#b91c1c\">ERR {}</p>", html_escape(e)));
            }
        }
        if let Some(js) = &self.js {
            s.push_str("<h2>console</h2>");
            for l in js.logs.iter().rev().take(40) {
                s.push_str(&alloc::format!("<p>{}</p>", html_escape(l)));
            }
        }
        s.push_str("<p><a href=\"about:home\">home</a></p></div></body></html>");
        s
    }

    fn go(&mut self, raw: &str, push_history: bool) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        if trimmed.starts_with("javascript:") || trimmed.starts_with("mailto:") || trimmed.starts_with("tel:") {
            return;
        }
        let mut view_source = false;
        let body_input = match trimmed.strip_prefix("view-source:") {
            Some(r) => {
                view_source = true;
                r.trim()
            }
            None => trimmed,
        };
        let target: String = if body_input.eq_ignore_ascii_case("about:home") {
            String::from("about:home")
        } else if body_input.eq_ignore_ascii_case("about:debug") {
            String::from("about:debug")
        } else if body_input.eq_ignore_ascii_case("about:compat") {
            String::from("about:compat")
        } else {
            normalize_input_url(body_input)
        };

        if push_history {
            if self.hist_pos + 1 < self.history.len() {
                self.history.truncate(self.hist_pos + 1);
            }
            if self.history.last().map(|h| h != &target).unwrap_or(true) {
                self.history.push(target.clone());
            }
            while self.history.len() > MAX_HISTORY {
                self.history.remove(0);
            }
            self.hist_pos = self.history.len() - 1;
        }

        self.url_input = if view_source { alloc::format!("view-source:{}", target) } else { target.clone() };
        self.suggestions.clear();
        self.suggest_want = None;

        if target == "about:home" {
            self.client.state = HttpState::Idle;
            self.show_html(HOME_HTML, "about:home");
            return;
        }
        if target == "about:debug" {
            let page = self.debug_page();
            self.client.state = HttpState::Idle;
            self.show_html(&page, "about:debug");
            return;
        }
        if target == "about:compat" {
            // Self-test page: its inline script probes ES / DOM / Web API support and lists what is missing.
            self.client.state = HttpState::Idle;
            self.show_html(COMPAT_HTML, "about:compat");
            self.start_next_fetch();
            return;
        }

        self.css_queue.clear();
        self.img_queue.clear();
        self.view_source = view_source;
        self.active = Active::Main;
        self.nav(&target);
    }

    /// Starts a GET on the page-load client, attaching matching cookies.
    fn nav(&mut self, url: &str) {
        self.client.cookie_header = self.cookies.borrow().header_for(url);
        let net = net_global();
        self.client.navigate(url, net);
    }

    fn go_back(&mut self) {
        if self.hist_pos > 0 {
            self.hist_pos -= 1;
            let t = self.history[self.hist_pos].clone();
            self.refresh_hops = 0;
            self.go(&t, false);
        }
    }

    fn go_forward(&mut self) {
        if self.hist_pos + 1 < self.history.len() {
            self.hist_pos += 1;
            let t = self.history[self.hist_pos].clone();
            self.refresh_hops = 0;
            self.go(&t, false);
        }
    }

    fn reload(&mut self) {
        let t = self.current_url.clone();
        self.refresh_hops = 0;
        self.go(&t, false);
    }

    fn follow_link(&mut self, href: &str) {
        let h = href.trim();
        if h.is_empty() || h.starts_with('#') {
            return;
        }
        let target = if h.eq_ignore_ascii_case("about:home")
            || h.eq_ignore_ascii_case("about:debug")
            || h.eq_ignore_ascii_case("about:compat")
        {
            String::from(h)
        } else {
            resolve_url(&self.current_url, h)
        };
        self.refresh_hops = 0;
        self.go(&target, true);
    }

    // ---------- inputs and forms (the DOM `value` attribute is the source of truth) ----------

    fn input_value(&self, node: usize) -> String {
        String::from(self.tree.borrow().attr(node, "value").unwrap_or(""))
    }

    fn element_of(&self, node: usize) -> Option<usize> {
        let t = self.tree.borrow();
        let n = t.nodes.get(node)?;
        match n.node_type {
            NodeType::Element { .. } => Some(node),
            NodeType::Text(_) => n.parent,
        }
    }

    fn form_of(&self, node: usize) -> Option<usize> {
        let t = self.tree.borrow();
        let mut cur = Some(node);
        let mut guard = 0;
        while let Some(id) = cur {
            if t.tag(id) == Some("form") {
                return Some(id);
            }
            cur = t.nodes.get(id).and_then(|n| n.parent);
            guard += 1;
            if guard > 500 {
                break;
            }
        }
        None
    }

    fn js_bool(&mut self, name: &str, args: &[Value]) -> bool {
        let r = match self.js.as_mut() {
            Some(js) => run_js_call(js, name, args),
            None => return false,
        };
        self.sync_after_js();
        matches!(r, Value::Bool(true))
    }

    fn edit_input(&mut self, node: usize, value: String) {
        self.tree.borrow_mut().set_attr(node, "value", &value);
        self.layout_dirty = true;
        if self.js.is_some() {
            self.js_bool("__changed", &[Value::Num(node as f64)]);
        }
    }

    fn submit_form(&mut self, action: &str, name: &str, target_seq: Option<usize>) {
        let mut node: Option<usize> = None;
        if let Some(seq) = target_seq {
            for h in self.hits.iter() {
                if let HitKind::Input { seq: s, .. } = &h.kind {
                    if *s == seq {
                        node = Some(h.node);
                        break;
                    }
                }
            }
        }
        if node.is_none() {
            node = self.focused_input;
        }
        let value = match node {
            Some(n) => self.input_value(n),
            None => String::new(),
        };
        let dest = build_form_url(&self.current_url, action, name, &value);
        self.refresh_hops = 0;
        self.go(&dest, true);
    }

    fn enter_in_input(&mut self, node: usize) {
        let mut info: Option<(String, String, usize)> = None;
        for h in self.hits.iter() {
            if h.node == node {
                if let HitKind::Input { seq, action, name } = &h.kind {
                    info = Some((action.clone(), name.clone(), *seq));
                    break;
                }
            }
        }
        let (action, name, seq) = match info {
            Some(i) => i,
            None => return,
        };
        if self.js.is_some() {
            if self.js_bool("__key", &[Value::Num(node as f64), Value::str("Enter")]) {
                return;
            }
            if let Some(form) = self.form_of(node) {
                if self.js_bool("__submit", &[Value::Num(form as f64)]) {
                    return;
                }
            }
        }
        self.submit_form(&action, &name, Some(seq));
    }

    fn handle_click(&mut self, rel_x: i32, rel_y: i32) {
        let chosen = self.hits.iter().rev().find(|h| inside(h.rect, rel_x, rel_y)).cloned();
        self.focused_input = None;
        let h = match chosen {
            Some(h) => h,
            None => return,
        };
        if self.js.is_some() {
            if let Some(el) = self.element_of(h.node) {
                if self.js_bool("__click", &[Value::Num(el as f64)]) {
                    return;
                }
            }
        }
        match h.kind {
            HitKind::Input { .. } => {
                self.focused_input = Some(h.node);
            }
            HitKind::Submit { action, name, target } => {
                if self.js.is_some() {
                    if let Some(form) = self.form_of(h.node) {
                        if self.js_bool("__submit", &[Value::Num(form as f64)]) {
                            return;
                        }
                    }
                }
                self.submit_form(&action, &name, target);
            }
            HitKind::Node(Some(href)) => self.follow_link(&href),
            HitKind::Node(None) => {}
        }
    }

    fn refresh_target(&self) -> Option<String> {
        let t = self.tree.borrow();
        for node in t.nodes.iter() {
            if let NodeType::Element { tag, .. } = &node.node_type {
                if tag != "meta" {
                    continue;
                }
                let equiv = t.attr(node.id, "http-equiv").unwrap_or("");
                if !equiv.eq_ignore_ascii_case("refresh") {
                    continue;
                }
                let content = t.attr(node.id, "content").unwrap_or("");
                let lower = content.to_ascii_lowercase();
                if let Some(pos) = lower.find("url=") {
                    let raw = content[pos + 4..].trim().trim_matches('\'').trim_matches('"');
                    if !raw.is_empty() {
                        return Some(resolve_url(&self.current_url, raw));
                    }
                }
            }
        }
        None
    }

    // ---------- page loading pipeline: main document -> css -> scripts -> images ----------

    fn on_main_loaded(&mut self) {
        let status = self.client.status_code;
        let final_url = self.final_url_from_client();
        let mut body = self.client.html_body.clone();
        if body.len() > MAX_HTML_BYTES {
            let mut cut = MAX_HTML_BYTES;
            while cut > 0 && !body.is_char_boundary(cut) {
                cut -= 1;
            }
            body.truncate(cut);
        }
        self.last_status = status;
        self.page_bytes = body.len();

        if body.trim().is_empty() {
            let reason = alloc::format!("EMPTY RESPONSE (HTTP {})", status);
            self.show_error(&final_url, &reason);
            return;
        }

        if self.view_source {
            self.view_source = false;
            let page = view_source_page(&body, &final_url);
            self.show_html(&page, &final_url);
            self.url_input = alloc::format!("view-source:{}", final_url);
            return;
        }

        let (bytes, code) = (body.len(), status);
        self.show_html(&body, &final_url);
        self.last_status = code;
        self.page_bytes = bytes;

        if self.refresh_hops < MAX_REFRESH_HOPS {
            let visible_len = {
                let t = self.tree.borrow();
                t.text_content(t.root).trim().len()
            };
            if visible_len < 80 {
                if let Some(next) = self.refresh_target() {
                    if next != final_url {
                        self.refresh_hops += 1;
                        self.go(&next, false);
                        return;
                    }
                }
            }
        }

        self.queue_new_resources();
        self.active = Active::None;
    }

    /// Finds stylesheets and images in the DOM that have not been requested yet.
    fn queue_new_resources(&mut self) {
        let base = self.current_url.clone();
        let (links, imgs) = {
            let t = self.tree.borrow();
            (t.collect_stylesheet_links(), t.collect_image_sources())
        };
        for l in links {
            let full = resolve_url(&base, &l);
            if (full.starts_with("http://") || full.starts_with("https://")) && !self.css_tried.contains(&full) && self.css_tried.len() < MAX_CSS_TRIED {
                self.css_tried.push(full.clone());
                self.css_queue.insert(0, full);
            }
        }
        for src in imgs {
            if let Some(reason) = image_skip_reason(&src) {
                if !reason.is_empty() && self.img_skipped.len() < 200 && !self.img_skipped.iter().any(|(k, _)| *k == src) {
                    self.img_skipped.push((src.clone(), reason));
                }
                continue;
            }
            if self.img_tried.contains(&src) || self.images.iter().any(|(k, _)| *k == src) {
                continue;
            }
            if self.img_tried.len() >= MAX_IMG_TRIED {
                break;
            }
            let full = resolve_url(&base, &src);
            if full.starts_with("http://") || full.starts_with("https://") {
                self.img_tried.push(src.clone());
                self.img_queue.insert(0, (src, full));
                self.img_total += 1;
            }
        }
    }

    fn ensure_js(&mut self) {
        if self.js.is_none() {
            let (it, host) = create_runtime(self.tree.clone(), &self.current_url, self.cookies.clone());
            self.js = Some(it);
            self.jshost = Some(host);
        }
    }

    /// Pulls state changed by scripts back into the page (layout, title, stylesheet, navigation).
    fn sync_after_js(&mut self) {
        let host = match &self.jshost {
            Some(h) => h.clone(),
            None => return,
        };
        if host.dirty.replace(false) {
            self.layout_dirty = true;
            let title = self.tree.borrow().title();
            if let Some(t) = title {
                self.page_title = t;
            }
            if self.style_signature() != self.style_sig {
                self.rebuild_stylesheet();
            }
        }
        let nav = host.navigate.borrow_mut().take();
        if let Some(u) = nav {
            self.pending_nav = Some(u);
        }
    }

    fn fire_ready(&mut self) {
        if let Some(js) = self.js.as_mut() {
            run_js_call(js, "__fire_ready", &[]);
        }
        self.sync_after_js();
    }

    fn fire_load(&mut self, node: usize, ok: bool) {
        if let Some(js) = self.js.as_mut() {
            run_js_call(js, "__fire_load", &[Value::Num(node as f64), Value::Bool(ok)]);
        }
        self.sync_after_js();
    }

    /// Runs inline scripts in document order; starts a fetch (and returns true) at the first external one.
    fn advance_scripts(&mut self) -> bool {
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > 400 || self.pending_nav.is_some() {
                return false;
            }
            let first = {
                let t = self.tree.borrow();
                pending_scripts(&t, &self.scripts_done).into_iter().next()
            };
            let item = match first {
                Some(i) => i,
                None => {
                    if self.js.is_some() && !self.js_ready {
                        self.js_ready = true;
                        self.fire_ready();
                        continue;
                    }
                    return false;
                }
            };
            if self.scripts_done.len() >= MAX_SCRIPTS {
                return false;
            }
            self.scripts_done.push(item.node);
            match item.src {
                None => {
                    self.ensure_js();
                    let label = alloc::format!("inline script {}", item.node);
                    if let Some(js) = self.js.as_mut() {
                        run_script(js, &label, &item.code);
                    }
                    self.sync_after_js();
                }
                Some(src) => {
                    let full = resolve_url(&self.current_url, &src);
                    if !(full.starts_with("http://") || full.starts_with("https://")) {
                        continue;
                    }
                    self.ensure_js();
                    self.active = Active::Script(item.node);
                    self.nav(&full);
                    return true;
                }
            }
        }
    }

    fn start_next_fetch(&mut self) {
        if let Some(css_url) = self.css_queue.pop() {
            self.active = Active::Css;
            self.nav(&css_url);
            return;
        }
        if self.advance_scripts() {
            return;
        }
        self.queue_new_resources();
        if let Some(css_url) = self.css_queue.pop() {
            self.active = Active::Css;
            self.nav(&css_url);
            return;
        }
        if let Some((key, full)) = self.img_queue.pop() {
            self.active = Active::Img(key);
            self.nav(&full);
            return;
        }
        self.active = Active::None;
        self.layout_dirty = true;
    }

    fn pump_network(&mut self) -> bool {
        let mut redraw = false;

        if self.active != Active::None && !matches!(self.client.state, HttpState::Idle | HttpState::Complete | HttpState::Failed) {
            let net = net_global();
            self.client.poll(net);
            redraw = true;
        }

        let done = matches!(self.client.state, HttpState::Complete | HttpState::Failed);
        if done && self.active != Active::None {
            if !self.client.set_cookies.is_empty() {
                let jar = self.cookies.clone();
                for (host_name, c) in self.client.set_cookies.iter() {
                    jar.borrow_mut().store_from_response(c, host_name);
                }
                self.client.set_cookies.clear();
            }
            let ok = self.client.state == HttpState::Complete;
            let current = self.active.clone();
            match current {
                Active::Main => {
                    if ok {
                        self.on_main_loaded();
                        self.layout_dirty = true;
                        if self.active == Active::None {
                            self.start_next_fetch();
                        }
                    } else {
                        let url = self.url_input.clone();
                        let reason = match self.client.tls_error {
                            Some(e) => alloc::format!("SECURE CONNECTION REFUSED: {}", e),
                            None => String::from("NETWORK OR TLS ERROR"),
                        };
                        self.show_error(&url, &reason);
                    }
                }
                Active::Css => {
                    if ok {
                        let text = self.client.html_body.clone();
                        self.stylesheet.append_css(&text);
                        self.css_external.push(text);
                    }
                    self.start_next_fetch();
                }
                Active::Script(node) => {
                    let loaded = ok && self.client.status_code < 400;
                    if loaded {
                        let code = self.client.html_body.clone();
                        let full = self.final_url_from_client();
                        let short: String = full.chars().rev().take(70).collect::<Vec<char>>().into_iter().rev().collect();
                        let label = alloc::format!("script#{} {}", node, short);
                        if let Some(js) = self.js.as_mut() {
                            run_script(js, &label, &code);
                        }
                    } else if let Some(h) = &self.jshost {
                        h.note_error(alloc::format!("script {} failed to load (HTTP {})", node, self.client.status_code));
                    }
                    self.sync_after_js();
                    self.fire_load(node, loaded);
                    self.start_next_fetch();
                }
                Active::Img(key) => {
                    if ok && self.client.status_code < 400 {
                        let bytes = self.client.raw_body.clone();
                        if let Some(img) = DecodedImage::from_bytes(&bytes, IMG_MAX_SIDE) {
                            self.images.push((key, img));
                        }
                    }
                    self.img_done += 1;
                    self.start_next_fetch();
                }
                Active::Suggest => {
                    if ok && self.url_focused {
                        let body = self.client.html_body.clone();
                        self.suggestions = parse_suggestions(&body);
                    }
                    self.active = Active::None;
                }
                Active::None => {}
            }
            redraw = true;
        }
        if self.active == Active::None {
            let before = self.active.clone();
            self.maybe_start_suggest();
            if self.active != before {
                redraw = true;
            }
        }
        redraw
    }

    /// Per-frame JavaScript work: timers, JS network requests, resource discovery.
    fn js_frame(&mut self) -> bool {
        if self.js.is_none() {
            return false;
        }
        let mut redraw = false;
        self.js_now += 16.0;
        let idle = self.active == Active::None;
        if let Some(js) = self.js.as_mut() {
            if js.now_ms > self.js_now {
                self.js_now = js.now_ms;
            }
            let n = js.run_timers(self.js_now, 20);
            if n > 0 {
                redraw = true;
            }
            if idle {
                for _ in 0..3 {
                    if !pump_step(js) {
                        break;
                    }
                }
            }
        }
        let was_dirty = self.jshost.as_ref().map(|h| h.dirty.get()).unwrap_or(false);
        self.sync_after_js();
        if was_dirty {
            redraw = true;
            if self.active == Active::None {
                self.start_next_fetch();
            }
        }
        redraw
    }

    fn final_url_from_client(&self) -> String {
        let scheme = if self.client.https { "https" } else { "http" };
        let default_port = if self.client.https { 443 } else { 80 };
        if self.client.remote_port != default_port {
            alloc::format!(
                "{}://{}:{}{}",
                scheme,
                self.client.host,
                self.client.remote_port,
                self.client.path
            )
        } else {
            alloc::format!("{}://{}{}", scheme, self.client.host, self.client.path)
        }
    }


    fn viewport_rect(area: Rect, unit: i32) -> Rect {
        let top = unit * 3;
        let status_h = unit * 5 / 2;
        let h = (area.height - top - status_h - unit / 2).max(unit * 4);
        Rect::new(area.x + unit / 2, area.y + top, (area.width - unit).max(unit * 4), h)
    }

    fn back_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + unit, area.y + unit * 3 / 4, unit * 3, unit * 2)
    }

    fn fwd_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + unit * 4 + unit / 4, area.y + unit * 3 / 4, unit * 3, unit * 2)
    }

    fn reload_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + unit * 7 + unit / 2, area.y + unit * 3 / 4, unit * 3, unit * 2)
    }

    fn url_box_rect(area: Rect, unit: i32) -> Rect {
        let x = area.x + unit * 11;
        let w = (area.width - unit * 11 - unit * 6).max(unit * 6);
        Rect::new(x, area.y + unit * 3 / 4, w, unit * 2)
    }

    fn go_btn_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + area.width - unit * 5, area.y + unit * 3 / 4, unit * 4, unit * 2)
    }


    /// Feeds pointer position and keyboard focus into the DOM so `:hover` / `:focus` / `:focus-within`
    /// rules can match. Only does work for pages whose stylesheet actually uses those selectors.
    fn sync_pointer_state(&mut self, input: &Input, viewport: Rect) {
        if !self.stylesheet.has_state_rules {
            return;
        }
        let mut new_hover: Option<usize> = None;
        if let Some(pointer) = input.pointer {
            if inside(viewport, pointer.x, pointer.y) {
                let rel_x = pointer.x - viewport.x;
                let rel_y = pointer.y - viewport.y + self.scroll_y;
                new_hover = self.hits.iter().rev().find(|h| inside(h.rect, rel_x, rel_y)).map(|h| h.node);
            }
        }
        let new_focus = self.focused_input;
        let changed = {
            let mut t = self.tree.borrow_mut();
            let c = t.hover_node != new_hover || t.focus_node != new_focus;
            t.hover_node = new_hover;
            t.focus_node = new_focus;
            c
        };
        if changed {
            self.layout_dirty = true;
        }
    }

    fn ensure_layout(&mut self, viewport_w: i32, viewport_h: i32) -> bool {
        let width_changed = self.last_layout_w != viewport_w;
        if !self.layout_dirty && !width_changed && self.layout_built {
            return false;
        }
        if !width_changed && self.layout_built && self.tick.wrapping_sub(self.last_layout_tick) < LAYOUT_MIN_TICKS {
            return false;
        }
        // @media rules are evaluated against the real viewport, not a fixed desktop width.
        self.stylesheet.set_viewport(viewport_w, viewport_h);
        set_viewport(viewport_w, viewport_h);
        let mut imgs = core::mem::take(&mut self.images);
        self.layout = LayoutEngine::build(&self.tree.borrow(), &self.stylesheet, &mut imgs, viewport_w);
        self.images = imgs;
        self.last_layout_w = viewport_w;
        self.last_layout_tick = self.tick;
        self.layout_dirty = false;
        self.layout_built = true;
        let mut hits = Vec::new();
        for b in self.layout.boxes.iter() {
            collect_hits(b, &mut hits, 0);
        }
        self.hits = hits;
        true
    }

    fn max_scroll(&self, viewport: Rect) -> i32 {
        (self.layout.total_height - viewport.height).max(0)
    }

    fn clip_to_viewport(&self, r: Rect, vp: Rect) -> Option<Rect> {
        let sy = vp.y + r.y - self.scroll_y;
        let top = sy.max(vp.y + 1);
        let bottom = (sy + r.height).min(vp.y + vp.height - 1);
        let left = (vp.x + r.x).max(vp.x + 1);
        let right = (vp.x + r.x + r.width).min(vp.x + vp.width - 8);
        if bottom <= top || right <= left {
            return None;
        }
        Some(Rect::new(left, top, right - left, bottom - top))
    }

    fn draw_image(&self, canvas: &mut Canvas<'_>, b: &LayoutBox, key: &str, alt: &str, vp: Rect) {
        let found = self.images.iter().find(|(k, _)| k == key);
        let dw = b.rect.width.max(1);
        let dh = b.rect.height.max(1);
        let sy0 = vp.y + b.rect.y - self.scroll_y;
        let sx0 = vp.x + b.rect.x;
        if sy0 + dh < vp.y || sy0 > vp.y + vp.height {
            return;
        }
        match found {
            Some((_, img)) => {
                let sw = img.width as i32;
                let sh = img.height as i32;
                if sw <= 0 || sh <= 0 || img.rgba.len() < (sw * sh * 4) as usize {
                    return;
                }
                let step: i32 = if dw * dh > 90_000 { 3 } else if dw * dh > 30_000 { 2 } else { 1 };
                let mut y = 0;
                while y < dh {
                    let py = sy0 + y;
                    if py >= vp.y + 1 && py + step <= vp.y + vp.height - 1 {
                        let src_y = (y * sh / dh).min(sh - 1);
                        let mut x = 0;
                        while x < dw {
                            let px = sx0 + x;
                            if px + step <= vp.x + vp.width - 8 {
                                let src_x = (x * sw / dw).min(sw - 1);
                                let o = ((src_y * sw + src_x) * 4) as usize;
                                let a = img.rgba[o + 3];
                                if a >= 48 {
                                    let c = rgb(img.rgba[o], img.rgba[o + 1], img.rgba[o + 2]);
                                    canvas.fill_rect(px, py, step, step, c);
                                }
                            }
                            x += step;
                        }
                    }
                    y += step;
                }
            }
            None => {
                if let Some(r) = self.clip_to_viewport(b.rect, vp) {
                    draw_panel(canvas, r, 4, PANEL_FILL, 255, PANEL_BORDER, 255);
                    let st = text_style(11, 30);
                    let label = fit_head(alt, &st, b.rect.width - 12);
                    let ty = sy0 + (dh - st.cap_height) / 2;
                    if ty >= vp.y + 1 && ty + st.cap_height <= vp.y + vp.height - 1 {
                        draw_text_color(canvas, &label, sx0 + 6, ty, &st, TEXT_DIM);
                    }
                }
            }
        }
    }


    fn draw_box(
        &self,
        canvas: &mut Canvas<'_>,
        b: &LayoutBox,
        vp: Rect,
        under_bg: u32,
        accent: u32,
        depth: usize,
    ) {
        if depth > 40 {
            return;
        }
        match &b.widget {
            WidgetKind::HorizontalRule => {
                let sy = vp.y + b.rect.y - self.scroll_y;
                if sy >= vp.y && sy < vp.y + vp.height {
                    let w = b.rect.width.min(vp.width - b.rect.x - 8).max(0);
                    canvas.fill_rect(vp.x + b.rect.x, sy, w, 1, PANEL_BORDER);
                }
                return;
            }
            WidgetKind::Image { src, alt } => {
                self.draw_image(canvas, b, src, alt, vp);
                return;
            }
            WidgetKind::TextInput {
                input_id,
                value,
                placeholder,
                ..
            } => {
                let sy = vp.y + b.rect.y - self.scroll_y;
                if sy >= vp.y + 1 && sy + b.rect.height <= vp.y + vp.height - 1 {
                    let r = Rect::new(vp.x + b.rect.x, sy, b.rect.width, b.rect.height);
                    let focused = self.focused_input == Some(b.node_id);
                    let border = if focused { accent } else { PANEL_BORDER };
                    draw_panel(canvas, r, r.height / 3, WHITE, 255, border, 255);
                    let st = text_style(12, 30);
                    let typed = value.clone();
                    let inner_w = r.width - 16;
                    let (shown, col) = if typed.is_empty() {
                        (fit_head(placeholder, &st, inner_w), TEXT_DIM)
                    } else {
                        (fit_tail(&typed, &st, inner_w), TEXT_PRIMARY)
                    };
                    let ty = r.y + (r.height - st.cap_height) / 2;
                    draw_text_color(canvas, &shown, r.x + 8, ty, &st, col);
                    if focused {
                        let cx = r.x + 8 + text_width(&shown, &st) + 2;
                        if cx < r.x + r.width - 4 {
                            canvas.fill_rect(cx, r.y + 6, 1, r.height - 12, accent);
                        }
                    }
                }
                return;
            }
            WidgetKind::SubmitButton { label, .. } => {
                let sy = vp.y + b.rect.y - self.scroll_y;
                if sy >= vp.y + 1 && sy + b.rect.height <= vp.y + vp.height - 1 {
                    let r = Rect::new(vp.x + b.rect.x, sy, b.rect.width, b.rect.height);
                    draw_panel(canvas, r, r.height / 3, accent, 255, accent, 255);
                    let st = text_style(11, 30);
                    let shown = fit_head(label, &st, r.width - 10);
                    let tw = text_width(&shown, &st);
                    let tx = r.x + (r.width - tw) / 2;
                    let ty = r.y + (r.height - st.cap_height) / 2;
                    draw_text_color(canvas, &shown, tx, ty, &st, WHITE);
                }
                return;
            }
            WidgetKind::None => {}
        }

        if let Some(text) = &b.text {
            let st = page_text_style(&b.style);
            let sy = vp.y + b.rect.y - self.scroll_y + (b.rect.height - st.cap_height).max(0) / 2;
            let sx = vp.x + b.rect.x;
            if sy >= vp.y + 1
                && sy + st.cap_height <= vp.y + vp.height - 1
                && sx < vp.x + vp.width - 8
            {
                draw_text_color(canvas, text, sx, sy, &st, b.style.color);
                let deco = b.style.text_decoration;
                if deco != 0 {
                    let tw = text_width(text, &st).min(vp.x + vp.width - 8 - sx);
                    if deco & DECO_UNDERLINE != 0 {
                        canvas.fill_rect(sx, sy + st.cap_height + 2, tw, 1, b.style.color);
                    }
                    if deco & DECO_LINE_THROUGH != 0 {
                        canvas.fill_rect(sx, sy + st.cap_height / 2, tw, 1, b.style.color);
                    }
                    if deco & DECO_OVERLINE != 0 {
                        canvas.fill_rect(sx, sy - 2, tw, 1, b.style.color);
                    }
                }
            }
            return;
        }

        let mut child_bg = under_bg;
        if b.style.sides_custom {
            // Different border per side (e.g. only `border-bottom`): background first, then four strips.
            if let Some(bg) = b.style.bg_color {
                if bg != under_bg {
                    if let Some(r) = self.clip_to_viewport(b.rect, vp) {
                        let radius = b.style.border_radius.max(0).min(r.height / 2);
                        draw_panel(canvas, r, radius, bg, 255, bg, 0);
                    }
                }
                child_bg = bg;
            }
            let rc = b.rect;
            let w = b.style.border_sides;
            let strips = [
                Rect::new(rc.x, rc.y, rc.width, w[0]),
                Rect::new(rc.x + rc.width - w[1], rc.y, w[1], rc.height),
                Rect::new(rc.x, rc.y + rc.height - w[2], rc.width, w[2]),
                Rect::new(rc.x, rc.y, w[3], rc.height),
            ];
            for i in 0..4 {
                if w[i] > 0 {
                    let col = b.style.border_side_colors[i].unwrap_or(b.style.color);
                    if let Some(r) = self.clip_to_viewport(strips[i], vp) {
                        canvas.fill_rect(r.x, r.y, r.width, r.height, col);
                    }
                }
            }
        } else if let Some(bg) = b.style.bg_color {
            let has_border = b.style.border_width > 0 && b.style.border_color.is_some();
            if bg != under_bg || has_border {
                if let Some(r) = self.clip_to_viewport(b.rect, vp) {
                    let radius = b.style.border_radius.max(0).min(r.height / 2);
                    if has_border {
                        let bc = b.style.border_color.unwrap_or(bg);
                        draw_panel(canvas, r, radius, bg, 255, bc, 255);
                    } else {
                        draw_panel(canvas, r, radius, bg, 255, bg, 0);
                    }
                }
            }
            child_bg = bg;
        } else if b.style.border_width > 0 {
            if let Some(bc) = b.style.border_color {
                if let Some(r) = self.clip_to_viewport(b.rect, vp) {
                    let radius = b.style.border_radius.max(0).min(r.height / 2);
                    draw_panel(canvas, r, radius, under_bg, 255, bc, 255);
                }
            }
        }

        // `overflow: hidden` boxes skip children that lie completely outside them. Positioned
        // (absolute / fixed) boxes paint after normal flow so they appear on top, like in a browser.
        let clip = b.style.overflow_clip;
        let box_bottom = b.rect.y + b.rect.height;
        for pass in 0..2 {
            for c in b.children.iter() {
                if c.style.is_out_of_flow() != (pass == 1) {
                    continue;
                }
                if clip && (c.rect.y >= box_bottom || c.rect.y + c.rect.height <= b.rect.y) {
                    continue;
                }
                self.draw_box(canvas, c, vp, child_bg, accent, depth + 1);
            }
        }
    }


    fn status_label(&self) -> String {
        let phase = match self.client.state {
            HttpState::Idle => "READY",
            HttpState::WaitingNetwork => "WAITING FOR NETWORK",
            HttpState::ResolvingDns => "RESOLVING DNS",
            HttpState::ConnectingSyn => "CONNECTING",
            HttpState::Handshaking => "TLS HANDSHAKE",
            HttpState::Connected | HttpState::RequestSent => "WAITING FOR SERVER",
            HttpState::ReceivingData => "RECEIVING",
            HttpState::Complete => "DONE",
            HttpState::Failed => "FAILED",
        };
        let mut s = match &self.active {
            Active::None => {
                if self.img_total > 0 {
                    alloc::format!("READY  IMAGES {}/{}", self.images.len(), self.img_total)
                } else {
                    String::from("READY")
                }
            }
            Active::Main => alloc::format!("PAGE: {}", phase),
            Active::Css => alloc::format!("STYLE: {}", phase),
            Active::Script(_) => alloc::format!("SCRIPT {}: {}", self.scripts_done.len(), phase),
            Active::Img(_) => alloc::format!("IMAGE {}/{}: {}", self.img_done + 1, self.img_total, phase),
            Active::Suggest => alloc::format!("SUGGEST: {}", phase),
        };
        if let Some(h) = &self.jshost {
            let n = h.errors.borrow().len();
            if n > 0 {
                s.push_str(&alloc::format!("  JS ERR {}", n));
            }
        }
        s
    }
}

impl App for BrowserApp {
    fn info(&self) -> &AppInfo {
        &self.info
    }

    fn update(&mut self, ctx: &mut Context, input: &Input) -> Update {
        let unit = ctx.unit;
        self.tick = self.tick.wrapping_add(1);
        self.cookies.borrow_mut().tick(16.0);
        let mut redraw = self.pump_network();
        if self.js_frame() {
            redraw = true;
        }
        if let Some(u) = self.pending_nav.take() {
            let full = resolve_url(&self.current_url, &u);
            self.go(&full, true);
            redraw = true;
        }

        let viewport = Self::viewport_rect(input.area, unit);
        self.sync_pointer_state(input, viewport);
        if self.ensure_layout(viewport.width, viewport.height) {
            redraw = true;
        }
        let max_scroll = self.max_scroll(viewport);
        self.scroll_y = self.scroll_y.max(0).min(max_scroll);

        if let Some(key) = input.key {
            match key {
                KeyCode::Up => {
                    if !self.url_focused {
                        self.scroll_y = (self.scroll_y - unit * 3).max(0);
                        redraw = true;
                    }
                }
                KeyCode::Down => {
                    if !self.url_focused {
                        self.scroll_y = (self.scroll_y + unit * 3).min(max_scroll);
                        redraw = true;
                    }
                }
                KeyCode::Backspace => {
                    if self.url_focused {
                        self.url_input.pop();
                        self.note_typed();
                        redraw = true;
                    } else if let Some(n) = self.focused_input {
                        let mut v = self.input_value(n);
                        v.pop();
                        self.edit_input(n, v);
                        redraw = true;
                    }
                }
                KeyCode::Enter => {
                    if self.url_focused {
                        let t = self.url_input.clone();
                        self.refresh_hops = 0;
                        self.go(&t, true);
                        self.url_focused = false;
                        redraw = true;
                    } else if let Some(n) = self.focused_input {
                        self.enter_in_input(n);
                        redraw = true;
                    }
                }
                KeyCode::Char(ch) => {
                    if self.url_focused {
                        self.url_input.push(ch);
                        self.note_typed();
                        redraw = true;
                    } else if let Some(n) = self.focused_input {
                        let mut v = self.input_value(n);
                        if v.len() < 256 {
                            v.push(ch);
                        }
                        self.edit_input(n, v);
                        redraw = true;
                    }
                }
                _ => {}
            }
        }

        let mut n_back = false;
        let mut n_fwd = false;
        let mut n_reload = false;
        let mut n_go = false;

        if let Some(pointer) = input.pointer {
            let url_r = Self::url_box_rect(input.area, unit);
            n_back = inside(Self::back_rect(input.area, unit), pointer.x, pointer.y);
            n_fwd = inside(Self::fwd_rect(input.area, unit), pointer.x, pointer.y);
            n_reload = inside(Self::reload_rect(input.area, unit), pointer.x, pointer.y);
            n_go = inside(Self::go_btn_rect(input.area, unit), pointer.x, pointer.y);

            if pointer.clicked {
                redraw = true;
                let in_url = inside(url_r, pointer.x, pointer.y);
                let mut picked: Option<String> = None;
                if self.url_focused {
                    for (i, sg) in self.suggestions.iter().enumerate() {
                        if inside(Self::suggestion_rect(input.area, unit, i), pointer.x, pointer.y) {
                            picked = Some(sg.clone());
                            break;
                        }
                    }
                }
                if let Some(text) = picked {
                    self.url_input = text.clone();
                    self.refresh_hops = 0;
                    self.go(&text, true);
                    self.url_focused = false;
                    self.suggestions.clear();
                    return Update { redraw: true, close: false, install_app: None };
                }
                self.url_focused = in_url;
                if !in_url {
                    self.suggestions.clear();
                }
                if n_back {
                    self.go_back();
                } else if n_fwd {
                    self.go_forward();
                } else if n_reload {
                    self.reload();
                } else if n_go {
                    let t = self.url_input.clone();
                    self.refresh_hops = 0;
                    self.go(&t, true);
                    self.url_focused = false;
                } else if inside(viewport, pointer.x, pointer.y) {
                    let track_x = viewport.x + viewport.width - unit;
                    if pointer.x >= track_x && max_scroll > 0 {
                        let rel = (pointer.y - viewport.y).max(0).min(viewport.height);
                        self.scroll_y = (rel * max_scroll / viewport.height.max(1)).max(0).min(max_scroll);
                    } else {
                        let rel_x = pointer.x - viewport.x;
                        let rel_y = pointer.y - viewport.y + self.scroll_y;
                        self.handle_click(rel_x, rel_y);
                    }
                }
            }
        }

        if let Some(u) = self.pending_nav.take() {
            let full = resolve_url(&self.current_url, &u);
            self.go(&full, true);
            redraw = true;
        }
        if self.ensure_layout(viewport.width, viewport.height) {
            redraw = true;
        }

        if n_back != self.hover_back || n_fwd != self.hover_fwd || n_reload != self.hover_reload || n_go != self.hover_go {
            self.hover_back = n_back;
            self.hover_fwd = n_fwd;
            self.hover_reload = n_reload;
            self.hover_go = n_go;
            redraw = true;
        }

        Update { redraw, close: false, install_app: None }
    }

    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        let unit = ctx.unit;
        let accent = ctx.settings.accent_color();

        let toolbar = Rect::new(area.x + unit / 2, area.y + unit / 3, area.width - unit, unit * 11 / 4);
        draw_panel(canvas, toolbar, unit / 3, SIDEBAR, 255, PANEL_BORDER, 255);

        draw_button(canvas, Self::back_rect(area, unit), "B", self.hover_back, false);
        draw_button(canvas, Self::fwd_rect(area, unit), "F", self.hover_fwd, false);
        draw_button(canvas, Self::reload_rect(area, unit), "R", self.hover_reload, false);
        draw_input_box(canvas, Self::url_box_rect(area, unit), &self.url_input, self.url_focused);
        draw_accent_button(canvas, Self::go_btn_rect(area, unit), "GO", self.hover_go, accent);

        let viewport = Self::viewport_rect(area, unit);
        draw_panel(canvas, viewport, unit / 3, self.layout.page_bg, 255, PANEL_BORDER, 255);

        for b in self.layout.boxes.iter() {
            self.draw_box(canvas, b, viewport, self.layout.page_bg, accent, 0);
        }

        let max_scroll = self.max_scroll(viewport);
        if max_scroll > 0 {
            let track_h = viewport.height - 8;
            let thumb_h = (track_h * viewport.height / self.layout.total_height.max(1)).max(18).min(track_h);
            let thumb_y = viewport.y + 4 + (track_h - thumb_h) * self.scroll_y / max_scroll.max(1);
            canvas.fill_rect(viewport.x + viewport.width - 6, thumb_y, 4, thumb_h, PANEL_BORDER);
        }

        let status_h = unit * 5 / 2;
        let status_y = area.y + area.height - status_h;
        let status_bar = Rect::new(area.x + unit / 2, status_y, area.width - unit, status_h - unit / 4);
        draw_panel(canvas, status_bar, unit / 3, SIDEBAR, 255, PANEL_BORDER, 255);

        let small = text_style(unit * 50 / 100, 30);
        let label = self.status_label();
        let ty = status_bar.y + (status_bar.height - small.cap_height) / 2;
        draw_text_color(canvas, &label, status_bar.x + unit / 2, ty, &small, TEXT_MUTED);

        let badge_w = (unit * 16).min(status_bar.width / 2);
        let badge_h = status_bar.height - unit / 2;
        let title_style = text_style(unit * 50 / 100, 30);
        let title = fit_head(&sanitize_text(&self.page_title), &title_style, badge_w - unit);
        draw_badge(
            canvas,
            Rect::new(
                status_bar.x + status_bar.width - badge_w - unit / 4,
                status_bar.y + unit / 4,
                badge_w,
                badge_h,
            ),
            &title,
            ACCENT_BG,
            accent,
        );

        if self.url_focused && !self.suggestions.is_empty() {
            let row_h = unit * 2;
            let url = Self::url_box_rect(area, unit);
            let all = Rect::new(url.x, url.y + url.height + 2, url.width, row_h * self.suggestions.len() as i32);
            draw_panel(canvas, all, unit / 3, WHITE, 255, PANEL_BORDER, 255);
            let st = text_style(unit * 55 / 100, 30);
            for (i, sg) in self.suggestions.iter().enumerate() {
                let r = Self::suggestion_rect(area, unit, i);
                let shown = fit_head(sg, &st, r.width - unit);
                let ty = r.y + (r.height - st.cap_height) / 2;
                draw_text_color(canvas, &shown, r.x + unit / 2, ty, &st, TEXT_PRIMARY);
            }
        }
    }
}

