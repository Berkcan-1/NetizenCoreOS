#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use nc_app::icon::draw_line;
use nc_app::text::Line;
use nc_app::theme::{SIDEBAR, SIDEBAR_HOVER};
use nc_app::{App, AppInfo, Context, Icon, Input, KeyCode, Update};
use nc_font::text_width;
use nc_gfx::{rgb, Canvas, Rect};
use nc_net::{global as net_global, HttpState, TcpHttpClient};
use nc_ui::{
    draw_accent_button, draw_badge, draw_button, draw_card, draw_input_box, draw_panel,
    draw_text_color, text_style, ACCENT_BG, ACCENT_LINE, PANEL_BORDER, PANEL_FILL, TEXT_DIM,
    TEXT_MUTED, TEXT_PRIMARY, WHITE,
};

const MAX_DOM_DEPTH: usize = 28;
const MAX_DOM_NODES: usize = 1600;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DisplayType {
    Block,
    Inline,
    InlineBlock,
    Flex,
    TableRow,
    TableCell,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone)]
pub struct SvgShape {
    pub view_w: i32,
    pub view_h: i32,
    pub segments: Vec<(i32, i32, i32, i32)>,
    pub fill_color: Option<u32>,
}

#[derive(Clone, Copy)]
pub struct ComputedStyle {
    pub bg_color: Option<u32>,
    pub text_color: u32,
    pub border_color: Option<u32>,
    pub border_radius: i32,
    pub padding: i32,
    pub margin_top: i32,
    pub margin_bottom: i32,
    pub max_width: Option<i32>,
    pub center_block: bool,
    pub font_scale: i32,
    pub display: DisplayType,
    pub text_align: TextAlign,
    pub is_card: bool,
    pub is_button: bool,
    pub is_pill: bool,
    pub is_input: bool,
    pub is_link: bool,
}

impl ComputedStyle {
    pub fn default_for_tag(tag: &str) -> Self {
        let mut s = ComputedStyle {
            bg_color: None,
            text_color: TEXT_PRIMARY,
            border_color: None,
            border_radius: 6,
            padding: 0,
            margin_top: 2,
            margin_bottom: 2,
            max_width: None,
            center_block: false,
            font_scale: 60,
            display: DisplayType::Block,
            text_align: TextAlign::Left,
            is_card: false,
            is_button: false,
            is_pill: false,
            is_input: false,
            is_link: false,
        };
        match tag {
            "h1" => {
                s.font_scale = 130;
                s.margin_top = 8;
                s.margin_bottom = 6;
            }
            "h2" => {
                s.font_scale = 98;
                s.margin_top = 6;
                s.margin_bottom = 5;
            }
            "h3" => {
                s.font_scale = 80;
                s.margin_top = 5;
                s.margin_bottom = 4;
            }
            "h4" | "h5" | "h6" => {
                s.font_scale = 68;
                s.margin_top = 4;
                s.margin_bottom = 3;
            }
            "p" => {
                s.font_scale = 60;
                s.margin_top = 3;
                s.margin_bottom = 4;
            }
            "a" => {
                s.display = DisplayType::Inline;
                s.text_color = ACCENT_LINE;
                s.is_link = true;
            }
            "span" | "b" | "strong" | "i" | "em" | "small" | "label" | "abbr" | "time" | "sub" | "sup" => {
                s.display = DisplayType::Inline;
            }
            "svg" | "img" => {
                s.display = DisplayType::InlineBlock;
                s.margin_top = 2;
                s.margin_bottom = 2;
            }
            "button" => {
                s.display = DisplayType::InlineBlock;
                s.is_button = true;
                s.is_pill = true;
                s.padding = 6;
                s.margin_top = 3;
                s.margin_bottom = 3;
                s.text_align = TextAlign::Center;
            }
            "input" | "textarea" | "select" => {
                s.display = DisplayType::InlineBlock;
                s.is_input = true;
                s.padding = 6;
                s.margin_top = 3;
                s.margin_bottom = 3;
            }
            "form" => {
                s.bg_color = Some(rgb(248, 250, 252));
                s.border_color = Some(PANEL_BORDER);
                s.padding = 12;
                s.border_radius = 10;
                s.margin_top = 6;
                s.margin_bottom = 6;
                s.max_width = Some(460);
                s.center_block = true;
                s.is_card = true;
            }
            "nav" | "header" => {
                s.display = DisplayType::Flex;
                s.padding = 6;
                s.margin_bottom = 4;
            }
            "tr" => {
                s.display = DisplayType::TableRow;
                s.margin_top = 1;
                s.margin_bottom = 1;
            }
            "td" | "th" => {
                s.display = DisplayType::TableCell;
                s.padding = 3;
            }
            "pre" | "code" => {
                s.bg_color = Some(rgb(241, 245, 249));
                s.border_color = Some(PANEL_BORDER);
                s.padding = 6;
                s.border_radius = 6;
                s.font_scale = 54;
            }
            "blockquote" => {
                s.bg_color = Some(ACCENT_BG);
                s.border_color = Some(ACCENT_LINE);
                s.padding = 8;
                s.border_radius = 6;
                s.margin_top = 4;
                s.margin_bottom = 4;
            }
            "main" | "article" | "section" => {
                s.padding = 4;
                s.margin_top = 3;
                s.margin_bottom = 3;
            }
            _ => {}
        }
        s
    }
}

#[derive(Clone)]
pub struct CssRule {
    pub selector: String,
    pub properties: Vec<(String, String)>,
}

#[derive(Clone)]
pub struct DomNode {
    pub id: String,
    pub classes: Vec<String>,
    pub tag: String,
    pub text: String,
    pub href: Option<String>,
    pub onclick: Option<String>,
    pub value: String,
    pub placeholder: String,
    pub input_type: String,
    pub name_attr: String,
    pub svg: Option<SvgShape>,
    pub style: ComputedStyle,
    pub inline_styles: Vec<(String, String)>,
    pub children: Vec<DomNode>,
}

impl DomNode {
    pub fn new_element(tag: &str) -> Self {
        DomNode {
            id: String::new(),
            classes: Vec::new(),
            tag: String::from(tag),
            text: String::new(),
            href: None,
            onclick: None,
            value: String::new(),
            placeholder: String::new(),
            input_type: String::new(),
            name_attr: String::new(),
            svg: None,
            style: ComputedStyle::default_for_tag(tag),
            inline_styles: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn new_text(text: &str) -> Self {
        let mut node = DomNode::new_element("#text");
        node.style.display = DisplayType::Inline;
        node.text = String::from(text);
        node
    }

    pub fn find_by_id_mut(&mut self, target_id: &str) -> Option<&mut DomNode> {
        self.find_by_id_mut_bounded(target_id, 0)
    }

    fn find_by_id_mut_bounded(&mut self, target_id: &str, depth: usize) -> Option<&mut DomNode> {
        if depth > MAX_DOM_DEPTH {
            return None;
        }
        let clean = target_id.trim_start_matches('#');
        if (!self.id.is_empty() && self.id == clean) || self.tag == clean {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_by_id_mut_bounded(clean, depth + 1) {
                return Some(found);
            }
        }
        None
    }

    pub fn find_by_id(&self, target_id: &str) -> Option<&DomNode> {
        self.find_by_id_bounded(target_id, 0)
    }

    fn find_by_id_bounded(&self, target_id: &str, depth: usize) -> Option<&DomNode> {
        if depth > MAX_DOM_DEPTH {
            return None;
        }
        let clean = target_id.trim_start_matches('#');
        if (!self.id.is_empty() && self.id == clean) || self.tag == clean {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_id_bounded(clean, depth + 1) {
                return Some(found);
            }
        }
        None
    }

    pub fn set_inner_text(&mut self, new_text: &str) {
        self.text = String::from(new_text);
        self.children.clear();
    }

    pub fn inner_text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out, 0);
        out
    }

    fn collect_text(&self, out: &mut String, depth: usize) {
        if depth > MAX_DOM_DEPTH || self.style.display == DisplayType::None {
            return;
        }
        if !self.text.is_empty() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(&self.text);
        }
        for c in &self.children {
            c.collect_text(out, depth + 1);
        }
    }
}

fn parse_fixed100(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (neg, rest) = if let Some(r) = s.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (false, r)
    } else {
        (false, s)
    };
    let (int_p, frac_p) = match rest.split_once('.') {
        Some((a, b)) => (a, b),
        None => (rest, ""),
    };
    let int_val = if int_p.is_empty() { 0 } else { int_p.parse::<i32>().ok()? };
    let mut frac_val = 0i32;
    let mut mult = 10i32;
    for ch in frac_p.chars().take(2) {
        if ch.is_ascii_digit() {
            frac_val += (ch as i32 - '0' as i32) * mult;
            mult /= 10;
        } else {
            break;
        }
    }
    let res = int_val * 100 + frac_val;
    Some(if neg { -res } else { res })
}

struct SvgPathScanner<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> SvgPathScanner<'a> {
    fn new(d: &'a str) -> Self {
        SvgPathScanner {
            bytes: d.as_bytes(),
            pos: 0,
        }
    }

    fn skip_sep(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b' ' || b == b',' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek_cmd(&mut self) -> Option<u8> {
        self.skip_sep();
        if self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_alphabetic() {
            Some(self.bytes[self.pos])
        } else {
            None
        }
    }

    fn next_cmd(&mut self) -> Option<u8> {
        self.skip_sep();
        if self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_alphabetic() {
            let c = self.bytes[self.pos];
            self.pos += 1;
            Some(c)
        } else {
            None
        }
    }

    fn next_num(&mut self) -> Option<i32> {
        self.skip_sep();
        if self.pos >= self.bytes.len() {
            return None;
        }
        let start = self.pos;
        if self.bytes[self.pos] == b'-' || self.bytes[self.pos] == b'+' {
            self.pos += 1;
        }
        let mut has_dot = false;
        let mut digits = 0usize;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b.is_ascii_digit() {
                self.pos += 1;
                digits += 1;
            } else if b == b'.' && !has_dot {
                has_dot = true;
                self.pos += 1;
            } else {
                break;
            }
        }
        if digits == 0 {
            self.pos = start;
            return None;
        }
        if self.pos < self.bytes.len() && (self.bytes[self.pos] == b'e' || self.bytes[self.pos] == b'E') {
            self.pos += 1;
            if self.pos < self.bytes.len() && (self.bytes[self.pos] == b'-' || self.bytes[self.pos] == b'+') {
                self.pos += 1;
            }
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }
        let token = core::str::from_utf8(&self.bytes[start..self.pos]).ok()?;
        parse_fixed100(token)
    }
}

fn parse_svg_element(attr_str: &str, inner_svg: &str) -> Option<SvgShape> {
    let mut view_w = 2400i32;
    let mut view_h = 2400i32;
    let mut fill_color: Option<u32> = None;

    let mut temp_node = DomNode::new_element("svg");
    parse_attributes(attr_str, &mut temp_node);

    let lower_attrs = attr_str.to_ascii_lowercase();
    if let Some(vb_pos) = lower_attrs.find("viewbox") {
        let after = &attr_str[vb_pos..];
        if let Some(q1) = after.find('"').or_else(|| after.find('\'')) {
            let q_char = after.as_bytes()[q1] as char;
            let rest = &after[q1 + 1..];
            if let Some(q2) = rest.find(q_char) {
                let nums: Vec<i32> = rest[..q2]
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|s| !s.is_empty())
                    .filter_map(parse_fixed100)
                    .collect();
                if nums.len() == 4 && nums[2] > 0 && nums[3] > 0 {
                    view_w = nums[2];
                    view_h = nums[3];
                }
            }
        }
    }

    let mut segments: Vec<(i32, i32, i32, i32)> = Vec::new();
    let mut search = inner_svg;
    while let Some(d_idx) = search.find(" d=").or_else(|| search.find("\td=")).or_else(|| search.find("<path")) {
        let after = &search[d_idx..];
        let Some(d_attr) = after.find("d=") else {
            search = &after[5.min(after.len())..];
            continue;
        };
        let d_rest = &after[d_attr + 2..].trim_start();
        if d_rest.is_empty() {
            break;
        }
        let quote = d_rest.as_bytes()[0];
        if quote != b'"' && quote != b'\'' {
            search = d_rest;
            continue;
        }
        let inside_d = &d_rest[1..];
        let Some(end_q) = inside_d.find(quote as char) else {
            break;
        };
        let path_str = &inside_d[..end_q];
        parse_svg_path_data(path_str, &mut segments);
        search = &inside_d[end_q + 1..];
        if segments.len() > 600 {
            break;
        }
    }

    if let Some(f_pos) = inner_svg.find("fill=\"") {
        let f_rest = &inner_svg[f_pos + 6..];
        if let Some(end_f) = f_rest.find('"') {
            fill_color = parse_css_color(&f_rest[..end_f]);
        }
    }

    if segments.is_empty() {
        None
    } else {
        Some(SvgShape {
            view_w: view_w.max(100),
            view_h: view_h.max(100),
            segments,
            fill_color,
        })
    }
}

