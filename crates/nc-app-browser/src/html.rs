extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use crate::css::floor_utf8_boundary;
use crate::dom::{DomTree, NodeType};

/// Largest document the parser accepts. Kept equal to the app-level response limit so that
/// the two layers can no longer disagree (it used to be 1 MB here and 3 MB in `lib.rs`).
pub const MAX_HTML_BYTES: usize = 3 * 1024 * 1024;
const MAX_TOKENS: usize = 16_000;
const MAX_DOM_NODES: usize = 6_000;
const MAX_DOM_DEPTH: usize = 28;

#[derive(Debug)]
enum Token {
    StartTag {
        name: String,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    },
    EndTag {
        name: String,
    },
    Text(String),
    /// Raw body of a `<script>` element. Attached only directly under that script element.
    ScriptText(String),
}

pub struct HtmlParser {
    chars: Vec<char>,
    pos: usize,
    script_blobs: Vec<String>,
    diag: Vec<String>,
}

impl HtmlParser {
    pub fn parse(html: &str) -> DomTree {
        let safe_len = floor_utf8_boundary(html, MAX_HTML_BYTES);
        let mut parser = HtmlParser {
            chars: html[..safe_len].chars().collect(),
            pos: 0,
            script_blobs: Vec::new(),
            diag: Vec::new(),
        };
        if safe_len < html.len() {
            parser.diag.push(alloc::format!(
                "document truncated: {} of {} bytes parsed (limit {})",
                safe_len,
                html.len(),
                MAX_HTML_BYTES
            ));
        }
        let tokens = parser.tokenize();
        let mut tree = parser.build_tree(tokens);
        parser.inject_dynamic_web_layers(&mut tree);
        tree.diagnostics.extend(core::mem::take(&mut parser.diag));
        tree
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_ahead(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn next(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn is_void_tag(tag: &str) -> bool {
        matches!(
            tag,
            "area" | "base" | "br" | "col" | "embed" | "hr" | "img"
                | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
        )
    }

    fn is_block_tag(tag: &str) -> bool {
        matches!(
            tag,
            "address" | "article" | "aside" | "blockquote" | "div" | "dl"
                | "fieldset" | "figcaption" | "figure" | "footer" | "form"
                | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "header" | "hr"
                | "main" | "nav" | "ol" | "p" | "pre" | "section" | "table" | "ul"
        )
    }

    fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while self.pos < self.chars.len() && tokens.len() < MAX_TOKENS {
            if self.peek() == Some('<') {
                self.next();
                if self.peek() == Some('!') {
                    self.next();
                    if self.peek() == Some('-') && self.peek_ahead(1) == Some('-') {
                        self.next();
                        self.next();
                        while self.pos < self.chars.len() {
                            if self.peek() == Some('-')
                                && self.peek_ahead(1) == Some('-')
                                && self.peek_ahead(2) == Some('>')
                            {
                                self.pos += 3;
                                break;
                            }
                            self.pos += 1;
                        }
                    } else {
                        while let Some(ch) = self.next() {
                            if ch == '>' {
                                break;
                            }
                        }
                    }
                    continue;
                }

                if self.peek() == Some('?') {
                    while let Some(ch) = self.next() {
                        if ch == '>' {
                            break;
                        }
                    }
                    continue;
                }

                let is_closing = if self.peek() == Some('/') {
                    self.next();
                    true
                } else {
                    false
                };

                let mut tag_name = String::new();
                while let Some(ch) = self.peek() {
                    if ch.is_whitespace() || ch == '>' || ch == '/' {
                        break;
                    }
                    tag_name.push(ch.to_ascii_lowercase());
                    self.next();
                }

                if tag_name.is_empty() {
                    continue;
                }

                if is_closing {
                    while let Some(ch) = self.next() {
                        if ch == '>' {
                            break;
                        }
                    }
                    tokens.push(Token::EndTag { name: tag_name });
                } else {
                    let mut attributes = Vec::new();
                    let mut self_closing = false;
                    loop {
                        self.consume_whitespace();
                        match self.peek() {
                            Some('>') => {
                                self.next();
                                break;
                            }
                            Some('/') => {
                                self.next();
                                if self.peek() == Some('>') {
                                    self.next();
                                    self_closing = true;
                                    break;
                                }
                            }
                            Some(_) => {
                                let (k, v) = self.parse_attribute();
                                if !k.is_empty() {
                                    let clean_v = if k == "href" {
                                        unwrap_redirect_href(&decode_entities(&v))
                                    } else {
                                        // Unicode is preserved in the DOM. Glyph folding for the
                                        // built-in font happens only when text is displayed (see `sanitize_text`).
                                        decode_entities(&v)
                                    };
                                    attributes.push((k, clean_v));
                                }
                            }
                            None => break,
                        }
                    }

                    if Self::is_void_tag(&tag_name) {
                        self_closing = true;
                    }

                    let is_script = !self_closing && (tag_name == "script" || tag_name == "noscript");
                    let is_style_or_title = !self_closing && (tag_name == "style" || tag_name == "title");
                    let closing = alloc::format!("</{}", tag_name);

                    tokens.push(Token::StartTag {
                        name: tag_name,
                        attributes,
                        self_closing,
                    });

                    if is_script {
                        let script_text = self.read_raw_text(&closing);
                        if self.script_blobs.len() < 4
                            && (script_text.contains("ytInitialData") || script_text.contains("videoRenderer"))
                        {
                            self.script_blobs.push(script_text.clone());
                        }
                        // Keep the script body as an unmodified raw text node under <script> so that
                        // `pending_scripts` (jsdom.rs) can execute inline code. The text is never
                        // entity-decoded or folded. <noscript> content stays discarded because
                        // scripting is enabled. <script> has `display:none`, so it is never painted.
                        if closing == "</script" && !script_text.trim().is_empty() {
                            tokens.push(Token::ScriptText(script_text));
                        }
                    } else if is_style_or_title {
                        let raw_content = self.read_raw_text(&closing);
                        if !raw_content.trim().is_empty() {
                            if closing == "</title" {
                                tokens.push(Token::Text(decode_entities(&raw_content)));
                            } else {
                                tokens.push(Token::Text(raw_content));
                            }
                        }
                    }
                }
            } else {
                let mut text = String::new();
                while let Some(ch) = self.peek() {
                    if ch == '<' {
                        break;
                    }
                    text.push(ch);
                    self.next();
                }
                let decoded = decode_entities(&text);
                if !decoded.trim().is_empty() {
                    tokens.push(Token::Text(decoded));
                }
            }
        }
        if self.pos < self.chars.len() {
            self.diag.push(alloc::format!(
                "token limit reached ({} tokens): the rest of the document was not parsed",
                MAX_TOKENS
            ));
        }
        tokens
    }

    fn read_raw_text(&mut self, closing: &str) -> String {
        let mut content = String::new();
        let needle: Vec<char> = closing.chars().collect();
        while self.pos < self.chars.len() {
            if self.chars[self.pos] == '<' && self.pos + needle.len() <= self.chars.len() {
                let mut matched = true;
                for (i, expected) in needle.iter().enumerate() {
                    if self.chars[self.pos + i].to_ascii_lowercase() != *expected {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    return content;
                }
            }
            content.push(self.chars[self.pos]);
            self.pos += 1;
        }
        content
    }

    fn consume_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.next();
            } else {
                break;
            }
        }
    }

