//! Accessibility Semantic Tree, High-Contrast Tokens & Screen-Reader Queue (Feature 46).
//!
//! Exposes semantic UI roles (button, label, text field, switch, list item),
//! dynamic text scaling, high-contrast palette adaptation, keyboard focus
//! traversal, and a bounded screen-reader event queue.
//! Note: speech output remains explicitly marked unavailable until hardware
//! audio output is operational.

use crate::{Palette, UiRect};

pub const MAX_A11Y_NODES: usize = 48;
pub const MAX_A11Y_EVENTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticRole {
    Button = 1,
    Label = 2,
    TextField = 3,
    Switch = 4,
    ListItem = 5,
    Navigation = 6,
}

#[derive(Clone, Copy, Debug)]
pub struct A11yPrefs {
    /// Text scale percentage (100..=200).
    pub text_scale_pct: u16,
    pub high_contrast: bool,
    pub reduce_motion: bool,
    pub screen_reader_enabled: bool,
}

impl A11yPrefs {
    pub const DEFAULT: Self = Self {
        text_scale_pct: 100,
        high_contrast: false,
        reduce_motion: false,
        screen_reader_enabled: false,
    };

    pub fn scale_font(&self, base_px: i32) -> i32 {
        let pct = self.text_scale_pct.clamp(100, 200) as i32;
        (base_px * pct / 100).max(base_px)
    }

    pub fn effective_palette(&self, dark: bool) -> Palette {
        if !self.high_contrast {
            return Palette::for_dark(dark);
        }
        if dark {
            Palette {
                bg: 0x0000_0000,
                card: 0x0012_1212,
                ink: 0x00FF_FFFF,
                muted: 0x00E2_E8F0,
                subtle: 0x00CB_D5E1,
                track: 0x0033_4155,
                divider: 0x0064_748B,
                row_sel: 0x001E_293B,
            }
        } else {
            Palette {
                bg: 0x00FF_FFFF,
                card: 0x00FF_FFFF,
                ink: 0x0000_0000,
                muted: 0x001E_293B,
                subtle: 0x0033_4155,
                track: 0x0094_A3B8,
                divider: 0x0047_5569,
                row_sel: 0x00E2_E8F0,
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct SemanticNode {
    pub id: u32,
    pub role: SemanticRole,
    pub bounds: UiRect,
    pub focusable: bool,
    pub checked: bool,
    pub label_len: u8,
    pub label: [u8; 48],
}

impl SemanticNode {
    pub const EMPTY: Self = Self {
        id: 0,
        role: SemanticRole::Label,
        bounds: UiRect::new(0, 0, 0, 0),
        focusable: false,
        checked: false,
        label_len: 0,
        label: [0; 48],
    };

    pub fn label_str(&self) -> &str {
        core::str::from_utf8(&self.label[..self.label_len as usize]).unwrap_or("")
    }
}

pub struct SemanticTree {
    nodes: [SemanticNode; MAX_A11Y_NODES],
    count: usize,
    focused_index: Option<usize>,
    event_queue: [u32; MAX_A11Y_EVENTS],
    event_count: usize,
}

impl SemanticTree {
    pub const fn new() -> Self {
        Self {
            nodes: [SemanticNode::EMPTY; MAX_A11Y_NODES],
            count: 0,
            focused_index: None,
            event_queue: [0; MAX_A11Y_EVENTS],
            event_count: 0,
        }
    }

    pub fn clear(&mut self) {
        self.count = 0;
        self.focused_index = None;
    }

    pub fn push(
        &mut self,
        id: u32,
        role: SemanticRole,
        bounds: UiRect,
        focusable: bool,
        checked: bool,
        label: &str,
    ) -> bool {
        if self.count >= MAX_A11Y_NODES {
            return false;
        }
        let mut node = SemanticNode::EMPTY;
        node.id = id;
        node.role = role;
        node.bounds = bounds;
        node.focusable = focusable;
        node.checked = checked;
        let b = label.as_bytes();
        let n = b.len().min(48);
        node.label_len = n as u8;
        node.label[..n].copy_from_slice(&b[..n]);
        self.nodes[self.count] = node;
        self.count += 1;
        true
    }

    pub fn focus_next(&mut self) -> Option<&SemanticNode> {
        if self.count == 0 {
            return None;
        }
        let start = self.focused_index.map_or(0, |i| (i + 1) % self.count);
        for step in 0..self.count {
            let idx = (start + step) % self.count;
            if self.nodes[idx].focusable {
                self.focused_index = Some(idx);
                if self.event_count < MAX_A11Y_EVENTS {
                    self.event_queue[self.event_count] = self.nodes[idx].id;
                    self.event_count += 1;
                }
                return Some(&self.nodes[idx]);
            }
        }
        None
    }

    pub fn pending_events(&self) -> &[u32] {
        &self.event_queue[..self.event_count]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a11y_focus_traversal_text_scaling_and_high_contrast() {
        let mut prefs = A11yPrefs::DEFAULT;
        prefs.text_scale_pct = 150;
        prefs.high_contrast = true;
        assert_eq!(prefs.scale_font(12), 18);
        assert_eq!(prefs.effective_palette(true).ink, 0x00FF_FFFF);
        assert_eq!(prefs.effective_palette(false).ink, 0x0000_0000);

        let mut tree = SemanticTree::new();
        tree.push(1, SemanticRole::Label, UiRect::new(0, 0, 100, 20), false, false, "Title");
        tree.push(2, SemanticRole::Switch, UiRect::new(0, 24, 100, 32), true, true, "Wi-Fi");
        tree.push(3, SemanticRole::Button, UiRect::new(0, 60, 100, 36), true, false, "Save");

        assert_eq!(tree.focus_next().unwrap().id, 2);
        assert_eq!(tree.focus_next().unwrap().id, 3);
        assert_eq!(tree.focus_next().unwrap().id, 2);
        assert_eq!(tree.pending_events(), &[2, 3, 2]);
    }
}
