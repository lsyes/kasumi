//! Frame orchestration: input, reconcile, layout, paint.
//!
//! The backend never calls into the tree directly. It feeds an `InputState`,
//! asks the runtime for a frame, and renders the resulting `DrawList`.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kasumi_core::{
    Align, CursorIcon, FlexDirection, Id, InputEvent, InputState, Key, Length, Modifiers,
    MouseButton, Point, Size, Style, Vec2,
};
use kasumi_layout::{LayoutEngine, LayoutStats};
use kasumi_paint::{InteractionState, PaintStats, TextEngine, cursor_for, paint};
use kasumi_state::StateStore;
use kasumi_tree::{A11ySnapshot, NodeKind, TextEdit, TextInputState, TextMotion, Tree};

use crate::cx::{Cx, KeyPress, ScrollDrag};
use crate::theme::Theme;
use crate::ui::Ui;

/// Implemented by the application. Called once per rendered frame.
pub trait App {
    fn view(&mut self, ui: &mut Ui);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FrameStats {
    pub frame: u64,
    pub nodes: usize,
    pub created_nodes: usize,
    pub removed_nodes: usize,
    pub state_entries: usize,
    pub layout: LayoutStats,
    pub paint: PaintStats,
    pub cpu_time: Duration,
}

#[derive(Debug, Clone)]
pub struct FrameOutput {
    pub cursor: CursorIcon,
    /// `None` means the scene is static and the backend may sleep. A zero
    /// duration asks for another frame immediately.
    pub next_frame_in: Option<Duration>,
    /// Clipboard update requested during this frame, executed by the backend.
    pub clipboard: Option<ClipboardAction>,
    /// Text-input focus and caret rectangle used to drive platform IME.
    pub ime: ImeState,
    pub stats: FrameStats,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ImeState {
    pub focused: bool,
    pub cursor: Option<kasumi_core::Rect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardAction {
    SetText(String),
    /// Read platform text and deliver it as `InputEvent::Paste` on the next
    /// frame. This keeps the runtime and widget layers platform-independent.
    ReadText,
}

pub struct Runtime {
    cx: Cx,
    layout: LayoutEngine,
    texts: Arc<TextEngine>,
    draw: kasumi_core::DrawList,
    stats: FrameStats,
    last_ime_area: Option<kasumi_core::Rect>,
}

impl Runtime {
    pub fn new(texts: Arc<TextEngine>, viewport: Size, theme: Theme) -> Self {
        let state = Rc::new(StateStore::new());
        let tree = Tree::new();
        Self {
            cx: Cx {
                tree,
                state,
                texts: Arc::clone(&texts),
                theme,
                input: InputState {
                    viewport,
                    ..InputState::default()
                },
                delta_time: 0.0,
                hovered: None,
                active: None,
                focused: None,
                clicked: None,
                released: None,
                context_clicked: None,
                pointer_down: false,
                dragging: false,
                repaint: false,
                animations_running: false,
                state_paths: HashMap::new(),
                #[cfg(debug_assertions)]
                seen_ids: HashMap::new(),
                frame: 0,
                keys: Vec::new(),
                context_menu: None,
                clipboard_action: None,
                clipboard_target: None,
                scroll_drag: None,
                context_menu_dismissed: false,
                last_click: None,
                transitions: crate::transition::TransitionStore::default(),
            },
            layout: LayoutEngine::new(),
            texts,
            draw: kasumi_core::DrawList::default(),
            stats: FrameStats::default(),
            last_ime_area: None,
        }
    }

    pub fn frame(&mut self, app: &mut dyn App, input: InputState) -> FrameOutput {
        let started = Instant::now();
        let previous_time = self.cx.input.time;
        self.cx.input = input;
        self.cx.delta_time = (self.cx.input.time - previous_time).clamp(0.0, 0.05) as f32;
        self.cx.frame += 1;
        self.cx.clicked = None;
        self.cx.released = None;
        self.cx.context_clicked = None;
        self.cx.context_menu_dismissed = false;
        self.cx.keys.clear();
        self.cx.clipboard_action = None;
        self.cx.repaint = false;
        self.cx.animations_running = false;
        #[cfg(debug_assertions)]
        self.cx.seen_ids.clear();
        self.cx.state.begin_frame(self.cx.frame);

        let viewport = sanitize_size(self.cx.input.viewport);
        self.cx.input.viewport = viewport;
        let root = self.cx.tree.begin_frame(viewport);

        self.process_input();
        self.cx.input.events.clear();
        let now = self.cx.input.time;
        if self.cx.tree.tick_scroll(now) {
            self.cx.repaint = true;
            self.cx.animations_running = true;
        }
        if self.cx.tree.tick_animations(now) {
            self.cx.repaint = true;
            self.cx.animations_running = true;
        }
        self.recompute_hover();
        if self.cx.tree.tick_hover(
            self.cx.hovered,
            true,
            (now - previous_time).clamp(0.0, 0.05) as f32,
        ) {
            self.cx.repaint = true;
            self.cx.animations_running = true;
        }

        {
            let mut ui = Ui::new(&mut self.cx, root, Id::ROOT, Self::root_style());
            app.view(&mut ui);
            ui.commit_style();
        }

        if self.cx.tree.tick_animations(now) {
            self.cx.repaint = true;
            self.cx.animations_running = true;
        }

        let removed = self.cx.tree.end_frame().to_vec();
        self.layout.remove_nodes(&removed);
        let removed_count = removed.len();
        self.cleanup_after_reconcile();
        let layout_stats = self
            .layout
            .compute(&mut self.cx.tree, viewport, self.texts.as_ref());
        if layout_stats.computed {
            self.recompute_hover();
            if self.cx.tree.tick_scroll(now) {
                self.cx.repaint = true;
                self.cx.animations_running = true;
            }
        }

        self.draw.clear();
        let paint_stats = paint(
            &self.cx.tree,
            Arc::clone(&self.texts),
            InteractionState {
                hovered: self.cx.hovered,
                active: self.cx.active,
                focused: self.cx.focused,
                pointer_down: self.cx.pointer_down,
                pointer: self.cx.input.pointer,
            },
            viewport,
            &mut self.draw,
        );

        let cursor = cursor_for(&self.cx.tree, self.cx.hovered);

        // A state write during `view` may have happened after the widgets that
        // read it were declared. Request one follow-up frame so every part of
        // the UI observes the new value.
        let state_repaint = self.cx.state.take_repaint();
        let next_frame_in = if self.cx.repaint || state_repaint || self.cx.animations_running {
            Some(Duration::ZERO)
        } else {
            None
        };
        self.stats = FrameStats {
            frame: self.cx.frame,
            nodes: self.cx.tree.node_count(),
            created_nodes: self.cx.tree.created_last_frame(),
            removed_nodes: removed_count,
            state_entries: self.cx.state.entry_count(),
            layout: layout_stats,
            paint: paint_stats,
            cpu_time: started.elapsed(),
        };

        self.cx.state.end_frame();

        FrameOutput {
            cursor,
            next_frame_in,
            clipboard: self.cx.clipboard_action.take(),
            ime: self.ime_state(),
            stats: self.stats,
        }
    }

    pub fn draw_list(&self) -> &kasumi_core::DrawList {
        &self.draw
    }

    pub fn texts(&self) -> Arc<TextEngine> {
        Arc::clone(&self.texts)
    }

    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    pub fn viewport(&self) -> Size {
        self.cx.input.viewport
    }

    pub fn scale_factor(&self) -> f32 {
        self.cx.input.scale_factor
    }

    pub fn theme(&self) -> &Theme {
        &self.cx.theme
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.cx.theme = theme;
        self.cx.repaint = true;
    }

    pub fn request_repaint(&mut self) {
        self.cx.repaint = true;
    }

    pub fn frame_clipboard_target(&self) -> Option<Id> {
        self.cx.clipboard_target
    }

    /// Returns the IME caret only when `rect` differs materially from the last
    /// rectangle reported to the backend. Keeping this decision in the runtime
    /// prevents candidate windows from being repositioned every frame.
    pub fn take_ime_area_update(&mut self, rect: kasumi_core::Rect) -> Option<kasumi_core::Rect> {
        if rect.is_empty() {
            self.last_ime_area = None;
            return None;
        }
        if let Some(previous) = self.last_ime_area
            && (previous.left() - rect.left()).abs() < 1.0
            && (previous.top() - rect.top()).abs() < 1.0
            && (previous.width() - rect.width()).abs() < 1.0
            && (previous.height() - rect.height()).abs() < 1.0
        {
            return None;
        }
        self.last_ime_area = Some(rect);
        Some(rect)
    }

    fn ime_state(&self) -> ImeState {
        let Some(focused) = self.cx.focused else {
            return ImeState::default();
        };
        let Some(node) = self.cx.tree.node_by_id(focused) else {
            return ImeState::default();
        };
        if !Self::accepts_text_input(node) {
            return ImeState::default();
        }
        let rect = self.cx.tree.visual_rect_of(focused);
        let content = rect.inset(node.style.padding);
        let layout = self.texts.layout(
            node.text_str().unwrap_or_default(),
            &node.style.text,
            node.text_shape_width.get(),
        );
        let input = node.text_input.as_ref();
        let caret = input.map(|input| input.paint_cursor).unwrap_or_default();
        let scroll_x = input.map(|input| input.scroll_x).unwrap_or(0.0);
        let caret_x = (content.left() + layout.caret_x(caret) - scroll_x)
            .clamp(content.left(), content.right().max(content.left()));
        let vertical_offset = ((content.height() - layout.size.height) * 0.5).max(0.0);
        ImeState {
            focused: true,
            cursor: Some(kasumi_core::Rect::from_xywh(
                caret_x,
                content.top() + vertical_offset,
                1.5,
                layout.line_height.max(1.0),
            )),
        }
    }

    fn accepts_text_input(node: &kasumi_tree::Node) -> bool {
        node.kind == NodeKind::TextInput && node.enabled && !node.style.hidden
    }

    /// True while a pointer drag is owned by a widget. Backends use this to
    /// forward every pointer move even when the hover target does not change.
    pub fn pointer_captured(&self) -> bool {
        self.cx.active.is_some() || self.cx.scroll_drag.is_some()
    }

    /// Sets how many frames an untouched state value survives.
    pub fn set_state_ttl_frames(&mut self, frames: u64) {
        self.cx.state.set_ttl_frames(frames);
    }

    pub fn focused(&self) -> Option<Id> {
        self.cx.focused
    }

    /// True when the currently focused node accepts text input. Backends use
    /// this to route clipboard shortcuts without depending on tree internals.
    pub fn text_input_focused(&self) -> bool {
        self.cx.focused.is_some_and(|id| {
            self.cx
                .tree
                .node_by_id(id)
                .is_some_and(Self::accepts_text_input)
        })
    }

    /// The hover target from the last rendered frame.
    pub fn hovered(&self) -> Option<Id> {
        self.cx.hovered
    }

    /// Hit-tests the current retained layout without running a frame. The
    /// backend uses this to avoid rendering when a pointer move stays inside
    /// the same hovered node.
    pub fn hover_at(&self, point: Point) -> Option<Id> {
        self.cx.tree.hit_test(point)
    }

    pub fn a11y_snapshot(&self) -> A11ySnapshot {
        self.cx.tree.a11y_snapshot(self.cx.focused)
    }

    fn root_style() -> Style {
        Style {
            direction: FlexDirection::Column,
            align_items: Align::Stretch,
            width: Length::Percent(100.0),
            height: Length::Percent(100.0),
            ..Style::default()
        }
    }

    fn process_input(&mut self) {
        let events = self.cx.input.take_events();
        for event in events {
            match event {
                InputEvent::PointerMoved(position) => {
                    self.cx.input.pointer = position;
                    self.cx.input.pointer_inside = true;
                    if self.cx.pointer_down {
                        self.cx.dragging = true;
                        if let Some(drag) = self.cx.scroll_drag {
                            self.cx.tree.drag_scrollbar(
                                drag.geometry,
                                position,
                                self.cx.input.time,
                            );
                            self.cx.repaint = true;
                        } else if let Some(active) = self.cx.active
                            && self
                                .cx
                                .tree
                                .node_by_id(active)
                                .is_some_and(|node| node.kind == NodeKind::TextInput)
                        {
                            let x = self.text_pointer_x(active, position);
                            self.queue_text_edit_for(active, TextEdit::PointerDrag { x });
                            self.cx.repaint = true;
                        }
                    }
                }
                InputEvent::PointerDown { position, button } => {
                    self.cx.input.pointer = position;
                    self.cx.input.pointer_inside = true;
                    if button == MouseButton::Right {
                        self.cx.context_menu = None;
                        let hit = self.cx.tree.hit_test(position);
                        self.cx.context_clicked = hit;
                        if let Some(id) = hit
                            && self
                                .cx
                                .tree
                                .node_by_id(id)
                                .is_some_and(|node| node.kind == NodeKind::TextInput)
                        {
                            self.cx.focused = Some(id);
                        }
                        continue;
                    }
                    if button != MouseButton::Left {
                        continue;
                    }
                    if self.cx.context_menu.is_some()
                        && !self.cx.context_menu_dismissed
                        && !self.context_menu_contains(position)
                    {
                        self.cx.context_menu = None;
                        self.cx.context_menu_dismissed = true;
                        continue;
                    }
                    self.cx.pointer_down = true;
                    self.cx.dragging = false;
                    if let Some(geometry) = self.cx.tree.scrollbar_at(position) {
                        self.cx.active = Some(geometry.container);
                        self.cx.focused = Some(geometry.container);
                        self.cx.scroll_drag = Some(ScrollDrag { geometry });
                        self.cx
                            .tree
                            .drag_scrollbar(geometry, position, self.cx.input.time);
                        self.cx.repaint = true;
                        continue;
                    }
                    let hit = self.cx.tree.hit_test(position);
                    self.cx.active = hit.filter(|id| {
                        self.cx
                            .tree
                            .node_by_id(*id)
                            .map(|node| node.sense.is_clickable())
                            .unwrap_or(false)
                    });
                    if let Some(id) = hit
                        && self
                            .cx
                            .tree
                            .node_by_id(id)
                            .map(|node| node.sense.is_focusable())
                            .unwrap_or(false)
                    {
                        self.cx.focused = Some(id);
                    } else {
                        self.cx.focused = None;
                    }
                    if let Some(active) = self.cx.active
                        && self
                            .cx
                            .tree
                            .node_by_id(active)
                            .is_some_and(|node| node.kind == NodeKind::TextInput)
                    {
                        let x = self.text_pointer_x(active, position);
                        let count = match self.cx.last_click {
                            Some((id, time, count))
                                if id == active && self.cx.input.time - time < 0.35 =>
                            {
                                (count % 3) + 1
                            }
                            _ => 1,
                        };
                        self.cx.last_click = Some((active, self.cx.input.time, count));
                        let edit = match count {
                            2 => TextEdit::SelectWord { x },
                            3 => TextEdit::SelectAll,
                            _ => TextEdit::PointerDown {
                                x,
                                extend: self.cx.input.modifiers.shift,
                            },
                        };
                        self.queue_text_edit_for(active, edit);
                    }
                }
                InputEvent::PointerUp { position, button } => {
                    self.cx.input.pointer = position;
                    if button != MouseButton::Left {
                        continue;
                    }
                    if let Some(active) = self.cx.active
                        && self
                            .cx
                            .tree
                            .node_by_id(active)
                            .is_some_and(|node| node.kind == NodeKind::TextInput)
                    {
                        self.queue_text_edit_for(active, TextEdit::PointerUp);
                    }
                    self.cx.pointer_down = false;
                    self.cx.dragging = false;
                    self.cx.scroll_drag = None;
                    let hit = self.cx.tree.hit_test(position);
                    if let Some(active) = self.cx.active {
                        if hit == Some(active) {
                            self.cx.clicked = Some(active);
                        }
                        self.cx.released = Some(active);
                    }
                    self.cx.active = None;
                }
                InputEvent::PointerLeft => {
                    self.cx.input.pointer_inside = false;
                    self.cx.hovered = None;
                    if !self.cx.pointer_down {
                        self.cx.active = None;
                    }
                }
                InputEvent::Scroll(delta) => {
                    let pointer = self.cx.input.pointer;
                    let now = self.cx.input.time;
                    let scrolled = self.cx.tree.scroll_by_at(pointer, delta, now);
                    if scrolled.is_some() {
                        self.cx.repaint = true;
                        self.cx.animations_running = true;
                    }
                }
                InputEvent::KeyDown { key, repeat } => {
                    self.cx.keys.push(KeyPress {
                        key: key.clone(),
                        modifiers: self.cx.input.modifiers,
                        repeat,
                    });
                    self.process_key(key, repeat);
                }
                InputEvent::KeyUp { .. } => {}
                InputEvent::Text(text) => {
                    if !text.is_empty() {
                        self.discard_text_input_if_disabled();
                        self.queue_text_edit(TextEdit::Insert(text));
                    }
                }
                InputEvent::Paste(text) => {
                    self.discard_text_input_if_disabled();
                    let target = self.cx.focused.filter(|id| {
                        self.cx
                            .tree
                            .node_by_id(*id)
                            .is_some_and(Self::accepts_text_input)
                    });
                    if let Some(target) = target {
                        self.queue_text_edit_for(target, TextEdit::Paste(text));
                    }
                }
                InputEvent::PasteTo { target, text } => {
                    self.cx.clipboard_target = None;
                    let valid = self
                        .cx
                        .tree
                        .node_by_id(target)
                        .is_some_and(Self::accepts_text_input);
                    if valid {
                        self.queue_text_edit_for(target, TextEdit::Paste(text));
                    }
                }
                InputEvent::ImePreedit { text, cursor } => {
                    self.discard_text_input_if_disabled();
                    self.queue_text_edit(TextEdit::SetPreedit { text, cursor });
                }
                InputEvent::ModifiersChanged(modifiers) => {
                    self.cx.input.modifiers = modifiers;
                }
                InputEvent::FocusChanged(focused) => {
                    if !focused {
                        self.cx.active = None;
                        self.cx.scroll_drag = None;
                        self.cx.pointer_down = false;
                        self.cx.dragging = false;
                        self.cx.hovered = None;
                        self.cx.context_menu = None;
                    }
                }
            }
        }
        self.cx.input.events.clear();
    }

    fn process_key(&mut self, key: Key, repeat: bool) {
        self.discard_text_input_if_disabled();
        let modifiers = self.cx.input.modifiers;
        let text_focused = self.cx.focused.is_some_and(|id| {
            self.cx
                .tree
                .node_by_id(id)
                .is_some_and(Self::accepts_text_input)
        });
        match key {
            Key::Tab | Key::BackTab if !repeat => {
                let backwards = key == Key::BackTab || modifiers.shift;
                self.advance_focus(backwards);
            }
            Key::Enter | Key::Space if !repeat => {
                if let Some(focused) = self.cx.focused
                    && self.cx.tree.node_by_id(focused).is_some_and(|node| {
                        matches!(
                            node.kind,
                            NodeKind::Button
                                | NodeKind::Checkbox
                                | NodeKind::Radio
                                | NodeKind::ComboBox
                        ) && node.sense.is_clickable()
                    })
                {
                    self.cx.clicked = Some(focused);
                }
            }
            Key::Backspace => {
                self.queue_text_edit(if modifiers.ctrl {
                    TextEdit::BackspaceWord
                } else {
                    TextEdit::Backspace
                });
            }
            Key::Delete => {
                self.queue_text_edit(if modifiers.ctrl {
                    TextEdit::DeleteWord
                } else {
                    TextEdit::Delete
                });
            }
            Key::ArrowLeft | Key::ArrowRight if text_focused => {
                self.queue_text_edit(TextEdit::MoveCaret {
                    motion: if key == Key::ArrowLeft {
                        TextMotion::Left
                    } else {
                        TextMotion::Right
                    },
                    extend: modifiers.shift,
                    by_word: modifiers.ctrl || modifiers.alt,
                });
            }
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown if !text_focused => {
                if let Some(focused) = self.cx.focused
                    && let Some(node) = self.cx.tree.node_by_id(focused)
                    && (node.style.scroll_x || node.style.scroll_y)
                {
                    let delta = match key {
                        Key::ArrowLeft => Vec2::new(-52.0, 0.0),
                        Key::ArrowRight => Vec2::new(52.0, 0.0),
                        Key::ArrowUp => Vec2::new(0.0, -52.0),
                        Key::ArrowDown => Vec2::new(0.0, 52.0),
                        _ => Vec2::ZERO,
                    };
                    if self.cx.tree.scroll_by(focused, delta, self.cx.input.time) {
                        self.cx.animations_running = true;
                        self.cx.repaint = true;
                    }
                }
            }
            Key::PageUp | Key::PageDown if !text_focused => {
                if let Some(focused) = self.cx.focused
                    && let Some(container) = self
                        .cx
                        .tree
                        .nearest_scrollable(focused, kasumi_tree::ScrollAxis::Vertical)
                {
                    let page = self.cx.tree.visual_rect_of(container).height() * 0.85;
                    let delta = if key == Key::PageDown { page } else { -page };
                    if self
                        .cx
                        .tree
                        .scroll_by(container, Vec2::new(0.0, delta), self.cx.input.time)
                    {
                        self.cx.animations_running = true;
                        self.cx.repaint = true;
                    }
                }
            }
            Key::Home if text_focused => {
                self.queue_text_edit(TextEdit::MoveCaret {
                    motion: TextMotion::Home,
                    extend: modifiers.shift,
                    by_word: false,
                });
            }
            Key::End if text_focused => {
                self.queue_text_edit(TextEdit::MoveCaret {
                    motion: TextMotion::End,
                    extend: modifiers.shift,
                    by_word: false,
                });
            }
            Key::Home | Key::End if !text_focused => {
                if let Some(focused) = self.cx.focused
                    && self
                        .cx
                        .tree
                        .node_by_id(focused)
                        .is_some_and(|node| node.style.scroll_x || node.style.scroll_y)
                {
                    let delta = if key == Key::End {
                        1_000_000.0
                    } else {
                        -1_000_000.0
                    };
                    if self
                        .cx
                        .tree
                        .scroll_by(focused, Vec2::new(delta, delta), self.cx.input.time)
                    {
                        self.cx.animations_running = true;
                        self.cx.repaint = true;
                    }
                }
            }
            Key::Character(character) if modifiers.ctrl || modifiers.logo => {
                match character.to_ascii_lowercase().as_str() {
                    "a" if text_focused => self.queue_text_edit(TextEdit::SelectAll),
                    "c" if text_focused => self.queue_text_edit(TextEdit::Copy),
                    "x" if text_focused => self.queue_text_edit(TextEdit::Cut),
                    "z" if text_focused => self.queue_text_edit(if modifiers.shift {
                        TextEdit::Redo
                    } else {
                        TextEdit::Undo
                    }),
                    "y" if text_focused => self.queue_text_edit(TextEdit::Redo),
                    _ => {}
                }
            }
            Key::Escape => {
                self.cx.context_menu = None;
                let keep_combo_focus = self.cx.focused.is_some_and(|id| {
                    self.cx
                        .tree
                        .node_by_id(id)
                        .is_some_and(|node| node.kind == NodeKind::ComboBox)
                });
                if !keep_combo_focus {
                    self.cx.focused = None;
                }
            }
            _ => {}
        }
    }

    fn queue_text_edit(&mut self, edit: TextEdit) {
        let Some(focused) = self.cx.focused else {
            return;
        };
        self.queue_text_edit_for(focused, edit);
    }

    /// Configured widgets publish their current `enabled` bit during `view`,
    /// which is after input events for the frame are processed. If they
    /// advertise a disabled focusable control, reject this batch immediately
    /// so a state refresh and input event in the same frame cannot write
    /// through the old enabled state.
    fn discard_text_input_if_disabled(&mut self) {
        if self.cx.focused.is_some_and(|id| {
            self.cx.tree.node_by_id(id).is_some_and(|node| {
                node.kind == NodeKind::TextInput
                    && (!node.enabled || !node.sense.is_focusable() || node.style.hidden)
            })
        }) {
            self.cx.focused = None;
        }
    }

    fn queue_text_edit_for(&mut self, id: Id, edit: TextEdit) {
        let should_clear_preedit = matches!(
            edit,
            TextEdit::PointerDown { .. }
                | TextEdit::PointerDrag { .. }
                | TextEdit::SelectWord { .. }
        );
        if let Some(node) = self.cx.tree.node_by_id_mut(id)
            && Self::accepts_text_input(node)
        {
            let input = node.text_input.get_or_insert_with(TextInputState::default);
            if should_clear_preedit {
                input.preedit.clear();
                input.preedit_cursor = None;
            }
            input.pending.push(edit);
            self.cx.repaint = true;
        }
    }

    fn text_pointer_x(&self, id: Id, position: Point) -> f32 {
        let Some(node) = self.cx.tree.node_by_id(id) else {
            return 0.0;
        };
        let rect = self.cx.tree.visual_rect_of(id);
        position.x - rect.left() - node.style.padding.left
    }

    fn context_menu_contains(&self, point: Point) -> bool {
        self.cx.tree.hit_test(point).is_some_and(|hit| {
            self.cx
                .tree
                .ancestor_or_self_has_kind(hit, NodeKind::Overlay)
        })
    }

    fn advance_focus(&mut self, backwards: bool) {
        let focusables = self.cx.tree.focusables();
        if focusables.is_empty() {
            self.cx.focused = None;
            return;
        }
        let current = self
            .cx
            .focused
            .and_then(|focused| focusables.iter().position(|id| *id == focused));
        let next = match (current, backwards) {
            (Some(index), false) => (index + 1) % focusables.len(),
            (Some(index), true) => (index + focusables.len() - 1) % focusables.len(),
            (None, false) => 0,
            (None, true) => focusables.len() - 1,
        };
        self.cx.focused = Some(focusables[next]);
    }

    fn recompute_hover(&mut self) {
        if !self.cx.input.pointer_inside {
            self.cx.hovered = None;
            return;
        }
        self.cx.hovered = self.cx.tree.hit_test(self.cx.input.pointer);
    }

    fn cleanup_after_reconcile(&mut self) {
        if let Some(focused) = self.cx.focused
            && self
                .cx
                .tree
                .node_by_id(focused)
                .is_none_or(|node| !node.enabled || !node.sense.is_focusable() || node.style.hidden)
        {
            self.cx.focused = None;
        }
        if let Some(active) = self.cx.active
            && self.cx.tree.find(active).is_none()
        {
            self.cx.active = None;
            self.cx.scroll_drag = None;
            self.cx.dragging = false;
        }
        if let Some(hovered) = self.cx.hovered
            && self.cx.tree.find(hovered).is_none()
        {
            self.cx.hovered = None;
        }
    }
}

fn sanitize_size(size: Size) -> Size {
    Size::new(size.width.max(1.0), size.height.max(1.0))
}

/// Convenience input state builder used by tests and embedders.
pub fn input_state(viewport: Size, time: f64, pointer: Point) -> InputState {
    InputState {
        time,
        viewport,
        scale_factor: 1.0,
        pointer,
        pointer_inside: true,
        modifiers: Modifiers::default(),
        events: Vec::new(),
    }
}

/// A minimal app used by tests and by the backend when the user only wants to
/// inspect the empty root.
pub struct EmptyApp;

impl App for EmptyApp {
    fn view(&mut self, _ui: &mut Ui) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasumi_core::MouseButton;

    struct Counter {
        clicks: u32,
        values: Vec<u32>,
    }

    impl App for Counter {
        fn view(&mut self, ui: &mut Ui) {
            let count = ui.state(|| 0u32);
            let list = ui.state(|| vec![1u32, 2, 3]);
            ui.column(|ui| {
                ui.size_full().padding(20.0).gap(8.0);
                if ui.button("increment").clicked() {
                    count.update(|value| *value += 1);
                    self.clicks += 1;
                }
                ui.label(format!("count: {}", count.get()));
                for value in list.get() {
                    ui.keyed(value, |ui| {
                        ui.label(format!("row {value}"));
                    });
                }
                self.values = list.get();
            });
        }
    }

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    #[test]
    fn click_flows_through_state_and_reconcile() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = Counter {
            clicks: 0,
            values: Vec::new(),
        };

        let mut input = input_state(Size::new(400.0, 300.0), 0.0, Point::new(0.0, 0.0));
        runtime.frame(&mut app, input.clone());

        // Find the button rect from the a11y tree and click its center.
        let snapshot = runtime.a11y_snapshot();
        let button = snapshot
            .nodes
            .iter()
            .find(|node| node.role == kasumi_tree::Role::Button)
            .expect("button missing from a11y tree");
        let point = button.bounds.center();

        input.time = 0.1;
        input.pointer = point;
        input.events.push(InputEvent::PointerDown {
            position: point,
            button: MouseButton::Left,
        });
        runtime.frame(&mut app, input.clone());

        input.time = 0.2;
        input.events.push(InputEvent::PointerUp {
            position: point,
            button: MouseButton::Left,
        });
        runtime.frame(&mut app, input);

        assert_eq!(app.clicks, 1);
        assert_eq!(runtime.stats().nodes, 8);
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| { node.label.as_deref() == Some("count: 1") })
        );
    }