fn parse_svg_path_data(d: &str, segments: &mut Vec<(i32, i32, i32, i32)>) {
    let mut scanner = SvgPathScanner::new(d);
    let mut cur_x = 0i32;
    let mut cur_y = 0i32;
    let mut start_x = 0i32;
    let mut start_y = 0i32;
    let mut prev_ctrl_x = 0i32;
    let mut prev_ctrl_y = 0i32;
    let mut cmd = b'M';

    while segments.len() < 600 {
        if let Some(c) = scanner.peek_cmd() {
            cmd = scanner.next_cmd().unwrap_or(b'M');
            if c == b'Z' || c == b'z' {
                if cur_x != start_x || cur_y != start_y {
                    segments.push((cur_x, cur_y, start_x, start_y));
                }
                cur_x = start_x;
                cur_y = start_y;
                continue;
            }
        }

        match cmd {
            b'M' | b'm' => {
                let (Some(x), Some(y)) = (scanner.next_num(), scanner.next_num()) else { break };
                if cmd == b'm' {
                    cur_x += x;
                    cur_y += y;
                } else {
                    cur_x = x;
                    cur_y = y;
                }
                start_x = cur_x;
                start_y = cur_y;
                prev_ctrl_x = cur_x;
                prev_ctrl_y = cur_y;
                cmd = if cmd == b'm' { b'l' } else { b'L' };
            }
            b'L' | b'l' => {
                let (Some(x), Some(y)) = (scanner.next_num(), scanner.next_num()) else { break };
                let nx = if cmd == b'l' { cur_x + x } else { x };
                let ny = if cmd == b'l' { cur_y + y } else { y };
                segments.push((cur_x, cur_y, nx, ny));
                cur_x = nx;
                cur_y = ny;
                prev_ctrl_x = cur_x;
                prev_ctrl_y = cur_y;
            }
            b'H' | b'h' => {
                let Some(x) = scanner.next_num() else { break };
                let nx = if cmd == b'h' { cur_x + x } else { x };
                segments.push((cur_x, cur_y, nx, cur_y));
                cur_x = nx;
                prev_ctrl_x = cur_x;
            }
            b'V' | b'v' => {
                let Some(y) = scanner.next_num() else { break };
                let ny = if cmd == b'v' { cur_y + y } else { y };
                segments.push((cur_x, cur_y, cur_x, ny));
                cur_y = ny;
                prev_ctrl_y = cur_y;
            }
            b'C' | b'c' => {
                let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                    scanner.next_num(),
                    scanner.next_num(),
                    scanner.next_num(),
                    scanner.next_num(),
                    scanner.next_num(),
                    scanner.next_num(),
                ) else {
                    break;
                };
                let (cx1, cy1, cx2, cy2, nx, ny) = if cmd == b'c' {
                    (cur_x + x1, cur_y + y1, cur_x + x2, cur_y + y2, cur_x + x, cur_y + y)
                } else {
                    (x1, y1, x2, y2, x, y)
                };
                let mid_x = (cur_x + 3 * cx1 + 3 * cx2 + nx) / 8;
                let mid_y = (cur_y + 3 * cy1 + 3 * cy2 + ny) / 8;
                segments.push((cur_x, cur_y, mid_x, mid_y));
                segments.push((mid_x, mid_y, nx, ny));
                prev_ctrl_x = cx2;
                prev_ctrl_y = cy2;
                cur_x = nx;
                cur_y = ny;
            }
            b'S' | b's' => {
                let (Some(x2), Some(y2), Some(x), Some(y)) =
                    (scanner.next_num(), scanner.next_num(), scanner.next_num(), scanner.next_num())
                else {
                    break;
                };
                let cx1 = 2 * cur_x - prev_ctrl_x;
                let cy1 = 2 * cur_y - prev_ctrl_y;
                let (cx2, cy2, nx, ny) = if cmd == b's' {
                    (cur_x + x2, cur_y + y2, cur_x + x, cur_y + y)
                } else {
                    (x2, y2, x, y)
                };
                let mid_x = (cur_x + 3 * cx1 + 3 * cx2 + nx) / 8;
                let mid_y = (cur_y + 3 * cy1 + 3 * cy2 + ny) / 8;
                segments.push((cur_x, cur_y, mid_x, mid_y));
                segments.push((mid_x, mid_y, nx, ny));
                prev_ctrl_x = cx2;
                prev_ctrl_y = cy2;
                cur_x = nx;
                cur_y = ny;
            }
            b'Q' | b'q' => {
                let (Some(x1), Some(y1), Some(x), Some(y)) =
                    (scanner.next_num(), scanner.next_num(), scanner.next_num(), scanner.next_num())
                else {
                    break;
                };
                let (cx1, cy1, nx, ny) = if cmd == b'q' {
                    (cur_x + x1, cur_y + y1, cur_x + x, cur_y + y)
                } else {
                    (x1, y1, x, y)
                };
                let mid_x = (cur_x + 2 * cx1 + nx) / 4;
                let mid_y = (cur_y + 2 * cy1 + ny) / 4;
                segments.push((cur_x, cur_y, mid_x, mid_y));
                segments.push((mid_x, mid_y, nx, ny));
                prev_ctrl_x = cx1;
                prev_ctrl_y = cy1;
                cur_x = nx;
                cur_y = ny;
            }
            b'A' | b'a' => {
                let (Some(_), Some(_), Some(_), Some(_), Some(_), Some(x), Some(y)) = (
                    scanner.next_num(),
                    scanner.next_num(),
                    scanner.next_num(),
                    scanner.next_num(),
                    scanner.next_num(),
                    scanner.next_num(),
                    scanner.next_num(),
                ) else {
                    break;
                };
                let nx = if cmd == b'a' { cur_x + x } else { x };
                let ny = if cmd == b'a' { cur_y + y } else { y };
                segments.push((cur_x, cur_y, nx, ny));
                cur_x = nx;
                cur_y = ny;
            }
            _ => break,
        }
    }
}

fn draw_svg_shape(canvas: &mut Canvas<'_>, rect: Rect, svg: &SvgShape, fallback_color: u32) {
    if rect.width <= 2 || rect.height <= 2 || svg.segments.is_empty() {
        return;
    }
    let color = svg.fill_color.unwrap_or(fallback_color);
    let vw = svg.view_w.max(1);
    let vh = svg.view_h.max(1);

    let mut xs = [0i32; 32];
    for py in 0..rect.height {
        let vy = (py * 2 + 1) * vh / (rect.height * 2);
        let mut count = 0usize;
        for &(x0, y0, x1, y1) in &svg.segments {
            if (y0 <= vy && y1 > vy) || (y1 <= vy && y0 > vy) {
                let dy = y1 - y0;
                if dy != 0 && count < xs.len() {
                    let vx = x0 + (vy - y0) * (x1 - x0) / dy;
                    let px = rect.x + vx * rect.width / vw;
                    xs[count] = px;
                    count += 1;
                }
            }
        }
        for i in 1..count {
            let key = xs[i];
            let mut j = i;
            while j > 0 && xs[j - 1] > key {
                xs[j] = xs[j - 1];
                j -= 1;
            }
            xs[j] = key;
        }
        let mut k = 0;
        while k + 1 < count {
            let start_x = xs[k].max(rect.x);
            let end_x = xs[k + 1].min(rect.x + rect.width - 1);
            if end_x >= start_x {
                canvas.fill_rect(start_x, rect.y + py, end_x - start_x + 1, 1, color);
            }
            k += 2;
        }
    }

    for &(x0, y0, x1, y1) in &svg.segments {
        let sx0 = rect.x + x0 * rect.width / vw;
        let sy0 = rect.y + y0 * rect.height / vh;
        let sx1 = rect.x + x1 * rect.width / vw;
        let sy1 = rect.y + y1 * rect.height / vh;
        draw_line(canvas, sx0, sy0, sx1, sy1, 1, color);
    }
}

pub fn parse_css_color(val: &str) -> Option<u32> {
    let v = val.trim().trim_end_matches("!important").trim();
    if let Some(hex) = v.strip_prefix('#') {
        if hex.len() >= 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(rgb(r, g, b));
        } else if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            return Some(rgb(r, g, b));
        }
    }
    if let Some(inner) = v
        .strip_prefix("rgb(")
        .or_else(|| v.strip_prefix("rgba("))
        .and_then(|s| s.strip_suffix(')'))
    {
        let mut parts = inner.split(|c| c == ',' || c == '/');
        let r = parts.next()?.trim().parse::<u8>().ok()?;
        let g = parts.next()?.trim().parse::<u8>().ok()?;
        let b = parts.next()?.trim().parse::<u8>().ok()?;
        return Some(rgb(r, g, b));
    }
    match v.to_ascii_lowercase().as_str() {
        "white" => Some(rgb(255, 255, 255)),
        "black" => Some(rgb(12, 15, 20)),
        "red" => Some(rgb(239, 68, 68)),
        "green" => Some(rgb(16, 185, 129)),
        "blue" => Some(rgb(29, 155, 240)),
        "yellow" => Some(rgb(245, 158, 11)),
        "cyan" | "aqua" => Some(rgb(20, 184, 166)),
        "purple" | "violet" => Some(rgb(139, 92, 246)),
        "orange" => Some(rgb(249, 115, 22)),
        "gray" | "grey" => Some(rgb(100, 116, 139)),
        "lightgray" | "lightgrey" | "silver" => Some(rgb(241, 245, 249)),
        "navy" => Some(rgb(30, 58, 138)),
        "transparent" | "inherit" | "currentcolor" => None,
        _ => None,
    }
}

fn parse_px(val: &str) -> Option<i32> {
    let v = val.trim().trim_end_matches("!important").trim();
    if let Some(rem_str) = v.strip_suffix("rem").or_else(|| v.strip_suffix("em")) {
        let f = parse_fixed100(rem_str.trim())?;
        return Some(f * 16 / 100);
    }
    let clean = v.trim_end_matches("px").trim_end_matches('%').trim();
    let num_part = clean.split_whitespace().next()?;
    let int_part = num_part.split('.').next()?;
    int_part.parse::<i32>().ok()
}