    fn parse_attribute(&mut self) -> (String, String) {
        let mut key = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() || ch == '=' || ch == '>' || ch == '/' {
                break;
            }
            key.push(ch.to_ascii_lowercase());
            self.next();
        }
        self.consume_whitespace();
        let mut val = String::new();
        if self.peek() == Some('=') {
            self.next();
            self.consume_whitespace();
            if let Some(quote) = self.peek() {
                if quote == '"' || quote == '\'' {
                    self.next();
                    while let Some(ch) = self.next() {
                        if ch == quote {
                            break;
                        }
                        val.push(ch);
                    }
                } else {
                    while let Some(ch) = self.peek() {
                        if ch.is_whitespace() || ch == '>' || ch == '/' {
                            break;
                        }
                        val.push(ch);
                        self.next();
                    }
                }
            }
        }
        (key, val)
    }

    fn build_tree(&self, tokens: Vec<Token>) -> DomTree {
        let mut tree = DomTree::new();
        let mut stack = alloc::vec![tree.root];
        let mut last_added: Option<usize> = None;

        for token in tokens {
            if tree.nodes.len() >= MAX_DOM_NODES {
                tree.diagnostics.push(alloc::format!(
                    "DOM node limit reached ({} nodes): later elements were dropped",
                    MAX_DOM_NODES
                ));
                break;
            }
            match token {
                Token::StartTag { name, attributes, self_closing } => {
                    if name == "body" {
                        if let Some(pos) = stack.iter().rposition(|&id| tree.tag(id) == Some("head")) {
                            stack.truncate(pos);
                        }
                    } else if Self::is_block_tag(&name) {
                        if let Some(&top_id) = stack.last() {
                            if tree.tag(top_id) == Some("p") && stack.len() > 1 {
                                stack.pop();
                            }
                        }
                    } else if name == "li" {
                        if let Some(&top_id) = stack.last() {
                            if tree.tag(top_id) == Some("li") && stack.len() > 1 {
                                stack.pop();
                            }
                        }
                    } else if name == "td" || name == "th" {
                        if let Some(&top_id) = stack.last() {
                            if (tree.tag(top_id) == Some("td") || tree.tag(top_id) == Some("th")) && stack.len() > 1 {
                                stack.pop();
                            }
                        }
                    } else if name == "tr" {
                        if let Some(pos) = stack.iter().rposition(|&id| tree.tag(id) == Some("tr")) {
                            if pos > 0 {
                                stack.truncate(pos);
                            }
                        }
                    }

                    let parent = *stack.last().unwrap_or(&tree.root);
                    let node_id = tree.add_node(
                        NodeType::Element {
                            tag: name,
                            attributes,
                        },
                        parent,
                    );
                    last_added = Some(node_id);
                    if !self_closing && stack.len() < MAX_DOM_DEPTH {
                        stack.push(node_id);
                    }
                }
                Token::EndTag { name } => {
                    if stack.len() > 1 {
                        let mut found_idx = None;
                        for idx in (1..stack.len()).rev() {
                            if tree.tag(stack[idx]) == Some(name.as_str()) {
                                found_idx = Some(idx);
                                break;
                            }
                        }
                        if let Some(idx) = found_idx {
                            stack.truncate(idx);
                        }
                    }
                }
                Token::Text(text) => {
                    let parent = *stack.last().unwrap_or(&tree.root);
                    tree.add_node(NodeType::Text(text), parent);
                }
                Token::ScriptText(code) => {
                    // Only attach to the <script> element that was just created, never to a visible parent.
                    if let Some(sid) = last_added {
                        if tree.tag(sid) == Some("script") {
                            tree.add_node(NodeType::Text(code), sid);
                        }
                    }
                }
            }
        }
        tree
    }

    fn inject_dynamic_web_layers(&self, tree: &mut DomTree) {
        let mut videos: Vec<(String, String, String)> = Vec::new();
        for blob in &self.script_blobs {
            extract_youtube_videos(blob, &mut videos);
        }

     if !videos.is_empty() {
            let kept: Vec<usize> = tree.nodes[tree.root]
                .children
                .iter()
                .copied()
                .filter(|&cid| tree.tag(cid) == Some("head") || tree.tag(cid) == Some("title"))
                .collect();
            tree.nodes[tree.root].children = kept;

            let container = tree.add_node(
                NodeType::Element {
                    tag: String::from("div"),
                    attributes: alloc::vec![(
                        String::from("style"),
                        String::from("background-color: #0f172a; padding: 12px; border-radius: 8px;")
                    )],
                },
                tree.root,
            );

            let header = tree.add_node(
                NodeType::Element {
                    tag: String::from("h2"),
                    attributes: alloc::vec![(String::from("style"), String::from("color: #f8fafc; margin-bottom: 8px;"))],
                },
                container,
            );
            tree.add_node(NodeType::Text(String::from("YOUTUBE LIVE VIDEO STREAM")), header);

            let form = tree.add_node(
                NodeType::Element {
                    tag: String::from("form"),
                    attributes: alloc::vec![(String::from("action"), String::from("https://www.youtube.com/results"))],
                },
                container,
            );
            tree.add_node(
                NodeType::Element {
                    tag: String::from("input"),
                    attributes: alloc::vec![
                        (String::from("type"), String::from("text")),
                        (String::from("name"), String::from("search_query")),
                        (String::from("placeholder"), String::from("Search YouTube videos...")),
                    ],
                },
                form,
            );
            tree.add_node(
                NodeType::Element {
                    tag: String::from("input"),
                    attributes: alloc::vec![
                        (String::from("type"), String::from("submit")),
                        (String::from("value"), String::from("SEARCH YOUTUBE")),
                    ],
                },
                form,
            );

            for (vid_id, title, thumb_url) in videos.into_iter().take(10) {
                let card = tree.add_node(
                    NodeType::Element {
                        tag: String::from("div"),
                        attributes: alloc::vec![(
                            String::from("style"),
                            String::from("background-color: #1e293b; border: 1px solid #334155; border-radius: 6px; padding: 10px; margin-top: 8px;")
                        )],
                    },
                    container,
                );

                if !thumb_url.is_empty() {
                    tree.add_node(
                        NodeType::Element {
                            tag: String::from("img"),
                            attributes: alloc::vec![
                                (String::from("src"), thumb_url),
                                (String::from("alt"), String::from("VIDEO THUMBNAIL")),
                                (String::from("width"), String::from("120")),
                                (String::from("height"), String::from("90")),
                            ],
                        },
                        card,
                    );
                }

                let link = tree.add_node(
                    NodeType::Element {
                        tag: String::from("a"),
                        attributes: alloc::vec![
                            (String::from("href"), alloc::format!("https://www.youtube.com/watch?v={}", vid_id)),
                            (String::from("style"), String::from("color: #38bdf8; font-size: 14px;")),
                        ],
                    },
                    card,
                );
                tree.add_node(NodeType::Text(title), link);
            }
            return;
        }

        let body_text = tree.text_content(tree.root);
        if body_text.trim().len() < 30 {
            let mut og_title = None;
            let mut og_desc = None;
            let mut og_image = None;

            for node in &tree.nodes {
                if let NodeType::Element { ref tag, .. } = node.node_type {
                    if tag == "meta" {
                        let prop = tree.attr(node.id, "property").or_else(|| tree.attr(node.id, "name")).unwrap_or("");
                        let content = tree.attr(node.id, "content").unwrap_or("");
                        if !content.is_empty() {
                            if prop.eq_ignore_ascii_case("og:title") || prop.eq_ignore_ascii_case("twitter:title") {
                                og_title = Some(String::from(content));
                            } else if prop.eq_ignore_ascii_case("og:description") || prop.eq_ignore_ascii_case("description") {
                                og_desc = Some(String::from(content));
                            } else if prop.eq_ignore_ascii_case("og:image") {
                                og_image = Some(String::from(content));
                            }
                        }
                    }
                }
            }

            let title_fallback = og_title.or_else(|| tree.title());
            if let Some(t) = title_fallback {
                let card = tree.add_node(
                    NodeType::Element {
                        tag: String::from("div"),
                        attributes: alloc::vec![(
                            String::from("style"),
                            String::from("background-color: #f8fafc; border: 1px solid #cbd5e1; border-radius: 8px; padding: 16px; margin-top: 10px;")
                        )],
                    },
                    tree.root,
                );
                let h1 = tree.add_node(
                    NodeType::Element {
                        tag: String::from("h1"),
                        attributes: alloc::vec![(String::from("style"), String::from("color: #0f172a; margin-bottom: 8px;"))],
                    },
                    card,
                );
                tree.add_node(NodeType::Text(t), h1);

                if let Some(desc) = og_desc {
                    let p = tree.add_node(
                        NodeType::Element {
                            tag: String::from("p"),
                            attributes: alloc::vec![(String::from("style"), String::from("color: #475569; margin-bottom: 10px;"))],
                        },
                        card,
                    );
                    tree.add_node(NodeType::Text(desc), p);
                }

                if let Some(img_url) = og_image {
                    tree.add_node(
                        NodeType::Element {
                            tag: String::from("img"),
                            attributes: alloc::vec![
                                (String::from("src"), img_url),
                                (String::from("alt"), String::from("PREVIEW IMAGE")),
                                (String::from("width"), String::from("180")),
                                (String::from("height"), String::from("120")),
                            ],
                        },
                        card,
                    );
                }
            }
        }
    }
}