    struct StateOrderApp {
        count_seen: u32,
    }

    impl App for StateOrderApp {
        fn view(&mut self, ui: &mut Ui) {
            let count = ui.state(|| 0u32);
            // Read the state before the button that mutates it. The label in
            // this frame will be stale unless the runtime schedules another
            // frame.
            self.count_seen = count.get();
            ui.column(|ui| {
                ui.size_full().padding(20.0);
                ui.label(format!("count: {}", count.get()));
                if ui.button("increment").clicked() {
                    count.update(|value| *value += 1);
                }
            });
        }
    }

    #[test]
    fn state_write_requests_a_followup_frame() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = StateOrderApp { count_seen: 0 };
        let mut input = input_state(Size::new(400.0, 300.0), 0.0, Point::new(0.0, 0.0));
        runtime.frame(&mut app, input.clone());

        let button = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.role == kasumi_tree::Role::Button)
            .expect("button missing");
        let point = button.bounds.center();
        input.time = 0.1;
        input.pointer = point;
        input.events = vec![InputEvent::PointerMoved(point)];
        runtime.frame(&mut app, input.clone());
        input.time = 0.2;
        input.events = vec![InputEvent::PointerDown {
            position: point,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        input.time = 0.3;
        input.events = vec![InputEvent::PointerUp {
            position: point,
            button: MouseButton::Left,
        }];
        let click_frame = runtime.frame(&mut app, input.clone());
        assert_eq!(click_frame.next_frame_in, Some(Duration::ZERO));

        input.time = 0.4;
        input.events.clear();
        runtime.frame(&mut app, input);
        assert_eq!(app.count_seen, 1);
    }

    struct StaticApp;

    impl App for StaticApp {
        fn view(&mut self, ui: &mut Ui) {
            ui.column(|ui| {
                ui.size_full().padding(20.0).gap(10.0);
                ui.add(crate::Label::new("static heading").font_size(20.0));
                ui.label("a fixed label");
                ui.add(crate::Button::new("OK"));
            });
        }
    }

    #[test]
    fn static_frames_do_no_layout_work_and_reuse_nodes() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = StaticApp;
        let input = input_state(Size::new(400.0, 300.0), 0.0, Point::new(1.0, 1.0));
        let first = runtime.frame(&mut app, input.clone());
        assert!(first.stats.created_nodes > 0);
        let nodes = first.stats.nodes;

        let mut last = first.clone();
        for step in 1..=4 {
            let mut input = input.clone();
            input.time = step as f64 * 0.016;
            last = runtime.frame(&mut app, input);
        }
        assert_eq!(last.stats.nodes, nodes);
        assert_eq!(last.stats.created_nodes, 0);
        assert_eq!(last.stats.layout.style_updates, 0);
        assert_eq!(last.stats.layout.child_updates, 0);
        assert_eq!(last.stats.layout.measures, 0);
        assert!(!last.stats.layout.computed);
        assert!(last.next_frame_in.is_none());
    }

    struct HoverApp {
        hovered: bool,
        pressed: bool,
        clicked: bool,
    }

    impl App for HoverApp {
        fn view(&mut self, ui: &mut Ui) {
            ui.column(|ui| {
                ui.size_full().padding(20.0);
                let response = ui.add(crate::Button::new("target").min_width(120.0));
                self.hovered = response.hovered();
                self.pressed = response.pressed();
                self.clicked = response.clicked();
            });
        }
    }

    #[test]
    fn hover_press_and_release_are_reported() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = HoverApp {
            hovered: false,
            pressed: false,
            clicked: false,
        };
        let mut input = input_state(Size::new(400.0, 300.0), 0.0, Point::new(0.0, 0.0));
        runtime.frame(&mut app, input.clone());

        let button = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.role == kasumi_tree::Role::Button)
            .expect("button missing");
        let point = button.bounds.center();

        input.time = 0.1;
        input.pointer = point;
        input.events = vec![InputEvent::PointerMoved(point)];
        runtime.frame(&mut app, input.clone());
        assert!(app.hovered, "button should be hovered");

        input.time = 0.2;
        input.events = vec![InputEvent::PointerDown {
            position: point,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        assert!(app.pressed, "button should report pressed while held");

        input.time = 0.3;
        input.events = vec![InputEvent::PointerUp {
            position: point,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input);
        assert!(app.clicked, "button should report a click on release");
    }

    #[test]
    fn keyboard_focus_and_activation() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = HoverApp {
            hovered: false,
            pressed: false,
            clicked: false,
        };
        let mut input = input_state(Size::new(400.0, 300.0), 0.0, Point::new(0.0, 0.0));
        runtime.frame(&mut app, input.clone());

        input.time = 0.1;
        input.events = vec![InputEvent::KeyDown {
            key: kasumi_core::Key::Tab,
            repeat: false,
        }];
        runtime.frame(&mut app, input.clone());
        assert!(runtime.focused().is_some());
        assert!(!app.hovered);
        // The focused button reports focus through Response::focused.
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.focused)
        );

        input.time = 0.2;
        input.events = vec![InputEvent::KeyDown {
            key: kasumi_core::Key::Enter,
            repeat: false,
        }];
        runtime.frame(&mut app, input);
        assert!(app.clicked, "Enter on a focused button activates it");
    }

    struct AnimatedApp;

    impl App for AnimatedApp {
        fn view(&mut self, ui: &mut Ui) {
            ui.column(|ui| {
                ui.size_full().padding(20.0);
                let value = ui.animate(1.0, Duration::from_millis(200));
                ui.label(format!("{value:.2}"));
            });
        }
    }

    #[test]
    fn animation_schedules_frames_until_finished() {
        let mut runtime = Runtime::new(texts(), Size::new(300.0, 200.0), Theme::default());
        let mut app = AnimatedApp;
        let mut input = input_state(Size::new(300.0, 200.0), 0.0, Point::new(0.0, 0.0));
        let first = runtime.frame(&mut app, input.clone());
        assert_eq!(first.next_frame_in, Some(Duration::ZERO));

        input.time = 1.0;
        let done = runtime.frame(&mut app, input);
        assert!(done.next_frame_in.is_none(), "animation should be finished");
    }

    struct DuplicateIds;

    impl App for DuplicateIds {
        fn view(&mut self, ui: &mut Ui) {
            ui.keyed(1u32, |ui| {
                ui.label("first");
            });
            ui.keyed(1u32, |ui| {
                ui.label("second");
            });
        }
    }

    #[test]
    #[should_panic(expected = "duplicate element id")]
    fn duplicate_ids_panic() {
        let mut runtime = Runtime::new(texts(), Size::new(100.0, 100.0), Theme::default());
        let mut app = DuplicateIds;
        runtime.frame(
            &mut app,
            input_state(Size::new(100.0, 100.0), 0.0, Point::ZERO),
        );
    }

    #[test]
    fn keyed_rows_keep_their_identity() {
        let mut runtime = Runtime::new(texts(), Size::new(300.0, 200.0), Theme::default());
        let mut app = Counter {
            clicks: 0,
            values: Vec::new(),
        };
        let input = input_state(Size::new(300.0, 200.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input);
        assert_eq!(app.values, vec![1, 2, 3]);
    }

    #[test]
    fn tab_moves_focus_to_a_button() {
        let mut runtime = Runtime::new(texts(), Size::new(300.0, 200.0), Theme::default());
        let mut app = Counter {
            clicks: 0,
            values: Vec::new(),
        };
        let mut input = input_state(Size::new(300.0, 200.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        input.events.push(InputEvent::KeyDown {
            key: kasumi_core::Key::Tab,
            repeat: false,
        });
        runtime.frame(&mut app, input);
        assert!(runtime.focused().is_some());
    }

    struct EditorApp {
        value: String,
    }

    impl App for EditorApp {
        fn view(&mut self, ui: &mut Ui) {
            let value = ui.state(String::new);
            ui.column(|ui| {
                ui.size_full().padding(20.0);
                ui.add(crate::TextInput::new(&value).width(300.0));
            });
            self.value = value.get();
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    enum TwoInputMode {
        Enabled,
        Disabled,
    }

    struct TwoInputApp {
        first: String,
        second: String,
        mode: TwoInputMode,
    }

    impl App for TwoInputApp {
        fn view(&mut self, ui: &mut Ui) {
            let first = ui.state(String::new);
            let second = ui.state(String::new);
            ui.column(|ui| {
                ui.size_full().padding(20.0).gap(10.0);
                ui.add(
                    crate::TextInput::new(&first)
                        .width(260.0)
                        .enabled(self.mode == TwoInputMode::Enabled),
                );
                ui.add(crate::TextInput::new(&second).width(260.0));
            });
            self.first = first.get();
            self.second = second.get();
        }
    }

    fn text_field_bounds(runtime: &Runtime, index: usize) -> kasumi_core::Rect {
        runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .filter(|node| node.role == kasumi_tree::Role::TextField)
            .nth(index)
            .expect("text field missing")
            .bounds
    }

    fn click_text_field(
        runtime: &mut Runtime,
        app: &mut dyn App,
        input: &mut InputState,
        index: usize,
        time: f64,
    ) {
        let point = text_field_bounds(runtime, index).center();
        input.time = time;
        input.pointer = point;
        input.events = vec![
            InputEvent::PointerMoved(point),
            InputEvent::PointerDown {
                position: point,
                button: MouseButton::Left,
            },
        ];
        runtime.frame(app, input.clone());
        input.time = time + 0.05;
        input.events = vec![InputEvent::PointerUp {
            position: point,
            button: MouseButton::Left,
        }];
        runtime.frame(app, input.clone());
    }

    #[test]
    fn context_menu_paste_does_not_leak_into_later_ctrl_v() {
        let mut runtime = Runtime::new(texts(), Size::new(360.0, 220.0), Theme::default());
        let mut app = TwoInputApp {
            first: String::new(),
            second: String::new(),
            mode: TwoInputMode::Enabled,
        };
        let mut input = input_state(Size::new(360.0, 220.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        click_text_field(&mut runtime, &mut app, &mut input, 1, 0.1);
        let second = runtime.focused().expect("second field should be focused");

        input.time = 0.3;
        input.events = vec![InputEvent::PasteTo {
            target: second,
            text: "one".to_owned(),
        }];
        runtime.frame(&mut app, input.clone());
        assert_eq!(app.second, "one");
        assert!(runtime.frame_clipboard_target().is_none());

        click_text_field(&mut runtime, &mut app, &mut input, 0, 0.4);
        let first = runtime.focused().expect("first field should be focused");
        input.time = 0.6;
        input.events = vec![InputEvent::Paste("two".to_owned())];
        runtime.frame(&mut app, input.clone());
        assert_eq!(app.first, "two", "paste should follow current focus");
        assert_eq!(
            app.second, "one",
            "old context-menu target must not be reused"
        );
        assert_eq!(runtime.focused(), Some(first));
    }

    #[test]
    fn disabled_text_input_rejects_keyboard_and_paste_and_clears_focus() {
        let mut runtime = Runtime::new(texts(), Size::new(360.0, 220.0), Theme::default());
        let mut app = TwoInputApp {
            first: String::new(),
            second: String::new(),
            mode: TwoInputMode::Enabled,
        };
        let mut input = input_state(Size::new(360.0, 220.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        click_text_field(&mut runtime, &mut app, &mut input, 0, 0.1);
        runtime.focused().expect("first field should be focused");

        let field = text_field_bounds(&runtime, 0);
        app.mode = TwoInputMode::Disabled;
        input.time = 0.3;
        input.events = vec![InputEvent::Text("typed".to_owned())];
        runtime.frame(&mut app, input.clone());
        assert!(
            runtime.focused().is_none(),
            "disabled focus must be cleared"
        );
        assert!(!runtime.frame(&mut app, input.clone()).ime.focused);
        assert_eq!(app.first, "");

        input.time = 0.4;
        input.events = vec![InputEvent::Text("again".to_owned())];
        runtime.frame(&mut app, input.clone());
        assert_eq!(app.first, "");

        app.mode = TwoInputMode::Enabled;
        input.events.clear();
        runtime.frame(&mut app, input.clone());
        input.time = 0.5;
        input.pointer = field.center();
        input.events = vec![
            InputEvent::PointerMoved(input.pointer),
            InputEvent::PointerDown {
                position: input.pointer,
                button: MouseButton::Left,
            },
        ];
        runtime.frame(&mut app, input.clone());
        input.time = 0.6;
        input.events = vec![InputEvent::PointerUp {
            position: input.pointer,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        let first = runtime
            .focused()
            .expect("re-enabled field should be focusable");

        app.mode = TwoInputMode::Disabled;
        input.time = 0.4;
        input.events = vec![InputEvent::PasteTo {
            target: first,
            text: "pasted".to_owned(),
        }];
        runtime.frame(&mut app, input);
        assert_eq!(app.first, "");
    }

    #[test]
    fn text_input_supports_selection_clipboard_and_undo() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 160.0), Theme::default());
        let mut app = EditorApp {
            value: String::new(),
        };
        let mut input = input_state(Size::new(400.0, 160.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let field = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.role == kasumi_tree::Role::TextField)
            .expect("text field missing");
        let point = field.bounds.center();

        input.time = 0.1;
        input.pointer = point;
        input.events = vec![
            InputEvent::PointerMoved(point),
            InputEvent::PointerDown {
                position: point,
                button: MouseButton::Left,
            },
        ];
        runtime.frame(&mut app, input.clone());
        input.time = 0.2;
        input.events = vec![InputEvent::PointerUp {
            position: point,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());

        input.time = 0.3;
        input.events = vec![InputEvent::Text("hello".to_owned())];
        runtime.frame(&mut app, input.clone());
        assert_eq!(app.value, "hello");

        input.time = 0.4;
        input.modifiers.ctrl = true;
        input.events = vec![InputEvent::KeyDown {
            key: Key::Character("a".to_owned()),
            repeat: false,
        }];
        runtime.frame(&mut app, input.clone());
        input.time = 0.5;
        input.events = vec![InputEvent::KeyDown {
            key: Key::Character("c".to_owned()),
            repeat: false,
        }];
        let copied = runtime.frame(&mut app, input.clone());
        assert_eq!(
            copied.clipboard,
            Some(ClipboardAction::SetText("hello".to_owned()))
        );

        input.time = 0.6;
        input.events = vec![InputEvent::KeyDown {
            key: Key::Character("x".to_owned()),
            repeat: false,
        }];
        let cut = runtime.frame(&mut app, input.clone());
        assert_eq!(app.value, "");
        assert_eq!(
            cut.clipboard,
            Some(ClipboardAction::SetText("hello".to_owned()))
        );

        input.time = 0.7;
        input.modifiers.ctrl = false;
        input.events = vec![InputEvent::Paste("world".to_owned())];
        runtime.frame(&mut app, input.clone());
        assert_eq!(app.value, "world");

        input.time = 0.8;
        input.modifiers.ctrl = true;
        input.events = vec![InputEvent::KeyDown {
            key: Key::Character("z".to_owned()),
            repeat: false,
        }];
        runtime.frame(&mut app, input);
        assert_eq!(app.value, "");
    }

    #[test]
    fn ime_caret_matches_text_paint_geometry_and_stays_inside_content() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 160.0), Theme::default());
        let mut app = EditorApp {
            value: String::new(),
        };
        let mut input = input_state(Size::new(400.0, 160.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        click_text_field(&mut runtime, &mut app, &mut input, 0, 0.1);
        input.time = 0.3;
        input.events = vec![InputEvent::Text("hello".to_owned())];
        let output = runtime.frame(&mut app, input);
        let cursor = output.ime.cursor.expect("focused text input missing caret");
        let field = text_field_bounds(&runtime, 0);
        assert!(output.ime.focused);
        assert!(cursor.left() >= field.left() + 12.0);
        assert!(cursor.right() <= field.right() - 12.0);
        assert!(cursor.top() > field.top() + 1.0);
        assert!(cursor.bottom() < field.bottom() - 1.0);
        assert!(cursor.height() < field.height());
    }

    #[test]
    fn ime_preedit_replaces_selection_and_retains_reported_cursor() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 160.0), Theme::default());
        let mut app = EditorApp {
            value: String::new(),
        };
        let mut input = input_state(Size::new(400.0, 160.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        click_text_field(&mut runtime, &mut app, &mut input, 0, 0.1);
        input.time = 0.3;
        input.events = vec![InputEvent::Text("hello world".to_owned())];
        runtime.frame(&mut app, input.clone());
        let field = runtime.focused().expect("text input missing");

        input.time = 0.4;
        input.events = vec![InputEvent::KeyDown {
            key: Key::Character("a".to_owned()),
            repeat: false,
        }];
        input.modifiers.ctrl = true;
        runtime.frame(&mut app, input.clone());
        input.modifiers.ctrl = false;
        input.time = 0.5;
        input.events = vec![InputEvent::ImePreedit {
            text: "ni".to_owned(),
            cursor: Some((1, 1)),
        }];
        runtime.frame(&mut app, input.clone());
        let editor = runtime
            .cx
            .tree
            .node_by_id(field)
            .unwrap()
            .text_input
            .clone()
            .unwrap();
        assert_eq!(app.value, "hello world");
        assert_eq!(editor.preedit, "ni");
        assert_eq!(editor.preedit_cursor, Some((1, 1)));
        assert_eq!(editor.paint_anchor, 0);
        assert_eq!(editor.paint_cursor, 1);

        input.time = 0.6;
        input.events = vec![InputEvent::Text("你".to_owned())];
        runtime.frame(&mut app, input);
        assert_eq!(app.value, "你");
    }

    struct ScrollApp {
        offset: Point,
    }

    impl App for ScrollApp {
        fn view(&mut self, ui: &mut Ui) {
            let response = ui.scroll(
                crate::ScrollView::vertical().size(Length::Percent(100.0), 120.0),
                |ui| {
                    ui.column(|ui| {
                        ui.width_full().gap(8.0);
                        for index in 0..20 {
                            ui.label(format!("scroll row {index}"));
                        }
                    });
                },
            );
            self.offset = response.scroll_offset();
        }
    }

    #[test]
    fn wheel_scrolling_uses_a_smooth_retained_animation() {
        let mut runtime = Runtime::new(texts(), Size::new(300.0, 220.0), Theme::default());
        let mut app = ScrollApp {
            offset: Point::ZERO,
        };
        let mut input = input_state(Size::new(300.0, 220.0), 0.0, Point::new(40.0, 40.0));
        runtime.frame(&mut app, input.clone());
        input.time = 0.05;
        runtime.frame(&mut app, input.clone());

        input.time = 0.1;
        input.events = vec![InputEvent::Scroll(Vec2::new(0.0, 120.0))];
        let started = runtime.frame(&mut app, input.clone());
        assert_eq!(started.next_frame_in, Some(Duration::ZERO));
        let scroll_id = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.style.scroll_y)
            .map(|(_, node)| node.id)
            .expect("scroll node missing");
        let target = runtime
            .cx
            .tree
            .node_by_id(scroll_id)
            .expect("scroll node disappeared")
            .scroll
            .target;
        assert!(target.y > 0.0, "wheel did not update a retained target");

        input.time = 0.16;
        input.events.clear();
        runtime.frame(&mut app, input);
        assert!(app.offset.y > 0.0, "scroll spring did not advance");
        assert!(app.offset.y < 120.0, "scroll should not snap immediately");
    }

    #[test]
    fn scrollbar_drag_uses_the_pointer_grab_anchor() {
        let mut runtime = Runtime::new(texts(), Size::new(300.0, 220.0), Theme::default());
        let mut app = ScrollApp {
            offset: Point::ZERO,
        };
        let input = input_state(Size::new(300.0, 220.0), 0.0, Point::new(40.0, 40.0));
        runtime.frame(&mut app, input.clone());
        runtime.frame(&mut app, input);
        let scroll_id = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.style.scroll_y)
            .map(|(_, node)| node.id)
            .expect("scroll area missing");
        let geometry = runtime
            .cx
            .tree
            .scrollbar_at(geometry_thumb_center(&runtime, scroll_id))
            .expect("scrollbar is not hittable");
        assert!(geometry.thumb.height() >= 32.0);
        assert!(geometry.track.width() >= 12.0);
        assert!(geometry.track.contains(geometry.thumb.center()));
        let pointer = Point::new(geometry.thumb.center().x, geometry.thumb.center().y + 20.0);
        runtime.cx.tree.drag_scrollbar(geometry, pointer, 0.1);
        assert!(
            runtime
                .cx
                .tree
                .node_by_id(scroll_id)
                .expect("scroll area disappeared")
                .scroll
                .offset
                .y
                > 0.0
        );
    }

    fn geometry_thumb_center(runtime: &Runtime, scroll_id: Id) -> Point {
        let node = runtime
            .cx
            .tree
            .node_by_id(scroll_id)
            .expect("scroll area missing");
        Point::new(node.rect.right() - 6.0, node.rect.top() + 18.0)
    }

    struct MenuApp {
        reset: bool,
    }

    impl App for MenuApp {
        fn view(&mut self, ui: &mut Ui) {
            let area = ui.add(crate::Button::new("Area").min_width(120.0));
            ui.context_menu(&area, |ui| {
                if ui.add(crate::Button::new("Reset").full_width()).clicked() {
                    self.reset = true;
                }
            });
        }
    }

    #[test]
    fn context_menu_is_an_overlay_and_invokes_items() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = MenuApp { reset: false };
        let mut input = input_state(Size::new(400.0, 300.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let area = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Area"))
            .expect("area button missing");
        let point = area.bounds.center();

        input.time = 0.1;
        input.pointer = point;
        input.events = vec![InputEvent::PointerDown {
            position: point,
            button: MouseButton::Right,
        }];
        runtime.frame(&mut app, input.clone());
        let reset = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| {
                node.role == kasumi_tree::Role::Button && node.label.as_deref() == Some("Reset")
            })
            .expect("context menu item missing");
        let reset_point = reset.bounds.center();

        input.time = 0.2;
        input.events.clear();
        input.pointer = reset_point;
        runtime.frame(&mut app, input.clone());
        let reset = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Reset"))
            .expect("context menu item disappeared");
        let reset_point = reset.bounds.center();
        input.time = 0.3;
        input.events = vec![InputEvent::PointerDown {
            position: reset_point,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Reset"))
        );
        input.time = 0.4;
        input.pointer = reset_point;
        input.events = vec![InputEvent::PointerUp {
            position: reset_point,
            button: MouseButton::Left,
        }];
        let menu_click = runtime.frame(&mut app, input.clone());
        assert!(app.reset);
        input.time = 0.5;
        input.events.clear();
        runtime.frame(&mut app, input);
        assert!(menu_click.next_frame_in.is_some());
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .all(|node| node.label.as_deref() != Some("Reset"))
        );
    }

    #[test]
    fn context_menu_item_enters_hover_state_before_click() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = MenuApp { reset: false };
        let mut input = input_state(Size::new(400.0, 300.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let area = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Area"))
            .expect("area button missing");

        input.time = 0.1;
        input.pointer = area.bounds.center();
        input.events = vec![InputEvent::PointerDown {
            position: input.pointer,
            button: MouseButton::Right,
        }];
        runtime.frame(&mut app, input.clone());
        let reset = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Reset"))
            .expect("context menu item missing");

        input.time = 0.2;
        input.pointer = reset.bounds.center();
        input.events = vec![InputEvent::PointerMoved(input.pointer)];
        runtime.frame(&mut app, input.clone());
        let hovered = runtime.hovered().expect("menu item should be hovered");
        let item = runtime
            .cx
            .tree
            .node_by_id(hovered)
            .expect("hovered menu item is missing");
        assert_eq!(item.kind, NodeKind::Button);
        assert!(runtime.cx.tree.node_by_id(hovered).unwrap().hover_progress > 0.0);

        input.time = 0.3;
        input.events.clear();
        runtime.frame(&mut app, input);
        assert!(
            runtime.cx.tree.node_by_id(hovered).unwrap().hover_progress > 0.0,
            "hover transition should persist after the movement event"
        );
        assert!(!app.reset);
    }

    struct DismissMenuApp {
        reset: bool,
    }

    impl App for DismissMenuApp {
        fn view(&mut self, ui: &mut Ui) {
            let area = ui.add(crate::Button::new("Area").min_width(120.0));
            ui.context_menu(&area, |ui| {
                if ui.add(crate::Button::new("Reset").full_width()).clicked() {
                    self.reset = true;
                }
            });
        }
    }

    #[test]
    fn any_left_press_dismisses_the_context_menu() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = DismissMenuApp { reset: false };
        let mut input = input_state(Size::new(400.0, 300.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let area = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Area"))
            .expect("area button missing");

        input.time = 0.1;
        input.pointer = area.bounds.center();
        input.events = vec![InputEvent::PointerDown {
            position: input.pointer,
            button: MouseButton::Right,
        }];
        runtime.frame(&mut app, input.clone());
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Reset"))
        );

        let outside = Point::new(380.0, 280.0);
        input.time = 0.2;
        input.pointer = outside;
        input.events = vec![InputEvent::PointerDown {
            position: outside,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .all(|node| node.label.as_deref() != Some("Reset")),
            "outside click should remove the menu in the same frame"
        );
        assert!(!app.reset);
    }

    struct MenuPasteApp {
        value: String,
    }

    impl App for MenuPasteApp {
        fn view(&mut self, ui: &mut Ui) {
            ui.column(|ui| {
                ui.size_full().padding(20.0);
                let text = ui.state(String::new);
                let response = ui.add(crate::TextInput::new(&text).width(260.0));
                if response.changed() {
                    self.value = text.get();
                }
            });
        }
    }

    #[test]
    fn menu_paste_targets_the_owning_text_input() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 180.0), Theme::default());
        let mut app = MenuPasteApp {
            value: String::new(),
        };
        let mut input = input_state(Size::new(400.0, 180.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let field = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.role == kasumi_tree::Role::TextField)
            .expect("text field missing");
        let point = field.bounds.center();

        input.time = 0.1;
        input.pointer = point;
        input.events = vec![InputEvent::PointerDown {
            position: point,
            button: MouseButton::Right,
        }];
        runtime.frame(&mut app, input.clone());
        let paste = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| {
                node.role == kasumi_tree::Role::Button && node.label.as_deref() == Some("Paste")
            })
            .expect("paste item missing");

        input.time = 0.2;
        input.pointer = paste.bounds.center();
        input.events = vec![InputEvent::PointerDown {
            position: paste.bounds.center(),
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        let paste = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| {
                node.role == kasumi_tree::Role::Button && node.label.as_deref() == Some("Paste")
            })
            .expect("paste item disappeared before release");
        input.time = 0.3;
        input.events = vec![InputEvent::PointerUp {
            position: paste.bounds.center(),
            button: MouseButton::Left,
        }];
        let paste_action = runtime.frame(&mut app, input.clone());
        assert_eq!(paste_action.clipboard, Some(ClipboardAction::ReadText));
        let target = runtime
            .frame_clipboard_target()
            .expect("menu paste target missing");
        input.time = 0.4;
        input.events = vec![InputEvent::PasteTo {
            target,
            text: "pasted".to_owned(),
        }];
        runtime.frame(&mut app, input);
        assert_eq!(app.value, "pasted");
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .all(|node| node.label.as_deref() != Some("Paste")),
            "menu should close immediately when dismissed"
        );
    }

    struct ScrollLayoutApp;

    impl App for ScrollLayoutApp {
        fn view(&mut self, ui: &mut Ui) {
            ui.column(|ui| {
                ui.size_full().padding(20.0).gap(10.0);
                ui.row(|ui| {
                    ui.width_full().height(30.0);
                });
                ui.scroll(
                    crate::ScrollView::vertical()
                        .size(Length::Percent(100.0), Length::Px(120.0))
                        .smooth(false),
                    |ui| {
                        ui.column(|ui| {
                            ui.width_full().gap(7.0);
                            for _ in 0..18 {
                                ui.row(|ui| {
                                    ui.width_full().height(34.0);
                                });
                            }
                        });
                    },
                );
            });
        }
    }

    #[test]
    fn ctrl_v_goes_through_the_clipboard_request_path() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 160.0), Theme::default());
        let mut app = MenuPasteApp {
            value: String::new(),
        };
        let mut input = input_state(Size::new(400.0, 160.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let field = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.role == kasumi_tree::Role::TextField)
            .expect("text field missing");
        let point = field.bounds.center();
        input.time = 0.1;
        input.pointer = point;
        input.events = vec![InputEvent::PointerDown {
            position: point,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        input.time = 0.2;
        input.events = vec![InputEvent::PointerUp {
            position: point,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());

        input.time = 0.3;
        input.events = vec![InputEvent::Paste("clipboard text".to_owned())];
        runtime.frame(&mut app, input);
        assert_eq!(app.value, "clipboard text");
    }

    #[test]
    fn scroll_children_keep_their_natural_height() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 220.0), Theme::default());
        let mut app = ScrollLayoutApp;
        let input = input_state(Size::new(400.0, 220.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        runtime.frame(&mut app, input);
        let scroll = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| {
                node.style.scroll_y && node.scroll.content_size.height > node.rect.height()
            })
            .map(|(_, node)| node.scroll)
            .expect("scroll state missing");
        assert!(
            scroll.content_size.height > 500.0,
            "scroll content was compressed to {}",
            scroll.content_size.height
        );
    }

    struct ImageApp;

    impl App for ImageApp {
        fn view(&mut self, ui: &mut Ui) {
            ui.column(|ui| {
                ui.add(
                    crate::Image::from_rgba8(2, 2, vec![255; 16])
                        .size(Length::Px(24.0), Length::Px(24.0)),
                );
            });
        }
    }

    #[test]
    fn images_enter_the_shared_draw_list() {
        let mut runtime = Runtime::new(texts(), Size::new(100.0, 100.0), Theme::default());
        let mut app = ImageApp;
        runtime.frame(
            &mut app,
            input_state(Size::new(100.0, 100.0), 0.0, Point::ZERO),
        );
        assert_eq!(runtime.draw_list().image_count(), 1);
    }
}