pub fn apply_css_property(style: &mut ComputedStyle, key: &str, val: &str) {
    let k = key.trim().to_ascii_lowercase();
    let v = val.trim();
    match k.as_str() {
        "background" | "background-color" | "bgcolor" => {
            if let Some(c) = parse_css_color(v) {
                style.bg_color = Some(c);
            }
        }
        "color" | "text" => {
            if let Some(c) = parse_css_color(v) {
                style.text_color = c;
            }
        }
        "border-color" => {
            if let Some(c) = parse_css_color(v) {
                style.border_color = Some(c);
            }
        }
        "border" => {
            for token in v.split_whitespace() {
                if let Some(c) = parse_css_color(token) {
                    style.border_color = Some(c);
                }
            }
        }
        "padding" | "padding-top" | "padding-left" => {
            if let Some(p) = parse_px(v) {
                style.padding = p.clamp(0, 36);
            }
        }
        "margin" => {
            if v.contains("auto") {
                style.center_block = true;
            }
            if let Some(m) = parse_px(v) {
                style.margin_top = m.clamp(0, 32);
                style.margin_bottom = m.clamp(0, 32);
            }
        }
        "margin-top" => {
            if let Some(m) = parse_px(v) {
                style.margin_top = m.clamp(0, 32);
            }
        }
        "margin-bottom" => {
            if let Some(m) = parse_px(v) {
                style.margin_bottom = m.clamp(0, 32);
            }
        }
        "max-width" | "width" => {
            if !v.ends_with('%') {
                if let Some(w) = parse_px(v) {
                    if w >= 120 {
                        style.max_width = Some(w.min(960));
                    }
                }
            }
        }
        "border-radius" => {
            if let Some(r) = parse_px(v) {
                if r >= 24 {
                    style.is_pill = true;
                }
                style.border_radius = r.clamp(0, 32);
            }
        }
        "font-size" => {
            if let Some(px) = parse_px(v) {
                style.font_scale = (px * 4).clamp(45, 175);
            }
        }
        "text-align" | "align" => match v.to_ascii_lowercase().as_str() {
            "center" | "middle" => style.text_align = TextAlign::Center,
            "right" => style.text_align = TextAlign::Right,
            _ => style.text_align = TextAlign::Left,
        },
        "display" => match v.to_ascii_lowercase().as_str() {
            "none" => style.display = DisplayType::None,
            "inline" => style.display = DisplayType::Inline,
            "inline-block" | "inline-flex" => style.display = DisplayType::InlineBlock,
            "flex" | "grid" => style.display = DisplayType::Flex,
            _ => style.display = DisplayType::Block,
        },
        "visibility" => {
            if v.eq_ignore_ascii_case("hidden") {
                style.display = DisplayType::None;
            }
        }
        "box-shadow" => {
            if v != "none" {
                style.is_card = true;
            }
        }
        _ => {}
    }
}

pub fn parse_css_stylesheet(css: &str) -> Vec<CssRule> {
    let mut rules = Vec::new();
    let mut cleaned = String::with_capacity(css.len().min(49152));
    let mut rest_c = &css[..css.len().min(49152)];
    while let Some(c_start) = rest_c.find("/*") {
        cleaned.push_str(&rest_c[..c_start]);
        if let Some(c_end) = rest_c[c_start + 2..].find("*/") {
            rest_c = &rest_c[c_start + 2 + c_end + 2..];
        } else {
            rest_c = "";
            break;
        }
    }
    cleaned.push_str(rest_c);

    let mut rest = cleaned.as_str();
    while let Some(open_brace) = rest.find('{') {
        if rules.len() >= 200 {
            break;
        }
        let selector_raw = rest[..open_brace].trim();
        let after_open = &rest[open_brace + 1..];
        if selector_raw.starts_with("@media") || selector_raw.starts_with("@supports") {
            rest = after_open;
            continue;
        }
        let Some(close_brace) = after_open.find('}') else {
            break;
        };
        let body = &after_open[..close_brace];
        rest = &after_open[close_brace + 1..];
        if selector_raw.starts_with('@') {
            continue;
        }
        let mut props = Vec::new();
        for decl in body.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                props.push((String::from(k.trim()), String::from(v.trim())));
            }
        }
        for sel in selector_raw.split(',') {
            let clean_sel = sel.trim();
            if !clean_sel.is_empty() && rules.len() < 200 {
                rules.push(CssRule {
                    selector: String::from(clean_sel),
                    properties: props.clone(),
                });
            }
        }
    }
    rules
}

fn selector_matches(node: &DomNode, selector: &str) -> bool {
    let last_part = selector
        .split(|c: char| c.is_whitespace() || c == '>')
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or(selector);
    let base = last_part.split(':').next().unwrap_or(last_part).trim();
    if base.is_empty() {
        return false;
    }
    if base == "*" {
        return true;
    }
    if let Some(id_name) = base.strip_prefix('#') {
        return node.id.eq_ignore_ascii_case(id_name);
    }
    if let Some(class_name) = base.strip_prefix('.') {
        return node.classes.iter().any(|c| c.eq_ignore_ascii_case(class_name));
    }
    if let Some((tag_p, class_p)) = base.split_once('.') {
        return node.tag.eq_ignore_ascii_case(tag_p)
            && node.classes.iter().any(|c| c.eq_ignore_ascii_case(class_p));
    }
    node.tag.eq_ignore_ascii_case(base)
}

pub fn apply_styles_recursive(node: &mut DomNode, rules: &[CssRule], inherited_color: Option<u32>, depth: usize) {
    if depth > MAX_DOM_DEPTH {
        return;
    }
    if let Some(c) = inherited_color {
        if !node.style.is_link {
            node.style.text_color = c;
        }
    }
    for rule in rules {
        if selector_matches(node, &rule.selector) {
            for (k, v) in &rule.properties {
                apply_css_property(&mut node.style, k, v);
            }
        }
    }
    for class_name in &node.classes {
        let cl = class_name.to_ascii_lowercase();
        if cl.contains("card") || cl.contains("panel") || cl.contains("modal") || cl.contains("box") {
            node.style.is_card = true;
            if node.style.padding == 0 {
                node.style.padding = 10;
            }
        } else if cl.contains("btn") || cl.contains("button") {
            node.style.is_button = true;
            node.style.is_pill = true;
        } else if cl == "row" || cl.contains("flex") || cl.contains("navbar") {
            node.style.display = DisplayType::Flex;
        } else if cl.contains("hidden") || cl == "sr-only" || cl == "visually-hidden" {
            node.style.display = DisplayType::None;
        }
    }
    for i in 0..node.inline_styles.len() {
        let k = node.inline_styles[i].0.clone();
        let v = node.inline_styles[i].1.clone();
        apply_css_property(&mut node.style, &k, &v);
    }
    let next_color = Some(node.style.text_color);
    for child in &mut node.children {
        apply_styles_recursive(child, rules, next_color, depth + 1);
    }
}

fn decode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            while let Some(&c) = chars.peek() {
                chars.next();
                if c == ';' || entity.len() > 10 {
                    break;
                }
                entity.push(c);
            }
            match entity.as_str() {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "quot" => out.push('"'),
                "apos" | "#39" | "#x27" => out.push('\''),
                "nbsp" | "#160" => out.push(' '),
                "copy" => out.push_str("(C)"),
                "mdash" | "ndash" | "#8211" | "#8212" => out.push('-'),
                "bull" | "middot" => out.push('*'),
                _ => {
                    if let Some(num_str) = entity.strip_prefix('#') {
                        let code = if let Some(hex) = num_str.strip_prefix('x').or_else(|| num_str.strip_prefix('X')) {
                            u32::from_str_radix(hex, 16).ok()
                        } else {
                            num_str.parse::<u32>().ok()
                        };
                        if let Some(c) = code.and_then(char::from_u32) {
                            out.push(c);
                        }
                    } else {
                        out.push('&');
                        out.push_str(&entity);
                    }
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(c);
            last_space = false;
        }
    }
    String::from(out.trim())
}

fn parse_attributes(attr_str: &str, node: &mut DomNode) {
    let lower_full = attr_str.to_ascii_lowercase();
    for tok in lower_full.split_whitespace() {
        if tok == "hidden" || tok.starts_with("hidden=") {
            node.style.display = DisplayType::None;
        }
    }

    let mut rest = attr_str.trim();
    while !rest.is_empty() {
        let Some(eq_idx) = rest.find('=') else {
            break;
        };
        let key = rest[..eq_idx].split_whitespace().last().unwrap_or("").trim().to_ascii_lowercase();
        rest = rest[eq_idx + 1..].trim();
        let val = if let Some(stripped) = rest.strip_prefix('"') {
            if let Some(end_q) = stripped.find('"') {
                let v = &stripped[..end_q];
                rest = stripped[end_q + 1..].trim();
                v
            } else {
                break;
            }
        } else if let Some(stripped) = rest.strip_prefix('\'') {
            if let Some(end_q) = stripped.find('\'') {
                let v = &stripped[..end_q];
                rest = stripped[end_q + 1..].trim();
                v
            } else {
                break;
            }
        } else {
            let end_sp = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let v = &rest[..end_sp];
            rest = rest[end_sp..].trim();
            v
        };
        match key.as_str() {
            "id" => node.id = String::from(val.trim()),
            "name" => {
                node.name_attr = val.trim().to_ascii_lowercase();
                if node.tag == "input"
                    && (node.name_attr == "flow"
                        || node.name_attr.contains("token")
                        || node.name_attr.contains("csrf")
                        || node.name_attr == "authenticity_token")
                {
                    node.style.display = DisplayType::None;
                }
            }
            "class" => {
                for c in val.split_whitespace() {
                    node.classes.push(String::from(c));
                }
            }
            "href" => node.href = Some(decode_html_entities(val.trim())),
            "onclick" => node.onclick = Some(decode_html_entities(val.trim())),
            "aria-hidden" => {
                if val.trim().eq_ignore_ascii_case("true") && node.tag != "svg" {
                    node.style.display = DisplayType::None;
                }
            }
            "role" => {
                if val.trim().eq_ignore_ascii_case("button") {
                    node.style.is_button = true;
                    node.style.is_pill = true;
                }
            }
            "bgcolor" => {
                apply_css_property(&mut node.style, "background-color", val);
            }
            "text" => {
                apply_css_property(&mut node.style, "color", val);
            }
            "align" => {
                apply_css_property(&mut node.style, "text-align", val);
            }
            "type" => {
                node.input_type = val.trim().to_ascii_lowercase();
                if node.input_type == "hidden" {
                    node.style.display = DisplayType::None;
                } else if node.input_type == "submit" || node.input_type == "button" {
                    node.style.is_input = false;
                    node.style.is_button = true;
                    node.style.is_pill = true;
                }
            }
            "value" => {
                node.value = decode_html_entities(val.trim());
                if node.tag == "input"
                    && node.input_type.is_empty()
                    && node.placeholder.is_empty()
                    && node.value.eq_ignore_ascii_case("login")
                {
                    node.style.display = DisplayType::None;
                }
            }
            "placeholder" | "aria-label" | "title" | "alt" => {
                if node.placeholder.is_empty() {
                    node.placeholder = decode_html_entities(val.trim());
                }
            }
            "style" => {
                for decl in val.split(';') {
                    if let Some((k, v)) = decl.split_once(':') {
                        node.inline_styles.push((String::from(k.trim()), String::from(v.trim())));
                    }
                }
            }
            _ => {}
        }
    }
}

pub struct ParsedDocument {
    pub root: DomNode,
    pub css_rules: Vec<CssRule>,
    pub scripts: Vec<String>,
    pub stylesheet_urls: Vec<String>,
    pub title: String,
}

