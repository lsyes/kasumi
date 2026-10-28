//! Per-frame context shared by the runtime and the `Ui` facade.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use kasumi_core::{Id, InputState, Key, Modifiers, Point};
use kasumi_paint::TextEngine;
use kasumi_state::StateStore;
use kasumi_tree::{ScrollbarGeometry, Tree};

use crate::response::{Response, ResponseFlags};
use crate::runtime::ClipboardAction;
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub(crate) struct KeyPress {
    pub key: Key,
    pub modifiers: Modifiers,
    pub repeat: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextMenuState {
    pub owner: Id,
    pub position: Point,
    pub root: Option<Id>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScrollDrag {
    pub geometry: ScrollbarGeometry,
}

pub(crate) struct Cx {
    pub tree: Tree,
    pub state: Rc<StateStore>,
    pub texts: Arc<TextEngine>,
    pub theme: Theme,
    pub input: InputState,
    pub delta_time: f32,
    pub hovered: Option<Id>,
    pub active: Option<Id>,
    pub focused: Option<Id>,
    pub clicked: Option<Id>,
    pub released: Option<Id>,
    pub context_clicked: Option<Id>,
    pub pointer_down: bool,
    pub dragging: bool,
    pub repaint: bool,
    pub animations_running: bool,
    pub state_paths: HashMap<Id, Rc<str>>,
    #[cfg(debug_assertions)]
    pub seen_ids: HashMap<Id, &'static str>,
    pub frame: u64,
    pub keys: Vec<KeyPress>,
    pub context_menu: Option<ContextMenuState>,
    pub clipboard_action: Option<ClipboardAction>,
    pub clipboard_target: Option<Id>,
    pub scroll_drag: Option<ScrollDrag>,
    pub context_menu_dismissed: bool,
    pub last_click: Option<(Id, f64, u8)>,
    pub(crate) transitions: crate::transition::TransitionStore,
}

impl Cx {
    pub fn response(&self, id: Id) -> Response {
        let rect = self.tree.visual_rect_of(id);
        let mut flags = ResponseFlags::default();
        if self.hovered == Some(id) {
            flags.insert(ResponseFlags::HOVERED);
        }
        if self.active == Some(id) && self.pointer_down {
            flags.insert(ResponseFlags::PRESSED);
        }
        if self.clicked == Some(id) {
            flags.insert(ResponseFlags::CLICKED);
        }
        if self.released == Some(id) {
            flags.insert(ResponseFlags::RELEASED);
        }
        if self.focused == Some(id) {
            flags.insert(ResponseFlags::FOCUSED);
        }
        if self.dragging && self.active == Some(id) {
            flags.insert(ResponseFlags::DRAGGED);
        }
        if self.context_clicked == Some(id) {
            flags.insert(ResponseFlags::CONTEXT_CLICKED);
        }
        if self
            .tree
            .node_by_id(id)
            .map(|node| node.changed)
            .unwrap_or(false)
        {
            flags.insert(ResponseFlags::CHANGED);
        }
        let scroll = self
            .tree
            .node_by_id(id)
            .map(|node| node.scroll.offset)
            .unwrap_or(Point::ZERO);
        Response::new(id, rect, flags).with_scroll_offset(scroll)
    }

    pub fn viewport(&self) -> kasumi_core::Size {
        self.tree.viewport()
    }
}
