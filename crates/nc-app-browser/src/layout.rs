extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use nc_font::{text_width, Style};
use nc_gfx::{rgb, Rect};
use nc_ui::{ACCENT_LINE, PANEL_BORDER, TEXT_MUTED, TEXT_PRIMARY, WHITE};
use core::sync::atomic::{AtomicI32, Ordering};
use crate::cssvalues::{self, LenCtx};
use crate::css::{parse_css_color, parse_css_font_size, parse_css_px, parse_declarations, parse_float_i32, Declaration, SafeClamp, Stylesheet};
use crate::dom::{DomTree, NodeType};
use crate::html::sanitize_text;
use crate::image::{rasterize_svg_tree, DecodedImage};

/// Maximum nesting guard for layout recursion. Every nested element costs two levels
/// (`build_node_box` + `layout_flow_children`), so 60 allows about 30 nested elements.
/// It used to be 22 (about 11 elements deep), which silently dropped deeply nested content.
const MAX_LAYOUT_DEPTH: usize = 60;
// Old caps (96 / 64 / 200 / 600) silently flattened real designs; these only guard against absurd values.
const MAX_MARGIN: i32 = 480;
const MAX_PADDING: i32 = 320;
const MAX_HEIGHT: i32 = 4000;

static VIEWPORT_W: AtomicI32 = AtomicI32::new(1280);
static VIEWPORT_H: AtomicI32 = AtomicI32::new(720);

