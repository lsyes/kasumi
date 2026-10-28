//! A snapshot of the retained tree for accessibility consumers.
//!
//! The skeleton does not bind to a platform accessibility API. It produces a
//! flat, ordered snapshot that can be handed to AccessKit or a debug overlay in
//! a later milestone.

use kasumi_core::{Id, Rect};

use crate::arena::NodeId;
use crate::node::NodeKind;
use crate::tree::Tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Window,
    Group,
    Label,
    Button,
    TextField,
    CheckBox,
    RadioButton,
    Slider,
    ProgressIndicator,
    ComboBox,
    Image,
}

#[derive(Debug, Clone)]
pub struct A11yNode {
    pub id: Id,
    pub role: Role,
    pub label: Option<String>,
    pub bounds: Rect,
    pub focusable: bool,
    pub focused: bool,
    pub depth: usize,
}

#[derive(Debug, Clone, Default)]
pub struct A11ySnapshot {
    pub nodes: Vec<A11yNode>,
}

impl Tree {
    pub fn a11y_snapshot(&self, focused: Option<Id>) -> A11ySnapshot {
        let mut nodes = Vec::new();
        self.walk_a11y(self.root(), 0, focused, &mut nodes);
        A11ySnapshot { nodes }
    }

    fn walk_a11y(
        &self,
        node_id: NodeId,
        depth: usize,
        focused: Option<Id>,
        out: &mut Vec<A11yNode>,
    ) {
        if depth > 512 {
            return;
        }
        let Some(node) = self.get(node_id) else {
            return;
        };
        if node.style.hidden || node.a11y_hidden || node.visual.opacity() <= 0.001 {
            return;
        }
        let role = match node.kind {
            NodeKind::Root => Role::Window,
            NodeKind::Container | NodeKind::Stack | NodeKind::Overlay | NodeKind::Spacer => {
                Role::Group
            }
            NodeKind::Text => Role::Label,
            NodeKind::Button => Role::Button,
            NodeKind::TextInput => Role::TextField,
            NodeKind::Checkbox => Role::CheckBox,
            NodeKind::Radio => Role::RadioButton,
            NodeKind::Slider => Role::Slider,
            NodeKind::ProgressBar => Role::ProgressIndicator,
            NodeKind::ComboBox => Role::ComboBox,
            NodeKind::Image => Role::Image,
        };
        let label = node
            .accessibility_label
            .clone()
            .or_else(|| match node.kind {
                NodeKind::Text | NodeKind::TextInput => node.text_str().map(str::to_owned),
                NodeKind::Button
                | NodeKind::Checkbox
                | NodeKind::Radio
                | NodeKind::ComboBox
                | NodeKind::Image => self.label_of(node_id),
                _ => None,
            });
        out.push(A11yNode {
            id: node.id,
            role,
            label,
            bounds: node.rect,
            focusable: node.sense.is_focusable(),
            focused: focused == Some(node.id),
            depth,
        });
        for child in &node.children {
            self.walk_a11y(*child, depth + 1, focused, out);
        }
    }
}
