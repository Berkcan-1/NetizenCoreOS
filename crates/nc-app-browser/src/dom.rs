extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug)]
pub enum NodeType {
    Element {
        tag: String,
        attributes: Vec<(String, String)>,
    },
    Text(String),
}

#[derive(Clone, Debug)]
pub struct DomNode {
    pub id: usize,
    pub node_type: NodeType,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
}

pub struct DomTree {
    pub nodes: Vec<DomNode>,
    pub root: usize,
    /// Parser notes (size/limit truncation etc.), shown on `about:debug`.
    pub diagnostics: Vec<String>,
    /// Element currently under the pointer (for `:hover` / `:focus-within` style matching).
    pub hover_node: Option<usize>,
    /// Element that has keyboard focus (for `:focus` / `:focus-within`).
    pub focus_node: Option<usize>,
}

impl DomTree {
    pub fn new() -> Self {
        let root_node = DomNode {
            id: 0,
            node_type: NodeType::Element {
                tag: String::from("document"),
                attributes: Vec::new(),
            },
            children: Vec::new(),
            parent: None,
        };
        DomTree {
            nodes: alloc::vec![root_node],
            root: 0,
            diagnostics: Vec::new(),
            hover_node: None,
            focus_node: None,
        }
    }

    pub fn add_node(&mut self, node_type: NodeType, parent_id: usize) -> usize {
        let id = self.nodes.len();
        let node = DomNode {
            id,
            node_type,
            children: Vec::new(),
            parent: Some(parent_id),
        };
        self.nodes.push(node);
        self.nodes[parent_id].children.push(id);
        id
    }

    pub fn tag(&self, node_id: usize) -> Option<&str> {
        match &self.nodes.get(node_id)?.node_type {
            NodeType::Element { tag, .. } => Some(tag.as_str()),
            NodeType::Text(_) => None,
        }
    }

    pub fn attr(&self, node_id: usize, key: &str) -> Option<&str> {
        match &self.nodes.get(node_id)?.node_type {
            NodeType::Element { attributes, .. } => {
                for (k, v) in attributes {
                    if k.eq_ignore_ascii_case(key) {
                        return Some(v.as_str());
                    }
                }
                None
            }
            NodeType::Text(_) => None,
        }
    }

    pub fn id_attr(&self, node_id: usize) -> Option<&str> {
        self.attr(node_id, "id")
    }

    pub fn has_class(&self, node_id: usize, target_class: &str) -> bool {
        if let Some(classes) = self.attr(node_id, "class") {
            for c in classes.split_whitespace() {
                if c.eq_ignore_ascii_case(target_class) {
                    return true;
                }
            }
        }
        false
    }

    pub fn text_content(&self, node_id: usize) -> String {
        let mut out = String::new();
        self.collect_text_recursive(node_id, &mut out);
        out
    }

    fn collect_text_recursive(&self, node_id: usize, out: &mut String) {
        let Some(node) = self.nodes.get(node_id) else {
            return;
        };
        match &node.node_type {
            NodeType::Text(t) => {
                if !out.is_empty() && !out.ends_with(' ') && !t.starts_with(' ') {
                    out.push(' ');
                }
                out.push_str(t);
            }
            NodeType::Element { tag, .. } => {
                if tag == "script" || tag == "style" {
                    return;
                }
                for &child in &node.children {
                    self.collect_text_recursive(child, out);
                }
            }
        }
    }

    pub fn title(&self) -> Option<String> {
        for node in &self.nodes {
            if let NodeType::Element { ref tag, .. } = node.node_type {
                if tag == "title" {
                    let mut t = String::new();
                    for &c in &node.children {
                        if let NodeType::Text(ref s) = self.nodes[c].node_type {
                            t.push_str(s.trim());
                        }
                    }
                    if !t.is_empty() {
                        return Some(t);
                    }
                }
            }
        }
        None
    }

    pub fn collect_style_texts(&self) -> Vec<String> {
        let mut styles = Vec::new();
        for node in &self.nodes {
            if let NodeType::Element { ref tag, .. } = node.node_type {
                if tag == "style" {
                    let mut buf = String::new();
                    for &c in &node.children {
                        if let NodeType::Text(ref s) = self.nodes[c].node_type {
                            buf.push_str(s);
                            buf.push('\n');
                        }
                    }
                    if !buf.trim().is_empty() {
                        styles.push(buf);
                    }
                }
            }
        }
        styles
    }

