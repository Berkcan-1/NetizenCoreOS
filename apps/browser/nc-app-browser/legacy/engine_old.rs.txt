extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use nc_font::{text_width, Style};
use nc_gfx::rgb;
use nc_ui::text_style;

#[derive(Clone, PartialEq, Eq)]
pub enum TagType {
    Heading1,
    Heading2,
    Paragraph,
    Anchor { href: String },
    Button,
    InputBox { placeholder: String },
    GoogleLogo,
    ListItem,
    HorizontalRule,
    Unknown,
}

#[derive(Clone)]
pub struct DomNode {
    pub tag: TagType,
    pub text: String,
}

#[derive(Clone)]
pub struct LayoutLine {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub style: Style,
    pub color: u32,
    pub href: Option<String>,
    pub is_button: bool,
    pub is_input: bool,
    pub is_google_logo: bool,
    pub is_hr: bool,
}

pub struct HtmlEngine;

impl HtmlEngine {
    pub fn parse(html: &str) -> Vec<DomNode> {
        let mut nodes = Vec::new();
        let mut cursor = 0;
        let bytes = html.as_bytes();
        let len = bytes.len();

        while cursor < len {
            if bytes[cursor] == b'<' {
                let tag_start = cursor + 1;
                let mut tag_end = tag_start;
                while tag_end < len && bytes[tag_end] != b'>' {
                    tag_end += 1;
                }
                if tag_end >= len {
                    break;
                }

                let tag_content = core::str::from_utf8(&bytes[tag_start..tag_end]).unwrap_or("");
                cursor = tag_end + 1;

                if tag_content.starts_with('/') {
                    continue;
                }

                if tag_content.eq_ignore_ascii_case("hr") || tag_content.starts_with("hr ") {
                    nodes.push(DomNode {
                        tag: TagType::HorizontalRule,
                        text: String::new(),
                    });
                    continue;
                }

                if tag_content.eq_ignore_ascii_case("google-logo") {
                    nodes.push(DomNode {
                        tag: TagType::GoogleLogo,
                        text: String::from("Google"),
                    });
                    continue;
                }

                let mut tag_type = TagType::Unknown;
                let lower_tag = tag_content.split_whitespace().next().unwrap_or("");
                if lower_tag.eq_ignore_ascii_case("h1") {
                    tag_type = TagType::Heading1;
                } else if lower_tag.eq_ignore_ascii_case("h2") || lower_tag.eq_ignore_ascii_case("h3") {
                    tag_type = TagType::Heading2;
                } else if lower_tag.eq_ignore_ascii_case("p") {
                    tag_type = TagType::Paragraph;
                } else if lower_tag.eq_ignore_ascii_case("li") {
                    tag_type = TagType::ListItem;
                } else if lower_tag.eq_ignore_ascii_case("button") {
                    tag_type = TagType::Button;
                } else if lower_tag.eq_ignore_ascii_case("input") {
                    let mut placeholder = String::from("Google'da arayin veya bir URL yazin");
                    if let Some(pos) = tag_content.find("placeholder=\"") {
                        let rest = &tag_content[pos + 13..];
                        if let Some(end_q) = rest.find('"') {
                            placeholder = String::from(&rest[..end_q]);
                        }
                    }
                    tag_type = TagType::InputBox { placeholder };
                } else if lower_tag.to_ascii_lowercase().starts_with("a") {
                    let mut href = String::new();
                    if let Some(pos) = tag_content.find("href=\"") {
                        let rest = &tag_content[pos + 6..];
                        if let Some(end_q) = rest.find('"') {
                            href = String::from(&rest[..end_q]);
                        }
                    } else if let Some(pos) = tag_content.find("href='") {
                        let rest = &tag_content[pos + 6..];
                        if let Some(end_q) = rest.find('\'') {
                            href = String::from(&rest[..end_q]);
                        }
                    }
                    tag_type = TagType::Anchor { href };
                }

                let mut text_buf = String::new();
                while cursor < len && bytes[cursor] != b'<' {
                    text_buf.push(bytes[cursor] as char);
                    cursor += 1;
                }

                let clean_text = text_buf.trim();
                nodes.push(DomNode {
                    tag: tag_type,
                    text: String::from(clean_text),
                });
            } else {
                let mut text_buf = String::new();
                while cursor < len && bytes[cursor] != b'<' {
                    text_buf.push(bytes[cursor] as char);
                    cursor += 1;
                }
                let clean = text_buf.trim();
                if !clean.is_empty() {
                    nodes.push(DomNode {
                        tag: TagType::Paragraph,
                        text: String::from(clean),
                    });
                }
            }
        }
        nodes
    }
}

pub struct LayoutEngine;