fn extract_youtube_videos(json: &str, out: &mut Vec<(String, String, String)>) {
    let mut search_from = 0;
    let marker = "\"videoId\":\"";
    while search_from < json.len() {
        let valid_start = floor_utf8_boundary(json, search_from);
        let Some(pos) = json[valid_start..].find(marker) else {
            break;
        };
        let start = valid_start + pos + marker.len();
        let rest = &json[start..];
        let Some(end_q) = rest.find('"') else {
            break;
        };
        let video_id = &rest[..end_q];
        search_from = start + end_q;

        if video_id.len() == 11 && !out.iter().any(|(id, _, _)| id == video_id) {
            let window_end = floor_utf8_boundary(json, (search_from + 1400).min(json.len()));
            let window = &json[search_from..window_end];
            let title = extract_json_field(window, "\"text\":\"")
                .or_else(|| extract_json_field(window, "\"simpleText\":\""))
                .unwrap_or_else(|| alloc::format!("YouTube Video ({})", video_id));
            let thumb = alloc::format!("https://i.ytimg.com/vi/{}/default.jpg", video_id);
            out.push((String::from(video_id), title, thumb));
            if out.len() >= 10 {
                break;
            }
        }
    }
}

fn extract_json_field(slice: &str, key: &str) -> Option<String> {
    let pos = slice.find(key)?;
    let rest = &slice[pos + key.len()..];
    let end = rest.find('"')?;
    let val = rest[..end].replace("\\\"", "\"").replace("\\u0026", "&");
    if val.trim().is_empty() {
        None
    } else {
        Some(val)
    }
}