pub fn parse_html_document(html: &str) -> ParsedDocument {
    let mut root = DomNode::new_element("body");
    root.style.padding = 8;
    let mut stack: Vec<DomNode> = Vec::new();
    let mut css_rules: Vec<CssRule> = Vec::new();
    let mut scripts: Vec<String> = Vec::new();
    let mut stylesheet_urls: Vec<String> = Vec::new();
    let mut page_title = String::from("NetizenWeb");
    let mut node_count = 0usize;

    let bytes = html.as_bytes();
    let mut i = 0;
    let len = bytes.len();

    while i < len && node_count < MAX_DOM_NODES {
        if bytes[i] == b'<' {
            if i + 4 <= len && &bytes[i..i + 4] == b"<!--" {
                if let Some(end_c) = html[i + 4..].find("-->") {
                    i += 4 + end_c + 3;
                    continue;
                } else {
                    break;
                }
            }
            let Some(close_angle) = html[i..].find('>') else {
                break;
            };
            let raw_tag = html[i + 1..i + close_angle].trim();
            i += close_angle + 1;

            if raw_tag.starts_with('!') || raw_tag.starts_with('?') {
                continue;
            }

            let is_closing = raw_tag.starts_with('/');
            let self_closing = raw_tag.ends_with('/');
            let clean_tag = raw_tag.trim_start_matches('/').trim_end_matches('/').trim();
            let (tag_name_raw, attr_part) = match clean_tag.find(char::is_whitespace) {
                Some(pos) => (&clean_tag[..pos], &clean_tag[pos..]),
                None => (clean_tag, ""),
            };
            let tag_name = tag_name_raw.to_ascii_lowercase();

            if !is_closing && tag_name == "link" {
                let lower_attr = attr_part.to_ascii_lowercase();
                if lower_attr.contains("stylesheet") && stylesheet_urls.len() < 2 {
                    let mut tmp = DomNode::new_element("link");
                    parse_attributes(attr_part, &mut tmp);
                    if let Some(href) = tmp.href {
                        if !href.is_empty() {
                            stylesheet_urls.push(href);
                        }
                    }
                }
                continue;
            }

            if !is_closing && tag_name == "meta" {
                let lower_attr = attr_part.to_ascii_lowercase();
                if lower_attr.contains("color-scheme") && lower_attr.contains("dark") && root.style.bg_color.is_none() {
                    root.style.bg_color = Some(rgb(12, 15, 20));
                    root.style.text_color = rgb(231, 233, 234);
                }
                continue;
            }

            if !is_closing
                && (tag_name == "style"
                    || tag_name == "script"
                    || tag_name == "title"
                    || tag_name == "svg"
                    || tag_name == "iframe")
            {
                if tag_name == "svg" && self_closing {
                    continue;
                }
                let close_needle = alloc::format!("</{}", tag_name);
                let search_window = &html[i..len.min(i + 131072)];
                let lower_rest = search_window.to_ascii_lowercase();
                if let Some(end_pos) = lower_rest.find(&close_needle) {
                    let inner_content = &html[i..i + end_pos];
                    if tag_name == "style" && css_rules.len() < 200 {
                        css_rules.extend(parse_css_stylesheet(inner_content));
                    } else if tag_name == "script" && inner_content.len() <= 8192 && scripts.len() < 10 {
                        let trimmed_scr = inner_content.trim();
                        if !trimmed_scr.is_empty() {
                            scripts.push(String::from(trimmed_scr));
                        }
                    } else if tag_name == "title" {
                        let t = collapse_whitespace(&decode_html_entities(inner_content));
                        if !t.is_empty() {
                            page_title = t;
                        }
                    } else if tag_name == "svg" {
                        if let Some(svg_shape) = parse_svg_element(attr_part, inner_content) {
                            let mut svg_node = DomNode::new_element("svg");
                            parse_attributes(attr_part, &mut svg_node);
                            svg_node.svg = Some(svg_shape);
                            node_count += 1;
                            if let Some(parent) = stack.last_mut() {
                                parent.children.push(svg_node);
                            } else {
                                root.children.push(svg_node);
                            }
                        }
                    }
                    i += end_pos;
                    if let Some(after_close) = html[i..].find('>') {
                        i += after_close + 1;
                    }
                    continue;
                } else {
                    continue;
                }
            }

            if is_closing {
                if tag_name == "head" || tag_name == "noscript" || tag_name == "html" || tag_name == "body" {
                    continue;
                }
                if let Some(pos) = stack.iter().rposition(|n| n.tag == tag_name) {
                    while stack.len() > pos {
                        let popped = stack.pop().unwrap();
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(popped);
                        } else {
                            root.children.push(popped);
                        }
                    }
                }
                continue;
            }

            if tag_name == "html" || tag_name == "head" || tag_name == "noscript" {
                continue;
            }
            if tag_name == "body" {
                parse_attributes(attr_part, &mut root);
                continue;
            }

            let mut node = DomNode::new_element(&tag_name);
            parse_attributes(attr_part, &mut node);
            node_count += 1;

            let is_void = matches!(
                tag_name.as_str(),
                "br" | "hr" | "img" | "input" | "meta" | "link" | "source" | "area" | "base" | "col"
            ) || self_closing;

            if is_void || stack.len() >= MAX_DOM_DEPTH {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root.children.push(node);
                }
            } else {
                stack.push(node);
            }
        } else {
            let next_tag = html[i..].find('<').unwrap_or(len - i);
            let raw_text = &html[i..i + next_tag];
            i += next_tag;
            let decoded = collapse_whitespace(&decode_html_entities(raw_text));
            if !decoded.is_empty() && decoded != "." {
                node_count += 1;
                let text_node = DomNode::new_text(&decoded);
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(text_node);
                } else {
                    root.children.push(text_node);
                }
            }
        }
    }

    while let Some(popped) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(popped);
        } else {
            root.children.push(popped);
        }
    }

    normalize_dom_tree(&mut root, 0);
    let root_color = root.style.text_color;
    apply_styles_recursive(&mut root, &css_rules, Some(root_color), 0);

    ParsedDocument {
        root,
        css_rules,
        scripts,
        stylesheet_urls,
        title: page_title,
    }
}

fn normalize_dom_tree(node: &mut DomNode, depth: usize) {
    if depth > MAX_DOM_DEPTH {
        return;
    }

    node.children.retain(|c| {
        if c.style.display == DisplayType::None {
            return false;
        }
        if c.tag == "#text" && (c.text.trim() == "." || c.text.trim().is_empty()) {
            return false;
        }
        if c.tag == "img" && c.placeholder.is_empty() {
            return false;
        }
        true
    });

    for child in &mut node.children {
        normalize_dom_tree(child, depth + 1);
    }

    let mut idx = 0;
    while idx + 1 < node.children.len() {
        let a_txt = node.children[idx].inner_text();
        let b_txt = node.children[idx + 1].inner_text();
        if !a_txt.is_empty() && a_txt.eq_ignore_ascii_case(&b_txt) && node.children[idx + 1].tag != "input" {
            node.children.remove(idx + 1);
            continue;
        }
        if !a_txt.is_empty()
            && node.children[idx + 1].tag == "input"
            && node.children[idx + 1].placeholder.eq_ignore_ascii_case(&a_txt)
        {
            node.children.remove(idx);
            continue;
        }
        idx += 1;
    }

    for child in &mut node.children {
        let txt_up = child.inner_text().to_ascii_uppercase();
        if child.tag == "a"
            && (txt_up.starts_with("CONTINUE WITH")
                || txt_up.starts_with("SIGN IN")
                || txt_up.starts_with("SIGN UP")
                || txt_up.starts_with("LOG IN")
                || txt_up.starts_with("CREATE ACCOUNT"))
        {
            child.style.display = DisplayType::Block;
            child.style.is_button = true;
            child.style.is_pill = true;
            child.style.is_link = false;
            child.style.bg_color = Some(rgb(241, 245, 249));
            child.style.border_color = Some(rgb(203, 213, 225));
            child.style.text_color = TEXT_PRIMARY;
        }
    }
}

#[derive(Clone, Debug)]
pub enum JsVal {
    Num(i64),
    Str(String),
    Bool(bool),
    DomRef(String),
    Undefined,
}

impl JsVal {
    pub fn as_str(&self) -> String {
        match self {
            JsVal::Num(n) => alloc::format!("{}", n),
            JsVal::Str(s) => s.clone(),
            JsVal::Bool(b) => String::from(if *b { "true" } else { "false" }),
            JsVal::DomRef(id) => alloc::format!("[Element #{}]", id),
            JsVal::Undefined => String::from("undefined"),
        }
    }

    pub fn as_num(&self) -> i64 {
        match self {
            JsVal::Num(n) => *n,
            JsVal::Bool(b) => if *b { 1 } else { 0 },
            JsVal::Str(s) => s.trim().parse::<i64>().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            JsVal::Num(n) => *n != 0,
            JsVal::Bool(b) => *b,
            JsVal::Str(s) => !s.is_empty(),
            JsVal::DomRef(_) => true,
            JsVal::Undefined => false,
        }
    }
}

#[derive(Clone)]
pub struct JsFunc {
    pub name: String,
    pub params: Vec<String>,
    pub body: String,
}

pub struct JsEngine {
    pub vars: Vec<(String, JsVal)>,
    pub funcs: Vec<JsFunc>,
    pub logs: Vec<String>,
    steps: usize,
    depth: usize,
}

impl JsEngine {
    pub fn new() -> Self {
        JsEngine {
            vars: Vec::new(),
            funcs: Vec::new(),
            logs: Vec::new(),
            steps: 0,
            depth: 0,
        }
    }

    pub fn set_var(&mut self, name: &str, val: JsVal) {
        if let Some((_, existing)) = self.vars.iter_mut().find(|(k, _)| k == name) {
            *existing = val;
        } else if self.vars.len() < 64 {
            self.vars.push((String::from(name), val));
        }
    }

    pub fn get_var(&self, name: &str) -> JsVal {
        self.vars
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or(JsVal::Undefined)
    }

    pub fn execute(&mut self, script: &str, dom: &mut DomNode) {
        self.steps = 0;
        self.depth = 0;
        self.execute_bounded(script, dom);
    }

    fn execute_bounded(&mut self, script: &str, dom: &mut DomNode) {
        if self.depth > 10 || self.steps > 400 || script.len() > 8192 {
            return;
        }
        self.depth += 1;
        let mut s = script.trim();
        while !s.is_empty() && self.steps < 400 {
            self.steps += 1;
            if s.starts_with("//") {
                if let Some(nl) = s.find('\n') {
                    s = s[nl + 1..].trim();
                    continue;
                } else {
                    break;
                }
            }
            if let Some(rest) = s.strip_prefix("function ") {
                if let Some(open_p) = rest.find('(') {
                    let fn_name = rest[..open_p].trim();
                    if let Some(close_p) = rest[open_p + 1..].find(')') {
                        let params_raw = &rest[open_p + 1..open_p + 1 + close_p];
                        let mut params = Vec::new();
                        for p in params_raw.split(',') {
                            let pt = p.trim();
                            if !pt.is_empty() {
                                params.push(String::from(pt));
                            }
                        }
                        let after_p = rest[open_p + 1 + close_p + 1..].trim();
                        if let Some((block, after_block)) = Self::extract_braced(after_p) {
                            if fn_name.len() > 1 && self.funcs.len() < 32 {
                                if let Some(existing) = self.funcs.iter_mut().find(|f| f.name == fn_name) {
                                    existing.params = params;
                                    existing.body = String::from(block);
                                } else {
                                    self.funcs.push(JsFunc {
                                        name: String::from(fn_name),
                                        params,
                                        body: String::from(block),
                                    });
                                }
                            }
                            s = after_block.trim();
                            continue;
                        }
                    }
                }
            }
            if let Some(rest) = s.strip_prefix("if") {
                let rest_t = rest.trim();
                if rest_t.starts_with('(') {
                    if let Some((cond_str, after_cond)) = Self::extract_parens(rest_t) {
                        let cond_val = self.eval_expr(cond_str, dom);
                        let after_c = after_cond.trim();
                        if let Some((if_body, after_if)) = Self::extract_braced(after_c) {
                            let mut rem = after_if.trim();
                            let mut else_body: Option<&str> = None;
                            if let Some(after_else_kw) = rem.strip_prefix("else") {
                                let ae = after_else_kw.trim();
                                if let Some((eb, after_eb)) = Self::extract_braced(ae) {
                                    else_body = Some(eb);
                                    rem = after_eb.trim();
                                }
                            }
                            if cond_val.is_truthy() {
                                self.execute_bounded(if_body, dom);
                            } else if let Some(eb) = else_body {
                                self.execute_bounded(eb, dom);
                            }
                            s = rem;
                            continue;
                        }
                    }
                }
            }
            let stmt_end = Self::find_stmt_end(s);
            let stmt = s[..stmt_end].trim();
            s = if stmt_end < s.len() { s[stmt_end + 1..].trim() } else { "" };
            if !stmt.is_empty() {
                self.exec_statement(stmt, dom);
            }
        }
        self.depth = self.depth.saturating_sub(1);
    }