    pub fn collect_stylesheet_links(&self) -> Vec<String> {
        let mut links = Vec::new();
        for node in &self.nodes {
            if let NodeType::Element { ref tag, .. } = node.node_type {
                if tag == "link" {
                    let rel = self.attr(node.id, "rel").unwrap_or("");
                    let href = self.attr(node.id, "href").unwrap_or("");
                    if (rel.eq_ignore_ascii_case("stylesheet") || href.ends_with(".css")) && !href.is_empty() {
                        links.push(String::from(href));
                    }
                }
            }
        }
        links
    }

    pub fn collect_image_sources(&self) -> Vec<String> {
        let mut srcs = Vec::new();
        for node in &self.nodes {
            if let NodeType::Element { ref tag, .. } = node.node_type {
                if tag == "img" {
                    let src = self.attr(node.id, "src").or_else(|| self.attr(node.id, "data-src")).unwrap_or("");
                    if !src.is_empty() && !srcs.iter().any(|s| s == src) {
                        srcs.push(String::from(src));
                    }
                }
            }
        }
        srcs
    }

    // ---------- mutation API (used by the JS bridge) ----------

    pub fn create_element(&mut self, tag: &str) -> usize {
        let id = self.nodes.len();
        self.nodes.push(DomNode {
            id,
            node_type: NodeType::Element { tag: tag.to_ascii_lowercase(), attributes: Vec::new() },
            children: Vec::new(),
            parent: None,
        });
        id
    }

    pub fn create_text(&mut self, text: &str) -> usize {
        let id = self.nodes.len();
        self.nodes.push(DomNode { id, node_type: NodeType::Text(String::from(text)), children: Vec::new(), parent: None });
        id
    }

    pub fn is_ancestor(&self, ancestor: usize, mut id: usize) -> bool {
        let mut guard = 0;
        loop {
            if id == ancestor {
                return true;
            }
            match self.nodes.get(id).and_then(|n| n.parent) {
                Some(p) => id = p,
                None => return false,
            }
            guard += 1;
            if guard > 10_000 {
                return false;
            }
        }
    }

    pub fn detach(&mut self, id: usize) {
        if id >= self.nodes.len() || id == self.root {
            return;
        }
        if let Some(p) = self.nodes[id].parent {
            self.nodes[p].children.retain(|c| *c != id);
        }
        self.nodes[id].parent = None;
    }

    pub fn append_child(&mut self, parent: usize, child: usize) -> bool {
        self.insert_before(parent, child, None)
    }

    pub fn insert_before(&mut self, parent: usize, child: usize, before: Option<usize>) -> bool {
        if parent >= self.nodes.len() || child >= self.nodes.len() || child == self.root || self.is_ancestor(child, parent) {
            return false;
        }
        if matches!(self.nodes[parent].node_type, NodeType::Text(_)) {
            return false;
        }
        self.detach(child);
        let pos = match before {
            Some(b) => self.nodes[parent].children.iter().position(|c| *c == b),
            None => None,
        };
        match pos {
            Some(i) => self.nodes[parent].children.insert(i, child),
            None => self.nodes[parent].children.push(child),
        }
        self.nodes[child].parent = Some(parent);
        true
    }

    pub fn clear_children(&mut self, id: usize) {
        if id >= self.nodes.len() {
            return;
        }
        let kids = core::mem::take(&mut self.nodes[id].children);
        for k in kids {
            self.nodes[k].parent = None;
        }
    }

    pub fn set_attr(&mut self, id: usize, key: &str, value: &str) {
        if let Some(DomNode { node_type: NodeType::Element { attributes, .. }, .. }) = self.nodes.get_mut(id) {
            for (k, v) in attributes.iter_mut() {
                if k.eq_ignore_ascii_case(key) {
                    *v = String::from(value);
                    return;
                }
            }
            attributes.push((key.to_ascii_lowercase(), String::from(value)));
        }
    }

    pub fn remove_attr(&mut self, id: usize, key: &str) {
        if let Some(DomNode { node_type: NodeType::Element { attributes, .. }, .. }) = self.nodes.get_mut(id) {
            attributes.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
        }
    }