fn unwrap_redirect_href(href: &str) -> String {
    if let Some(rest) = href.strip_prefix("/url?q=").or_else(|| href.strip_prefix("https://www.google.com/url?q=")) {
        let target = rest.split('&').next().unwrap_or(rest);
        return percent_decode_url(target);
    }
    String::from(href)
}

fn percent_decode_url(input: &str) -> String {
    let mut bytes = Vec::new();
    let raw = input.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' && i + 2 < raw.len() {
            if let Ok(hex) = core::str::from_utf8(&raw[i + 1..i + 3]) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    bytes.push(b);
                    i += 3;
                    continue;
                }
            }
        }
        bytes.push(raw[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Set to `true` once `nc-font` can draw Latin-1 Supplement and Latin Extended-A glyphs
/// (Turkish `ğ ş ı İ ç ö ü`, `é`, `ñ`, ...). Until then they are folded to the nearest ASCII letters
/// *at display time only*. The DOM, `textContent`, form values and URLs always keep the original Unicode.
pub const FONT_LATIN_EXT: bool = false;

/// Display folding for the built-in font. This is the single place that knows the font repertoire.
/// It must be applied to every string that is measured or painted, and to nothing that is stored.
pub fn sanitize_text(input: &str) -> String {
    if input.is_ascii() {
        return String::from(input);
    }
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii() {
            out.push(ch);
            continue;
        }
        if FONT_LATIN_EXT && is_latin_ext(ch) {
            out.push(ch);
            continue;
        }
        match fold_char(ch) {
            Some(mapped) => out.push_str(mapped),
            None => {
                // Unknown letters become a visible placeholder instead of silently changing the text.
                if !is_ignorable(ch) {
                    out.push('?');
                }
            }
        }
    }
    out
}

fn is_latin_ext(ch: char) -> bool {
    let c = ch as u32;
    (0x00C0..=0x024F).contains(&c) && c != 0x00D7 && c != 0x00F7
}

/// Characters that carry no visible glyph of their own: dropped without a placeholder.
fn is_ignorable(ch: char) -> bool {
    let c = ch as u32;
    matches!(c,
        0x00AD
        | 0x0300..=0x036F
        | 0x200B..=0x200F
        | 0x2028..=0x202E
        | 0x2060..=0x206F
        | 0xFE00..=0xFE0F
        | 0xFEFF
        | 0x2600..=0x27BF
        | 0x1F000..=0x1FAFF
        | 0xE0000..=0xE007F)
}

fn fold_char(ch: char) -> Option<&'static str> {
    Some(match ch {
        // Latin letters (Turkish first, then the rest of Latin-1 / Latin Extended-A)
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => "C",
        'ğ' | 'ĝ' | 'ġ' | 'ģ' => "g",
        'Ğ' | 'Ĝ' | 'Ġ' | 'Ģ' => "G",
        'ı' | 'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' => "i",
        'İ' | 'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' => "I",
        'ö' | 'ò' | 'ó' | 'ô' | 'õ' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
        'Ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => "O",
        'ş' | 'ś' | 'ŝ' | 'š' => "s",
        'Ş' | 'Ś' | 'Ŝ' | 'Š' => "S",
        'ü' | 'ù' | 'ú' | 'û' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'Ü' | 'Ù' | 'Ú' | 'Û' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => "U",
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' => "A",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => "E",
        'ď' | 'đ' | 'ð' => "d",
        'Ď' | 'Đ' | 'Ð' => "D",
        'ĥ' | 'ħ' => "h",
        'Ĥ' | 'Ħ' => "H",
        'ĵ' => "j",
        'Ĵ' => "J",
        'ķ' => "k",
        'Ķ' => "K",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' => "L",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' => "N",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'Ŕ' | 'Ŗ' | 'Ř' => "R",
        'ţ' | 'ť' | 'ŧ' => "t",
        'Ţ' | 'Ť' | 'Ŧ' => "T",
        'ŵ' => "w",
        'Ŵ' => "W",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'Ý' | 'Ÿ' | 'Ŷ' => "Y",
        'ź' | 'ż' | 'ž' => "z",
        'Ź' | 'Ż' | 'Ž' => "Z",
        'ß' => "ss",
        'æ' => "ae",
        'Æ' => "AE",
        'œ' => "oe",
        'Œ' => "OE",
        'þ' => "th",
        'Þ' => "TH",
        // Quotes, dashes, spaces, punctuation
        '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' => "'",
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' => "\"",
        '\u{201A}' => ",",
        '\u{2010}'..='\u{2015}' | '\u{2212}' => "-",
        '\u{2026}' => "...",
        '\u{2022}' | '\u{00B7}' | '\u{2219}' => "*",
        '\u{2000}'..='\u{200A}' | '\u{00A0}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => " ",
        '\u{2020}' | '\u{2021}' => "+",
        '\u{2030}' => "%o",
        '\u{00AB}' | '\u{2039}' | '\u{2190}' => "<",
        '\u{00BB}' | '\u{203A}' | '\u{2192}' => ">",
        '\u{2191}' => "^",
        '\u{2193}' => "v",
        '\u{2194}' => "<>",
        // Symbols
        '\u{00A9}' => "(c)",
        '\u{00AE}' => "(R)",
        '\u{2122}' => "TM",
        '\u{20AC}' => "EUR",
        '\u{20BA}' => "TL",
        '\u{00A3}' => "GBP",
        '\u{00A5}' => "JPY",
        '\u{00A2}' => "c",
        '\u{20B9}' => "INR",
        '\u{00B0}' => "deg",
        '\u{00B1}' => "+/-",
        '\u{00D7}' => "x",
        '\u{00F7}' => "/",
        '\u{00BC}' => "1/4",
        '\u{00BD}' => "1/2",
        '\u{00BE}' => "3/4",
        '\u{00B2}' => "2",
        '\u{00B3}' => "3",
        '\u{00B9}' => "1",
        '\u{00B5}' => "u",
        '\u{00A7}' => "S",
        '\u{00B6}' => "P",
        '\u{2116}' => "No",
        '\u{2260}' => "!=",
        '\u{2264}' => "<=",
        '\u{2265}' => ">=",
        '\u{221E}' => "inf",
        _ => return None,
    })
}