    fn extract_braced(input: &str) -> Option<(&str, &str)> {
        let trimmed = input.trim();
        if !trimmed.starts_with('{') {
            return None;
        }
        let bytes = trimmed.as_bytes();
        let mut depth = 0i32;
        let mut in_str: Option<u8> = None;
        for (idx, &b) in bytes.iter().enumerate() {
            if let Some(q) = in_str {
                if b == q {
                    in_str = None;
                }
                continue;
            }
            if b == b'"' || b == b'\'' {
                in_str = Some(b);
            } else if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
                if depth == 0 {
                    return Some((&trimmed[1..idx], &trimmed[idx + 1..]));
                }
            }
        }
        None
    }

    fn extract_parens(input: &str) -> Option<(&str, &str)> {
        let trimmed = input.trim();
        if !trimmed.starts_with('(') {
            return None;
        }
        let bytes = trimmed.as_bytes();
        let mut depth = 0i32;
        let mut in_str: Option<u8> = None;
        for (idx, &b) in bytes.iter().enumerate() {
            if let Some(q) = in_str {
                if b == q {
                    in_str = None;
                }
                continue;
            }
            if b == b'"' || b == b'\'' {
                in_str = Some(b);
            } else if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth -= 1;
                if depth == 0 {
                    return Some((&trimmed[1..idx], &trimmed[idx + 1..]));
                }
            }
        }
        None
    }

    fn find_stmt_end(s: &str) -> usize {
        let bytes = s.as_bytes();
        let mut in_str: Option<u8> = None;
        let mut paren_depth = 0i32;
        let mut brace_depth = 0i32;
        for (idx, &b) in bytes.iter().enumerate() {
            if let Some(q) = in_str {
                if b == q {
                    in_str = None;
                }
                continue;
            }
            if b == b'"' || b == b'\'' {
                in_str = Some(b);
            } else if b == b'(' {
                paren_depth += 1;
            } else if b == b')' {
                paren_depth -= 1;
            } else if b == b'{' {
                brace_depth += 1;
            } else if b == b'}' {
                brace_depth -= 1;
            } else if (b == b';' || b == b'\n') && paren_depth <= 0 && brace_depth <= 0 {
                return idx;
            }
        }
        s.len()
    }

    fn exec_statement(&mut self, stmt: &str, dom: &mut DomNode) {
        let clean = stmt
            .strip_prefix("let ")
            .or_else(|| stmt.strip_prefix("var "))
            .or_else(|| stmt.strip_prefix("const "))
            .unwrap_or(stmt)
            .trim();

        if let Some(var_name) = clean.strip_suffix("++") {
            let vn = var_name.trim();
            let cur = self.get_var(vn).as_num();
            self.set_var(vn, JsVal::Num(cur + 1));
            return;
        }
        if let Some(var_name) = clean.strip_suffix("--") {
            let vn = var_name.trim();
            let cur = self.get_var(vn).as_num();
            self.set_var(vn, JsVal::Num(cur - 1));
            return;
        }

        if let Some((lhs, rhs)) = Self::split_assignment(clean) {
            let val = self.eval_expr(rhs, dom);
            let lhs = lhs.trim();

            if let Some(elem_id) = Self::extract_dom_selector(lhs) {
                if let Some(prop) = lhs.split(')').nth(1) {
                    Self::apply_dom_mutation(dom, &elem_id, prop.trim(), &val);
                    return;
                }
            }

            if let Some((obj_name, prop_path)) = lhs.split_once('.') {
                if let JsVal::DomRef(elem_id) = self.get_var(obj_name.trim()) {
                    let dot_prop = alloc::format!(".{}", prop_path.trim());
                    Self::apply_dom_mutation(dom, &elem_id, &dot_prop, &val);
                    return;
                }
            }

            self.set_var(lhs, val);
            return;
        }

        let _ = self.eval_expr(clean, dom);
    }

    fn split_assignment(s: &str) -> Option<(&str, &str)> {
        let bytes = s.as_bytes();
        let mut in_str: Option<u8> = None;
        for i in 0..bytes.len() {
            let b = bytes[i];
            if let Some(q) = in_str {
                if b == q {
                    in_str = None;
                }
                continue;
            }
            if b == b'"' || b == b'\'' {
                in_str = Some(b);
            } else if b == b'=' {
                let prev = if i > 0 { bytes[i - 1] } else { 0 };
                let next = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
                if prev != b'=' && prev != b'!' && prev != b'<' && prev != b'>' && next != b'=' {
                    return Some((&s[..i], &s[i + 1..]));
                }
            }
        }
        None
    }

    fn extract_dom_selector(expr: &str) -> Option<String> {
        for prefix in ["document.getElementById(", "document.querySelector("] {
            if let Some(start) = expr.find(prefix) {
                let after = &expr[start + prefix.len()..];
                let close = after.find(')')?;
                let arg = after[..close].trim().trim_matches('"').trim_matches('\'');
                return Some(String::from(arg));
            }
        }
        None
    }

    fn apply_dom_mutation(dom: &mut DomNode, elem_id: &str, prop: &str, val: &JsVal) {
        let Some(target) = dom.find_by_id_mut(elem_id) else {
            return;
        };
        let val_str = val.as_str();
        match prop {
            ".innerText" | ".textContent" | ".innerHTML" => {
                target.set_inner_text(&val_str);
            }
            ".value" => {
                target.value = val_str;
            }
            ".style.backgroundColor" | ".style.background" => {
                apply_css_property(&mut target.style, "background-color", &val_str);
            }
            ".style.color" => {
                apply_css_property(&mut target.style, "color", &val_str);
            }
            ".style.display" => {
                apply_css_property(&mut target.style, "display", &val_str);
            }
            ".style.borderColor" => {
                apply_css_property(&mut target.style, "border-color", &val_str);
            }
            ".style.padding" => {
                apply_css_property(&mut target.style, "padding", &val_str);
            }
            _ => {}
        }
    }

    pub fn eval_expr(&mut self, expr: &str, dom: &mut DomNode) -> JsVal {
        if self.steps > 400 || self.depth > 10 {
            return JsVal::Undefined;
        }
        let e = expr.trim();
        if e.is_empty() {
            return JsVal::Undefined;
        }

        if let Some((l, r)) = Self::split_binary_op(e, "==") {
            let lv = self.eval_expr(l, dom);
            let rv = self.eval_expr(r, dom);
            return JsVal::Bool(lv.as_str() == rv.as_str());
        }
        if let Some((l, r)) = Self::split_binary_op(e, "!=") {
            let lv = self.eval_expr(l, dom);
            let rv = self.eval_expr(r, dom);
            return JsVal::Bool(lv.as_str() != rv.as_str());
        }
        if let Some((l, r)) = Self::split_binary_op(e, "+") {
            let lv = self.eval_expr(l, dom);
            let rv = self.eval_expr(r, dom);
            if matches!(lv, JsVal::Str(_)) || matches!(rv, JsVal::Str(_)) {
                return JsVal::Str(alloc::format!("{}{}", lv.as_str(), rv.as_str()));
            }
            return JsVal::Num(lv.as_num().wrapping_add(rv.as_num()));
        }
        if let Some((l, r)) = Self::split_binary_op(e, "-") {
            if !l.trim().is_empty() {
                return JsVal::Num(self.eval_expr(l, dom).as_num().wrapping_sub(self.eval_expr(r, dom).as_num()));
            }
        }

        if (e.starts_with('"') && e.ends_with('"')) || (e.starts_with('\'') && e.ends_with('\'')) {
            return JsVal::Str(String::from(&e[1..e.len() - 1]));
        }
        if e == "true" {
            return JsVal::Bool(true);
        }
        if e == "false" {
            return JsVal::Bool(false);
        }
        if let Ok(n) = e.parse::<i64>() {
            return JsVal::Num(n);
        }

        if let Some(arg_part) = e.strip_prefix("console.log(").and_then(|s| s.strip_suffix(')')) {
            let v = self.eval_expr(arg_part, dom);
            if self.logs.len() >= 10 {
                self.logs.remove(0);
            }
            self.logs.push(v.as_str());
            return JsVal::Undefined;
        }

        if let Some(elem_id) = Self::extract_dom_selector(e) {
            if let Some(prop) = e.split(')').nth(1) {
                let p = prop.trim();
                if p.is_empty() {
                    return JsVal::DomRef(elem_id);
                }
                if let Some(node) = dom.find_by_id(&elem_id) {
                    if p == ".innerText" || p == ".textContent" || p == ".innerHTML" {
                        return JsVal::Str(node.inner_text());
                    }
                    if p == ".value" {
                        return JsVal::Str(node.value.clone());
                    }
                }
            }
            return JsVal::DomRef(elem_id);
        }

        if let Some(open_p) = e.find('(') {
            if e.ends_with(')') {
                let fn_name = e[..open_p].trim();
                let args_str = &e[open_p + 1..e.len() - 1];
                if let Some(func) = self.funcs.iter().find(|f| f.name == fn_name).cloned() {
                    let mut arg_vals = Vec::new();
                    if !args_str.trim().is_empty() {
                        for a in args_str.split(',') {
                            arg_vals.push(self.eval_expr(a, dom));
                        }
                    }
                    for (idx, param) in func.params.iter().enumerate() {
                        let v = arg_vals.get(idx).cloned().unwrap_or(JsVal::Undefined);
                        self.set_var(param, v);
                    }
                    self.execute_bounded(&func.body, dom);
                    return JsVal::Undefined;
                }
            }
        }

        self.get_var(e)
    }

    fn split_binary_op<'a>(s: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
        let bytes = s.as_bytes();
        let op_bytes = op.as_bytes();
        let mut in_str: Option<u8> = None;
        let mut paren_depth = 0i32;
        let mut i = 0;
        while i + op_bytes.len() <= bytes.len() {
            let b = bytes[i];
            if let Some(q) = in_str {
                if b == q {
                    in_str = None;
                }
                i += 1;
                continue;
            }
            if b == b'"' || b == b'\'' {
                in_str = Some(b);
            } else if b == b'(' {
                paren_depth += 1;
            } else if b == b')' {
                paren_depth -= 1;
            } else if paren_depth == 0 && &bytes[i..i + op_bytes.len()] == op_bytes {
                return Some((&s[..i], &s[i + op_bytes.len()..]));
            }
            i += 1;
        }
        None
    }
}

#[derive(Clone)]
pub enum RenderCmd {
    Panel {
        rect: Rect,
        radius: i32,
        bg: u32,
        border: Option<u32>,
        is_card: bool,
    },
    Text {
        x: i32,
        y: i32,
        text: String,
        cap_height: i32,
        color: u32,
        underline: bool,
    },
    Button {
        rect: Rect,
        label: String,
        bg: Option<u32>,
        fg: u32,
        border: Option<u32>,
        is_pill: bool,
    },
    InputBox {
        rect: Rect,
        id: String,
        value: String,
        placeholder: String,
        dark_mode: bool,
    },
    SvgIcon {
        rect: Rect,
        svg: SvgShape,
        color: u32,
    },
    RuleLine {
        x: i32,
        y: i32,
        width: i32,
        color: u32,
    },
}

#[derive(Clone)]
pub struct HitRegion {
    pub rect: Rect,
    pub onclick: Option<String>,
    pub href: Option<String>,
    pub input_id: Option<String>,
}

pub struct LayoutBuilder {
    pub cmds: Vec<RenderCmd>,
    pub hits: Vec<HitRegion>,
    pub total_height: i32,
    unit: i32,
    dark_page: bool,
}

impl LayoutBuilder {
    pub fn empty() -> Self {
        LayoutBuilder {
            cmds: Vec::new(),
            hits: Vec::new(),
            total_height: 0,
            unit: 20,
            dark_page: false,
        }
    }