    pub fn attr_names(&self, id: usize) -> Vec<String> {
        match self.nodes.get(id).map(|n| &n.node_type) {
            Some(NodeType::Element { attributes, .. }) => attributes.iter().map(|(k, _)| k.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// Exact concatenation of all descendant text (DOM `textContent`).
    pub fn raw_text(&self, id: usize) -> String {
        let mut out = String::new();
        self.raw_text_into(id, &mut out, 0);
        out
    }

    fn raw_text_into(&self, id: usize, out: &mut String, depth: usize) {
        if depth > 200 {
            return;
        }
        if let Some(n) = self.nodes.get(id) {
            match &n.node_type {
                NodeType::Text(t) => out.push_str(t),
                NodeType::Element { .. } => {
                    for &c in &n.children {
                        self.raw_text_into(c, out, depth + 1);
                    }
                }
            }
        }
    }

    pub fn set_text_content(&mut self, id: usize, text: &str) {
        if id >= self.nodes.len() {
            return;
        }
        if let NodeType::Text(t) = &mut self.nodes[id].node_type {
            *t = String::from(text);
            return;
        }
        self.clear_children(id);
        if !text.is_empty() {
            let t = self.create_text(text);
            self.append_child(id, t);
        }
    }

    /// Deep-copies the children of `src_node` (from another tree) under `dest_parent`.
    pub fn import_children(&mut self, src: &DomTree, src_node: usize, dest_parent: usize) {
        let kids: Vec<usize> = match src.nodes.get(src_node) {
            Some(n) => n.children.clone(),
            None => return,
        };
        for k in kids {
            self.import_node(src, k, dest_parent, 0);
        }
    }

    fn import_node(&mut self, src: &DomTree, id: usize, dest_parent: usize, depth: usize) {
        if depth > 200 {
            return;
        }
        let nt = src.nodes[id].node_type.clone();
        let new_id = self.add_node(nt, dest_parent);
        let kids = src.nodes[id].children.clone();
        for k in kids {
            self.import_node(src, k, new_id, depth + 1);
        }
    }

    pub fn clone_node(&mut self, id: usize, deep: bool) -> usize {
        if id >= self.nodes.len() {
            return 0;
        }
        let nt = self.nodes[id].node_type.clone();
        let new_id = self.nodes.len();
        self.nodes.push(DomNode { id: new_id, node_type: nt, children: Vec::new(), parent: None });
        if deep {
            let kids = self.nodes[id].children.clone();
            for k in kids {
                let c = self.clone_node(k, true);
                self.append_child(new_id, c);
            }
        }
        new_id
    }

    pub fn set_inner_html(&mut self, id: usize, html_text: &str) {
        if id >= self.nodes.len() || matches!(self.nodes[id].node_type, NodeType::Text(_)) {
            return;
        }
        let parsed = crate::html::HtmlParser::parse(html_text);
        let mut base = parsed.root;
        for n in parsed.nodes.iter() {
            if let NodeType::Element { tag, .. } = &n.node_type {
                if tag == "body" {
                    base = n.id;
                    break;
                }
            }
        }
        self.clear_children(id);
        self.import_children(&parsed, base, id);
    }

    pub fn serialize(&self, id: usize, include_self: bool) -> String {
        let mut out = String::new();
        self.serialize_into(id, include_self, &mut out, 0);
        out
    }

    fn serialize_into(&self, id: usize, include_self: bool, out: &mut String, depth: usize) {
        if depth > 200 {
            return;
        }
        let n = match self.nodes.get(id) {
            Some(n) => n,
            None => return,
        };
        match &n.node_type {
            NodeType::Text(t) => {
                let raw_parent = n.parent.and_then(|p| self.tag(p)).map(|t| t == "script" || t == "style").unwrap_or(false);
                if raw_parent {
                    out.push_str(t);
                } else {
                    for c in t.chars() {
                        match c {
                            '&' => out.push_str("&amp;"),
                            '<' => out.push_str("&lt;"),
                            '>' => out.push_str("&gt;"),
                            c => out.push(c),
                        }
                    }
                }
            }
            NodeType::Element { tag, attributes } => {
                if include_self && id != self.root {
                    out.push('<');
                    out.push_str(tag);
                    for (k, v) in attributes {
                        out.push(' ');
                        out.push_str(k);
                        out.push_str("=\"");
                        for c in v.chars() {
                            match c {
                                '&' => out.push_str("&amp;"),
                                '"' => out.push_str("&quot;"),
                                c => out.push(c),
                            }
                        }
                        out.push('"');
                    }
                    out.push('>');
                    if matches!(tag.as_str(), "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr") {
                        return;
                    }
                }
                for &c in &n.children {
                    self.serialize_into(c, true, out, depth + 1);
                }
                if include_self && id != self.root {
                    out.push_str("</");
                    out.push_str(tag);
                    out.push('>');
                }
            }
        }
    }
}
