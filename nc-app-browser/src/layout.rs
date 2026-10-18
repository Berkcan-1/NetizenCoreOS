extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use nc_font::{text_width, Style};
use nc_gfx::{rgb, Rect};
use nc_ui::{text_style, ACCENT_LINE, PANEL_BORDER, TEXT_MUTED, TEXT_PRIMARY, WHITE};
use crate::css::{parse_css_color, parse_css_px, parse_declarations, Declaration, SafeClamp, Stylesheet};
use crate::dom::{DomTree, NodeType};
use crate::image::{rasterize_svg_tree, DecodedImage};

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
        if depth > 22 {
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
        let mut line_h = (parent_style.font_size * 15 / 10).max(16);

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
            *line_h = (parent_style.font_size * 15 / 10).max(16);
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
        if depth > 24 {
            return;
        }
        for &nid in node_ids {
            match &tree.nodes[nid].node_type {
                NodeType::Text(raw_text) => {
                    let mut st = parent_style.clone();
                    st.color = ensure_contrast(st.color, effective_bg, inherited_link.is_some());
                    st.is_link = inherited_link.is_some();
                    st.margin_right = (st.font_size / 2).max(5);
                    let font = text_style(st.font_size, 30);
                    let lh = (st.font_size * 15 / 10).max(15);
                    for word in raw_text.split_whitespace() {
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
                    if st.display == DisplayType::None {
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
        if depth > 22 {
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

                if style.display == DisplayType::None {
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
                    let bx = align_x(x, available_width, iw, style.text_align);
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
                    let alt = tree.attr(node_id, "alt").unwrap_or("IMAGE");
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

                    let bx = align_x(x, available_width, iw, style.text_align);
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
                    let val = String::from(tree.attr(node_id, "value").unwrap_or(""));
                    let placeholder = String::from(
                        tree.attr(node_id, "placeholder")
                            .or_else(|| tree.attr(node_id, "title"))
                            .or_else(|| tree.attr(node_id, "aria-label"))
                            .unwrap_or("Search or enter text..."),
                    );

                    let top_y = *cursor_y + collapsed_top;

                    if input_type == "submit" || input_type == "button" {
                        let label = if val.is_empty() { String::from("SEARCH") } else { val };
                        let font = text_style(style.font_size, 30);
                        let bw = (text_width(&label, &font) + 28).sclamp(88, available_width);
                        let bh = 30;
                        let bx = align_x(x, available_width, bw, style.text_align);
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
                    let bx = align_x(x, available_width, iw, style.text_align);
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
                        String::from(
                            tree.attr(node_id, "aria-label")
                                .or_else(|| tree.attr(node_id, "title"))
                                .unwrap_or(""),
                        )
                    } else {
                        String::from(label_raw.trim())
                    };
                    if label.is_empty() {
                        return None;
                    }
                    let font = text_style(style.font_size, 30);
                    let bw = (text_width(&label, &font) + 28).sclamp(72, available_width);
                    let bh = 30;
                    let bx = align_x(x, available_width, bw, style.text_align);
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
                let box_w = if style.display == DisplayType::InlineBlock {
                    let intrinsic = measure_intrinsic_width(tree, stylesheet, node_id, &style, avail_after_margin);
                    style.width.unwrap_or(intrinsic).sclamp(24, avail_after_margin)
                } else {
                    style
                        .width
                        .map(|w| w.sclamp(32, avail_after_margin))
                        .unwrap_or(avail_after_margin)
                };

                let box_x = align_x(x + style.margin_left, avail_after_margin, box_w, style.text_align);
                let start_y = *cursor_y + collapsed_top;
                let inner_x = box_x + style.padding_left + style.border_width;
                let inner_w = (box_w - style.padding_left - style.padding_right - style.border_width * 2).max(24);
                let mut inner_y = start_y + style.padding_top + style.border_width;

                let is_row = (style.display == DisplayType::Flex && style.flex_row) || tag == "tr";
                let child_boxes = if is_row && !node.children.is_empty() {
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

                if child_boxes.is_empty() {
                    return None;
                }

                let natural_h = inner_y - start_y + style.padding_bottom + style.border_width;
                let block_h = natural_h;
                *cursor_y = start_y + block_h + style.margin_bottom;
                *prev_margin_bottom = style.margin_bottom;

                Some(LayoutBox {
                    node_id: node_id,
                    rect: Rect::new(box_x, start_y, box_w, block_h),
                    text: None,
                    style,
                    link_url,
                    widget: WidgetKind::None,
                    children: child_boxes,
                })
            }
            NodeType::Text(text) => {
                let mut st = parent_style.clone();
                st.color = ensure_contrast(st.color, parent_bg, inherited_link.is_some());
                let font = text_style(st.font_size, 30);
                let lines = wrap_text(text, &font, available_width);
                if lines.is_empty() {
                    return None;
                }
                let start_y = *cursor_y;
                let line_h = (st.font_size * 15 / 10).max(16);
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
            let font = text_style(parent_style.font_size, 30);
            (text_width(t.trim(), &font) + 12).sclamp(24, max_width)
        }
        NodeType::Element { tag, .. } => {
            let st = compute_node_style(tree, stylesheet, node_id, Some(parent_style), max_width);
            if let Some(w) = st.width {
                return (w + st.padding_left + st.padding_right + st.border_width * 2).min(max_width);
            }
            if tag == "input" {
                let itype = tree.attr(node_id, "type").unwrap_or("text").to_ascii_lowercase();
                if itype == "submit" || itype == "button" {
                    let val = tree.attr(node_id, "value").unwrap_or("SEARCH");
                    let font = text_style(st.font_size, 30);
                    return (text_width(val, &font) + 32).sclamp(88, max_width);
                }
                return (max_width * 3 / 5).sclamp(180, 380);
            }
            if tag == "img" || tag == "svg" {
                return st.width.unwrap_or(96).min(max_width);
            }
            let text = tree.text_content(node_id);
            let font = text_style(st.font_size, 30);
            let tw = text_width(text.trim(), &font);
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
    }

    let tag = tree.tag(node_id).unwrap_or("");
    match tag {
        "head" | "style" | "script" | "noscript" | "meta" | "link" | "title" | "template" | "iframe" | "object" => {
            style.display = DisplayType::None;
            return style;
        }
        "span" | "b" | "strong" | "i" | "em" | "u" | "small" | "label" | "font" | "sub" | "sup" => {
            style.display = DisplayType::Inline;
        }
        "a" => {
            style.display = DisplayType::Inline;
            style.color = ACCENT_LINE;
            style.is_link = true;
        }
        "img" | "svg" | "button" | "input" | "select" | "textarea" => {
            style.display = DisplayType::InlineBlock;
        }
        "h1" => {
            style.font_size = 20;
            style.margin_top = 8;
            style.margin_bottom = 8;
        }
        "h2" => {
            style.font_size = 16;
            style.margin_top = 6;
            style.margin_bottom = 6;
        }
        "h3" => {
            style.font_size = 14;
            style.margin_top = 4;
            style.margin_bottom = 4;
        }
        "h4" | "h5" | "h6" => {
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

    for decl in stylesheet.matching_declarations(tree, node_id) {
        apply_declaration(&mut style, &decl, container_width);
    }

    if let Some(inline_css) = tree.attr(node_id, "style") {
        for mut decl in parse_declarations(inline_css) {
            decl.value = stylesheet.resolve_all_vars(&decl.value);
            apply_declaration(&mut style, &decl, container_width);
        }
    }

    style
}

fn apply_declaration(style: &mut ComputedStyle, decl: &Declaration, container_width: i32) {
    let prop = decl.property.as_str();
    let val = decl.value.trim();

    match prop {
        "display" => {
            let v = val.to_ascii_lowercase();
            if v == "none" {
                style.display = DisplayType::None;
            } else if v.starts_with("inline-flex") || v.starts_with("inline-grid") {
                style.display = DisplayType::InlineBlock;
            } else if v.contains("flex") || v.contains("grid") {
                style.display = DisplayType::Block;
            } else if v == "inline-block" {
                style.display = DisplayType::InlineBlock;
            } else if v == "inline" {
                style.display = DisplayType::Inline;
            } else if v == "block" {
                style.display = DisplayType::Block;
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
            style.flex_row = !val.to_ascii_lowercase().contains("column");
        }
        "justify-content" => {
            let v = val.to_ascii_lowercase();
            if v.contains("space-between") {
                style.justify_content = JustifyContent::SpaceBetween;
            } else if v.contains("space-around") || v.contains("space-evenly") {
                style.justify_content = JustifyContent::SpaceAround;
            } else if v.contains("center") {
                style.justify_content = JustifyContent::Center;
                style.text_align = TextAlign::Center;
            } else if v.contains("end") || v.contains("right") {
                style.justify_content = JustifyContent::End;
                style.text_align = TextAlign::Right;
            } else {
                style.justify_content = JustifyContent::Start;
            }
        }
        "align-items" => {
            let v = val.to_ascii_lowercase();
            if v.contains("center") && !style.flex_row {
                style.text_align = TextAlign::Center;
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
        "gap" | "column-gap" | "grid-gap" => {
            if let Some(px) = parse_css_px(val, style.font_size, container_width) {
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
            let v = val.to_ascii_lowercase();
            let px = match v.as_str() {
                "0" | "0px" => {
                    style.display = DisplayType::None;
                    None
                }
                "xx-small" | "x-small" => Some(10),
                "small" => Some(11),
                "medium" => Some(13),
                "large" => Some(16),
                "x-large" => Some(20),
                "xx-large" => Some(24),
                _ => parse_css_px(val, style.font_size, container_width),
            };
            if let Some(p) = px {
                style.font_size = p.sclamp(10, 28);
            }
        }
        "padding" => {
            let (t, r, b, l) = parse_box_shorthand(val, style.font_size, container_width);
            style.padding_top = t;
            style.padding_right = r;
            style.padding_bottom = b;
            style.padding_left = l;
        }
        "padding-top" => {
            if let Some(p) = parse_css_px(val, style.font_size, container_width) {
                style.padding_top = p.sclamp(0, 32);
            }
        }
        "padding-bottom" => {
            if let Some(p) = parse_css_px(val, style.font_size, container_width) {
                style.padding_bottom = p.sclamp(0, 32);
            }
        }
        "padding-left" => {
            if let Some(p) = parse_css_px(val, style.font_size, container_width) {
                style.padding_left = p.sclamp(0, 32);
            }
        }
        "padding-right" => {
            if let Some(p) = parse_css_px(val, style.font_size, container_width) {
                style.padding_right = p.sclamp(0, 32);
            }
        }
        "margin" => {
            if val.contains("auto") {
                style.text_align = TextAlign::Center;
            }
            let (t, r, b, l) = parse_box_shorthand(val, style.font_size, container_width);
            style.margin_top = t;
            style.margin_right = r;
            style.margin_bottom = b;
            style.margin_left = l;
        }
        "margin-top" => {
            if let Some(p) = parse_css_px(val, style.font_size, container_width) {
                style.margin_top = p.sclamp(0, 32);
            }
        }
        "margin-bottom" => {
            if let Some(p) = parse_css_px(val, style.font_size, container_width) {
                style.margin_bottom = p.sclamp(0, 32);
            }
        }
        "margin-left" => {
            if val.trim() == "auto" {
                style.text_align = TextAlign::Right;
            } else if let Some(p) = parse_css_px(val, style.font_size, container_width) {
                style.margin_left = p.sclamp(0, 32);
            }
        }
        "margin-right" => {
            if let Some(p) = parse_css_px(val, style.font_size, container_width) {
                style.margin_right = p.sclamp(0, 32);
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
            if let Some(w) = parse_css_px(val, style.font_size, container_width) {
                style.border_width = w.sclamp(0, 4);
            }
        }
        "border-radius" => {
            if let Some(r) = parse_css_px(val, style.font_size, container_width) {
                style.border_radius = r.sclamp(0, 16);
            }
        }
        "width" => {
            if let Some(w) = parse_css_px(val, style.font_size, container_width) {
                if w > 0 {
                    style.width = Some(w.min(container_width));
                }
            }
        }
        "height" => {
            if let Some(h) = parse_css_px(val, style.font_size, container_width) {
                if h > 0 {
                    style.height = Some(h.min(200));
                }
            }
        }
        _ => {
            let _ = TEXT_MUTED;
        }
    }
}

fn parse_box_shorthand(val: &str, base_font: i32, container_width: i32) -> (i32, i32, i32, i32) {
    let vals: Vec<i32> = val
        .split_whitespace()
        .map(|part| parse_css_px(part, base_font, container_width).unwrap_or(0).sclamp(0, 32))
        .collect();
    match vals.len() {
        1 => (vals[0], vals[0], vals[0], vals[0]),
        2 => (vals[0], vals[1], vals[0], vals[1]),
        3 => (vals[0], vals[1], vals[2], vals[1]),
        4.. => (vals[0], vals[1], vals[2], vals[3]),
        _ => (0, 0, 0, 0),
    }
}

fn wrap_text(text: &str, style: &Style, max_width: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
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