    pub fn build(dom: &DomNode, viewport_width: i32, unit: i32) -> Self {
        let page_bg = dom.style.bg_color.unwrap_or(WHITE);
        let dark_page = ((page_bg >> 16) & 0xFF) < 60 && ((page_bg >> 8) & 0xFF) < 60;
        let mut builder = LayoutBuilder {
            cmds: Vec::new(),
            hits: Vec::new(),
            total_height: 0,
            unit,
            dark_page,
        };
        let mut cursor_y = unit / 2;
        builder.layout_block(
            dom,
            unit / 2,
            (viewport_width - unit).max(unit * 6),
            &mut cursor_y,
            None,
            None,
            dom.style,
            0,
        );
        builder.total_height = cursor_y + unit * 2;
        builder
    }

    fn is_inline_flow_node(node: &DomNode) -> bool {
        matches!(node.style.display, DisplayType::Inline | DisplayType::InlineBlock)
            || node.tag == "#text"
            || node.tag == "a"
            || node.tag == "span"
            || node.tag == "b"
            || node.tag == "strong"
            || node.tag == "i"
            || node.tag == "em"
            || node.tag == "small"
            || node.tag == "label"
            || node.tag == "svg"
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_inline_run(
        &mut self,
        nodes: &[DomNode],
        x: i32,
        width: i32,
        cursor_y: &mut i32,
        inherited_href: Option<String>,
        inherited_onclick: Option<String>,
        parent_style: ComputedStyle,
        depth: usize,
    ) {
        let mut cur_x = x;
        let mut line_h = (self.unit * parent_style.font_scale / 100 * 145 / 100).max(self.unit);
        let right_edge = x + width;

        for node in nodes {
            if node.style.display == DisplayType::None || depth > MAX_DOM_DEPTH {
                continue;
            }
            let active_href = node.href.clone().or_else(|| inherited_href.clone());
            let active_onclick = node.onclick.clone().or_else(|| inherited_onclick.clone());

            if node.tag == "br" {
                *cursor_y += line_h;
                cur_x = x;
                continue;
            }

            if let Some(ref svg_shape) = node.svg {
                let icon_size = (self.unit * 2).min(width / 3).max(self.unit);
                if cur_x + icon_size > right_edge && cur_x > x {
                    *cursor_y += line_h;
                    cur_x = x;
                }
                let rect = Rect::new(cur_x, *cursor_y, icon_size, icon_size);
                self.cmds.push(RenderCmd::SvgIcon {
                    rect,
                    svg: svg_shape.clone(),
                    color: parent_style.text_color,
                });
                if active_href.is_some() || active_onclick.is_some() {
                    self.hits.push(HitRegion {
                        rect,
                        onclick: active_onclick,
                        href: active_href,
                        input_id: None,
                    });
                }
                cur_x += icon_size + self.unit / 2;
                line_h = line_h.max(icon_size + 4);
                continue;
            }

            if node.style.is_button || node.tag == "button" {
                let label = {
                    let t = node.inner_text();
                    if t.is_empty() {
                        if node.value.is_empty() {
                            String::from("SUBMIT")
                        } else {
                            node.value.clone()
                        }
                    } else {
                        t
                    }
                };
                let cap = (self.unit * 56 / 100).max(10);
                let st = text_style(cap, 26);
                let btn_w = (text_width(&label, &st) + self.unit * 2).min(width).max(self.unit * 5);
                let btn_h = self.unit * 2;
                if cur_x + btn_w > right_edge && cur_x > x {
                    *cursor_y += line_h + 2;
                    cur_x = x;
                }
                let rect = Rect::new(cur_x, *cursor_y, btn_w, btn_h);
                self.cmds.push(RenderCmd::Button {
                    rect,
                    label,
                    bg: node.style.bg_color,
                    fg: node.style.text_color,
                    border: node.style.border_color,
                    is_pill: node.style.is_pill,
                });
                self.hits.push(HitRegion {
                    rect,
                    onclick: active_onclick,
                    href: active_href,
                    input_id: None,
                });
                cur_x += btn_w + self.unit / 2;
                line_h = line_h.max(btn_h + 4);
                continue;
            }

            if node.style.is_input || node.tag == "input" {
                let inp_w = (self.unit * 13).min(width).max(self.unit * 6);
                let inp_h = self.unit * 2;
                if cur_x + inp_w > right_edge && cur_x > x {
                    *cursor_y += line_h + 2;
                    cur_x = x;
                }
                let rect = Rect::new(cur_x, *cursor_y, inp_w, inp_h);
                self.cmds.push(RenderCmd::InputBox {
                    rect,
                    id: node.id.clone(),
                    value: node.value.clone(),
                    placeholder: node.placeholder.clone(),
                    dark_mode: self.dark_page,
                });
                self.hits.push(HitRegion {
                    rect,
                    onclick: active_onclick,
                    href: None,
                    input_id: if node.id.is_empty() { None } else { Some(node.id.clone()) },
                });
                cur_x += inp_w + self.unit / 2;
                line_h = line_h.max(inp_h + 4);
                continue;
            }

            let txt = if node.tag == "#text" {
                node.text.clone()
            } else {
                node.inner_text()
            };
            if txt.is_empty() {
                if !node.children.is_empty() {
                    self.layout_inline_run(
                        &node.children,
                        cur_x,
                        right_edge - cur_x,
                        cursor_y,
                        active_href,
                        active_onclick,
                        node.style,
                        depth + 1,
                    );
                }
                continue;
            }

            let scale = if parent_style.font_scale != 60 {
                parent_style.font_scale
            } else {
                node.style.font_scale
            };
            let cap = (self.unit * scale / 100).max(9);
            let word_h = cap + (cap * 45 / 100).max(5);
            line_h = line_h.max(word_h);
            let style = text_style(cap, 24);
            let space_w = (cap * 35 / 100).max(4);
            let is_link = parent_style.is_link || node.style.is_link || active_href.is_some();
            let color = if is_link {
                rgb(29, 155, 240)
            } else if node.tag != "#text" {
                node.style.text_color
            } else if self.dark_page && parent_style.text_color == TEXT_PRIMARY {
                rgb(231, 233, 234)
            } else {
                parent_style.text_color
            };

            let words: Vec<&str> = txt.split_whitespace().collect();
            if parent_style.text_align == TextAlign::Center && words.len() <= 8 && cur_x == x {
                let full_tw = text_width(&txt, &style);
                if full_tw <= width {
                    let tx = x + (width - full_tw) / 2;
                    let rect = Rect::new(tx, *cursor_y, full_tw, word_h);
                    self.cmds.push(RenderCmd::Text {
                        x: tx,
                        y: *cursor_y,
                        text: txt,
                        cap_height: cap,
                        color,
                        underline: is_link,
                    });
                    if active_href.is_some() || active_onclick.is_some() {
                        self.hits.push(HitRegion {
                            rect,
                            onclick: active_onclick,
                            href: active_href,
                            input_id: None,
                        });
                    }
                    cur_x = tx + full_tw + space_w;
                    continue;
                }
            }

            for word in words {
                let ww = text_width(word, &style);
                if cur_x + ww > right_edge && cur_x > x {
                    *cursor_y += line_h;
                    cur_x = x;
                }
                let word_rect = Rect::new(cur_x, *cursor_y, ww, word_h);
                self.cmds.push(RenderCmd::Text {
                    x: cur_x,
                    y: *cursor_y,
                    text: String::from(word),
                    cap_height: cap,
                    color,
                    underline: is_link,
                });
                if active_href.is_some() || active_onclick.is_some() {
                    self.hits.push(HitRegion {
                        rect: word_rect,
                        onclick: active_onclick.clone(),
                        href: active_href.clone(),
                        input_id: None,
                    });
                }
                cur_x += ww + space_w;
            }
        }

        if cur_x > x {
            *cursor_y += line_h;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_block(
        &mut self,
        node: &DomNode,
        x: i32,
        width: i32,
        cursor_y: &mut i32,
        inherited_href: Option<String>,
        inherited_onclick: Option<String>,
        parent_style: ComputedStyle,
        depth: usize,
    ) {
        if depth > MAX_DOM_DEPTH || node.style.display == DisplayType::None {
            return;
        }

        let active_href = node.href.clone().or(inherited_href);
        let active_onclick = node.onclick.clone().or(inherited_onclick);

        if node.tag == "br" {
            *cursor_y += self.unit * 2 / 3;
            return;
        }
        if node.tag == "hr" {
            *cursor_y += self.unit / 2;
            self.cmds.push(RenderCmd::RuleLine {
                x,
                y: *cursor_y,
                width,
                color: if self.dark_page { rgb(47, 51, 54) } else { PANEL_BORDER },
            });
            *cursor_y += self.unit / 2;
            return;
        }

        if let Some(ref svg_shape) = node.svg {
            let icon_size = (self.unit * 5 / 2).min(width / 2).max(self.unit);
            let sx = match node.style.text_align {
                TextAlign::Center => x + (width - icon_size) / 2,
                TextAlign::Right => x + width - icon_size,
                TextAlign::Left => x,
            };
            *cursor_y += node.style.margin_top;
            let rect = Rect::new(sx, *cursor_y, icon_size, icon_size);
            self.cmds.push(RenderCmd::SvgIcon {
                rect,
                svg: svg_shape.clone(),
                color: node.style.text_color,
            });
            if active_href.is_some() || active_onclick.is_some() {
                self.hits.push(HitRegion {
                    rect,
                    onclick: active_onclick,
                    href: active_href,
                    input_id: None,
                });
            }
            *cursor_y += icon_size + node.style.margin_bottom;
            return;
        }

        if node.style.is_button || node.tag == "button" {
            let label = {
                let t = node.inner_text();
                if t.is_empty() {
                    if node.value.is_empty() {
                        String::from("CONTINUE")
                    } else {
                        node.value.clone()
                    }
                } else {
                    t
                }
            };
            let btn_w = width.min(self.unit * 15).max(self.unit * 6);
            let btn_h = self.unit * 2 + 2;
            let btn_x = match node.style.text_align {
                TextAlign::Center => x + (width - btn_w).max(0) / 2,
                TextAlign::Right => x + (width - btn_w).max(0),
                TextAlign::Left => x,
            };
            *cursor_y += node.style.margin_top;
            let rect = Rect::new(btn_x, *cursor_y, btn_w, btn_h);
            self.cmds.push(RenderCmd::Button {
                rect,
                label,
                bg: node.style.bg_color,
                fg: node.style.text_color,
                border: node.style.border_color,
                is_pill: node.style.is_pill,
            });
            self.hits.push(HitRegion {
                rect,
                onclick: active_onclick,
                href: active_href,
                input_id: None,
            });
            *cursor_y += btn_h + node.style.margin_bottom + 2;
            return;
        }

        if node.style.is_input || node.tag == "input" {
            let inp_w = width.min(self.unit * 15).max(self.unit * 7);
            let inp_h = self.unit * 2 + 2;
            *cursor_y += node.style.margin_top;
            let rect = Rect::new(x, *cursor_y, inp_w, inp_h);
            self.cmds.push(RenderCmd::InputBox {
                rect,
                id: node.id.clone(),
                value: node.value.clone(),
                placeholder: node.placeholder.clone(),
                dark_mode: self.dark_page,
            });
            self.hits.push(HitRegion {
                rect,
                onclick: active_onclick,
                href: None,
                input_id: if node.id.is_empty() { None } else { Some(node.id.clone()) },
            });
            *cursor_y += inp_h + node.style.margin_bottom + 2;
            return;
        }

        if node.tag == "#text" {
            self.layout_inline_run(
                core::slice::from_ref(node),
                x,
                width,
                cursor_y,
                active_href,
                active_onclick,
                parent_style,
                depth + 1,
            );
            return;
        }

        let effective_w = match node.style.max_width {
            Some(mw) => width.min(mw * self.unit / 20),
            None => width,
        };
        let effective_x = if node.style.center_block && effective_w < width {
            x + (width - effective_w) / 2
        } else {
            x
        };

        *cursor_y += node.style.margin_top;
        let pad = node.style.padding * self.unit / 12;
        let start_y = *cursor_y;
        let panel_slot = self.cmds.len();
        let has_box =
            (node.style.bg_color.is_some() && node.tag != "body") || node.style.border_color.is_some() || node.style.is_card;

        if has_box {
            self.cmds.push(RenderCmd::Panel {
                rect: Rect::EMPTY,
                radius: (node.style.border_radius * self.unit / 10).max(3),
                bg: node.style.bg_color.unwrap_or(if self.dark_page { rgb(22, 26, 34) } else { PANEL_FILL }),
                border: node.style.border_color,
                is_card: node.style.is_card && !self.dark_page,
            });
            *cursor_y += pad.max(self.unit / 3);
        }

        let inner_x = effective_x + pad;
        let inner_w = (effective_w - pad * 2).max(self.unit * 4);

        if node.tag == "li" {
            let cap = (self.unit * node.style.font_scale / 100).max(9);
            self.cmds.push(RenderCmd::Text {
                x: inner_x,
                y: *cursor_y,
                text: String::from("*"),
                cap_height: cap,
                color: ACCENT_LINE,
                underline: false,
            });
        }
        let child_x = if node.tag == "li" { inner_x + self.unit } else { inner_x };
        let child_w = if node.tag == "li" { inner_w - self.unit } else { inner_w };

        if !node.text.is_empty() {
            let txt_node = DomNode::new_text(&node.text);
            self.layout_inline_run(
                core::slice::from_ref(&txt_node),
                child_x,
                child_w,
                cursor_y,
                active_href.clone(),
                active_onclick.clone(),
                node.style,
                depth + 1,
            );
        }

        let is_row = (node.style.display == DisplayType::Flex || node.style.display == DisplayType::TableRow)
            && node.children.len() > 1
            && node.children.len() <= 6;

        if is_row {
            let count = node.children.len() as i32;
            let gap = self.unit / 2;
            let col_w = ((child_w - gap * (count - 1)) / count).max(self.unit * 3);
            let row_top = *cursor_y;
            let mut max_bottom = row_top;
            for (idx, child) in node.children.iter().enumerate() {
                let mut col_y = row_top;
                let cx = child_x + idx as i32 * (col_w + gap);
                self.layout_block(
                    child,
                    cx,
                    col_w,
                    &mut col_y,
                    active_href.clone(),
                    active_onclick.clone(),
                    node.style,
                    depth + 1,
                );
                if col_y > max_bottom {
                    max_bottom = col_y;
                }
            }
            *cursor_y = max_bottom;
        } else {
            let mut idx = 0;
            while idx < node.children.len() {
                if Self::is_inline_flow_node(&node.children[idx]) {
                    let start = idx;
                    while idx < node.children.len() && Self::is_inline_flow_node(&node.children[idx]) {
                        idx += 1;
                    }
                    self.layout_inline_run(
                        &node.children[start..idx],
                        child_x,
                        child_w,
                        cursor_y,
                        active_href.clone(),
                        active_onclick.clone(),
                        node.style,
                        depth + 1,
                    );
                } else {
                    self.layout_block(
                        &node.children[idx],
                        child_x,
                        child_w,
                        cursor_y,
                        active_href.clone(),
                        active_onclick.clone(),
                        node.style,
                        depth + 1,
                    );
                    idx += 1;
                }
            }
        }

        if has_box {
            *cursor_y += pad.max(self.unit / 3);
            let box_rect = Rect::new(effective_x, start_y, effective_w, (*cursor_y - start_y).max(self.unit));
            if let RenderCmd::Panel { ref mut rect, .. } = self.cmds[panel_slot] {
                *rect = box_rect;
            }
            if active_onclick.is_some() || active_href.is_some() {
                self.hits.push(HitRegion {
                    rect: box_rect,
                    onclick: active_onclick,
                    href: active_href,
                    input_id: None,
                });
            }
        }

        *cursor_y += node.style.margin_bottom;
    }
}

const HOME_HTML: &str = concat!(
    "<html><head><title>NetizenWeb Browser</title><style>",
    ".hero { background-color: rgb(15, 23, 42); color: rgb(255, 255, 255); border-color: rgb(59, 130, 246); border-radius: 10px; padding: 14px; margin-bottom: 8px; }",
    ".row { display: flex; margin-top: 6px; margin-bottom: 6px; }",
    ".card { background-color: rgb(255, 255, 255); border-color: rgb(226, 232, 240); border-radius: 8px; padding: 12px; box-shadow: 1; }",
    "h1 { color: rgb(255, 255, 255); font-size: 24px; }",
    "h2 { color: rgb(15, 23, 42); font-size: 18px; }",
    ".muted { color: rgb(148, 163, 184); font-size: 14px; }",
    "</style><script>",
    "let sayac = 0;\n",
    "function artir() {\n",
    "    sayac = sayac + 1;\n",
    "    document.getElementById('counter_label').innerText = 'JS DOM Sayaci: ' + sayac;\n",
    "    console.log('Sayac: ' + sayac);\n",
    "}\n",
    "</script></head><body>",
    "<div class='hero'>",
    "<svg viewBox='0 0 24 24'><path d='M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5'/></svg>",
    "<h1>NetizenWeb Universal Engine</h1>",
    "<p class='muted'>Gercek SVG Path Rasterizer, Harici CSS Indirici, Inline/Tablo/Flex Akisi ve Canli JS Motoru.</p>",
    "<button onclick='artir()' style='background-color: rgb(29, 155, 240); color: white;'>JS TEST (+1)</button>",
    "</div>",
    "<div class='row'>",
    "<div class='card'>",
    "<h2>Gercek Web Testleri</h2>",
    "<p><a href='https://example.com'>example.com</a> | <a href='https://www.x.com'>x.com</a></p>",
    "<p><a href='https://news.ycombinator.com'>news.ycombinator.com</a></p>",
    "<p><a href='http://info.cern.ch'>info.cern.ch</a></p>",
    "</div>",
    "<div class='card'>",
    "<h2>Motor Durumu</h2>",
    "<p id='counter_label'>JS DOM Sayaci: 0</p>",
    "<p>Harici CSS + SVG Path + Inline Flow aktif.</p>",
    "</div>",
    "</div></body></html>"
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum FetchKind {
    MainHtml,
    Stylesheet,
}

pub struct BrowserApp {
    info: AppInfo,
    url_input: String,
    url_focused: bool,
    client: TcpHttpClient,
    fetch_kind: FetchKind,
    page_host: String,
    page_https: bool,
    pending_css_urls: Vec<String>,
    last_http_state: HttpState,
    dom: DomNode,
    css_rules: Vec<CssRule>,
    js: JsEngine,
    page_title: String,
    scroll_y: i32,
    focused_dom_input: Option<String>,
    cached_layout: LayoutBuilder,
    last_viewport_w: i32,
    layout_dirty: bool,
    hover_go: bool,
    hover_home: bool,
    hover_up: bool,
    hover_down: bool,
    hover_hit: Option<usize>,
}

impl BrowserApp {
    pub fn new() -> Self {
        let parsed = parse_html_document(HOME_HTML);
        let mut dom = parsed.root;
        let mut js = JsEngine::new();
        for scr in &parsed.scripts {
            js.execute(scr, &mut dom);
        }
        BrowserApp {
            info: AppInfo::new("nc.browser", "BROWSER", "4.0.0", Icon::Generic, 48, 30),
            url_input: String::from("about:home"),
            url_focused: false,
            client: TcpHttpClient::new(),
            fetch_kind: FetchKind::MainHtml,
            page_host: String::new(),
            page_https: true,
            pending_css_urls: Vec::new(),
            last_http_state: HttpState::Idle,
            dom,
            css_rules: parsed.css_rules,
            js,
            page_title: parsed.title,
            scroll_y: 0,
            focused_dom_input: None,
            cached_layout: LayoutBuilder::empty(),
            last_viewport_w: 0,
            layout_dirty: true,
            hover_go: false,
            hover_home: false,
            hover_up: false,
            hover_down: false,
            hover_hit: None,
        }
    }

    fn resolve_url(&self, href: &str) -> String {
        let h = href.trim();
        if h.starts_with("http://") || h.starts_with("https://") || h.starts_with("about:") {
            return String::from(h);
        }
        let scheme = if self.page_https { "https://" } else { "http://" };
        if let Some(rest) = h.strip_prefix("//") {
            return alloc::format!("{}{}", scheme, rest);
        }
        if self.page_host.is_empty() {
            return String::from(h);
        }
        if h.starts_with('/') {
            alloc::format!("{}{}{}", scheme, self.page_host, h)
        } else {
            alloc::format!("{}{}/{}", scheme, self.page_host, h)
        }
    }

    fn load_html(&mut self, html: &str) {
        let parsed = parse_html_document(html);
        let mut dom = parsed.root;
        let mut js = JsEngine::new();
        for scr in &parsed.scripts {
            js.execute(scr, &mut dom);
        }
        self.dom = dom;
        self.css_rules = parsed.css_rules;
        self.pending_css_urls = parsed.stylesheet_urls;
        self.js = js;
        self.page_title = parsed.title;
        self.scroll_y = 0;
        self.focused_dom_input = None;
        self.layout_dirty = true;
    }

    fn trigger_navigation(&mut self, target_url: &str) {
        let resolved = self.resolve_url(target_url);
        self.url_input = resolved.clone();
        self.pending_css_urls.clear();
        self.fetch_kind = FetchKind::MainHtml;

        if resolved.eq_ignore_ascii_case("about:home") || resolved.is_empty() {
            self.client.state = HttpState::Idle;
            self.page_host.clear();
            self.load_html(HOME_HTML);
            return;
        }

        let net = net_global();
        self.client.navigate(&resolved, net);
        self.page_host = self.client.host.clone();
        self.page_https = self.client.https;
    }

    fn ensure_layout(&mut self, viewport_w: i32, unit: i32) {
        if self.layout_dirty || self.last_viewport_w != viewport_w {
            self.cached_layout = LayoutBuilder::build(&self.dom, viewport_w, unit);
            self.last_viewport_w = viewport_w;
            self.layout_dirty = false;
        }
    }

    fn home_btn_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + unit, area.y + unit * 3 / 4, unit * 4, unit * 2)
    }

    fn url_box_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + unit * 11 / 2, area.y + unit * 3 / 4, area.width - unit * 16, unit * 2)
    }