pub fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return String::from(input);
    }
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            let mut end = i + 1;
            while end < bytes.len()
                && end - i <= 12
                && bytes[end] != b';'
                && bytes[end] != b'&'
                && !bytes[end].is_ascii_whitespace()
            {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b';' {
                let entity = safe_entity_slice(input, i + 1, end);
                if let Some(decoded_ch) = resolve_entity(entity) {
                    out.push(decoded_ch);
                    i = end + 1;
                    continue;
                }
            }
        }
        let valid_i = floor_utf8_boundary(input, i);
        if let Some(ch) = input[valid_i..].chars().next() {
            out.push(ch);
            i = valid_i + ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

fn safe_entity_slice(s: &str, start: usize, end: usize) -> &str {
    let a = floor_utf8_boundary(s, start);
    let b = floor_utf8_boundary(s, end).max(a);
    &s[a..b]
}

/// HTML named entities for U+00A0..=U+00FF in code point order.
const LATIN1_ENTITIES: [&str; 96] = [
    "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf", "laquo", "not", "shy",
    "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro", "para", "middot", "cedil", "sup1", "ordm",
    "raquo", "frac14", "frac12", "frac34", "iquest", "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig",
    "Ccedil", "Egrave", "Eacute", "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve",
    "Oacute", "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute", "THORN",
    "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring", "aelig", "ccedil", "egrave", "eacute", "ecirc",
    "euml", "igrave", "iacute", "icirc", "iuml", "eth", "ntilde", "ograve", "oacute", "ocirc", "otilde", "ouml",
    "divide", "oslash", "ugrave", "uacute", "ucirc", "uuml", "yacute", "thorn", "yuml",
];

fn resolve_entity(entity: &str) -> Option<char> {
    if let Some(num_str) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
        let code = u32::from_str_radix(num_str, 16).ok()?;
        return Some(char::from_u32(code).filter(|c| *c != '\0').unwrap_or('\u{FFFD}'));
    }
    if let Some(num_str) = entity.strip_prefix('#') {
        let code = num_str.parse::<u32>().ok()?;
        return Some(char::from_u32(code).filter(|c| *c != '\0').unwrap_or('\u{FFFD}'));
    }
    match entity {
        "amp" => return Some('&'),
        "lt" => return Some('<'),
        "gt" => return Some('>'),
        "quot" => return Some('"'),
        "apos" => return Some('\''),
        _ => {}
    }
    if let Some(i) = LATIN1_ENTITIES.iter().position(|n| *n == entity) {
        return char::from_u32(0xA0 + i as u32);
    }
    let code: u32 = match entity {
        "OElig" => 0x152,
        "oelig" => 0x153,
        "Scaron" => 0x160,
        "scaron" => 0x161,
        "Yuml" => 0x178,
        "fnof" => 0x192,
        "circ" => 0x2C6,
        "tilde" => 0x2DC,
        "ensp" => 0x2002,
        "emsp" => 0x2003,
        "thinsp" => 0x2009,
        "zwnj" => 0x200C,
        "zwj" => 0x200D,
        "lrm" => 0x200E,
        "rlm" => 0x200F,
        "ndash" => 0x2013,
        "mdash" => 0x2014,
        "lsquo" => 0x2018,
        "rsquo" => 0x2019,
        "sbquo" => 0x201A,
        "ldquo" => 0x201C,
        "rdquo" => 0x201D,
        "bdquo" => 0x201E,
        "dagger" => 0x2020,
        "Dagger" => 0x2021,
        "bull" => 0x2022,
        "hellip" => 0x2026,
        "permil" => 0x2030,
        "prime" => 0x2032,
        "Prime" => 0x2033,
        "lsaquo" => 0x2039,
        "rsaquo" => 0x203A,
        "euro" => 0x20AC,
        "trade" => 0x2122,
        "larr" => 0x2190,
        "uarr" => 0x2191,
        "rarr" => 0x2192,
        "darr" => 0x2193,
        "harr" => 0x2194,
        "minus" => 0x2212,
        "infin" => 0x221E,
        "ne" => 0x2260,
        "le" => 0x2264,
        "ge" => 0x2265,
        "hearts" => 0x2665,
        "check" => 0x2713,
        "alpha" => 0x3B1,
        "beta" => 0x3B2,
        "gamma" => 0x3B3,
        "delta" => 0x3B4,
        "pi" => 0x3C0,
        "sigma" => 0x3C3,
        "omega" => 0x3C9,
        "Omega" => 0x3A9,
        "Delta" => 0x394,
        _ => return None,
    };
    char::from_u32(code)
}