/// Tells the style engine the real viewport so `vw`, `vh`, `vmin`, `vmax` resolve correctly.
pub fn set_viewport(w: i32, h: i32) {
    VIEWPORT_W.store(w.max(1), Ordering::Relaxed);
    VIEWPORT_H.store(h.max(1), Ordering::Relaxed);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextTransform {
    None,
    Upper,
    Lower,
    Capitalize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WhiteSpace {
    Normal,
    NoWrap,
    Pre,
    PreWrap,
    PreLine,
}

pub const DECO_UNDERLINE: u8 = 1;
pub const DECO_LINE_THROUGH: u8 = 2;
pub const DECO_OVERLINE: u8 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisplayType {
    Block,
    Inline,
    InlineBlock,
    Flex,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JustifyContent {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// How an element lays out its children.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContainerKind {
    Flow,
    Flex,
    Grid,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlignItems {
    Stretch,
    Start,
    Center,
    End,
}

/// One `grid-template-columns` track. `Fr` and `MinMax` flex values are stored x100 (1fr = 100).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GridTrack {
    Px(i32),
    Fr(i32),
    Auto,
    MinMax(i32, i32),
}

#[derive(Clone, Debug)]
pub struct ComputedStyle {
    pub display: DisplayType,
    pub font_size: i32,
    pub color: u32,
    pub bg_color: Option<u32>,
    pub border_color: Option<u32>,
    pub border_width: i32,
    pub border_radius: i32,
    pub margin_top: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub margin_right: i32,
    pub padding_top: i32,
    pub padding_bottom: i32,
    pub padding_left: i32,
    pub padding_right: i32,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub text_align: TextAlign,
    pub justify_content: JustifyContent,
    pub flex_row: bool,
    pub gap: i32,
    pub is_link: bool,
    pub kind: ContainerKind,
    pub flex_reverse: bool,
    pub flex_wrap: bool,
    pub align_items: AlignItems,
    pub align_self: Option<AlignItems>,
    /// flex-grow / flex-shrink are stored x100 (1 = 100).
    pub flex_grow: i32,
    pub flex_shrink: i32,
    pub flex_basis: Option<i32>,
    pub row_gap: i32,
    pub col_gap: i32,
    pub grid_cols: Vec<GridTrack>,
    /// `repeat(auto-fill | auto-fit, track)`: (track, is_auto_fit)
    pub grid_auto: Option<(GridTrack, bool)>,
    pub grid_span: i32,
    pub max_width: Option<i32>,
    pub min_width: Option<i32>,
    pub min_height: Option<i32>,
    pub margin_left_auto: bool,
    pub margin_right_auto: bool,
    /// 100..900 (400 normal, 700 bold). Inherited.
    pub font_weight: i32,
    /// Stored for completeness; nc-font has no slanted glyphs yet. Inherited.
    pub italic: bool,
    /// Multiplier x100 of the font size (150 = 1.5). `None` = `normal`. Inherited.
    pub line_height: Option<i32>,
    /// Bit set of `DECO_*`. Propagates to descendants.
    pub text_decoration: u8,
    pub text_transform: TextTransform,
    /// Extra advance per glyph in layout units (CSS px * 3/4). Inherited.
    pub letter_spacing: i32,
    pub white_space: WhiteSpace,
    pub position: Position,
    pub top: Option<i32>,
    pub right: Option<i32>,
    pub bottom: Option<i32>,
    pub left: Option<i32>,
    pub z_index: i32,
    /// `overflow: hidden | clip` (auto/scroll are not clipped because nothing can scroll yet).
    pub overflow_clip: bool,
    pub max_height: Option<i32>,
    /// Parsed `box-sizing: border-box`; sizing still follows the engine's border-box convention.
    pub border_box: bool,
    /// Cascade state for the four border sides (top, right, bottom, left).
    pub bw: [i32; 4],
    pub bs: [bool; 4],
    pub bc: [Option<u32>; 4],
    /// Effective per-side widths/colours, valid when `sides_custom`.
    pub border_sides: [i32; 4],
    pub border_side_colors: [Option<u32>; 4],
    pub sides_custom: bool,
}

impl ComputedStyle {
    /// Effective border widths as [top, right, bottom, left].
    pub fn eff_borders(&self) -> [i32; 4] {
        if self.sides_custom {
            self.border_sides
        } else {
            [self.border_width; 4]
        }
    }

    pub fn is_out_of_flow(&self) -> bool {
        matches!(self.position, Position::Absolute | Position::Fixed)
    }
}

/// Text style for page content: weight -> stroke thickness, `letter-spacing` -> tracking.
/// Page text uses normal spacing (0); the 30% tracking is browser chrome styling only.
pub fn page_text_style(st: &ComputedStyle) -> Style {
    let cap = st.font_size;
    let mut stroke = (cap * 16 / 18).max(22);
    if st.font_weight >= 600 {
        stroke = stroke * 13 / 10 + 2;
    } else if st.font_weight <= 300 {
        stroke = (stroke * 85 / 100).max(18);
    }
    Style { cap_height: cap, stroke_sixteenths: stroke, tracking: st.letter_spacing }
}

/// Line box height for text of this style.
pub fn line_height_px(st: &ComputedStyle) -> i32 {
    match st.line_height {
        Some(m) => (st.font_size * m / 100).max(8),
        None => (st.font_size * 15 / 10).max(16),
    }
}

fn apply_text_transform(text: &str, mode: TextTransform) -> String {
    match mode {
        TextTransform::None => String::from(text),
        TextTransform::Upper => text.to_uppercase(),
        TextTransform::Lower => text.to_lowercase(),
        TextTransform::Capitalize => {
            let mut out = String::with_capacity(text.len());
            let mut start = true;
            for ch in text.chars() {
                if start && ch.is_alphabetic() {
                    out.extend(ch.to_uppercase());
                    start = false;
                } else {
                    out.push(ch);
                    if ch.is_whitespace() || ch == '-' {
                        start = true;
                    } else if ch.is_alphanumeric() {
                        start = false;
                    }
                }
            }
            out
        }
    }
}

fn len_ctx(style: &ComputedStyle, container_width: i32) -> LenCtx {
    LenCtx {
        font_px: style.font_size * 4 / 3,
        root_px: 16,
        percent_base: container_width,
        viewport_w: VIEWPORT_W.load(Ordering::Relaxed),
        viewport_h: VIEWPORT_H.load(Ordering::Relaxed),
    }
}

/// Resolves a CSS length (all units, `calc()`, `min()`, `max()`, `clamp()`).
/// `auto` / `none` / `normal` keep the legacy meaning "0 / unset" so existing arms behave as before.
fn css_len(val: &str, style: &ComputedStyle, container_width: i32) -> Option<i32> {
    let t = val.trim();
    if t.eq_ignore_ascii_case("auto") || t.eq_ignore_ascii_case("none") || t.eq_ignore_ascii_case("normal") {
        return Some(0);
    }
    let ctx = len_ctx(style, container_width);
    let first = if t.contains('(') { t } else { t.split_whitespace().next()? };
    cssvalues::eval_length(first, &ctx).or_else(|| parse_css_px(val, style.font_size, container_width))
}
#[derive(Clone, Debug)]
pub enum WidgetKind {
    None,
    TextInput {
        input_id: usize,
        form_action: String,
        input_name: String,
        value: String,
        placeholder: String,
    },
    SubmitButton {
        form_action: String,
        input_name: String,
        label: String,
        target_input_id: Option<usize>,
    },
    Image {
        src: String,
        alt: String,
    },
    HorizontalRule,
}

#[derive(Clone, Debug)]
pub struct LayoutBox {
    pub node_id: usize,
    pub rect: Rect,
    pub text: Option<String>,
    pub style: ComputedStyle,
    pub link_url: Option<String>,
    pub widget: WidgetKind,
    pub children: Vec<LayoutBox>,
}

pub struct LayoutEngine {
    pub boxes: Vec<LayoutBox>,
    pub total_height: i32,
    pub page_bg: u32,
    next_input_id: usize,
    active_form_action: String,
    last_form_input_id: Option<usize>,
    last_form_input_name: String,
    /// Width (border box) imposed by a flex/grid container on the next `build_node_box` call.
    forced_width: Option<i32>,
}

impl LayoutEngine {
    pub fn build(
        tree: &DomTree,
        stylesheet: &Stylesheet,
        images: &mut Vec<(String, DecodedImage)>,
        content_width: i32,
    ) -> Self {
        let mut engine = LayoutEngine {
            boxes: Vec::new(),
            total_height: 0,
            page_bg: WHITE,
            next_input_id: 1,
            active_form_action: String::from("/search"),
            last_form_input_id: None,
            last_form_input_name: String::from("q"),
            forced_width: None,
        };

        let root_style = default_computed_style();
        for node in &tree.nodes {
            if let NodeType::Element { ref tag, .. } = node.node_type {
                if tag == "body" || tag == "html" {
                    let s = compute_node_style(tree, stylesheet, node.id, Some(&root_style), content_width);
                    if let Some(bg) = s.bg_color {
                        engine.page_bg = bg;
                    }
                }
            }
        }

        let mut cursor_y = 12;
        engine.boxes = engine.layout_flow_children(
            tree,
            stylesheet,
            images,
            &tree.nodes[tree.root].children.clone(),
            12,
            &mut cursor_y,
            (content_width - 24).max(120),
            &root_style,
            None,
            engine.page_bg,
            0,
        );
        engine.total_height = cursor_y + 24;
        engine
    }

    fn is_inline_node(
        &self,
        tree: &DomTree,
        stylesheet: &Stylesheet,
        node_id: usize,
        parent_style: &ComputedStyle,
        avail_w: i32,
    ) -> bool {
        match &tree.nodes[node_id].node_type {
            NodeType::Text(_) => true,
            NodeType::Element { tag, .. } => {
                if tag == "br" {
                    return true;
                }
                let st = compute_node_style(tree, stylesheet, node_id, Some(parent_style), avail_w);
                matches!(st.display, DisplayType::Inline | DisplayType::InlineBlock)
            }
        }
    }

    fn layout_flow_children(
        &mut self,
        tree: &DomTree,
        stylesheet: &Stylesheet,
        images: &mut Vec<(String, DecodedImage)>,
        child_ids: &[usize],
        x: i32,
        cursor_y: &mut i32,
        available_width: i32,
        parent_style: &ComputedStyle,
        inherited_link: Option<String>,
        effective_bg: u32,
        depth: usize,
    ) -> Vec<LayoutBox> {
        let mut out = Vec::new();
        if depth > MAX_LAYOUT_DEPTH {
            return out;
        }

        let mut idx = 0;
        let mut prev_margin_bottom = 0i32;

        while idx < child_ids.len() {
            let cid = child_ids[idx];
            if self.is_inline_node(tree, stylesheet, cid, parent_style, available_width) {
                let run_start = idx;
                while idx < child_ids.len()
                    && self.is_inline_node(tree, stylesheet, child_ids[idx], parent_style, available_width)
                {
                    idx += 1;
                }
                let inline_boxes = self.layout_inline_formatting_context(
                    tree,
                    stylesheet,
                    images,
                    &child_ids[run_start..idx],
                    x,
                    cursor_y,
                    available_width,
                    parent_style,
                    inherited_link.clone(),
                    effective_bg,
                    depth + 1,
                );
                if !inline_boxes.is_empty() {
                    prev_margin_bottom = 4;
                    out.extend(inline_boxes);
                }
            } else {
                idx += 1;
                if let Some(b) = self.build_node_box(
                    tree,
                    stylesheet,
                    images,
                    cid,
                    x,
                    cursor_y,
                    available_width,
                    parent_style,
                    inherited_link.clone(),
                    effective_bg,
                    &mut prev_margin_bottom,
                    depth + 1,
                ) {
                    out.push(b);
                }
            }
        }

        out
    }

    fn layout_inline_formatting_context(
        &mut self,
        tree: &DomTree,
        stylesheet: &Stylesheet,
        images: &mut Vec<(String, DecodedImage)>,
        run_ids: &[usize],
        start_x: i32,
        cursor_y: &mut i32,
        max_width: i32,
        parent_style: &ComputedStyle,
        inherited_link: Option<String>,
        effective_bg: u32,
        depth: usize,
    ) -> Vec<LayoutBox> {
        let mut items: Vec<LayoutBox> = Vec::new();
        self.collect_inline_items(
            tree,
            stylesheet,
            images,
            run_ids,
            max_width,
            parent_style,
            inherited_link,
            effective_bg,
            depth,
            &mut items,
        );

        if items.is_empty() {
            return Vec::new();
        }

        let mut committed: Vec<LayoutBox> = Vec::new();
        let mut line_items: Vec<LayoutBox> = Vec::new();
        let mut line_w = 0i32;
        let mut line_h = line_height_px(parent_style);

        let flush_line = |line_items: &mut Vec<LayoutBox>,
                          committed: &mut Vec<LayoutBox>,
                          cursor_y: &mut i32,
                          line_w: &mut i32,
                          line_h: &mut i32| {
            if line_items.is_empty() {
                return;
            }
            let actual_w = (*line_w).max(0);
            let offset_x = match parent_style.text_align {
                TextAlign::Left => start_x,
                TextAlign::Center => start_x + (max_width - actual_w).max(0) / 2,
                TextAlign::Right => start_x + (max_width - actual_w).max(0),
            };
            let mut cur_x = offset_x;
            for mut item in line_items.drain(..) {
                let item_y = *cursor_y + (*line_h - item.rect.height).max(0) / 2;
                let dx = cur_x - item.rect.x;
                let dy = item_y - item.rect.y;
                offset_box_tree(&mut item, dx, dy);
                cur_x += item.rect.width + item.style.margin_right;
                committed.push(item);
            }
            *cursor_y += *line_h + 4;
            *line_w = 0;
            *line_h = line_height_px(parent_style);
        };

        for item in items {
            if item.rect.width == -1 {
                flush_line(&mut line_items, &mut committed, cursor_y, &mut line_w, &mut line_h);
                continue;
            }
            let item_total_w = item.style.margin_left + item.rect.width + item.style.margin_right;
            if !line_items.is_empty() && line_w + item_total_w > max_width {
                flush_line(&mut line_items, &mut committed, cursor_y, &mut line_w, &mut line_h);
            }
            line_w += item_total_w;
            if item.rect.height > line_h {
                line_h = item.rect.height;
            }
            line_items.push(item);
        }
        flush_line(&mut line_items, &mut committed, cursor_y, &mut line_w, &mut line_h);

        committed
    }

    fn collect_inline_items(
        &mut self,
        tree: &DomTree,
        stylesheet: &Stylesheet,
        images: &mut Vec<(String, DecodedImage)>,
        node_ids: &[usize],
        max_width: i32,
        parent_style: &ComputedStyle,
        inherited_link: Option<String>,
        effective_bg: u32,
        depth: usize,
        out: &mut Vec<LayoutBox>,
    ) {
        if depth > MAX_LAYOUT_DEPTH {
            return;
        }
        for &nid in node_ids {
            match &tree.nodes[nid].node_type {
                NodeType::Text(raw_text) => {
                    let mut st = parent_style.clone();
                    st.color = ensure_contrast(st.color, effective_bg, inherited_link.is_some());
                    st.is_link = inherited_link.is_some();
                    st.margin_right = (st.font_size / 2).max(5);
                    let font = page_text_style(&st);
                    let lh = line_height_px(&st);
                    let shown_text = apply_text_transform(&sanitize_text(raw_text), st.text_transform);
                    let keep_newlines = matches!(st.white_space, WhiteSpace::Pre | WhiteSpace::PreWrap | WhiteSpace::PreLine);
                    let pieces: Vec<&str> = if keep_newlines { shown_text.split('\n').collect() } else { alloc::vec![shown_text.as_str()] };
                    for (pi, piece) in pieces.iter().enumerate() {
                        if pi > 0 {
                            // same zero-width marker a <br> produces: forces a new line
                            out.push(LayoutBox {
                                node_id: nid,
                                rect: Rect::new(0, 0, -1, 0),
                                text: None,
                                style: parent_style.clone(),
                                link_url: None,
                                widget: WidgetKind::None,
                                children: Vec::new(),
                            });
                        }
                        for word in piece.split_whitespace() {
                            let ww = text_width(word, &font).sclamp(6, max_width);
                            out.push(LayoutBox {
                                node_id: nid,
                                rect: Rect::new(0, 0, ww, lh),
                                text: Some(String::from(word)),
                                style: st.clone(),
                                link_url: inherited_link.clone(),
                                widget: WidgetKind::None,
                                children: Vec::new(),
                            });
                        }
                    }
                }
                NodeType::Element { tag, .. } => {
                    if tree.attr(nid, "hidden").is_some() || tree.attr(nid, "aria-hidden") == Some("true") {
                        continue;
                    }
                    if tag == "br" {
                        out.push(LayoutBox {
                            node_id: nid,
                            rect: Rect::new(0, 0, -1, 0),
                            text: None,
                            style: parent_style.clone(),
                            link_url: None,
                            widget: WidgetKind::None,
                            children: Vec::new(),
                        });
                        continue;
                    }

                    let mut st = compute_node_style(tree, stylesheet, nid, Some(parent_style), max_width);
                    if st.display == DisplayType::None || is_visually_hidden(&st) {
                        continue;
                    }
                    let cur_bg = st.bg_color.unwrap_or(effective_bg);
                    let link = if tag == "a" {
                        tree.attr(nid, "href").map(String::from).or_else(|| inherited_link.clone())
                    } else {
                        inherited_link.clone()
                    };
                    if link.is_some() {
                        st.is_link = true;
                    }
                    st.color = ensure_contrast(st.color, cur_bg, st.is_link);

                    if matches!(tag.as_str(), "img" | "svg" | "input" | "button" | "textarea")
                        || st.display == DisplayType::InlineBlock
                        || st.bg_color.is_some()
                        || st.border_width > 0
                        || st.sides_custom
                    {
                        let mut dummy_y = 0i32;
                        let mut dummy_mb = 0i32;
                        let item_avail_w = st.width.unwrap_or(max_width).min(max_width);
                        if let Some(mut b) = self.build_node_box(
                            tree,
                            stylesheet,
                            images,
                            nid,
                            0,
                            &mut dummy_y,
                            item_avail_w,
                            parent_style,
                            link,
                            effective_bg,
                            &mut dummy_mb,
                            depth + 1,
                        ) {
                            if b.style.margin_right == 0 {
                                b.style.margin_right = 8;
                            }
                            out.push(b);
                        }
                    } else {
                        let children = tree.nodes[nid].children.clone();
                        self.collect_inline_items(
                            tree,
                            stylesheet,
                            images,
                            &children,
                            max_width,
                            &st,
                            link,
                            cur_bg,
                            depth + 1,
                            out,
                        );
                    }
                }
            }
        }
    }

    fn build_node_box(
        &mut self,
        tree: &DomTree,
        stylesheet: &Stylesheet,
        images: &mut Vec<(String, DecodedImage)>,
        node_id: usize,
        x: i32,
        cursor_y: &mut i32,
        available_width: i32,
        parent_style: &ComputedStyle,
        inherited_link: Option<String>,
        parent_bg: u32,
        prev_margin_bottom: &mut i32,
        depth: usize,
    ) -> Option<LayoutBox> {
        // A flex/grid parent may impose this node's border-box width; it must not leak to descendants.
        let forced = self.forced_width.take();
        if depth > MAX_LAYOUT_DEPTH {
            return None;
        }
        let node = &tree.nodes[node_id];
        match &node.node_type {
            NodeType::Element { tag, .. } => {
                if tree.attr(node_id, "hidden").is_some()
                    || tree.attr(node_id, "aria-hidden") == Some("true")
                {
                    return None;
                }

                let mut style = compute_node_style(
                    tree,
                    stylesheet,
                    node_id,
                    Some(parent_style),
                    available_width,
                );

                if style.display == DisplayType::None || is_visually_hidden(&style) {
                    return None;
                }

                let current_bg = style.bg_color.unwrap_or(parent_bg);
                let link_url = if tag == "a" {
                    tree.attr(node_id, "href").map(String::from).or(inherited_link)
                } else {
                    inherited_link
                };
                if link_url.is_some() {
                    style.is_link = true;
                }
                style.color = ensure_contrast(style.color, current_bg, style.is_link);

                let collapsed_top = (style.margin_top - *prev_margin_bottom).max(0);

                let prev_form_action = self.active_form_action.clone();
                if tag == "form" {
                    if let Some(act) = tree.attr(node_id, "action") {
                        self.active_form_action = String::from(act);
                    }
                }

                if tag == "br" {
                    *cursor_y += style.font_size;
                    return None;
                }

                if tag == "hr" {
                    *cursor_y += 6;
                    let hr_box = LayoutBox {
                        node_id: node_id,
                        rect: Rect::new(x, *cursor_y, available_width, 2),
                        text: None,
                        style,
                        link_url: None,
                        widget: WidgetKind::HorizontalRule,
                        children: Vec::new(),
                    };
                    *cursor_y += 8;
                    *prev_margin_bottom = 6;
                    return Some(hr_box);
                }

                if tag == "svg" {
                    let svg_w = style.width.unwrap_or(28).sclamp(14, 96);
                    let svg_h = style.height.unwrap_or(24).sclamp(14, 96);
                    let svg_key = alloc::format!("__inline_svg_{}", node_id);
                    if !images.iter().any(|(k, _)| k == &svg_key) {
                        if let Some(decoded) = rasterize_svg_tree(tree, node_id, svg_w as u32, svg_h as u32) {
                            images.push((svg_key.clone(), decoded));
                        }
                    }
                    let (iw, ih) = images
                        .iter()
                        .find(|(k, _)| k == &svg_key)
                        .map(|(_, img)| ((img.width as i32).min(available_width), img.height as i32))
                        .unwrap_or((svg_w, svg_h));
                    let bx = align_box_x(&style, x, available_width, iw);
                    let top_y = *cursor_y + collapsed_top;
                    let svg_box = LayoutBox {
                        node_id: node_id,
                        rect: Rect::new(bx, top_y, iw, ih),
                        text: None,
                        style: style.clone(),
                        link_url,
                        widget: WidgetKind::Image {
                            src: svg_key,
                            alt: String::from("ICON"),
                        },
                        children: Vec::new(),
                    };
                    *cursor_y = top_y + ih + style.margin_bottom;
                    *prev_margin_bottom = style.margin_bottom;
                    return Some(svg_box);
                }

                if tag == "img" {
                    let src = tree
                        .attr(node_id, "src")
                        .or_else(|| tree.attr(node_id, "data-src"))
                        .unwrap_or("");
                    if src.is_empty() {
                        return None;
                    }
                    let alt = sanitize_text(tree.attr(node_id, "alt").unwrap_or("IMAGE"));
                    if src.starts_with("data:") && !images.iter().any(|(k, _)| k == src) {
                        if let Some(decoded) = DecodedImage::from_data_uri(src, available_width as u32) {
                            images.push((String::from(src), decoded));
                        }
                    }
                    let decoded_dims = images
                        .iter()
                        .find(|(k, _)| k == src)
                        .map(|(_, img)| (img.width as i32, img.height as i32));

                    let (mut iw, mut ih) = match (style.width, style.height, decoded_dims) {
                        (Some(w), Some(h), _) => (w, h),
                        (Some(w), None, Some((dw, dh))) => (w, (w * dh / dw.max(1)).max(16)),
                        (None, Some(h), Some((dw, dh))) => ((h * dw / dh.max(1)).max(16), h),
                        (None, None, Some((dw, dh))) => (dw, dh),
                        (w_opt, h_opt, None) => (w_opt.unwrap_or(160), h_opt.unwrap_or(56)),
                    };

                    if iw > available_width {
                        ih = (ih * available_width / iw.max(1)).max(16);
                        iw = available_width;
                    }
                    ih = ih.sclamp(16, 220);

                    let bx = align_box_x(&style, x, available_width, iw);
                    let top_y = *cursor_y + collapsed_top;
                    let img_box = LayoutBox {
                        node_id: node_id,
                        rect: Rect::new(bx, top_y, iw, ih),
                        text: None,
                        style: style.clone(),
                        link_url,
                        widget: WidgetKind::Image {
                            src: String::from(src),
                            alt: String::from(alt),
                        },
                        children: Vec::new(),
                    };
                    *cursor_y = top_y + ih + style.margin_bottom;
                    *prev_margin_bottom = style.margin_bottom;
                    return Some(img_box);
                }

                if tag == "input" || tag == "textarea" {
                    let input_type = tree.attr(node_id, "type").unwrap_or("text").to_ascii_lowercase();
                    if matches!(input_type.as_str(), "hidden" | "checkbox" | "radio" | "file" | "image") {
                        return None;
                    }
                    let name = String::from(tree.attr(node_id, "name").unwrap_or("q"));
                    let val = sanitize_text(tree.attr(node_id, "value").unwrap_or(""));
                    let placeholder = sanitize_text(
                        tree.attr(node_id, "placeholder")
                            .or_else(|| tree.attr(node_id, "title"))
                            .or_else(|| tree.attr(node_id, "aria-label"))
                            .unwrap_or("Search or enter text..."),
                    );

                    let top_y = *cursor_y + collapsed_top;

                    if input_type == "submit" || input_type == "button" {
                        let label = if val.is_empty() { String::from("SEARCH") } else { val };
                        let font = page_text_style(&style);
                        let bw = (text_width(&label, &font) + 28).sclamp(88, available_width);
                        let bh = 30;
                        let bx = align_box_x(&style, x, available_width, bw);
                        let btn_box = LayoutBox {
                            node_id: node_id,
                            rect: Rect::new(bx, top_y, bw, bh),
                            text: Some(label.clone()),
                            style: style.clone(),
                            link_url: None,
                            widget: WidgetKind::SubmitButton {
                                form_action: self.active_form_action.clone(),
                                input_name: self.last_form_input_name.clone(),
                                label,
                                target_input_id: self.last_form_input_id,
                            },
                            children: Vec::new(),
                        };
                        *cursor_y = top_y + bh + style.margin_bottom;
                        *prev_margin_bottom = style.margin_bottom;
                        return Some(btn_box);
                    }

                    let input_id = self.next_input_id;
                    self.next_input_id += 1;
                    self.last_form_input_id = Some(input_id);
                    self.last_form_input_name = name.clone();

                    let iw = style
                        .width
                        .unwrap_or((available_width * 3 / 4).min(420))
                        .sclamp(140, available_width);
                    let ih = 34;
                    let bx = align_box_x(&style, x, available_width, iw);
                    let input_box = LayoutBox {
                        node_id: node_id,
                        rect: Rect::new(bx, top_y, iw, ih),
                        text: None,
                        style: style.clone(),
                        link_url: None,
                        widget: WidgetKind::TextInput {
                            input_id,
                            form_action: self.active_form_action.clone(),
                            input_name: name,
                            value: val,
                            placeholder,
                        },
                        children: Vec::new(),
                    };
                    *cursor_y = top_y + ih + style.margin_bottom;
                    *prev_margin_bottom = style.margin_bottom;
                    return Some(input_box);
                }

                if tag == "button" {
                    let label_raw = tree.text_content(node_id);
                    let label = if label_raw.trim().is_empty() {
                        sanitize_text(
                            tree.attr(node_id, "aria-label")
                                .or_else(|| tree.attr(node_id, "title"))
                                .unwrap_or(""),
                        )
                    } else {
                        sanitize_text(label_raw.trim())
                    };
                    if label.is_empty() {
                        return None;
                    }
                    let font = page_text_style(&style);
                    let bw = (text_width(&label, &font) + 28).sclamp(72, available_width);
                    let bh = 30;
                    let bx = align_box_x(&style, x, available_width, bw);
                    let top_y = *cursor_y + collapsed_top;
                    let btn_box = LayoutBox {
                        node_id: node_id,
                        rect: Rect::new(bx, top_y, bw, bh),
                        text: Some(label.clone()),
                        style: style.clone(),
                        link_url,
                        widget: WidgetKind::SubmitButton {
                            form_action: self.active_form_action.clone(),
                            input_name: self.last_form_input_name.clone(),
                            label,
                            target_input_id: self.last_form_input_id,
                        },
                        children: Vec::new(),
                    };
                    *cursor_y = top_y + bh + style.margin_bottom;
                    *prev_margin_bottom = style.margin_bottom;
                    return Some(btn_box);
                }

                let avail_after_margin = (available_width - style.margin_left - style.margin_right).max(40);
                let mut box_w = if let Some(fw) = forced {
                    fw.max(24)
                } else if style.display == DisplayType::InlineBlock {
                    let intrinsic = measure_intrinsic_width(tree, stylesheet, node_id, &style, avail_after_margin);
                    style.width.unwrap_or(intrinsic).sclamp(24, avail_after_margin)
                } else {
                    style
                        .width
                        .map(|w| w.sclamp(32, avail_after_margin))
                        .unwrap_or(avail_after_margin)
                };
                if forced.is_none() {
                    if let Some(mw) = style.max_width {
                        box_w = box_w.min(mw.max(24));
                    }
                    if let Some(mn) = style.min_width {
                        box_w = box_w.max(mn.min(avail_after_margin));
                    }
                }

                let saved_y = *cursor_y;
                let saved_prev = *prev_margin_bottom;
                let [b_top, b_right, b_bottom, b_left] = style.eff_borders();
                let box_x = align_box_x(&style, x + style.margin_left, avail_after_margin, box_w);
                let start_y = *cursor_y + collapsed_top;
                let inner_x = box_x + style.padding_left + b_left;
                let inner_w = (box_w - style.padding_left - style.padding_right - b_left - b_right).max(24);
                let mut inner_y = start_y + style.padding_top + b_top;

                let is_row = (style.display == DisplayType::Flex && style.flex_row) || tag == "tr";
                let child_boxes = if tag != "tr" && style.kind == ContainerKind::Flex && !node.children.is_empty() {
                    self.layout_flex_container(
                        tree,
                        stylesheet,
                        images,
                        &node.children.clone(),
                        inner_x,
                        &mut inner_y,
                        inner_w,
                        &style,
                        link_url.clone(),
                        current_bg,
                        depth + 1,
                    )
                } else if tag != "tr" && style.kind == ContainerKind::Grid && !node.children.is_empty() {
                    self.layout_grid_container(
                        tree,
                        stylesheet,
                        images,
                        &node.children.clone(),
                        inner_x,
                        &mut inner_y,
                        inner_w,
                        &style,
                        link_url.clone(),
                        current_bg,
                        depth + 1,
                    )
                } else if is_row && !node.children.is_empty() {
                    self.layout_flex_row(
                        tree,
                        stylesheet,
                        images,
                        &node.children.clone(),
                        inner_x,
                        &mut inner_y,
                        inner_w,
                        &style,
                        link_url.clone(),
                        current_bg,
                        depth + 1,
                    )
                } else {
                    self.layout_flow_children(
                        tree,
                        stylesheet,
                        images,
                        &node.children.clone(),
                        inner_x,
                        &mut inner_y,
                        inner_w,
                        &style,
                        link_url.clone(),
                        current_bg,
                        depth + 1,
                    )
                };

                if tag == "form" {
                    self.active_form_action = prev_form_action;
                }

                // A box with no children is still real when it has a size, a background or a border
                // (spacers, colour bars, dividers). Dropping it used to collapse many layouts.
                let has_presence = style.height.is_some()
                    || style.min_height.is_some()
                    || style.bg_color.is_some()
                    || style.border_width > 0
                    || style.sides_custom;
                if child_boxes.is_empty() && !has_presence {
                    return None;
                }

                let natural_h = inner_y - start_y + style.padding_bottom + b_bottom;
                let mut block_h = natural_h;
                if let Some(mh) = style.min_height {
                    block_h = block_h.max(mh);
                }
                if let Some(h) = style.height {
                    // `overflow: hidden` makes the height exact; otherwise it is a minimum so content
                    // can never spill into the boxes that follow.
                    block_h = if style.overflow_clip { h } else { block_h.max(h) };
                }
                if let Some(mx) = style.max_height {
                    if style.overflow_clip {
                        block_h = block_h.min(mx.max(style.min_height.unwrap_or(0)));
                    }
                }
                *cursor_y = start_y + block_h + style.margin_bottom;
                *prev_margin_bottom = style.margin_bottom;

                let pos = style.position;
                let (off_l, off_r, off_t, off_b) = (style.left, style.right, style.top, style.bottom);
                let mut result = LayoutBox {
                    node_id: node_id,
                    rect: Rect::new(box_x, start_y, box_w, block_h),
                    text: None,
                    style,
                    link_url,
                    widget: WidgetKind::None,
                    children: child_boxes,
                };
                match pos {
                    Position::Relative => {
                        let dx = off_l.unwrap_or_else(|| -off_r.unwrap_or(0));
                        let dy = off_t.unwrap_or_else(|| -off_b.unwrap_or(0));
                        if dx != 0 || dy != 0 {
                            offset_box_tree(&mut result, dx, dy);
                        }
                    }
                    Position::Absolute | Position::Fixed => {
                        // Out of flow: takes no space. Placement is approximate: `absolute` is measured from
                        // where it would have flowed, `fixed` from the top of the page (no scroll awareness).
                        *cursor_y = saved_y;
                        *prev_margin_bottom = saved_prev;
                        let fixed = pos == Position::Fixed;
                        let w = result.rect.width;
                        let h = result.rect.height;
                        let tx = if let Some(l) = off_l {
                            x + l
                        } else if let Some(r) = off_r {
                            x + available_width - w - r
                        } else {
                            result.rect.x
                        };
                        let ty = if let Some(t) = off_t {
                            if fixed { t } else { saved_y + t }
                        } else if let Some(bt) = off_b {
                            if fixed { VIEWPORT_H.load(Ordering::Relaxed) - h - bt } else { saved_y }
                        } else {
                            result.rect.y
                        };
                        let (dx, dy) = (tx - result.rect.x, ty - result.rect.y);
                        if dx != 0 || dy != 0 {
                            offset_box_tree(&mut result, dx, dy);
                        }
                    }
                    _ => {}
                }
                Some(result)
            }
            NodeType::Text(text) => {
                let mut st = parent_style.clone();
                st.color = ensure_contrast(st.color, parent_bg, inherited_link.is_some());
                let font = page_text_style(&st);
                let lines = wrap_text(&apply_text_transform(&sanitize_text(text), st.text_transform), &font, available_width);
                if lines.is_empty() {
                    return None;
                }
                let start_y = *cursor_y;
                let line_h = line_height_px(&st);
                let mut children = Vec::new();
                for l in lines {
                    let lw = text_width(&l, &font).min(available_width);
                    let lx = align_x(x, available_width, lw, st.text_align);
                    children.push(LayoutBox {
                        node_id: node_id,
                        rect: Rect::new(lx, *cursor_y, lw.max(8), line_h),
                        text: Some(l),
                        style: st.clone(),
                        link_url: inherited_link.clone(),
                        widget: WidgetKind::None,
                        children: Vec::new(),
                    });
                    *cursor_y += line_h;
                }
                let total_h = *cursor_y - start_y;
                *cursor_y += 4;
                *prev_margin_bottom = 4;
                Some(LayoutBox {
                    node_id: node_id,
                    rect: Rect::new(x, start_y, available_width, total_h),
                    text: None,
                    style: st,
                    link_url: inherited_link,
                    widget: WidgetKind::None,
                    children,
                })
            }
        }
    }

    /// Real CSS flexbox: `flex-direction`, `flex-wrap`, `flex-grow/shrink/basis`, `justify-content`,
    /// `align-items` / `align-self`, `gap`, and `margin:auto` on the main axis.
    /// Items are laid out as boxes whose widths are resolved first and then imposed through `forced_width`.
    fn layout_flex_container(
        &mut self,
        tree: &DomTree,
        stylesheet: &Stylesheet,
        images: &mut Vec<(String, DecodedImage)>,
        children: &[usize],
        inner_x: i32,
        inner_y: &mut i32,
        inner_w: i32,
        parent_style: &ComputedStyle,
        inherited_link: Option<String>,
        current_bg: u32,
        depth: usize,
    ) -> Vec<LayoutBox> {
        let mut ids = flex_item_ids(tree, children);
        if parent_style.flex_reverse {
            ids.reverse();
        }
        let mut items: Vec<FlexItem> = Vec::new();
        for &cid in &ids {
            if let Some(it) = make_flex_item(tree, stylesheet, cid, parent_style, inner_w) {
                items.push(it);
            }
        }
        if items.is_empty() {
            return Vec::new();
        }
        let gap_c = parent_style.col_gap;
        let gap_r = parent_style.row_gap;
        let mut out: Vec<LayoutBox> = Vec::new();

        // ---- column direction: stacked items with cross-axis alignment ----
        if !parent_style.flex_row {
            let mut y = *inner_y;
            let mut placed = 0;
            for it in items.iter() {
                let al = it.style.as_ref().and_then(|st| st.align_self).unwrap_or(parent_style.align_items);
                let avail_w = (inner_w - it.ml - it.mr).max(24);
                let explicit_w = it.style.as_ref().and_then(|st| st.width);
                let mut w = match (al, explicit_w) {
                    (_, Some(ew)) => ew.min(avail_w),
                    (AlignItems::Stretch, None) => avail_w,
                    (_, None) => measure_intrinsic_width(tree, stylesheet, it.id, parent_style, avail_w).min(avail_w),
                };
                w = w.min(it.max_w).max(it.min_w.min(avail_w)).max(24);
                self.forced_width = Some(w);
                let mut by = 0i32;
                let mut bm = 0i32;
                let built = self.build_node_box(
                    tree,
                    stylesheet,
                    images,
                    it.id,
                    0,
                    &mut by,
                    w + it.ml + it.mr,
                    parent_style,
                    inherited_link.clone(),
                    current_bg,
                    &mut bm,
                    depth + 1,
                );
                self.forced_width = None;
                let Some(mut b) = built else {
                    continue;
                };
                let free = (inner_w - b.rect.width - it.ml - it.mr).max(0);
                let off = if it.ml_auto && it.mr_auto {
                    free / 2
                } else if it.ml_auto {
                    free
                } else if it.mr_auto {
                    0
                } else {
                    match al {
                        AlignItems::Center => free / 2,
                        AlignItems::End => free,
                        _ => 0,
                    }
                };
                let tx = inner_x + it.ml + off;
                let ty = y + it.mt;
                let (dx, dy) = (tx - b.rect.x, ty - b.rect.y);
                offset_box_tree(&mut b, dx, dy);
                y = ty + b.rect.height + it.mb + gap_r;
                out.push(b);
                placed += 1;
            }
            if placed > 0 {
                *inner_y = y - gap_r;
            }
            return out;
        }

        // ---- row direction ----
        let mut lines: Vec<Vec<usize>> = Vec::new();
        let mut cur: Vec<usize> = Vec::new();
        let mut cur_w = 0i32;
        for (i, it) in items.iter().enumerate() {
            let outer = it.basis + it.ml + it.mr;
            if parent_style.flex_wrap && !cur.is_empty() && cur_w + gap_c + outer > inner_w {
                lines.push(core::mem::take(&mut cur));
                cur_w = 0;
            }
            cur_w += if cur.is_empty() { outer } else { gap_c + outer };
            cur.push(i);
        }
        if !cur.is_empty() {
            lines.push(cur);
        }

        // row-reverse mirrors the main axis: `start` packs to the right
        let justify = if parent_style.flex_reverse {
            match parent_style.justify_content {
                JustifyContent::Start => JustifyContent::End,
                JustifyContent::End => JustifyContent::Start,
                other => other,
            }
        } else {
            parent_style.justify_content
        };

        let n_lines = lines.len();
        for (li, line) in lines.iter().enumerate() {
            let n = line.len() as i32;
            let gaps = gap_c * (n - 1).max(0);
            let used: i32 = line.iter().map(|&i| items[i].basis + items[i].ml + items[i].mr).sum::<i32>() + gaps;
            let free = inner_w - used;

            // resolve flexible lengths
            if free > 0 {
                let total_grow: i32 = line.iter().map(|&i| items[i].grow).sum();
                if total_grow > 0 {
                    let mut given = 0i32;
                    let mut last_grower: Option<usize> = None;
                    for &i in line.iter() {
                        let g = items[i].grow;
                        if g > 0 {
                            let add = (free as i64 * g as i64 / total_grow as i64) as i32;
                            items[i].w = items[i].basis + add;
                            given += add;
                            last_grower = Some(i);
                        } else {
                            items[i].w = items[i].basis;
                        }
                    }
                    if let Some(i) = last_grower {
                        items[i].w += free - given;
                    }
                } else {
                    for &i in line.iter() {
                        items[i].w = items[i].basis;
                    }
                }
            } else if free < 0 {
                let total: i64 = line.iter().map(|&i| items[i].shrink as i64 * items[i].basis as i64).sum();
                for &i in line.iter() {
                    if total > 0 {
                        let red = ((-free) as i64 * items[i].shrink as i64 * items[i].basis as i64 / total) as i32;
                        let floor = items[i].min_w.max(core::cmp::min(24, items[i].basis));
                        items[i].w = (items[i].basis - red).max(floor);
                    } else {
                        items[i].w = items[i].basis;
                    }
                }
            } else {
                for &i in line.iter() {
                    items[i].w = items[i].basis;
                }
            }
            for &i in line.iter() {
                items[i].w = items[i].w.min(items[i].max_w).max(8);
                items[i].ml_eff = items[i].ml;
                items[i].mr_eff = items[i].mr;
            }

            // auto margins swallow leftover main-axis space (`margin-left:auto` pushes an item to the end)
            let used2: i32 = line.iter().map(|&i| items[i].w + items[i].ml + items[i].mr).sum::<i32>() + gaps;
            let free2 = inner_w - used2;
            let autos: i32 = line.iter().map(|&i| items[i].ml_auto as i32 + items[i].mr_auto as i32).sum();
            let auto_used = free2 > 0 && autos > 0;
            if auto_used {
                let share = free2 / autos;
                for &i in line.iter() {
                    if items[i].ml_auto {
                        items[i].ml_eff += share;
                    }
                    if items[i].mr_auto {
                        items[i].mr_eff += share;
                    }
                }
            }

            // build boxes at their resolved widths
            let mut built: Vec<(usize, LayoutBox)> = Vec::new();
            for &i in line.iter() {
                let it = &items[i];
                self.forced_width = Some(it.w);
                let mut by = 0i32;
                let mut bm = 0i32;
                let b = self.build_node_box(
                    tree,
                    stylesheet,
                    images,
                    it.id,
                    0,
                    &mut by,
                    it.w + it.ml + it.mr,
                    parent_style,
                    inherited_link.clone(),
                    current_bg,
                    &mut bm,
                    depth + 1,
                );
                self.forced_width = None;
                if let Some(b) = b {
                    built.push((i, b));
                }
            }
            if built.is_empty() {
                continue;
            }

            // main-axis distribution
            let count = built.len() as i32;
            let used3: i32 = built.iter().map(|(i, b)| b.rect.width + items[*i].ml_eff + items[*i].mr_eff).sum::<i32>()
                + gap_c * (count - 1).max(0);
            let free3 = if auto_used { 0 } else { (inner_w - used3).max(0) };
            let (mut cx, step) = match justify {
                JustifyContent::Start => (inner_x, gap_c),
                JustifyContent::Center => (inner_x + free3 / 2, gap_c),
                JustifyContent::End => (inner_x + free3, gap_c),
                JustifyContent::SpaceBetween => {
                    if count > 1 {
                        (inner_x, gap_c + free3 / (count - 1))
                    } else {
                        (inner_x, gap_c)
                    }
                }
                JustifyContent::SpaceAround => {
                    let sp = free3 / count;
                    (inner_x + sp / 2, gap_c + sp)
                }
                JustifyContent::SpaceEvenly => {
                    let sp = free3 / (count + 1);
                    (inner_x + sp, gap_c + sp)
                }
            };

            // cross-axis alignment inside the line
            let line_h: i32 = built.iter().map(|(i, b)| items[*i].mt + b.rect.height + items[*i].mb).max().unwrap_or(0);
            for (i, mut b) in built.into_iter() {
                let it = &items[i];
                let al = it.style.as_ref().and_then(|st| st.align_self).unwrap_or(parent_style.align_items);
                let outer_h = it.mt + b.rect.height + it.mb;
                let mut off = 0;
                match al {
                    AlignItems::Stretch => {
                        let auto_height = it.style.as_ref().map_or(false, |st| st.height.is_none());
                        if auto_height && outer_h < line_h {
                            b.rect.height = line_h - it.mt - it.mb;
                        }
                    }
                    AlignItems::Center => off = (line_h - outer_h) / 2,
                    AlignItems::End => off = line_h - outer_h,
                    AlignItems::Start => {}
                }
                cx += it.ml_eff;
                let ty = *inner_y + it.mt + off;
                let (dx, dy) = (cx - b.rect.x, ty - b.rect.y);
                offset_box_tree(&mut b, dx, dy);
                cx += b.rect.width + it.mr_eff + step;
                out.push(b);
            }
            *inner_y += line_h;
            if li + 1 < n_lines {
                *inner_y += gap_r;
            }
        }
        out
    }

    /// CSS grid subset: `grid-template-columns` (px, %, fr, auto, minmax, repeat incl. auto-fill/auto-fit),
    /// `gap`, `grid-column: span N`, row-major auto placement, equal-height rows, `align-items`.
    fn layout_grid_container(
        &mut self,
        tree: &DomTree,
        stylesheet: &Stylesheet,
        images: &mut Vec<(String, DecodedImage)>,
        children: &[usize],
        inner_x: i32,
        inner_y: &mut i32,
        inner_w: i32,
        parent_style: &ComputedStyle,
        inherited_link: Option<String>,
        current_bg: u32,
        depth: usize,
    ) -> Vec<LayoutBox> {
        let ids = flex_item_ids(tree, children);
        let mut items: Vec<FlexItem> = Vec::new();
        for &cid in &ids {
            if let Some(it) = make_flex_item(tree, stylesheet, cid, parent_style, inner_w) {
                items.push(it);
            }
        }
        if items.is_empty() {
            return Vec::new();
        }
        let gap_c = parent_style.col_gap;
        let gap_r = parent_style.row_gap;

        let mut tracks: Vec<GridTrack> = parent_style.grid_cols.clone();
        if let Some((tr, fit)) = parent_style.grid_auto {
            let min_px = match tr {
                GridTrack::Px(p) => p,
                GridTrack::MinMax(m, _) => m,
                _ => 120,
            }
            .max(24);
            let mut count = ((inner_w + gap_c) / (min_px + gap_c)).max(1).min(12);
            if fit {
                count = count.min(items.len() as i32).max(1);
            }
            tracks.clear();
            for _ in 0..count {
                tracks.push(tr);
            }
        }
        if tracks.is_empty() {
            tracks.push(GridTrack::Fr(100));
        }
        let ncols = tracks.len();

        // auto placement (row-major, honouring column spans)
        let mut placement: Vec<(usize, usize, usize)> = Vec::new();
        let mut col = 0usize;
        let mut row = 0usize;
        for it in items.iter() {
            let span = (it.style.as_ref().map_or(1, |st| st.grid_span).max(1) as usize).min(ncols);
            if col + span > ncols {
                col = 0;
                row += 1;
            }
            placement.push((col, span, row));
            col += span;
            if col >= ncols {
                col = 0;
                row += 1;
            }
        }
        let nrows = placement.iter().map(|p| p.2).max().unwrap_or(0) + 1;

        // track sizing
        let mut widths = alloc::vec![0i32; ncols];
        let mut frs = alloc::vec![0i32; ncols];
        for (j, tr) in tracks.iter().enumerate() {
            match *tr {
                GridTrack::Px(p) => widths[j] = p,
                GridTrack::Fr(f) => frs[j] = f,
                GridTrack::MinMax(m, f) => {
                    widths[j] = m;
                    frs[j] = f;
                }
                GridTrack::Auto => {
                    let mut w = 0i32;
                    for (k, it) in items.iter().enumerate() {
                        let (c, sp, _) = placement[k];
                        if c == j && sp == 1 {
                            w = w.max(it.basis + it.ml + it.mr);
                        }
                    }
                    widths[j] = w.min(inner_w);
                }
            }
        }
        let avail = (inner_w - gap_c * (ncols as i32 - 1)).max(24 * ncols as i32);
        let used: i32 = widths.iter().sum();
        let remaining = avail - used;
        let sum_f: i32 = frs.iter().sum();
        if remaining > 0 && sum_f > 0 {
            let denom = sum_f.max(100);
            for j in 0..ncols {
                if frs[j] > 0 {
                    widths[j] += (remaining as i64 * frs[j] as i64 / denom as i64) as i32;
                }
            }
        } else if remaining < 0 && used > 0 {
            for j in 0..ncols {
                widths[j] = (widths[j] as i64 * avail as i64 / used as i64) as i32;
            }
        }
        for j in 0..ncols {
            widths[j] = widths[j].max(24);
        }
        let mut xs = alloc::vec![0i32; ncols];
        let mut cx = inner_x;
        for j in 0..ncols {
            xs[j] = cx;
            cx += widths[j] + gap_c;
        }

        // build items at their cell widths
        let mut row_h = alloc::vec![0i32; nrows];
        let mut built: Vec<Option<LayoutBox>> = Vec::new();
        for (k, it) in items.iter().enumerate() {
            let (c, sp, r) = placement[k];
            let cell_w: i32 = widths[c..c + sp].iter().sum::<i32>() + gap_c * (sp as i32 - 1);
            let area = (cell_w - it.ml - it.mr).max(24);
            let fw = match it.style.as_ref().and_then(|st| st.width) {
                Some(w) => w.min(area),
                None => area,
            }
            .max(24);
            self.forced_width = Some(fw);
            let mut by = 0i32;
            let mut bm = 0i32;
            let b = self.build_node_box(
                tree,
                stylesheet,
                images,
                it.id,
                0,
                &mut by,
                fw + it.ml + it.mr,
                parent_style,
                inherited_link.clone(),
                current_bg,
                &mut bm,
                depth + 1,
            );
            self.forced_width = None;
            if let Some(bx) = &b {
                row_h[r] = row_h[r].max(it.mt + bx.rect.height + it.mb);
            }
            built.push(b);
        }

        let mut ys = alloc::vec![0i32; nrows];
        let mut cy = *inner_y;
        for r in 0..nrows {
            ys[r] = cy;
            cy += row_h[r] + gap_r;
        }

        let mut out: Vec<LayoutBox> = Vec::new();
        for (k, b_opt) in built.into_iter().enumerate() {
            let Some(mut b) = b_opt else {
                continue;
            };
            let it = &items[k];
            let (c, _, r) = placement[k];
            let outer_h = it.mt + b.rect.height + it.mb;
            let al = it.style.as_ref().and_then(|st| st.align_self).unwrap_or(parent_style.align_items);
            let mut off = 0;
            match al {
                AlignItems::Stretch => {
                    let auto_height = it.style.as_ref().map_or(false, |st| st.height.is_none());
                    if auto_height && outer_h < row_h[r] {
                        b.rect.height = row_h[r] - it.mt - it.mb;
                    }
                }
                AlignItems::Center => off = (row_h[r] - outer_h) / 2,
                AlignItems::End => off = row_h[r] - outer_h,
                AlignItems::Start => {}
            }
            let (dx, dy) = (xs[c] + it.ml - b.rect.x, ys[r] + it.mt + off - b.rect.y);
            offset_box_tree(&mut b, dx, dy);
            out.push(b);
        }
        if !out.is_empty() {
            *inner_y = ys[nrows - 1] + row_h[nrows - 1];
        }
        out
    }

    fn layout_flex_row(
        &mut self,
        tree: &DomTree,
        stylesheet: &Stylesheet,
        images: &mut Vec<(String, DecodedImage)>,
        children: &[usize],
        inner_x: i32,
        inner_y: &mut i32,
        inner_w: i32,
        parent_style: &ComputedStyle,
        inherited_link: Option<String>,
        current_bg: u32,
        depth: usize,
    ) -> Vec<LayoutBox> {
        let valid_children: Vec<usize> = children
            .iter()
            .copied()
            .filter(|&cid| {
                tree.tag(cid)
                    .map(|t| !matches!(t, "head" | "script" | "style" | "meta" | "link"))
                    .unwrap_or(true)
            })
            .collect();

        if valid_children.is_empty() {
            return Vec::new();
        }

        let mut built_items: Vec<LayoutBox> = Vec::new();
        for &cid in &valid_children {
            let intrinsic_w = measure_intrinsic_width(tree, stylesheet, cid, parent_style, inner_w);
            let item_w = intrinsic_w.sclamp(36, inner_w);
            let mut item_y = 0i32;
            let mut dummy_mb = 0i32;
            if let Some(b) = self.build_node_box(
                tree,
                stylesheet,
                images,
                cid,
                0,
                &mut item_y,
                item_w,
                parent_style,
                inherited_link.clone(),
                current_bg,
                &mut dummy_mb,
                depth + 1,
            ) {
                built_items.push(b);
            }
        }

        if built_items.is_empty() {
            return Vec::new();
        }

        let mut committed = Vec::new();
        let mut row_items: Vec<LayoutBox> = Vec::new();
        let mut row_w = 0i32;
        let mut row_h = 0i32;
        let gap = parent_style.gap.max(8);

        let flush_flex_row = |row_items: &mut Vec<LayoutBox>,
                              committed: &mut Vec<LayoutBox>,
                              inner_y: &mut i32,
                              row_w: &mut i32,
                              row_h: &mut i32| {
            if row_items.is_empty() {
                return;
            }
            let count = row_items.len() as i32;
            let total_items_w: i32 = row_items.iter().map(|b| b.rect.width).sum();
            let total_with_gaps = total_items_w + gap * (count - 1).max(0);
            let free_space = (inner_w - total_with_gaps).max(0);

            let (mut cur_x, step_gap) = match parent_style.justify_content {
                JustifyContent::Center => (inner_x + free_space / 2, gap),
                JustifyContent::End => (inner_x + free_space, gap),
                JustifyContent::SpaceBetween if count > 1 => {
                    (inner_x, ((inner_w - total_items_w).max(0)) / (count - 1))
                }
                JustifyContent::SpaceAround if count > 0 => {
                    let s = ((inner_w - total_items_w).max(0)) / (count + 1);
                    (inner_x + s, s)
                }
                _ => (inner_x, gap),
            };

            for mut item in row_items.drain(..) {
                let item_y = *inner_y + (*row_h - item.rect.height).max(0) / 2;
                let dx = cur_x - item.rect.x;
                let dy = item_y - item.rect.y;
                offset_box_tree(&mut item, dx, dy);
                cur_x += item.rect.width + step_gap;
                committed.push(item);
            }

            *inner_y += *row_h + gap;
            *row_w = 0;
            *row_h = 0;
        };

        for item in built_items {
            let needed = if row_items.is_empty() {
                item.rect.width
            } else {
                gap + item.rect.width
            };
            if !row_items.is_empty() && row_w + needed > inner_w {
                flush_flex_row(&mut row_items, &mut committed, inner_y, &mut row_w, &mut row_h);
            }
            row_w += if row_items.is_empty() { item.rect.width } else { gap + item.rect.width };
            if item.rect.height > row_h {
                row_h = item.rect.height;
            }
            row_items.push(item);
        }
        flush_flex_row(&mut row_items, &mut committed, inner_y, &mut row_w, &mut row_h);

        committed
    }
}

struct FlexItem {
    id: usize,
    style: Option<ComputedStyle>,
    /// Hypothetical main size (border box) before growing / shrinking.
    basis: i32,
    /// Resolved main size (border box).
    w: i32,
    ml: i32,
    mr: i32,
    mt: i32,
    mb: i32,
    ml_auto: bool,
    mr_auto: bool,
    ml_eff: i32,
    mr_eff: i32,
    grow: i32,
    shrink: i32,
    min_w: i32,
    max_w: i32,
}

/// Children that take part in flex/grid layout: elements that render, and non-blank text (anonymous items).
fn flex_item_ids(tree: &DomTree, children: &[usize]) -> Vec<usize> {
    children
        .iter()
        .copied()
        .filter(|&cid| match tree.nodes.get(cid).map(|n| &n.node_type) {
            Some(NodeType::Text(t)) => !t.trim().is_empty(),
            Some(NodeType::Element { tag, .. }) => {
                !matches!(
                    tag.as_str(),
                    "head" | "script" | "style" | "meta" | "link" | "title" | "template" | "noscript" | "br"
                ) && tree.attr(cid, "hidden").is_none()
                    && tree.attr(cid, "aria-hidden") != Some("true")
            }
            None => false,
        })
        .collect()
}

fn make_flex_item(
    tree: &DomTree,
    stylesheet: &Stylesheet,
    cid: usize,
    parent_style: &ComputedStyle,
    inner_w: i32,
) -> Option<FlexItem> {
    let style_opt = match tree.nodes.get(cid).map(|n| &n.node_type) {
        Some(NodeType::Element { .. }) => Some(compute_node_style(tree, stylesheet, cid, Some(parent_style), inner_w)),
        _ => None,
    };
    if let Some(st) = &style_opt {
        if st.display == DisplayType::None {
            return None;
        }
    }
    let explicit = style_opt.as_ref().and_then(|st| st.flex_basis.or(st.width));
    let basis_raw = match explicit {
        Some(b) => b,
        None => measure_intrinsic_width(tree, stylesheet, cid, parent_style, inner_w),
    };
    let (ml, mr, mt, mb, ml_auto, mr_auto, grow, shrink, min_w, max_w) = match &style_opt {
        Some(st) => (
            st.margin_left,
            st.margin_right,
            st.margin_top,
            st.margin_bottom,
            st.margin_left_auto,
            st.margin_right_auto,
            st.flex_grow,
            st.flex_shrink,
            st.min_width.unwrap_or(0),
            st.max_width.unwrap_or(i32::MAX),
        ),
        None => (0, 0, 0, 0, false, false, 0, 100, 0, i32::MAX),
    };
    let basis = basis_raw.max(min_w).min(max_w).max(0);
    Some(FlexItem {
        id: cid,
        style: style_opt,
        basis,
        w: basis,
        ml,
        mr,
        mt,
        mb,
        ml_auto,
        mr_auto,
        ml_eff: ml,
        mr_eff: mr,
        grow,
        shrink,
        min_w,
        max_w,
    })
}

fn offset_box_tree(b: &mut LayoutBox, dx: i32, dy: i32) {
    b.rect.x += dx;
    b.rect.y += dy;
    for child in &mut b.children {
        offset_box_tree(child, dx, dy);
    }
}

fn measure_intrinsic_width(
    tree: &DomTree,
    stylesheet: &Stylesheet,
    node_id: usize,
    parent_style: &ComputedStyle,
    max_width: i32,
) -> i32 {
    let Some(node) = tree.nodes.get(node_id) else {
        return 60;
    };
    match &node.node_type {
        NodeType::Text(t) => {
            let font = page_text_style(parent_style);
            (text_width(&sanitize_text(t.trim()), &font) + 12).sclamp(24, max_width)
        }
        NodeType::Element { tag, .. } => {
            let st = compute_node_style(tree, stylesheet, node_id, Some(parent_style), max_width);
            if let Some(w) = st.width {
                return (w + st.padding_left + st.padding_right + st.border_width * 2).min(max_width);
            }
            if tag == "input" {
                let itype = tree.attr(node_id, "type").unwrap_or("text").to_ascii_lowercase();
                if itype == "submit" || itype == "button" {
                    let val = sanitize_text(tree.attr(node_id, "value").unwrap_or("SEARCH"));
                    let font = page_text_style(&st);
                    return (text_width(&val, &font) + 32).sclamp(88, max_width);
                }
                return (max_width * 3 / 5).sclamp(180, 380);
            }
            if tag == "img" || tag == "svg" {
                return st.width.unwrap_or(96).min(max_width);
            }
            let text = tree.text_content(node_id);
            let font = page_text_style(&st);
            let tw = text_width(&sanitize_text(text.trim()), &font);
            let pad = st.padding_left + st.padding_right + st.border_width * 2 + 16;
            if tw > 0 {
                (tw + pad).sclamp(48, max_width)
            } else {
                (max_width / 3).max(64)
            }
        }
    }
}

fn luminance(color: u32) -> i32 {
    let r = ((color >> 16) & 0xFF) as i32;
    let g = ((color >> 8) & 0xFF) as i32;
    let b = (color & 0xFF) as i32;
    (r * 299 + g * 587 + b * 114) / 1000
}

fn ensure_contrast(fg: u32, bg: u32, is_link: bool) -> u32 {
    let bg_lum = luminance(bg);
    let fg_lum = luminance(fg);
    if (bg_lum - fg_lum).abs() < 65 {
        if bg_lum < 128 {
            if is_link {
                rgb(56, 189, 248)
            } else {
                rgb(241, 245, 249)
            }
        } else if is_link {
            ACCENT_LINE
        } else {
            TEXT_PRIMARY
        }
    } else {
        fg
    }
}

/// Horizontal position of a box inside `avail` px: real `margin: auto` first, `text-align` as the legacy fallback.
fn align_box_x(style: &ComputedStyle, x: i32, avail: i32, w: i32) -> i32 {
    let free = (avail - w).max(0);
    match (style.margin_left_auto, style.margin_right_auto) {
        (true, true) => x + free / 2,
        (true, false) => x + free,
        (false, true) => x,
        (false, false) => align_x(x, avail, w, style.text_align),
    }
}

fn align_x(x: i32, avail_w: i32, item_w: i32, align: TextAlign) -> i32 {
    match align {
        TextAlign::Left => x,
        TextAlign::Center => x + (avail_w - item_w).max(0) / 2,
        TextAlign::Right => x + (avail_w - item_w).max(0),
    }
}

fn default_computed_style() -> ComputedStyle {
    ComputedStyle {
        display: DisplayType::Block,
        font_size: 12,
        color: TEXT_PRIMARY,
        bg_color: None,
        border_color: None,
        border_width: 0,
        border_radius: 0,
        margin_top: 0,
        margin_bottom: 4,
        margin_left: 0,
        margin_right: 0,
        padding_top: 0,
        padding_bottom: 0,
        padding_left: 0,
        padding_right: 0,
        width: None,
        height: None,
        text_align: TextAlign::Left,
        justify_content: JustifyContent::Start,
        flex_row: true,
        gap: 8,
        is_link: false,
        kind: ContainerKind::Flow,
        flex_reverse: false,
        flex_wrap: false,
        align_items: AlignItems::Stretch,
        align_self: None,
        flex_grow: 0,
        flex_shrink: 100,
        flex_basis: None,
        row_gap: 0,
        col_gap: 0,
        grid_cols: Vec::new(),
        grid_auto: None,
        grid_span: 1,
        max_width: None,
        min_width: None,
        min_height: None,
        margin_left_auto: false,
        margin_right_auto: false,
        font_weight: 400,
        italic: false,
        line_height: None,
        text_decoration: 0,
        text_transform: TextTransform::None,
        letter_spacing: 0,
        white_space: WhiteSpace::Normal,
        position: Position::Static,
        top: None,
        right: None,
        bottom: None,
        left: None,
        z_index: 0,
        overflow_clip: false,
        max_height: None,
        border_box: false,
        bw: [3; 4],
        bs: [false; 4],
        bc: [None; 4],
        border_sides: [0; 4],
        border_side_colors: [None; 4],
        sides_custom: false,
    }
}

fn compute_node_style(
    tree: &DomTree,
    stylesheet: &Stylesheet,
    node_id: usize,
    parent_style: Option<&ComputedStyle>,
    container_width: i32,
) -> ComputedStyle {
    let mut style = default_computed_style();
    if let Some(p) = parent_style {
        style.color = p.color;
        style.font_size = p.font_size;
        style.text_align = p.text_align;
        style.is_link = p.is_link;
        style.font_weight = p.font_weight;
        style.italic = p.italic;
        style.line_height = p.line_height;
        style.text_decoration = p.text_decoration;
        style.text_transform = p.text_transform;
        style.letter_spacing = p.letter_spacing;
        style.white_space = p.white_space;
    }

    let tag = tree.tag(node_id).unwrap_or("");
    match tag {
        "head" | "style" | "script" | "noscript" | "meta" | "link" | "title" | "template" | "iframe" | "object" => {
            style.display = DisplayType::None;
            return style;
        }
        "span" | "label" | "font" | "abbr" | "cite" | "dfn" | "var" | "time" | "q" | "bdi" | "bdo" | "data" => {
            style.display = DisplayType::Inline;
        }
        "b" | "strong" => {
            style.display = DisplayType::Inline;
            style.font_weight = 700;
        }
        "i" | "em" => {
            style.display = DisplayType::Inline;
            style.italic = true;
        }
        "u" | "ins" => {
            style.display = DisplayType::Inline;
            style.text_decoration |= DECO_UNDERLINE;
        }
        "s" | "strike" | "del" => {
            style.display = DisplayType::Inline;
            style.text_decoration |= DECO_LINE_THROUGH;
        }
        "small" => {
            style.display = DisplayType::Inline;
            style.font_size = (style.font_size * 5 / 6).max(8);
        }
        "sub" | "sup" => {
            style.display = DisplayType::Inline;
            style.font_size = (style.font_size * 3 / 4).max(8);
        }
        "mark" => {
            style.display = DisplayType::Inline;
            style.bg_color = Some(rgb(255, 235, 59));
            style.color = rgb(17, 24, 39);
        }
        "kbd" | "samp" | "tt" => {
            style.display = DisplayType::Inline;
            style.font_size = (style.font_size * 11 / 12).max(8);
        }
        "a" => {
            style.display = DisplayType::Inline;
            style.color = ACCENT_LINE;
            style.is_link = true;
            style.text_decoration |= DECO_UNDERLINE;
        }
        "img" | "svg" | "button" | "input" | "select" | "textarea" => {
            style.display = DisplayType::InlineBlock;
            style.border_box = true;
        }
        "h1" => {
            style.font_weight = 700;
            style.font_size = 20;
            style.margin_top = 8;
            style.margin_bottom = 8;
        }
        "h2" => {
            style.font_weight = 700;
            style.font_size = 16;
            style.margin_top = 6;
            style.margin_bottom = 6;
        }
        "h3" => {
            style.font_weight = 700;
            style.font_size = 14;
            style.margin_top = 4;
            style.margin_bottom = 4;
        }
        "h4" | "h5" | "h6" => {
            style.font_weight = 700;
            style.font_size = 13;
            style.margin_bottom = 4;
        }
        "p" => {
            style.margin_bottom = 6;
        }
        "nav" | "header" => {
            style.padding_top = 4;
            style.padding_bottom = 4;
            style.margin_bottom = 6;
        }
        "pre" | "code" => {
            if tag == "pre" {
                style.white_space = WhiteSpace::Pre;
            }
            style.font_size = 11;
            style.bg_color = Some(rgb(241, 245, 249));
            style.border_color = Some(PANEL_BORDER);
            style.border_width = 1;
            style.border_radius = 4;
            style.padding_top = 6;
            style.padding_bottom = 6;
            style.padding_left = 8;
            style.padding_right = 8;
            style.margin_bottom = 6;
        }
        "blockquote" => {
            style.bg_color = Some(rgb(248, 250, 252));
            style.border_color = Some(ACCENT_LINE);
            style.border_width = 1;
            style.padding_top = 6;
            style.padding_bottom = 6;
            style.padding_left = 10;
            style.padding_right = 8;
            style.margin_bottom = 6;
        }
        "center" => {
            style.text_align = TextAlign::Center;
            style.justify_content = JustifyContent::Center;
        }
        "ul" | "ol" => {
            style.padding_left = 12;
            style.margin_bottom = 6;
        }
        "li" => {
            style.margin_bottom = 4;
        }
        "table" => {
            style.margin_bottom = 8;
        }
        "tr" => {
            style.display = DisplayType::Flex;
            style.flex_row = true;
            style.gap = 8;
            style.padding_top = 2;
            style.padding_bottom = 2;
        }
        "td" | "th" => {
            if tag == "th" {
                style.font_weight = 700;
            }
            style.padding_left = 4;
            style.padding_right = 4;
            style.padding_top = 2;
            style.padding_bottom = 2;
        }
        _ => {}
    }

    if let Some(id_val) = tree.id_attr(node_id) {
        if id_val == "gb" || id_val == "gba" || id_val == "gbar" || id_val == "guser" {
            style.display = DisplayType::Block;
            style.justify_content = JustifyContent::End;
            style.text_align = TextAlign::Right;
        }
    }

    if let Some(align) = tree.attr(node_id, "align") {
        if align.eq_ignore_ascii_case("center") {
            style.text_align = TextAlign::Center;
            style.justify_content = JustifyContent::Center;
        } else if align.eq_ignore_ascii_case("right") {
            style.text_align = TextAlign::Right;
            style.justify_content = JustifyContent::End;
        } else if align.eq_ignore_ascii_case("left") {
            style.text_align = TextAlign::Left;
            style.justify_content = JustifyContent::Start;
        }
    }
    if let Some(bg) = tree.attr(node_id, "bgcolor").and_then(parse_css_color) {
        style.bg_color = Some(bg);
    }
    if let Some(fg) = tree.attr(node_id, "color").and_then(parse_css_color) {
        style.color = fg;
    }
    if let Some(w) = tree.attr(node_id, "width").and_then(|v| parse_css_px(v, style.font_size, container_width)) {
        if w > 0 {
            style.width = Some(w.min(container_width));
        }
    }
    if let Some(h) = tree.attr(node_id, "height").and_then(|v| parse_css_px(v, style.font_size, container_width)) {
        if h > 0 {
            style.height = Some(h.min(220));
        }
    }

    // Cascade order (CSS Cascade 4): normal sheet < normal inline < !important sheet < !important inline.
    let sheet_decls = stylesheet.matching_declarations(tree, node_id);
    let inline_decls: Vec<Declaration> = match tree.attr(node_id, "style") {
        Some(inline_css) => parse_declarations(inline_css)
            .into_iter()
            .map(|mut d| {
                d.value = stylesheet.resolve_all_vars(&d.value);
                d
            })
            .collect(),
        None => Vec::new(),
    };
    for d in sheet_decls.iter().filter(|d| !d.important) {
        apply_expanded(&mut style, d, container_width);
    }
    for d in inline_decls.iter().filter(|d| !d.important) {
        apply_expanded(&mut style, d, container_width);
    }
    for d in sheet_decls.iter().filter(|d| d.important) {
        apply_expanded(&mut style, d, container_width);
    }
    for d in inline_decls.iter().filter(|d| d.important) {
        apply_expanded(&mut style, d, container_width);
    }

    style
}

/// Expands shorthands (`border`, `font`, `background`, `overflow`, `inset`, `text-decoration`, ...)
/// into longhands first, then applies each one.
fn apply_expanded(style: &mut ComputedStyle, decl: &Declaration, container_width: i32) {
    match cssvalues::expand(decl.property.as_str(), decl.value.as_str()) {
        Some(list) => {
            for (p, v) in list {
                let d = Declaration { property: p, value: v, important: decl.important };
                apply_declaration(style, &d, container_width);
            }
        }
        None => apply_declaration(style, decl, container_width),
    }
}

fn sync_borders(s: &mut ComputedStyle) {
    let mut eff = [0i32; 4];
    for i in 0..4 {
        eff[i] = if s.bs[i] { s.bw[i] } else { 0 };
    }
    s.border_sides = eff;
    s.border_side_colors = s.bc;
    let same_w = eff[0] == eff[1] && eff[1] == eff[2] && eff[2] == eff[3];
    let same_c = s.bc[0] == s.bc[1] && s.bc[1] == s.bc[2] && s.bc[2] == s.bc[3];
    if same_w && same_c {
        s.sides_custom = false;
        s.border_width = eff[0];
        s.border_color = if eff[0] > 0 { s.bc[0].or(Some(s.color)) } else { None };
    } else {
        s.sides_custom = true;
        s.border_width = 0;
    }
}

/// `border-width|style|color` (1-4 values) and `border-<side>-width|style|color`.
fn apply_border(style: &mut ComputedStyle, prop: &str, val: &str, container_width: i32) -> bool {
    let rest = match prop.strip_prefix("border-") {
        Some(r) => r,
        None => return false,
    };
    let (kind, sides): (&str, [Option<&str>; 4]) = if matches!(rest, "width" | "style" | "color") {
        let toks = cssvalues::tokens(val);
        let pick = |i: usize| toks.get(i).copied();
        let t = match toks.len() {
            1 => [pick(0), pick(0), pick(0), pick(0)],
            2 => [pick(0), pick(1), pick(0), pick(1)],
            3 => [pick(0), pick(1), pick(2), pick(1)],
            _ => [pick(0), pick(1), pick(2), pick(3)],
        };
        (rest, t)
    } else {
        let mut it = rest.splitn(2, '-');
        let side = it.next().unwrap_or("");
        let kind = it.next().unwrap_or("");
        let idx = match side {
            "top" => 0,
            "right" => 1,
            "bottom" => 2,
            "left" => 3,
            _ => return false,
        };
        if !matches!(kind, "width" | "style" | "color") {
            return false;
        }
        let mut t = [None; 4];
        t[idx] = Some(val.trim());
        (kind, t)
    };
    for i in 0..4 {
        let tok = match sides[i] {
            Some(t) => t,
            None => continue,
        };
        let l = tok.to_ascii_lowercase();
        match kind {
            "width" => {
                style.bw[i] = match l.as_str() {
                    "thin" => 1,
                    "medium" => 3,
                    "thick" => 5,
                    _ => css_len(tok, style, container_width).unwrap_or(0).sclamp(0, 16),
                };
            }
            "style" => style.bs[i] = l != "none" && l != "hidden" && !l.is_empty(),
            _ => style.bc[i] = parse_css_color(tok),
        }
    }
    sync_borders(style);
    true
}

fn apply_declaration(style: &mut ComputedStyle, decl: &Declaration, container_width: i32) {
    let prop = decl.property.as_str();
    let val = decl.value.trim();

    if prop.starts_with("border-") && apply_border(style, prop, val, container_width) {
        return;
    }

    match prop {
        "display" => {
            let v = val.to_ascii_lowercase();
            if v == "none" {
                style.display = DisplayType::None;
            } else if v.starts_with("inline-flex") {
                style.display = DisplayType::InlineBlock;
                style.kind = ContainerKind::Flex;
            } else if v.starts_with("inline-grid") {
                style.display = DisplayType::InlineBlock;
                style.kind = ContainerKind::Grid;
            } else if v.contains("flex") {
                style.display = DisplayType::Block;
                style.kind = ContainerKind::Flex;
            } else if v.contains("grid") {
                style.display = DisplayType::Block;
                style.kind = ContainerKind::Grid;
            } else if v == "inline-block" {
                style.display = DisplayType::InlineBlock;
                style.kind = ContainerKind::Flow;
            } else if v == "inline" {
                style.display = DisplayType::Inline;
                style.kind = ContainerKind::Flow;
            } else if v == "block" || v == "list-item" || v == "flow-root" || v == "table" {
                style.display = DisplayType::Block;
                style.kind = ContainerKind::Flow;
            }
        }
        "visibility" => {
            if val.eq_ignore_ascii_case("hidden") || val.eq_ignore_ascii_case("collapse") {
                style.display = DisplayType::None;
            }
        }
        "opacity" => {
            if val == "0" || val == "0.0" {
                style.display = DisplayType::None;
            }
        }
        "flex-direction" => {
            let v = val.to_ascii_lowercase();
            style.flex_row = !v.starts_with("column");
            style.flex_reverse = v.ends_with("reverse");
        }
        "flex-wrap" => {
            style.flex_wrap = val.to_ascii_lowercase().starts_with("wrap");
        }
        "flex-flow" => {
            for tok in val.to_ascii_lowercase().split_whitespace() {
                if tok.starts_with("column") {
                    style.flex_row = false;
                    style.flex_reverse = tok.ends_with("reverse");
                } else if tok.starts_with("row") {
                    style.flex_row = true;
                    style.flex_reverse = tok.ends_with("reverse");
                } else if tok.starts_with("wrap") {
                    style.flex_wrap = true;
                } else if tok == "nowrap" {
                    style.flex_wrap = false;
                }
            }
        }
        "justify-content" => {
            let v = val.to_ascii_lowercase();
            style.justify_content = if v.contains("space-between") {
                JustifyContent::SpaceBetween
            } else if v.contains("space-around") {
                JustifyContent::SpaceAround
            } else if v.contains("space-evenly") {
                JustifyContent::SpaceEvenly
            } else if v.contains("center") {
                JustifyContent::Center
            } else if v.contains("end") || v.contains("right") {
                JustifyContent::End
            } else {
                JustifyContent::Start
            };
        }
        "align-items" => {
            style.align_items = parse_align(val);
        }
        "align-self" => {
            style.align_self = if val.trim().eq_ignore_ascii_case("auto") { None } else { Some(parse_align(val)) };
        }
        "flex" => {
            let v = val.to_ascii_lowercase();
            match v.trim() {
                "none" => {
                    style.flex_grow = 0;
                    style.flex_shrink = 0;
                    style.flex_basis = None;
                }
                "auto" => {
                    style.flex_grow = 100;
                    style.flex_shrink = 100;
                    style.flex_basis = None;
                }
                "initial" => {
                    style.flex_grow = 0;
                    style.flex_shrink = 100;
                    style.flex_basis = None;
                }
                _ => {
                    let mut numbers: Vec<i32> = Vec::new();
                    let mut basis: Option<Option<i32>> = None;
                    for tok in v.split_whitespace() {
                        let unitless = !tok.is_empty() && tok.bytes().all(|b| b.is_ascii_digit() || b == b'.');
                        if unitless && numbers.len() < 2 {
                            if let Some(n) = parse_float_i32(tok, 100) {
                                numbers.push(n);
                            }
                        } else if tok == "auto" || tok == "content" {
                            basis = Some(None);
                        } else if let Some(px) = parse_css_px(tok, style.font_size, container_width) {
                            basis = Some(Some(px.max(0)));
                        }
                    }
                    if let Some(g) = numbers.first() {
                        style.flex_grow = *g;
                        style.flex_shrink = numbers.get(1).copied().unwrap_or(100);
                        // `flex: 1` means `1 1 0%`: equal shares regardless of content width.
                        style.flex_basis = match basis {
                            Some(b) => b,
                            None => Some(0),
                        };
                    } else if let Some(b) = basis {
                        style.flex_basis = b;
                    }
                }
            }
        }
        "flex-grow" => {
            if let Some(n) = parse_float_i32(val, 100) {
                style.flex_grow = n.max(0);
            }
        }
        "flex-shrink" => {
            if let Some(n) = parse_float_i32(val, 100) {
                style.flex_shrink = n.max(0);
            }
        }
        "flex-basis" => {
            if val.trim().eq_ignore_ascii_case("auto") || val.trim().eq_ignore_ascii_case("content") {
                style.flex_basis = None;
            } else if let Some(px) = css_len(val, style, container_width) {
                style.flex_basis = Some(px.max(0));
            }
        }
        "grid-template-columns" => {
            let (tracks, auto) = parse_grid_template(val, style.font_size, container_width);
            style.grid_cols = tracks;
            style.grid_auto = auto;
        }
        "grid-column" | "grid-column-end" => {
            if let Some(n) = parse_grid_span(val) {
                style.grid_span = n;
            }
        }
        "max-width" => {
            let v = val.trim();
            if v.eq_ignore_ascii_case("none") {
                style.max_width = None;
            } else if let Some(w) = parse_css_px(v, style.font_size, container_width) {
                if w > 0 {
                    style.max_width = Some(w);
                }
            }
        }
        "min-width" => {
            if let Some(w) = css_len(val, style, container_width) {
                style.min_width = if w > 0 { Some(w) } else { None };
            }
        }
        "min-height" if !val.contains('%') => {
            if let Some(h) = css_len(val, style, container_width) {
                style.min_height = if h > 0 { Some(h.sclamp(0, MAX_HEIGHT)) } else { None };
            }
        }
        "text-align" | "float" => {
            let v = val.to_ascii_lowercase();
            if v.contains("center") {
                style.text_align = TextAlign::Center;
                style.justify_content = JustifyContent::Center;
            } else if v.contains("right") || v.contains("end") {
                style.text_align = TextAlign::Right;
                style.justify_content = JustifyContent::End;
            } else if v.contains("left") || v.contains("start") {
                style.text_align = TextAlign::Left;
            }
        }
        "gap" | "grid-gap" => {
            let mut it = val.split_whitespace();
            let row = it.next().and_then(|t| parse_css_px(t, style.font_size, container_width));
            let col = it.next().and_then(|t| parse_css_px(t, style.font_size, container_width)).or(row);
            if let Some(r) = row {
                style.row_gap = r.sclamp(0, 64);
            }
            if let Some(c) = col {
                style.col_gap = c.sclamp(0, 64);
                style.gap = c.sclamp(0, 32);
            }
        }
        "row-gap" | "grid-row-gap" => {
            if let Some(px) = css_len(val, style, container_width) {
                style.row_gap = px.sclamp(0, 64);
            }
        }
        "column-gap" | "grid-column-gap" => {
            if let Some(px) = css_len(val, style, container_width) {
                style.col_gap = px.sclamp(0, 64);
                style.gap = px.sclamp(0, 32);
            }
        }
        "color" => {
            if let Some(c) = parse_css_color(val) {
                style.color = c;
            }
        }
        "background-color" | "background" => {
            if prop == "background" && (val.contains("gradient") || val.contains("url(")) {
                return;
            }
            if let Some(c) = parse_css_color(val) {
                style.bg_color = Some(c);
            }
        }
        "font-size" => {
            let v = val.trim().to_ascii_lowercase();
            if v == "0" || v == "0px" || v == "0em" || v == "0rem" || v == "0pt" {
                style.font_size = 2;
            } else if let Some(p) = parse_css_font_size(val, style.font_size) {
                style.font_size = p.sclamp(6, 72);
            } else if let Some(px) = cssvalues::eval_length(val, &len_ctx(style, container_width)) {
                style.font_size = (px * 3 / 4).sclamp(6, 72);
            }
        }
        "padding" => {
            let (t, r, b, l) = parse_box_shorthand(val, style.font_size, container_width, MAX_PADDING);
            style.padding_top = t;
            style.padding_right = r;
            style.padding_bottom = b;
            style.padding_left = l;
        }
        "padding-top" => {
            if let Some(p) = css_len(val, style, container_width) {
                style.padding_top = p.sclamp(0, MAX_PADDING);
            }
        }
        "padding-bottom" => {
            if let Some(p) = css_len(val, style, container_width) {
                style.padding_bottom = p.sclamp(0, MAX_PADDING);
            }
        }
        "padding-left" => {
            if let Some(p) = css_len(val, style, container_width) {
                style.padding_left = p.sclamp(0, MAX_PADDING);
            }
        }
        "padding-right" => {
            if let Some(p) = css_len(val, style, container_width) {
                style.padding_right = p.sclamp(0, MAX_PADDING);
            }
        }
        "margin" => {
            let (l_auto, r_auto) = margin_auto_sides(val);
            style.margin_left_auto = l_auto;
            style.margin_right_auto = r_auto;
            let (t, r, b, l) = parse_box_shorthand(val, style.font_size, container_width, MAX_MARGIN);
            style.margin_top = t;
            style.margin_right = r;
            style.margin_bottom = b;
            style.margin_left = l;
        }
        "margin-top" => {
            if let Some(p) = css_len(val, style, container_width) {
                style.margin_top = p.sclamp(0, MAX_MARGIN);
            }
        }
        "margin-bottom" => {
            if let Some(p) = css_len(val, style, container_width) {
                style.margin_bottom = p.sclamp(0, MAX_MARGIN);
            }
        }
        "margin-left" => {
            if val.trim().eq_ignore_ascii_case("auto") {
                style.margin_left_auto = true;
                style.margin_left = 0;
            } else if let Some(p) = css_len(val, style, container_width) {
                style.margin_left_auto = false;
                style.margin_left = p.sclamp(0, MAX_MARGIN);
            }
        }
        "margin-right" => {
            if val.trim().eq_ignore_ascii_case("auto") {
                style.margin_right_auto = true;
                style.margin_right = 0;
            } else if let Some(p) = css_len(val, style, container_width) {
                style.margin_right_auto = false;
                style.margin_right = p.sclamp(0, MAX_MARGIN);
            }
        }
        "border" => {
            if val.contains("none") || val == "0" {
                style.border_width = 0;
            } else {
                style.border_width = 1;
                style.border_color = parse_css_color(val).or(Some(PANEL_BORDER));
            }
        }
        "border-color" => {
            if let Some(c) = parse_css_color(val) {
                style.border_color = Some(c);
                if style.border_width == 0 {
                    style.border_width = 1;
                }
            }
        }
        "border-width" => {
            if let Some(w) = css_len(val, style, container_width) {
                style.border_width = w.sclamp(0, 4);
            }
        }
        "border-radius" => {
            if let Some(r) = css_len(val, style, container_width) {
                style.border_radius = r.sclamp(0, 64);
            }
        }
        "width" => {
            if let Some(w) = css_len(val, style, container_width) {
                if w > 0 {
                    style.width = Some(w.min(container_width));
                }
            }
        }
        "height" if !val.contains('%') => {
            if let Some(h) = css_len(val, style, container_width) {
                if h > 0 {
                    style.height = Some(h.min(MAX_HEIGHT));
                }
            }
        }
        "font-weight" => {
            if let Some(w) = cssvalues::font_weight(val, style.font_weight) {
                style.font_weight = w;
            }
        }
        "font-style" => {
            let v = val.to_ascii_lowercase();
            style.italic = v.starts_with("italic") || v.starts_with("oblique");
        }
        "line-height" => {
            let ctx = len_ctx(style, container_width);
            style.line_height = cssvalues::line_height_x100(val, &ctx);
        }
        "text-decoration-line" => {
            let v = val.to_ascii_lowercase();
            let mut bits = 0u8;
            if v.contains("underline") {
                bits |= DECO_UNDERLINE;
            }
            if v.contains("line-through") {
                bits |= DECO_LINE_THROUGH;
            }
            if v.contains("overline") {
                bits |= DECO_OVERLINE;
            }
            style.text_decoration = bits;
        }
        "text-transform" => {
            style.text_transform = match val.to_ascii_lowercase().as_str() {
                "uppercase" => TextTransform::Upper,
                "lowercase" => TextTransform::Lower,
                "capitalize" => TextTransform::Capitalize,
                _ => TextTransform::None,
            };
        }
        "letter-spacing" => {
            let px = css_len(val, style, container_width).unwrap_or(0);
            style.letter_spacing = (px * 3 / 4).sclamp(-3, 24);
        }
        "white-space" => {
            style.white_space = match val.to_ascii_lowercase().as_str() {
                "nowrap" => WhiteSpace::NoWrap,
                "pre" => WhiteSpace::Pre,
                "pre-wrap" | "break-spaces" => WhiteSpace::PreWrap,
                "pre-line" => WhiteSpace::PreLine,
                _ => WhiteSpace::Normal,
            };
        }
        "position" => {
            style.position = match val.to_ascii_lowercase().as_str() {
                "relative" => Position::Relative,
                "absolute" => Position::Absolute,
                "fixed" => Position::Fixed,
                "sticky" | "-webkit-sticky" => Position::Sticky,
                _ => Position::Static,
            };
        }
        "top" | "right" | "bottom" | "left" => {
            let v = if val.eq_ignore_ascii_case("auto") { None } else { css_len(val, style, container_width) };
            match prop {
                "top" => style.top = v,
                "right" => style.right = v,
                "bottom" => style.bottom = v,
                _ => style.left = v,
            }
        }
        "z-index" => {
            let ctx = len_ctx(style, container_width);
            style.z_index = cssvalues::eval_number(val, &ctx).map(|n| (n / 1000) as i32).unwrap_or(0);
        }
        "overflow-y" => {
            let v = val.to_ascii_lowercase();
            style.overflow_clip = v == "hidden" || v == "clip";
        }
        "max-height" if !val.contains('%') => {
            style.max_height = match css_len(val, style, container_width) {
                Some(h) if h > 0 && !val.eq_ignore_ascii_case("none") => Some(h.min(MAX_HEIGHT)),
                _ => None,
            };
        }
        "box-sizing" => {
            style.border_box = val.to_ascii_lowercase().starts_with("border");
        }
        "background-image" => {
            // Gradients are approximated by the mean of their first and last stop; url() images are not drawn yet.
            let cols = cssvalues::gradient_colors(val);
            if cols.len() >= 2 {
                if let (Some(a), Some(b)) = (parse_css_color(&cols[0]), parse_css_color(&cols[cols.len() - 1])) {
                    let mix = |sa: u32, sb: u32| ((sa + sb) / 2) & 0xFF;
                    let r = mix((a >> 16) & 0xFF, (b >> 16) & 0xFF);
                    let g = mix((a >> 8) & 0xFF, (b >> 8) & 0xFF);
                    let bl = mix(a & 0xFF, b & 0xFF);
                    style.bg_color = Some(rgb(r as u8, g as u8, bl as u8));
                }
            } else if cols.len() == 1 {
                if let Some(a) = parse_css_color(&cols[0]) {
                    style.bg_color = Some(a);
                }
            }
        }
        _ => {
            let _ = TEXT_MUTED;
        }
    }
}

fn parse_box_shorthand(val: &str, base_font: i32, container_width: i32, max: i32) -> (i32, i32, i32, i32) {
    let ctx = LenCtx {
        font_px: base_font * 4 / 3,
        root_px: 16,
        percent_base: container_width,
        viewport_w: VIEWPORT_W.load(Ordering::Relaxed),
        viewport_h: VIEWPORT_H.load(Ordering::Relaxed),
    };
    let vals: Vec<i32> = cssvalues::tokens(val)
        .into_iter()
        .map(|part| {
            if part.eq_ignore_ascii_case("auto") {
                return 0;
            }
            cssvalues::eval_length(part, &ctx)
                .or_else(|| parse_css_px(part, base_font, container_width))
                .unwrap_or(0)
                .sclamp(0, max)
        })
        .collect();
    match vals.len() {
        1 => (vals[0], vals[0], vals[0], vals[0]),
        2 => (vals[0], vals[1], vals[0], vals[1]),
        3 => (vals[0], vals[1], vals[2], vals[1]),
        4.. => (vals[0], vals[1], vals[2], vals[3]),
        _ => (0, 0, 0, 0),
    }
}

/// Which horizontal sides of a `margin` shorthand are `auto` (left, right).
fn margin_auto_sides(val: &str) -> (bool, bool) {
    let toks: Vec<bool> = val.split_whitespace().map(|t| t.eq_ignore_ascii_case("auto")).collect();
    match toks.len() {
        1 => (toks[0], toks[0]),
        2 | 3 => (toks[1], toks[1]),
        4.. => (toks[3], toks[1]),
        _ => (false, false),
    }
}

fn parse_align(val: &str) -> AlignItems {
    let v = val.to_ascii_lowercase();
    if v.contains("center") {
        AlignItems::Center
    } else if v.contains("end") {
        AlignItems::End
    } else if v.contains("start") {
        AlignItems::Start
    } else {
        AlignItems::Stretch
    }
}

/// Splits on whitespace outside parentheses: `repeat(3, 1fr) 200px` -> two tokens.
fn split_css_tokens(val: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for ch in val.chars() {
        if ch == '(' {
            depth += 1;
            cur.push(ch);
        } else if ch == ')' {
            depth -= 1;
            cur.push(ch);
        } else if ch.is_whitespace() && depth <= 0 {
            if !cur.is_empty() {
                out.push(core::mem::take(&mut cur));
            }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_track(tok: &str, font: i32, cw: i32) -> Option<GridTrack> {
    let t = tok.trim().to_ascii_lowercase();
    if t == "auto" || t == "min-content" || t == "max-content" || t.starts_with("fit-content") {
        return Some(GridTrack::Auto);
    }
    if let Some(n) = t.strip_suffix("fr") {
        return parse_float_i32(n, 100).map(|f| GridTrack::Fr(f.max(0)));
    }
    if let Some(inner) = t.strip_prefix("minmax(").and_then(|r| r.strip_suffix(')')) {
        let (a, b) = inner.split_once(',')?;
        let (a, b) = (a.trim(), b.trim());
        let min_px = if a == "auto" || a == "min-content" || a == "max-content" {
            0
        } else {
            parse_css_px(a, font, cw).unwrap_or(0).max(0)
        };
        if let Some(n) = b.strip_suffix("fr") {
            return Some(GridTrack::MinMax(min_px, parse_float_i32(n, 100).unwrap_or(100).max(0)));
        }
        if b == "auto" || b == "max-content" || b == "min-content" {
            return Some(GridTrack::MinMax(min_px, 100));
        }
        let max_px = parse_css_px(b, font, cw).unwrap_or(min_px);
        return Some(GridTrack::Px(max_px.max(min_px)));
    }
    parse_css_px(&t, font, cw).map(GridTrack::Px)
}

/// `grid-template-columns`: px / % / fr / auto / minmax() / repeat(N, ...) / repeat(auto-fill|auto-fit, ...).
fn parse_grid_template(val: &str, font: i32, cw: i32) -> (Vec<GridTrack>, Option<(GridTrack, bool)>) {
    let mut tracks: Vec<GridTrack> = Vec::new();
    let mut auto: Option<(GridTrack, bool)> = None;
    for tok in split_css_tokens(val) {
        let t = tok.to_ascii_lowercase();
        if t.starts_with('[') || t == "none" {
            continue;
        }
        if let Some(inner) = t.strip_prefix("repeat(").and_then(|r| r.strip_suffix(')')) {
            let (cnt, body) = match inner.split_once(',') {
                Some((a, b)) => (a.trim(), b.trim()),
                None => continue,
            };
            let sub: Vec<GridTrack> = split_css_tokens(body)
                .iter()
                .filter(|x| !x.starts_with('['))
                .filter_map(|x| parse_track(x, font, cw))
                .collect();
            if sub.is_empty() {
                continue;
            }
            if cnt == "auto-fill" || cnt == "auto-fit" {
                auto = Some((sub[0], cnt == "auto-fit"));
            } else if let Ok(n) = cnt.parse::<usize>() {
                for _ in 0..n.min(12) {
                    for tr in sub.iter() {
                        if tracks.len() < 24 {
                            tracks.push(*tr);
                        }
                    }
                }
            }
            continue;
        }
        if let Some(tr) = parse_track(&t, font, cw) {
            if tracks.len() < 24 {
                tracks.push(tr);
            }
        }
    }
    (tracks, auto)
}

/// `grid-column: span 2`, `1 / 3`, `1 / -1` -> number of columns spanned.
fn parse_grid_span(val: &str) -> Option<i32> {
    let v = val.to_ascii_lowercase();
    if let Some(p) = v.find("span") {
        let rest = v[p + 4..].trim();
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        return digits.parse::<i32>().ok().map(|n| n.max(1).min(24));
    }
    if let Some((a, b)) = v.split_once('/') {
        let a = a.trim().parse::<i32>().ok()?;
        let b = b.trim().parse::<i32>().ok()?;
        if b < 0 {
            return Some(24);
        }
        return Some((b - a).max(1).min(24));
    }
    None
}

/// Off-screen / 1px "screen-reader only" boxes are invisible to sighted users; drawing them in flow
/// leaked skip links and accessibility labels into the page.
fn is_visually_hidden(st: &ComputedStyle) -> bool {
    st.is_out_of_flow()
        && (st.left.map_or(false, |v| v <= -100)
            || st.top.map_or(false, |v| v <= -100)
            || (st.width == Some(1) && st.height == Some(1)))
}

/// Breaks a single over-long word into chunks that fit `max_width` (CSS `overflow-wrap: anywhere`).
fn break_long_word(word: &str, style: &Style, max_width: i32) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    for ch in word.chars() {
        let mut probe = cur.clone();
        probe.push(ch);
        if !cur.is_empty() && text_width(&probe, style) > max_width {
            parts.push(core::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

fn wrap_text(text: &str, style: &Style, max_width: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if max_width > 24 && text_width(word, style) > max_width {
            if !current_line.is_empty() {
                lines.push(core::mem::take(&mut current_line));
            }
            let mut chunks = break_long_word(word, style, max_width);
            if let Some(last) = chunks.pop() {
                lines.extend(chunks);
                current_line = last;
            }
            continue;
        }
        let test_line = if current_line.is_empty() {
            String::from(word)
        } else {
            alloc::format!("{} {}", current_line, word)
        };
        if text_width(&test_line, style) <= max_width {
            current_line = test_line;
        } else {
            if !current_line.is_empty() {
                lines.push(current_line);
            }
            current_line = String::from(word);
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    lines
}