    fn go_btn_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + area.width - unit * 10, area.y + unit * 3 / 4, unit * 4, unit * 2)
    }

    fn scroll_up_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + area.width - unit * 11 / 2, area.y + unit * 3 / 4, unit * 2, unit * 2)
    }

    fn scroll_down_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + area.width - unit * 3, area.y + unit * 3 / 4, unit * 2, unit * 2)
    }

    fn viewport_rect(area: Rect, unit: i32) -> Rect {
        Rect::new(area.x + unit, area.y + unit * 7 / 2, area.width - unit * 2, area.height - unit * 5)
    }
}

fn inside(area: Rect, x: i32, y: i32) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

impl App for BrowserApp {
    fn info(&self) -> &AppInfo {
        &self.info
    }

    fn update(&mut self, ctx: &mut Context, input: &Input) -> Update {
        let unit = ctx.unit;
        let mut redraw = false;

        if !matches!(self.client.state, HttpState::Idle | HttpState::Complete | HttpState::Failed) {
            let net = net_global();
            self.client.poll(net);
            if self.client.state == HttpState::Complete && self.last_http_state != HttpState::Complete {
                let body = self.client.html_body.clone();
                match self.fetch_kind {
                    FetchKind::MainHtml => {
                        self.page_host = self.client.host.clone();
                        self.page_https = self.client.https;
                        self.load_html(&body);
                        if let Some(css_href) = self.pending_css_urls.pop() {
                            let css_url = self.resolve_url(&css_href);
                            self.fetch_kind = FetchKind::Stylesheet;
                            self.client.navigate(&css_url, net);
                        }
                    }
                    FetchKind::Stylesheet => {
                        let extra_rules = parse_css_stylesheet(&body);
                        self.css_rules.extend(extra_rules);
                        let root_col = self.dom.style.text_color;
                        apply_styles_recursive(&mut self.dom, &self.css_rules, Some(root_col), 0);
                        self.layout_dirty = true;
                        self.fetch_kind = FetchKind::MainHtml;
                    }
                }
                redraw = true;
            }
        }
        if self.client.state != self.last_http_state {
            self.last_http_state = self.client.state;
            redraw = true;
        }

        let viewport = Self::viewport_rect(input.area, unit);
        self.ensure_layout(viewport.width, unit);
        let max_scroll = (self.cached_layout.total_height - viewport.height).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max_scroll);