impl LayoutEngine {
    pub fn build(
        nodes: &[DomNode],
        start_x: i32,
        start_y: i32,
        max_width: i32,
        unit: i32,
        accent: u32,
    ) -> (Vec<LayoutLine>, i32) {
        let mut lines = Vec::new();
        let mut curr_y = start_y;

        for node in nodes {
            match &node.tag {
                TagType::GoogleLogo => {
                    curr_y += unit;
                    let style = text_style(unit * 24 / 10, 20);
                    let w = unit * 16;
                    let h = unit * 3;
                    lines.push(LayoutLine {
                        text: String::from("Google"),
                        x: start_x + (max_width - w) / 2,
                        y: curr_y,
                        width: w,
                        height: h,
                        style,
                        color: rgb(66, 133, 244),
                        href: None,
                        is_button: false,
                        is_input: false,
                        is_google_logo: true,
                        is_hr: false,
                    });
                    curr_y += h + unit;
                }
                TagType::InputBox { placeholder } => {
                    curr_y += unit / 2;
                    let style = text_style(unit * 65 / 100, 30);
                    let w = (max_width * 4 / 5).min(unit * 32);
                    let h = unit * 5 / 2;
                    lines.push(LayoutLine {
                        text: placeholder.clone(),
                        x: start_x + (max_width - w) / 2,
                        y: curr_y,
                        width: w,
                        height: h,
                        style,
                        color: nc_ui::TEXT_MUTED,
                        href: None,
                        is_button: false,
                        is_input: true,
                        is_google_logo: false,
                        is_hr: false,
                    });
                    curr_y += h + unit;
                }
                TagType::Button => {
                    let style = text_style(unit * 55 / 100, 30);
                    let w = text_width(&node.text, &style) + unit * 3;
                    let h = unit * 2;
                    lines.push(LayoutLine {
                        text: node.text.clone(),
                        x: start_x + (max_width - w) / 2,
                        y: curr_y,
                        width: w,
                        height: h,
                        style,
                        color: nc_ui::TEXT_PRIMARY,
                        href: None,
                        is_button: true,
                        is_input: false,
                        is_google_logo: false,
                        is_hr: false,
                    });
                    curr_y += h + unit / 2;
                }
                TagType::HorizontalRule => {
                    curr_y += unit;
                    lines.push(LayoutLine {
                        text: String::new(),
                        x: start_x,
                        y: curr_y,
                        width: max_width,
                        height: 1,
                        style: text_style(unit * 5 / 10, 30),
                        color: nc_ui::PANEL_BORDER,
                        href: None,
                        is_button: false,
                        is_input: false,
                        is_google_logo: false,
                        is_hr: true,
                    });
                    curr_y += unit;
                }
                TagType::Heading1 => {
                    curr_y += unit;
                    let style = text_style(unit * 12 / 10, 20);
                    let line_h = unit * 2;
                    Self::wrap_text(&node.text, start_x, &mut curr_y, max_width, &style, line_h, nc_ui::TEXT_PRIMARY, None, &mut lines);
                    curr_y += unit / 2;
                }
                TagType::Heading2 => {
                    curr_y += unit * 3 / 4;
                    let style = text_style(unit * 9 / 10, 25);
                    let line_h = unit * 8 / 5;
                    Self::wrap_text(&node.text, start_x, &mut curr_y, max_width, &style, line_h, nc_ui::TEXT_PRIMARY, None, &mut lines);
                    curr_y += unit / 4;
                }
                TagType::Paragraph => {
                    curr_y += unit / 3;
                    let style = text_style(unit * 6 / 10, 30);
                    let line_h = unit * 5 / 4;
                    Self::wrap_text(&node.text, start_x, &mut curr_y, max_width, &style, line_h, nc_ui::TEXT_MUTED, None, &mut lines);
                    curr_y += unit / 2;
                }
                TagType::ListItem => {
                    curr_y += unit / 4;
                    let style = text_style(unit * 6 / 10, 30);
                    let line_h = unit * 5 / 4;
                    let mut bullet_text = String::from("•  ");
                    bullet_text.push_str(&node.text);
                    Self::wrap_text(&bullet_text, start_x + unit, &mut curr_y, max_width - unit, &style, line_h, nc_ui::TEXT_PRIMARY, None, &mut lines);
                }
                TagType::Anchor { href } => {
                    curr_y += unit / 4;
                    let style = text_style(unit * 6 / 10, 30);
                    let line_h = unit * 5 / 4;
                    Self::wrap_text(&node.text, start_x, &mut curr_y, max_width, &style, line_h, accent, Some(href.clone()), &mut lines);
                }
                TagType::Unknown => {}
            }
        }

        let total_content_height = curr_y - start_y;
        (lines, total_content_height)
    }

    fn wrap_text(
        text: &str,
        start_x: i32,
        curr_y: &mut i32,
        max_w: i32,
        style: &Style,
        line_height: i32,
        color: u32,
        href: Option<String>,
        lines: &mut Vec<LayoutLine>,
    ) {
        let mut curr_line = String::new();

        for word in text.split_whitespace() {
            let mut test_line = curr_line.clone();
            if !test_line.is_empty() {
                test_line.push(' ');
            }
            test_line.push_str(word);

            let w = text_width(&test_line, style);
            if w > max_w && !curr_line.is_empty() {
                let actual_w = text_width(&curr_line, style);
                lines.push(LayoutLine {
                    text: curr_line,
                    x: start_x,
                    y: *curr_y,
                    width: actual_w,
                    height: line_height,
                    style: *style,
                    color,
                    href: href.clone(),
                    is_button: false,
                    is_input: false,
                    is_google_logo: false,
                    is_hr: false,
                });
                *curr_y += line_height;
                curr_line = String::from(word);
            } else {
                curr_line = test_line;
            }
        }

        if !curr_line.is_empty() {
            let actual_w = text_width(&curr_line, style);
            lines.push(LayoutLine {
                text: curr_line,
                x: start_x,
                y: *curr_y,
                width: actual_w,
                height: line_height,
                style: *style,
                color,
                href,
                is_button: false,
                is_input: false,
                is_google_logo: false,
                is_hr: false,
            });
            *curr_y += line_height;
        }
    }
}