        if let Some(key) = input.key {
            match key {
                KeyCode::Up => {
                    self.scroll_y = (self.scroll_y - unit * 2).max(0);
                    redraw = true;
                }
                KeyCode::Down => {
                    self.scroll_y = (self.scroll_y + unit * 2).min(max_scroll);
                    redraw = true;
                }
                KeyCode::Backspace => {
                    if self.url_focused {
                        self.url_input.pop();
                        redraw = true;
                    } else if let Some(ref id) = self.focused_dom_input {
                        if let Some(node) = self.dom.find_by_id_mut(id) {
                            node.value.pop();
                            self.layout_dirty = true;
                            redraw = true;
                        }
                    }
                }
                KeyCode::Enter => {
                    if self.url_focused {
                        let target = self.url_input.clone();
                        self.trigger_navigation(&target);
                        redraw = true;
                    }
                }
                KeyCode::Char(ch) => {
                    if self.url_focused {
                        self.url_input.push(ch);
                        redraw = true;
                    } else if let Some(ref id) = self.focused_dom_input {
                        if let Some(node) = self.dom.find_by_id_mut(id) {
                            node.value.push(ch);
                            self.layout_dirty = true;
                            redraw = true;
                        }
                    }
                }
                _ => {}
            }
        }

        let mut next_go = false;
        let mut next_home = false;
        let mut next_up = false;
        let mut next_down = false;
        let mut next_hit = None;

        if let Some(pointer) = input.pointer {
            let home_r = Self::home_btn_rect(input.area, unit);
            let url_r = Self::url_box_rect(input.area, unit);
            let go_r = Self::go_btn_rect(input.area, unit);
            let up_r = Self::scroll_up_rect(input.area, unit);
            let down_r = Self::scroll_down_rect(input.area, unit);

            next_home = inside(home_r, pointer.x, pointer.y);
            next_go = inside(go_r, pointer.x, pointer.y);
            next_up = inside(up_r, pointer.x, pointer.y);
            next_down = inside(down_r, pointer.x, pointer.y);

            if inside(viewport, pointer.x, pointer.y) {
                let rel_x = pointer.x - viewport.x;
                let rel_y = pointer.y - viewport.y + self.scroll_y;
                for (idx, h) in self.cached_layout.hits.iter().enumerate().rev() {
                    if inside(h.rect, rel_x, rel_y) {
                        next_hit = Some(idx);
                        break;
                    }
                }
            }

            if pointer.clicked {
                self.url_focused = inside(url_r, pointer.x, pointer.y);
                self.focused_dom_input = None;
                redraw = true;

                if next_home {
                    self.trigger_navigation("about:home");
                } else if next_go {
                    let target = self.url_input.clone();
                    self.trigger_navigation(&target);
                } else if next_up {
                    self.scroll_y = (self.scroll_y - unit * 3).max(0);
                } else if next_down {
                    self.scroll_y = (self.scroll_y + unit * 3).min(max_scroll);
                } else if let Some(hit_idx) = next_hit {
                    let hit = self.cached_layout.hits[hit_idx].clone();
                    if let Some(inp_id) = hit.input_id {
                        self.focused_dom_input = Some(inp_id);
                    }
                    if let Some(script) = hit.onclick {
                        self.js.execute(&script, &mut self.dom);
                        self.layout_dirty = true;
                    }
                    if let Some(href) = hit.href {
                        self.trigger_navigation(&href);
                    }
                }
            }
        }

        if self.layout_dirty {
            self.ensure_layout(viewport.width, unit);
        }

        if next_go != self.hover_go
            || next_home != self.hover_home
            || next_up != self.hover_up
            || next_down != self.hover_down
            || next_hit != self.hover_hit
        {
            self.hover_go = next_go;
            self.hover_home = next_home;
            self.hover_up = next_up;
            self.hover_down = next_down;
            self.hover_hit = next_hit;
            redraw = true;
        }

        Update {
            redraw,
            close: false,
            install_app: None,
        }
    }

    fn draw(&self, canvas: &mut Canvas<'_>, area: Rect, ctx: &Context) {
        let unit = ctx.unit;
        let accent = ctx.settings.accent_color();

        let toolbar = Rect::new(area.x + unit / 2, area.y + unit / 3, area.width - unit, unit * 11 / 4);
        draw_panel(canvas, toolbar, unit / 3, SIDEBAR, 255, PANEL_BORDER, 255);

        draw_button(canvas, Self::home_btn_rect(area, unit), "HOME", self.hover_home, false);
        draw_input_box(canvas, Self::url_box_rect(area, unit), &self.url_input, self.url_focused);
        draw_accent_button(canvas, Self::go_btn_rect(area, unit), "GO", self.hover_go, accent);
        draw_button(canvas, Self::scroll_up_rect(area, unit), "UP", self.hover_up, false);
        draw_button(canvas, Self::scroll_down_rect(area, unit), "DN", self.hover_down, false);

        let viewport = Self::viewport_rect(area, unit);
        let page_bg = self.dom.style.bg_color.unwrap_or(WHITE);
        draw_panel(canvas, viewport, unit / 3, page_bg, 255, PANEL_BORDER, 255);

        for cmd in &self.cached_layout.cmds {
            match cmd {
                RenderCmd::Panel { rect, radius, bg, border, is_card } => {
                    let sy = viewport.y + rect.y - self.scroll_y;
                    let clipped_top = sy.max(viewport.y + 2);
                    let clipped_bottom = (sy + rect.height).min(viewport.y + viewport.height - 2);
                    if clipped_bottom > clipped_top {
                        let draw_r = Rect::new(viewport.x + rect.x, clipped_top, rect.width, clipped_bottom - clipped_top);
                        if *is_card {
                            draw_card(canvas, draw_r, *radius);
                        } else {
                            let b_col = border.unwrap_or(*bg);
                            let b_alpha = if border.is_some() { 255 } else { 0 };
                            draw_panel(canvas, draw_r, *radius, *bg, 255, b_col, b_alpha);
                        }
                    }
                }
                RenderCmd::Text { x, y, text, cap_height, color, underline } => {
                    let sy = viewport.y + *y - self.scroll_y;
                    if sy >= viewport.y + 2 && sy + *cap_height <= viewport.y + viewport.height - 2 {
                        let style = text_style(*cap_height, 24);
                        let sx = viewport.x + *x;
                        draw_text_color(canvas, text, sx, sy, &style, *color);
                        if *underline {
                            let tw = text_width(text, &style);
                            canvas.fill_rect(sx, sy + *cap_height + 2, tw, 1, *color);
                        }
                    }
                }
                RenderCmd::Button { rect, label, bg, fg, border, is_pill } => {
                    let sy = viewport.y + rect.y - self.scroll_y;
                    if sy >= viewport.y + 2 && sy + rect.height <= viewport.y + viewport.height - 2 {
                        let draw_r = Rect::new(viewport.x + rect.x, sy, rect.width, rect.height);
                        let rad = if *is_pill { draw_r.height / 2 } else { draw_r.height / 4 };
                        let fill_c = bg.unwrap_or(accent);
                        let border_c = border.unwrap_or(fill_c);
                        let border_a = if border.is_some() { 255 } else { 0 };
                        draw_panel(canvas, draw_r, rad, fill_c, 255, border_c, border_a);

                        let b_style = text_style(draw_r.height * 40 / 100, 26);
                        let tw = text_width(label, &b_style);
                        let tx = draw_r.x + (draw_r.width - tw) / 2;
                        let ty = draw_r.y + (draw_r.height - b_style.cap_height) / 2;
                        let text_col = if bg.is_some() { *fg } else { WHITE };
                        draw_text_color(canvas, label, tx, ty, &b_style, text_col);
                    }
                }
                RenderCmd::InputBox { rect, id, value, placeholder, dark_mode } => {
                    let sy = viewport.y + rect.y - self.scroll_y;
                    if sy >= viewport.y + 2 && sy + rect.height <= viewport.y + viewport.height - 2 {
                        let draw_r = Rect::new(viewport.x + rect.x, sy, rect.width, rect.height);
                        let focused = self.focused_dom_input.as_deref() == Some(id.as_str());
                        let fill = if *dark_mode { rgb(15, 20, 25) } else { PANEL_FILL };
                        let brd = if focused {
                            rgb(29, 155, 240)
                        } else if *dark_mode {
                            rgb(51, 65, 85)
                        } else {
                            PANEL_BORDER
                        };
                        draw_panel(canvas, draw_r, draw_r.height / 4, fill, 255, brd, 255);
                        let shown = if value.is_empty() { placeholder } else { value };
                        let col = if value.is_empty() {
                            TEXT_DIM
                        } else if *dark_mode {
                            WHITE
                        } else {
                            TEXT_PRIMARY
                        };
                        let st = text_style(draw_r.height * 40 / 100, 26);
                        let ty = draw_r.y + (draw_r.height - st.cap_height) / 2;
                        draw_text_color(canvas, shown, draw_r.x + unit / 2, ty, &st, col);
                    }
                }
                RenderCmd::SvgIcon { rect, svg, color } => {
                    let sy = viewport.y + rect.y - self.scroll_y;
                    if sy >= viewport.y + 2 && sy + rect.height <= viewport.y + viewport.height - 2 {
                        let draw_r = Rect::new(viewport.x + rect.x, sy, rect.width, rect.height);
                        draw_svg_shape(canvas, draw_r, svg, *color);
                    }
                }
                RenderCmd::RuleLine { x, y, width, color } => {
                    let sy = viewport.y + *y - self.scroll_y;
                    if sy >= viewport.y && sy < viewport.y + viewport.height {
                        canvas.fill_rect(viewport.x + *x, sy, *width, 1, *color);
                    }
                }
            }
        }

        let status_y = area.y + area.height - unit * 5 / 4;
        let small = text_style(unit * 50 / 100, 30);
        let state_label = match self.client.state {
            HttpState::Idle => "READY (UNIVERSAL ENGINE)",
            HttpState::WaitingNetwork => "WAITING FOR DHCP...",
            HttpState::ResolvingDns => "RESOLVING DNS...",
            HttpState::ConnectingSyn => "CONNECTING TCP...",
            HttpState::Handshaking => "TLS 1.3 HANDSHAKE...",
            HttpState::Connected | HttpState::RequestSent => {
                if self.fetch_kind == FetchKind::Stylesheet {
                    "FETCHING EXTERNAL CSS..."
                } else {
                    "SENDING HTTPS GET..."
                }
            }
            HttpState::ReceivingData => "RECEIVING DATA...",
            HttpState::Complete => "COMPLETE (200 OK)",
            HttpState::Failed => "NETWORK / TLS ERROR",
        };
        draw_text_color(canvas, state_label, area.x + unit, status_y, &small, TEXT_MUTED);

        if let Some(last_log) = self.js.logs.last() {
            let mut log_line = Line::new();
            log_line.text("JS: ").text(last_log);
            draw_badge(
                canvas,
                Rect::new(area.x + area.width - unit * 18, status_y - unit / 4, unit * 17, unit * 5 / 4),
                log_line.as_str(),
                SIDEBAR_HOVER,
                TEXT_PRIMARY,
            );
        } else {
            let mut short_title = Line::new();
            short_title.text(&self.page_title);
            draw_badge(
                canvas,
                Rect::new(area.x + area.width - unit * 15, status_y - unit / 4, unit * 14, unit * 5 / 4),
                short_title.as_str(),
                ACCENT_BG,
                accent,
            );
        }
    }
}
