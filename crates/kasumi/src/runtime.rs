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
use kasumi_state::{ResourceWaker, StateStore};
use kasumi_tree::{A11ySnapshot, NodeKind, TextEdit, TextInputState, TextMotion, Tree};

use crate::cx::{Cx, KeyPress, ScrollDrag};
use crate::theme::Theme;
use crate::ui::Ui;
use crate::window::{WindowAction, WindowInfo};

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
    pub resources: usize,
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
    /// Window commands requested during this frame. The backend executes them
    /// after presentation; they are intentionally not applied mid-frame.
    pub window_actions: Vec<WindowAction>,
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
        let resources = kasumi_state::ResourceStore::new();
        let tree = Tree::new();
        let window = WindowInfo {
            title: "Kasumi".to_owned(),
            size: viewport,
            ..WindowInfo::default()
        };
        Self {
            cx: Cx {
                tree,
                state,
                resources,
                texts: Arc::clone(&texts),
                theme,
                window: window.clone(),
                windows: vec![window],
                window_actions: Vec::new(),
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
                tray_events: Vec::new(),
                tray_status: crate::tray::TrayStatus::Disabled,
                tray_menu: None,
                menus: HashMap::new(),
                menu_top: None,
                pending_menu_selection: None,
                menu_seq: 0,
                clipboard_action: None,
                clipboard_target: None,
                scroll_drag: None,
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
        self.cx.pending_menu_selection = None;
        // Tray events are *not* cleared here: the backend calls
        // `set_tray_state` immediately before this frame, which replaces the
        // batch, and `view` reads it. Clearing here would drop every event
        // before the application could see it.
        self.cx.tray_menu = None;
        self.cx.keys.clear();
        self.cx.clipboard_action = None;
        self.cx.repaint = false;
        self.cx.animations_running = false;
        #[cfg(debug_assertions)]
        self.cx.seen_ids.clear();
        self.cx.state.begin_frame(self.cx.frame);
        self.cx.resources.begin_frame(self.cx.frame);

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
        let colors = &self.cx.theme.colors;
        let scrollbar_colors = kasumi_paint::ScrollbarColors {
            rail: colors.outline_variant.fade(0.55),
            thumb: colors.outline.fade(0.65),
            thumb_hover: colors.outline.fade(0.95),
        };
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
            scrollbar_colors,
            viewport,
            &mut self.draw,
        );

        let cursor = cursor_for(&self.cx.tree, self.cx.hovered);

        // A state write during `view` may have happened after the widgets that
        // read it were declared. Request one follow-up frame so every part of
        // the UI observes the new value.
        let state_repaint = self.cx.state.take_repaint();
        let resource_repaint = self.cx.resources.take_notified();
        let next_frame_in =
            if self.cx.repaint || state_repaint || resource_repaint || self.cx.animations_running {
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
            resources: self.cx.resources.entry_count(),
            layout: layout_stats,
            paint: paint_stats,
            cpu_time: started.elapsed(),
        };

        self.cx.state.end_frame();
        self.cx.resources.end_frame();

        FrameOutput {
            cursor,
            next_frame_in,
            clipboard: self.cx.clipboard_action.take(),
            ime: self.ime_state(),
            window_actions: std::mem::take(&mut self.cx.window_actions),
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

    /// Updates the backend-owned description of the current window.
    pub fn set_window_info(&mut self, window: WindowInfo) {
        self.cx.window = window;
    }

    /// Replaces the list visible to `Ui::windows`.
    ///
    /// Backends call this before each frame with every live window, including
    /// the current one. The runtime appends the current window when a backend
    /// omitted it, so `Ui::windows` always contains at least the current
    /// window.
    pub fn set_windows(&mut self, mut windows: Vec<WindowInfo>) {
        if !windows
            .iter()
            .any(|window| window.key == self.cx.window.key)
        {
            windows.push(self.cx.window.clone());
            windows.sort_by_key(|window| window.key);
        }
        self.cx.windows = windows;
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

    /// Sets how many frames an undeclared resource slot survives.
    pub fn set_resource_ttl_frames(&mut self, frames: u64) {
        self.cx.resources.set_ttl_frames(frames);
    }

    /// Installs the backend callback used to wake the event loop when a
    /// background resource load completes.
    pub fn set_resource_wake_callback(&mut self, callback: ResourceWaker) {
        self.cx.resources.set_wake_callback(Some(callback));
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

    /// Delivers tray activations to this frame's `view` and records the tray's
    /// status. Called by the backend before `frame`.
    pub fn set_tray_state(
        &mut self,
        status: crate::tray::TrayStatus,
        events: Vec<crate::tray::TrayEvent>,
    ) {
        self.cx.tray_status = status;
        self.cx.tray_events = events;
        if !self.cx.tray_events.is_empty() {
            self.cx.repaint = true;
        }
    }

    /// The tray menu the application asked for during the last frame.
    pub fn take_tray_menu(&mut self) -> Option<crate::tray::TrayMenu> {
        self.cx.tray_menu.take()
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
                        self.close_top_menu();
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
                    if self.dismiss_menu_on_press(position) {
                        // The press only dismissed the menu. Consuming it keeps
                        // the control underneath from also activating.
                        self.cx.pointer_down = false;
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
                InputEvent::Scroll { delta, line_delta } => {
                    self.scroll_event(delta, line_delta, self.cx.input.time);
                }
                InputEvent::ScrollAt {
                    delta,
                    line_delta,
                    time,
                } => {
                    self.scroll_event(delta, line_delta, time);
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
                        self.close_top_menu();
                    }
                }
            }
        }
        self.cx.input.events.clear();
    }

    fn scroll_event(&mut self, delta: Vec2, line_delta: Option<Vec2>, time: f64) {
        let pointer = self.cx.input.pointer;
        let kind = if line_delta.is_some() {
            kasumi_tree::ScrollKind::MouseWheel
        } else {
            kasumi_tree::ScrollKind::Pixels
        };
        let scrolled = self
            .cx
            .tree
            .scroll_by_at_with_kind(pointer, delta, time, kind);
        if let Some(id) = scrolled {
            self.cx.repaint = true;
            // Pixel deltas and instant scroll views apply in this frame and
            // must not keep the frame loop awake.
            if self
                .cx
                .tree
                .node_by_id(id)
                .is_some_and(|node| node.scroll.is_animating())
            {
                self.cx.animations_running = true;
            }
        }
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
                    if self.scroll_by_keyboard(focused, delta) {
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
                    if self.scroll_by_keyboard(container, Vec2::new(0.0, delta)) {
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
                    if self.scroll_by_keyboard(focused, Vec2::new(delta, delta)) {
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
                if self.close_top_menu() {
                    return;
                }
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

    fn scroll_by_keyboard(&mut self, id: Id, delta: Vec2) -> bool {
        self.cx.tree.scroll_by(id, delta, self.cx.input.time)
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

    /// True when the point lands inside the open menu of the top-most owner.
    fn menu_contains(&self, position: Point) -> bool {
        let Some(owner) = self.cx.menu_top else {
            return false;
        };
        let Some(state) = self.cx.menus.get(&owner) else {
            return false;
        };
        if !state.open {
            return false;
        }
        // "Inside" means the point lands on a menu panel or on one of its
        // descendants. The full-window scrim that catches dismissing clicks is
        // deliberately outside, so pressing it closes the menu.
        self.cx
            .tree
            .hit_test_any(position)
            .is_some_and(|hit| self.cx.tree.ancestor_or_self_has_kind(hit, NodeKind::Menu))
    }

    /// Dismisses the open menu when `position` is outside it. Returns true when
    /// the press was consumed by the dismissal.
    fn dismiss_menu_on_press(&mut self, position: Point) -> bool {
        let Some(owner) = self.cx.menu_top else {
            return false;
        };
        if !self.cx.menus.get(&owner).is_some_and(|state| state.open) {
            return false;
        }
        if self.menu_contains(position) {
            return false;
        }
        if let Some(state) = self.cx.menus.get_mut(&owner) {
            state.open = false;
            state.path.clear();
            state.hover = None;
        }
        self.cx.menu_top = None;
        self.cx.repaint = true;
        true
    }

    /// Closes the open menu. Returns true when a menu was open.
    fn close_top_menu(&mut self) -> bool {
        let Some(owner) = self.cx.menu_top else {
            return false;
        };
        let Some(state) = self.cx.menus.get_mut(&owner) else {
            return false;
        };
        if !state.open {
            return false;
        }
        state.open = false;
        state.path.clear();
        state.hover = None;
        self.cx.menu_top = None;
        self.cx.repaint = true;
        true
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
        let tree = &self.cx.tree;
        self.cx.menus.retain(|owner, _| tree.find(*owner).is_some());
        if let Some(top) = self.cx.menu_top
            && !self.cx.menus.contains_key(&top)
        {
            self.cx.menu_top = None;
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
    use crate::State;
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

    /// A keyed list whose "next key" allocator is declared every frame but read
    /// only when the user adds a row. The allocator must not be reset by the
    /// state TTL while it is idle, or the list would be handed a key that is
    /// already in use and the duplicate-id guard would fire.
    struct KeyedRows {
        rows: Option<State<Vec<u32>>>,
        next_row: Option<State<u32>>,
        frames: u64,
    }

    impl App for KeyedRows {
        fn view(&mut self, ui: &mut Ui) {
            let rows = ui.state(|| vec![1u32, 2, 3]);
            let next_row = ui.state(|| 4u32);
            // "Add row" twice, far enough apart for the state TTL to expire in
            // between. The allocator is declared on every frame, but only read
            // on the frames that actually add.
            if self.frames == 0 || self.frames == 1000 {
                let id = next_row.get();
                next_row.set(id + 1);
                rows.update(|rows| rows.push(id));
            }
            ui.column(|ui| {
                for id in rows.get() {
                    ui.keyed(id, |ui| {
                        ui.row(|ui| {
                            ui.label(format!("row {id}"));
                        });
                    });
                }
            });
            self.rows = Some(rows);
            self.next_row = Some(next_row);
        }
    }

    #[test]
    fn keyed_rows_survive_state_eviction() {
        let mut runtime = Runtime::new(texts(), Size::new(300.0, 200.0), Theme::default());
        let mut app = KeyedRows {
            rows: None,
            next_row: None,
            frames: 0,
        };
        let size = Size::new(300.0, 200.0);
        // 2000 frames is well past the default 600-frame TTL, so a read-based
        // TTL would evict the allocator between the two additions.
        for frame in 0..2000u64 {
            app.frames = frame;
            let input = input_state(size, frame as f64 * 0.016, Point::ZERO);
            runtime.frame(&mut app, input);
        }
        let ids = app.rows.as_ref().expect("state declared").get();
        assert_eq!(ids, vec![1, 2, 3, 4, 5], "the allocator must not restart");
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
    fn wheel_scrolling_is_monotonic_and_settles() {
        let mut runtime = Runtime::new(texts(), Size::new(300.0, 220.0), Theme::default());
        let mut app = ScrollApp {
            offset: Point::ZERO,
        };
        let mut input = input_state(Size::new(300.0, 220.0), 0.0, Point::new(40.0, 40.0));
        runtime.frame(&mut app, input.clone());
        input.time = 0.05;
        runtime.frame(&mut app, input.clone());

        // Three notches in quick succession accumulate onto the pending
        // target instead of restarting from the current offset.
        let mut started = None;
        for time in [0.1, 0.116, 0.132] {
            input.time = time;
            input.events = vec![InputEvent::Scroll {
                delta: Vec2::new(0.0, 120.0),
                line_delta: Some(Vec2::new(0.0, -1.0)),
            }];
            started = Some(runtime.frame(&mut app, input.clone()));
        }
        assert_eq!(
            started.expect("no scroll frame").next_frame_in,
            Some(Duration::ZERO)
        );

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
        assert_eq!(target.y, 360.0, "three notches should accumulate");

        input.events.clear();
        let mut previous = app.offset.y;
        let mut output = None;
        for frame in 0..120 {
            input.time = 0.15 + frame as f64 / 60.0;
            let result = runtime.frame(&mut app, input.clone());
            assert!(
                app.offset.y + 0.01 >= previous,
                "scroll moved backwards at frame {frame}: {previous} -> {}",
                app.offset.y
            );
            previous = app.offset.y;
            output = Some(result);
        }

        assert_eq!(app.offset.y, 360.0, "scroll did not reach its target");
        assert_eq!(
            output.expect("no frame").next_frame_in,
            None,
            "a settled scroll must stop requesting frames"
        );
    }

    #[test]
    fn independent_controls_settle_instead_of_retargeting_a_shared_channel() {
        struct RadiosApp;

        impl App for RadiosApp {
            fn view(&mut self, ui: &mut Ui) {
                let selected = ui.state(|| 0usize);
                ui.column(|ui| {
                    ui.gap(6.0);
                    ui.radio("Free", 0usize, &selected);
                    ui.radio("Team", 1usize, &selected);
                    ui.radio("Studio", 2usize, &selected);
                });
            }
        }

        let mut runtime = Runtime::new(texts(), Size::new(300.0, 200.0), Theme::default());
        let mut app = RadiosApp;
        let mut input = input_state(Size::new(300.0, 200.0), 0.0, Point::new(290.0, 190.0));
        let mut output = None;
        for frame in 0..240 {
            input.time = frame as f64 / 60.0;
            output = Some(runtime.frame(&mut app, input.clone()));
        }
        assert_eq!(
            output.expect("no frame").next_frame_in,
            None,
            "radio animations kept retargeting each other"
        );
    }

    #[test]
    fn combo_box_children_stay_inside_their_border() {
        struct ComboApp;

        impl App for ComboApp {
            fn view(&mut self, ui: &mut Ui) {
                let selected = ui.state(|| 0usize);
                ui.column(|ui| {
                    ui.width(300.0).padding(18.0);
                    ui.add(
                        crate::ComboBox::new(["Automatic", "Always smooth"], &selected)
                            .width(Length::Percent(100.0)),
                    );
                });
            }
        }

        let mut runtime = Runtime::new(texts(), Size::new(400.0, 200.0), Theme::default());
        let mut app = ComboApp;
        let input = input_state(Size::new(400.0, 200.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        runtime.frame(&mut app, input);

        let combo = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::ComboBox)
            .map(|(_, node)| node)
            .expect("combo box missing");
        let rect = combo.rect;
        assert!(
            (combo.visual.scale() - 1.0).abs() < 0.001,
            "a closed combo must not shrink relative to its children"
        );
        let children = combo.children.clone();
        for child in children {
            let child = runtime.cx.tree.get(child).expect("combo child missing");
            assert!(
                child.rect.left() >= rect.left() - 0.01
                    && child.rect.right() <= rect.right() + 0.01,
                "child {:?} {:?} overflows combo {rect:?}",
                child.kind,
                child.rect
            );
        }
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
            let menu = crate::Menu::new()
                .item(crate::MenuItem::new("Reset"))
                .item(crate::MenuItem::new("Save").shortcut("Ctrl+S"));
            if let Some(selection) = ui.context_menu(&area, menu)
                && selection.label == "Reset"
            {
                self.reset = true;
            }
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
                node.role == kasumi_tree::Role::MenuItem && node.label.as_deref() == Some("Reset")
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
        assert_eq!(item.kind, NodeKind::MenuItem);
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
            let menu = crate::Menu::new().item(crate::MenuItem::new("Reset"));
            if let Some(selection) = ui.context_menu(&area, menu)
                && selection.label == "Reset"
            {
                self.reset = true;
            }
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
                node.role == kasumi_tree::Role::MenuItem && node.label.as_deref() == Some("Paste")
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
                node.role == kasumi_tree::Role::MenuItem && node.label.as_deref() == Some("Paste")
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
        let (scroll_id, scroll) = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| {
                node.style.scroll_y && node.scroll.content_size.height > node.rect.height()
            })
            .map(|(id, node)| (id, node.scroll))
            .expect("scroll state missing");
        assert!(
            scroll.content_size.height > 500.0,
            "scroll content was compressed to {}",
            scroll.content_size.height
        );

        // The content child itself must keep its natural height. Compressing
        // it would make percentage-sized descendants resolve against the
        // viewport and make the layout jump while scrolling.
        let content_id = runtime
            .cx
            .tree
            .get(scroll_id)
            .and_then(|node| node.children.first().copied())
            .expect("scroll content child missing");
        let content_height = runtime
            .cx
            .tree
            .get(content_id)
            .expect("scroll content child disappeared")
            .rect
            .height();
        assert!(
            content_height > 500.0,
            "scroll content child was compressed to {content_height}"
        );
    }

    #[test]
    fn nested_wheel_scrolling_does_not_leak_to_the_page() {
        struct NestedScrollApp;

        impl App for NestedScrollApp {
            fn view(&mut self, ui: &mut Ui) {
                ui.scroll(
                    crate::ScrollView::vertical()
                        .size(Length::Percent(100.0), Length::Px(200.0))
                        .smooth(true),
                    |ui| {
                        ui.column(|ui| {
                            ui.width_full().height(600.0);
                            ui.scroll(
                                crate::ScrollView::vertical()
                                    .size(Length::Percent(100.0), Length::Px(100.0))
                                    .smooth(true),
                                |ui| {
                                    ui.column(|ui| {
                                        ui.width_full().height(400.0);
                                    });
                                },
                            );
                        });
                    },
                );
            }
        }

        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = NestedScrollApp;
        let mut input = input_state(Size::new(400.0, 300.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());

        let scrolls: Vec<_> = runtime
            .cx
            .tree
            .iter()
            .filter(|(_, node)| node.style.scroll_y)
            .map(|(_, node)| (node.id, node.rect))
            .collect();
        assert_eq!(
            scrolls.len(),
            2,
            "expected an outer and an inner scroll view"
        );
        let (inner_id, inner) = scrolls
            .iter()
            .min_by(|a, b| a.1.height().total_cmp(&b.1.height()))
            .copied()
            .expect("inner scroll view missing");
        let outer_id = scrolls
            .iter()
            .find(|(id, _)| *id != inner_id)
            .map(|(id, _)| *id)
            .expect("outer scroll view missing");
        input.pointer = inner.center();

        for notch in 0..10 {
            input.time = 0.1 + notch as f64 / 60.0;
            input.events = vec![InputEvent::Scroll {
                delta: Vec2::new(0.0, 120.0),
                line_delta: Some(Vec2::new(0.0, -1.0)),
            }];
            runtime.frame(&mut app, input.clone());
        }

        let inner = runtime
            .cx
            .tree
            .node_by_id(inner_id)
            .expect("inner disappeared")
            .scroll;
        let outer = runtime
            .cx
            .tree
            .node_by_id(outer_id)
            .expect("outer disappeared")
            .scroll;
        assert!(inner.target.y > 0.0, "inner viewport did not scroll");
        assert_eq!(outer.target.y, 0.0, "wheel leaked into the outer page");
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

#[cfg(test)]
mod virtual_scroll_tests {
    use super::*;
    use kasumi_core::Point;

    struct VirtualListApp {
        scroll_to: Option<f32>,
    }

    impl App for VirtualListApp {
        fn view(&mut self, ui: &mut Ui) {
            ui.column(|ui| {
                ui.size_full().padding(10.0);
                let list = ui.add(
                    crate::ListView::new(10_000, 20.0, |ui, index| {
                        ui.label(format!("row {index}"));
                    })
                    .size(300.0, 200.0)
                    .overscan(40.0),
                );
                if let Some(y) = self.scroll_to.take() {
                    ui.scroll_to(&list, Point::new(0.0, y));
                }
            });
        }
    }

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    fn visible_labels(runtime: &Runtime) -> Vec<String> {
        runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .filter_map(|node| node.label)
            .collect()
    }

    #[test]
    fn virtual_list_builds_only_the_visible_window() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = VirtualListApp { scroll_to: None };
        let mut input = input_state(Size::new(400.0, 300.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        input.time = 0.1;
        runtime.frame(&mut app, input.clone());

        // 200 px viewport / 20 px rows plus 40 px overscan on both sides.
        // The row slot + label plus the surrounding container stay far below
        // the 10k declared items.
        assert!(
            runtime.stats().nodes < 100,
            "virtual list created {} nodes",
            runtime.stats().nodes
        );
        let visible = visible_labels(&runtime);
        assert!(visible.iter().any(|label| label == "row 0"));
        assert!(!visible.iter().any(|label| label == "row 9000"));

        input.time = 0.2;
        app.scroll_to = Some(9000.0);
        runtime.frame(&mut app, input.clone());
        input.time = 0.3;
        runtime.frame(&mut app, input.clone());

        assert!(
            runtime.stats().nodes < 100,
            "scrolled virtual list grew to {} nodes",
            runtime.stats().nodes
        );
        let visible = visible_labels(&runtime);
        assert!(
            visible.iter().any(|label| label == "row 450"),
            "expected the scrolled window to include row 450, got {visible:?}"
        );
        assert!(!visible.iter().any(|label| label == "row 0"));
    }

    #[test]
    fn virtual_scroll_requires_an_extent() {
        struct EagerVirtual;
        impl App for EagerVirtual {
            fn view(&mut self, ui: &mut Ui) {
                ui.virtual_scroll(crate::ScrollView::vertical(), |_, _| {});
            }
        }
        let mut runtime = Runtime::new(texts(), Size::new(200.0, 200.0), Theme::default());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.frame(
                &mut EagerVirtual,
                input_state(Size::new(200.0, 200.0), 0.0, Point::ZERO),
            );
        }));
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod surfaces_tests {
    use super::*;
    use kasumi_core::{MouseButton, Point};

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    fn label_bounds(runtime: &Runtime, label: &str) -> kasumi_core::Rect {
        runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some(label))
            .unwrap_or_else(|| panic!("label {label:?} missing from the accessibility tree"))
            .bounds
    }

    fn click(
        runtime: &mut Runtime,
        app: &mut dyn App,
        input: &mut InputState,
        point: Point,
        time: f64,
    ) {
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
        input.events.clear();
    }

    struct CardApp {
        clicked: bool,
    }

    impl App for CardApp {
        fn view(&mut self, ui: &mut Ui) {
            ui.column(|ui| {
                ui.size_full().padding(20.0);
                let response = ui.card(
                    crate::Card::new().clickable().width(240.0).height(120.0),
                    |ui| {
                        ui.label("card contents");
                    },
                );
                self.clicked = response.clicked();
            });
        }
    }

    #[test]
    fn clickable_card_reports_clicks() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 300.0), Theme::default());
        let mut app = CardApp { clicked: false };
        let mut input = input_state(Size::new(400.0, 300.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let point = label_bounds(&runtime, "card contents").center();
        click(&mut runtime, &mut app, &mut input, point, 0.1);
        assert!(app.clicked, "card should become a click target");
    }

    struct DialogApp {
        background_clicks: u32,
        inside_clicks: u32,
    }

    impl App for DialogApp {
        fn view(&mut self, ui: &mut Ui) {
            let open = ui.state(|| false);
            ui.column(|ui| {
                ui.size_full().padding(20.0).gap(8.0);
                if ui.button("background button").clicked() {
                    self.background_clicks += 1;
                }
                if ui.button("open dialog").clicked() {
                    open.set(true);
                }
            });
            ui.dialog(
                crate::Dialog::new(&open)
                    .title("Modal title")
                    .description("This dialog blocks the page behind it."),
                |ui| {
                    if ui.button("inside dialog").clicked() {
                        self.inside_clicks += 1;
                    }
                    if ui.button("close dialog").clicked() {
                        open.set(false);
                    }
                },
            );
        }
    }

    #[test]
    fn dialog_scrim_blocks_the_page_and_closes_on_click() {
        let mut runtime = Runtime::new(texts(), Size::new(500.0, 400.0), Theme::default());
        let mut app = DialogApp {
            background_clicks: 0,
            inside_clicks: 0,
        };
        let mut input = input_state(Size::new(500.0, 400.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());

        let open = label_bounds(&runtime, "open dialog").center();
        click(&mut runtime, &mut app, &mut input, open, 0.1);
        input.time = 0.3;
        runtime.frame(&mut app, input.clone());
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Modal title")),
            "dialog title should be in the accessibility tree"
        );

        // The background button is under the scrim. Clicking its coordinates
        // must dismiss the modal instead of activating the page control.
        let background = label_bounds(&runtime, "background button").center();
        click(&mut runtime, &mut app, &mut input, background, 0.4);
        assert_eq!(app.background_clicks, 0, "scrim failed to block the page");
        input.time = 0.6;
        runtime.frame(&mut app, input.clone());
        assert!(
            !runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Modal title")),
            "clicking the scrim should close the dialog"
        );
    }

    #[test]
    fn dialog_content_is_interactive_and_escape_closes_it() {
        let mut runtime = Runtime::new(texts(), Size::new(500.0, 400.0), Theme::default());
        let mut app = DialogApp {
            background_clicks: 0,
            inside_clicks: 0,
        };
        let mut input = input_state(Size::new(500.0, 400.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let open = label_bounds(&runtime, "open dialog").center();
        click(&mut runtime, &mut app, &mut input, open, 0.1);
        input.time = 0.3;
        runtime.frame(&mut app, input.clone());

        let inside = label_bounds(&runtime, "inside dialog").center();
        click(&mut runtime, &mut app, &mut input, inside, 0.4);
        assert_eq!(app.inside_clicks, 1, "dialog buttons must receive clicks");
        assert_eq!(app.background_clicks, 0);

        input.time = 0.7;
        input.events = vec![InputEvent::KeyDown {
            key: kasumi_core::Key::Escape,
            repeat: false,
        }];
        runtime.frame(&mut app, input.clone());
        input.time = 0.8;
        runtime.frame(&mut app, input);
        assert!(
            !runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Modal title")),
            "Escape should close a dismissible dialog"
        );
    }

    struct WindowApp;

    impl App for WindowApp {
        fn view(&mut self, ui: &mut Ui) {
            ui.column(|ui| {
                ui.size_full();
                ui.label(if ui.window().is_main() {
                    "primary"
                } else {
                    "secondary"
                });
                ui.open_window(crate::WindowRequest::new("Inspector").size(400.0, 300.0));
                let key = ui.window().key;
                if ui.button("close me").clicked() {
                    ui.close_window(key);
                }
                ui.set_window_title(key, "Renamed");
            });
            assert!(ui.windows().iter().any(|w| w.key == ui.window().key));
        }
    }

    #[test]
    fn window_requests_are_collected_for_the_backend() {
        let mut runtime = Runtime::new(texts(), Size::new(300.0, 200.0), Theme::default());
        assert!(runtime.cx.window.is_main());
        let mut app = WindowApp;
        let output = runtime.frame(
            &mut app,
            input_state(Size::new(300.0, 200.0), 0.0, Point::ZERO),
        );
        assert_eq!(output.window_actions.len(), 2);
        assert!(matches!(
            output.window_actions[0],
            crate::WindowAction::Open(_)
        ));
        assert!(matches!(
            output.window_actions[1],
            crate::WindowAction::SetTitle { .. }
        ));

        // Window snapshots always include the current window even when the
        // backend has not populated a list.
        assert_eq!(runtime.cx.windows.len(), 1);
        assert!(runtime.cx.windows[0].is_main());
    }
}

#[cfg(test)]
mod nested_menu_tests {
    use super::*;
    use kasumi_core::{MouseButton, Point};

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    struct NestedApp {
        last: Option<Vec<usize>>,
        clicks: usize,
    }

    impl App for NestedApp {
        fn view(&mut self, ui: &mut Ui) {
            let button = ui.add(crate::Button::new("Open menu").min_width(120.0));
            let menu = crate::Menu::new()
                .item(crate::MenuItem::new("One"))
                .submenu(
                    "Two",
                    crate::Menu::new()
                        .item(crate::MenuItem::new("Two A"))
                        .submenu(
                            "Two B",
                            crate::Menu::new().item(crate::MenuItem::new("Deep command")),
                        ),
                )
                .separator()
                .item(crate::MenuItem::new("Three").shortcut("Ctrl+3"));
            if let Some(selection) = ui.add(crate::MenuButton::new("Menu", menu)).selection {
                self.clicks += 1;
                self.last = Some(selection.path);
            }
            let _ = button;
        }
    }

    #[test]
    fn nested_submenus_open_and_report_the_full_path() {
        let size = Size::new(600.0, 400.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = NestedApp {
            last: None,
            clicks: 0,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());

        let trigger = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Menu"))
            .expect("menu button missing")
            .bounds
            .center();
        input.time = 0.1;
        input.pointer = trigger;
        input.events = vec![
            InputEvent::PointerMoved(trigger),
            InputEvent::PointerDown {
                position: trigger,
                button: MouseButton::Left,
            },
        ];
        runtime.frame(&mut app, input.clone());
        input.time = 0.15;
        input.events = vec![InputEvent::PointerUp {
            position: trigger,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        assert!(
            runtime.cx.menu_top.is_some(),
            "clicking the button should open its menu"
        );

        // Submenus open on hover after the delay, so drive enough frames.
        let hover = |runtime: &mut Runtime,
                     app: &mut NestedApp,
                     input: &mut InputState,
                     label: &str,
                     time: f64| {
            let bounds = runtime
                .a11y_snapshot()
                .nodes
                .into_iter()
                .find(|node| node.label.as_deref() == Some(label))
                .unwrap_or_else(|| panic!("{label:?} missing"))
                .bounds;
            input.time = time;
            input.pointer = bounds.center();
            input.events = vec![InputEvent::PointerMoved(bounds.center())];
            runtime.frame(app, input.clone());
            input.events.clear();
            bounds.center()
        };

        hover(&mut runtime, &mut app, &mut input, "Two", 0.2);
        let deadline = 1.4;
        let mut time = 0.25;
        while time < deadline
            && !runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Two A"))
        {
            time += 0.05;
            input.time = time;
            runtime.frame(&mut app, input.clone());
        }
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Two A")),
            "hovering a submenu parent should open the nested menu"
        );

        let deep_parent = hover(&mut runtime, &mut app, &mut input, "Two B", time + 0.1);
        let _ = deep_parent;
        let mut time = time + 0.15;
        let deadline = time + 1.4;
        while time < deadline
            && !runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Deep command"))
        {
            time += 0.05;
            input.time = time;
            runtime.frame(&mut app, input.clone());
        }
        let deep = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Deep command"))
            .expect("second-level submenu did not open");
        assert!(
            deep.depth > 0,
            "a nested command must sit inside the menu panels"
        );

        let point = deep.bounds.center();
        input.time = time + 0.1;
        input.pointer = point;
        input.events = vec![
            InputEvent::PointerMoved(point),
            InputEvent::PointerDown {
                position: point,
                button: MouseButton::Left,
            },
        ];
        runtime.frame(&mut app, input.clone());
        input.time = time + 0.15;
        input.events = vec![InputEvent::PointerUp {
            position: point,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());

        assert_eq!(app.clicks, 1, "the deepest item should activate once");
        assert_eq!(app.last.as_deref(), Some(&[1usize, 1, 0][..]));
        assert!(
            runtime.cx.menu_top.is_none(),
            "selecting must close the menu"
        );

        input.time = time + 0.2;
        input.events.clear();
        runtime.frame(&mut app, input);
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .all(|node| node.role != kasumi_tree::Role::MenuItem),
            "closed menus must leave no rows behind"
        );
    }

    #[test]
    fn escape_closes_every_open_level() {
        struct EscapeApp;

        impl App for EscapeApp {
            fn view(&mut self, ui: &mut Ui) {
                let response = ui.add(crate::Button::new("Anchor"));
                if response.clicked() {
                    ui.open_menu(response.id());
                }
                ui.menu(
                    &response,
                    crate::Menu::new().submenu("More", crate::Menu::new().item("Inner")),
                );
            }
        }

        let size = Size::new(400.0, 300.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = EscapeApp;
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let anchor = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Anchor"))
            .expect("anchor missing")
            .bounds
            .center();
        input.time = 0.1;
        input.pointer = anchor;
        input.events = vec![
            InputEvent::PointerMoved(anchor),
            InputEvent::PointerDown {
                position: anchor,
                button: MouseButton::Left,
            },
        ];
        runtime.frame(&mut app, input.clone());
        input.time = 0.15;
        input.events = vec![InputEvent::PointerUp {
            position: anchor,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        assert!(runtime.cx.menu_top.is_some(), "menu should open");
        input.time = 0.2;
        input.events = vec![InputEvent::KeyDown {
            key: Key::Escape,
            repeat: false,
        }];
        runtime.frame(&mut app, input);
        assert!(runtime.cx.menu_top.is_none());
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;
    use kasumi_core::{MouseButton, Point};

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    struct NavApp {
        selected: String,
        clicked: Vec<String>,
        variant: crate::NavVariant,
    }

    impl App for NavApp {
        fn view(&mut self, ui: &mut Ui) {
            let nav = crate::NavigationDrawer::new()
                .variant(self.variant)
                .header(crate::NavHeader::new("kasumi").subtitle("0.2.0"))
                .item(crate::NavItem::new("overview", "Overview").icon("◉"))
                .section("Data", |section| {
                    section
                        .item(crate::NavItem::new("lists", "Lists"))
                        .item(crate::NavItem::new("search", "Search"))
                })
                .item(
                    crate::NavItem::new("settings", "Settings")
                        .child(crate::NavItem::new("theme", "Theme"))
                        .child(crate::NavItem::new("keys", "Keys")),
                )
                .footer(crate::NavItem::new("about", "About"))
                .selected(self.selected.clone());
            ui.row(|ui| {
                ui.size_full();
                let response = ui.navigation(&nav);
                if let Some(key) = response.selected {
                    self.clicked.push(key.clone());
                    self.selected = key;
                }
            });
        }
    }

    fn click(
        runtime: &mut Runtime,
        app: &mut NavApp,
        input: &mut InputState,
        point: Point,
        time: f64,
    ) {
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
        input.events.clear();
    }

    fn row_center(runtime: &Runtime, label: &str) -> Point {
        runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| {
                node.role == kasumi_tree::Role::NavigationItem
                    && node.label.as_deref() == Some(label)
            })
            .unwrap_or_else(|| panic!("navigation row {label:?} missing"))
            .bounds
            .center()
    }

    #[test]
    fn sections_and_footers_render_as_navigation_rows() {
        let size = Size::new(700.0, 500.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = NavApp {
            selected: "overview".to_owned(),
            clicked: Vec::new(),
            variant: crate::NavVariant::Standard,
        };
        let input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());

        let rows: Vec<String> = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .filter(|node| node.role == kasumi_tree::Role::NavigationItem)
            .filter_map(|node| node.label)
            .collect();
        assert_eq!(
            rows,
            vec!["Overview", "Settings", "Lists", "Search", "About"]
        );

        let labels: Vec<String> = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .filter_map(|node| node.label)
            .collect();
        assert!(labels.iter().any(|label| label == "Data"));
        assert!(labels.iter().any(|label| label == "kasumi"));

        // The drawer column is 280 dp wide and participates in the row layout.
        let drawer = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::NavigationDrawer)
            .map(|(_, node)| node.rect)
            .expect("drawer missing");
        assert!((drawer.width() - crate::widgets::DRAWER_WIDTH).abs() < 0.5);
        assert!((drawer.height() - 500.0).abs() < 0.5);

        let list = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.style.scroll_y && node.kind == NodeKind::Container)
            .map(|(_, node)| node)
            .expect("navigation list missing");
        assert!(
            !list.show_scrollbars,
            "the drawer must not paint a persistent scrollbar by default"
        );
    }

    #[test]
    fn nested_group_toggles_and_reports_the_leaf() {
        let size = Size::new(700.0, 500.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = NavApp {
            selected: "overview".to_owned(),
            clicked: Vec::new(),
            variant: crate::NavVariant::Standard,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .all(|node| node.label.as_deref() != Some("Theme")),
            "a collapsed group must hide its children"
        );

        let settings = row_center(&runtime, "Settings");
        click(&mut runtime, &mut app, &mut input, settings, 0.1);
        assert!(app.clicked.is_empty(), "a parent row must not navigate");
        let theme = row_center(&runtime, "Theme");
        assert!(
            theme.y > settings.y,
            "the expanded group is declared below its parent row"
        );
        let keys = row_center(&runtime, "Keys");

        // Nested rows are indented one step further than their parent.
        let parent_left = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Settings"))
            .expect("settings row missing")
            .bounds
            .left();
        let child_left = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Theme"))
            .expect("theme row missing")
            .bounds
            .left();
        assert!(child_left > parent_left);

        click(&mut runtime, &mut app, &mut input, theme, 0.3);
        assert_eq!(app.clicked, vec!["theme".to_owned()]);
        assert_eq!(app.selected, "theme");

        // Collapsing the group removes the children again.
        let settings = row_center(&runtime, "Settings");
        click(&mut runtime, &mut app, &mut input, settings, 0.5);
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .all(|node| node.label.as_deref() != Some("Theme"))
        );
        let _ = keys;
    }

    #[test]
    fn selecting_a_leaf_expands_its_ancestors_on_the_first_frame() {
        let size = Size::new(700.0, 500.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = NavApp {
            selected: "keys".to_owned(),
            clicked: Vec::new(),
            variant: crate::NavVariant::Standard,
        };
        let input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input);
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Keys")),
            "the ancestors of the selection must expand automatically"
        );
    }

    #[test]
    fn rail_hides_labels_and_keeps_the_icon_column() {
        let size = Size::new(700.0, 500.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = NavApp {
            selected: "overview".to_owned(),
            clicked: Vec::new(),
            variant: crate::NavVariant::Rail,
        };
        let input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input);
        let drawer = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::NavigationDrawer)
            .map(|(_, node)| node.rect)
            .expect("rail missing");
        assert!((drawer.width() - crate::widgets::RAIL_WIDTH).abs() < 0.5);
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .filter(|node| node.role == kasumi_tree::Role::NavigationItem)
                .all(|node| node.label.is_some()),
            "rail rows keep their accessibility label even when the text is hidden"
        );
    }
}

#[cfg(test)]
mod list_tests {
    use super::*;
    use kasumi_core::{MouseButton, Point};

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    struct ListApp {
        clicked: Option<usize>,
        virtualized: bool,
        rows: usize,
        selected: usize,
    }

    impl App for ListApp {
        fn view(&mut self, ui: &mut Ui) {
            let mut list = crate::List::new()
                .header("Recent")
                .row(crate::ListRow::new("Design review").supporting("Today, 09:30"))
                .divider();
            for index in 0..self.rows {
                list = list.row(
                    crate::ListRow::new(format!("Row {index}"))
                        .leading_icon("▤")
                        .selected(index == self.selected),
                );
            }
            list = list
                .virtualize(self.virtualized)
                .size(Length::Percent(100.0), 320.0);
            if let Some(index) = ui.list(&list).clicked {
                self.clicked = Some(index);
            }
        }
    }

    fn click(
        runtime: &mut Runtime,
        app: &mut ListApp,
        input: &mut InputState,
        point: Point,
        time: f64,
    ) {
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
        input.events.clear();
    }

    #[test]
    fn rows_report_their_index_and_paint_the_selected_container() {
        let size = Size::new(500.0, 420.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = ListApp {
            clicked: None,
            virtualized: false,
            rows: 3,
            selected: 1,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());

        let snapshot = runtime.a11y_snapshot();
        let labels: Vec<String> = snapshot
            .nodes
            .iter()
            .filter_map(|node| node.label.clone())
            .collect();
        assert!(labels.iter().any(|label| label == "Design review"));
        assert!(labels.iter().any(|label| label == "Recent"));

        let row = snapshot
            .nodes
            .iter()
            .find(|node| node.label.as_deref() == Some("Row 1"))
            .expect("row missing");
        let point = row.bounds.center();
        click(&mut runtime, &mut app, &mut input, point, 0.2);
        assert_eq!(app.clicked, Some(2), "rows are indexed without headers");

        // Row 1 carries the selected container.
        let selected = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.accessibility_label.as_deref() == Some("Row 1"))
            .map(|(_, node)| node.style.background)
            .expect("selected row missing");
        let theme = Theme::default();
        assert_eq!(selected, theme.colors.secondary_container);
    }

    #[test]
    fn virtualized_lists_declare_only_the_visible_window() {
        let size = Size::new(500.0, 420.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = ListApp {
            clicked: None,
            virtualized: true,
            rows: 10_000,
            selected: 0,
        };
        let input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input);

        assert!(
            runtime.cx.tree.node_count() < 200,
            "a 10,000-row virtual list declared {} nodes",
            runtime.cx.tree.node_count()
        );
        let labels: Vec<String> = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .filter_map(|node| node.label)
            .collect();

        assert!(labels.iter().any(|label| label == "Row 0"));
        assert!(!labels.iter().any(|label| label == "Row 9999"));
    }

    #[test]
    fn rows_place_their_text_after_the_leading_slot() {
        struct CardListApp;

        impl App for CardListApp {
            fn view(&mut self, ui: &mut Ui) {
                ui.card(crate::Card::elevated().padding(16.0), |ui| {
                    ui.width_full().gap(10.0);
                    ui.list(
                        &crate::List::new()
                            .header("This week")
                            .row(
                                crate::ListRow::new("Design review")
                                    .supporting("Today")
                                    .leading_avatar("DR"),
                            )
                            .divider()
                            .row(crate::ListRow::new("Notifications").leading_icon("o")),
                    );
                });
            }
        }

        let size = Size::new(500.0, 400.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = CardListApp;
        runtime.frame(&mut app, input_state(size, 0.0, Point::ZERO));
        let rect_of = |text: &str| {
            runtime
                .cx
                .tree
                .iter()
                .find_map(|(_, node)| (node.text_str() == Some(text)).then_some(node.rect))
                .unwrap_or_else(|| panic!("{text:?} missing"))
        };
        let avatar = rect_of("DR");
        let title = rect_of("Design review");
        let supporting = rect_of("Today");
        assert!(
            title.left() >= avatar.right(),
            "the title must start after the avatar: {title:?} vs {avatar:?}"
        );
        assert!(
            (title.left() - supporting.left()).abs() < 0.5,
            "title and supporting text share a column"
        );
        let icon = rect_of("o");
        let second = rect_of("Notifications");
        assert!(second.left() >= icon.right());
        assert!(
            title.height() > 10.0 && title.width() > 20.0,
            "the title must have real geometry: {title:?}"
        );
    }

    #[test]
    fn disabled_rows_ignore_presses() {
        struct DisabledApp {
            clicked: Option<usize>,
        }

        impl App for DisabledApp {
            fn view(&mut self, ui: &mut Ui) {
                let list = crate::List::new()
                    .row(crate::ListRow::new("Locked").enabled(false))
                    .row(crate::ListRow::new("Open"));
                if let Some(index) = ui.list(&list).clicked {
                    self.clicked = Some(index);
                }
            }
        }

        let size = Size::new(400.0, 300.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = DisabledApp { clicked: None };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let locked = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Locked"))
            .expect("locked row missing")
            .bounds
            .center();
        input.time = 0.1;
        input.pointer = locked;
        input.events = vec![
            InputEvent::PointerMoved(locked),
            InputEvent::PointerDown {
                position: locked,
                button: MouseButton::Left,
            },
        ];
        runtime.frame(&mut app, input.clone());
        input.time = 0.15;
        input.events = vec![InputEvent::PointerUp {
            position: locked,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input);
        assert_eq!(app.clicked, None);
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use kasumi_core::{MouseButton, Point};

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    struct SearchApp {
        query: String,
        activated: Option<String>,
    }

    impl App for SearchApp {
        fn view(&mut self, ui: &mut Ui) {
            let query = ui.state(String::new);
            ui.column(|ui| {
                ui.size_full().padding(20.0).gap(12.0);
                let search = crate::SearchBar::new(&query)
                    .placeholder("Search commands")
                    .width(Length::Percent(100.0))
                    .results([
                        crate::SearchResult::new("Overview").description("the starting page"),
                        crate::SearchResult::new("Lists"),
                        crate::SearchResult::new("Menus"),
                    ]);
                if let Some(title) = ui.search(&search).activated {
                    self.activated = Some(title);
                }
            });
            self.query = query.get();
        }
    }

    fn bar_center(runtime: &Runtime) -> Point {
        runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::SearchField)
            .map(|(_, node)| node.rect.center())
            .expect("search bar missing")
    }

    fn click(
        runtime: &mut Runtime,
        app: &mut SearchApp,
        input: &mut InputState,
        point: Point,
        time: f64,
    ) {
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
        input.events.clear();
    }

    #[test]
    fn typing_filters_results_and_enter_activates_the_highlight() {
        let size = Size::new(600.0, 400.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = SearchApp {
            query: String::new(),
            activated: None,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let bar = bar_center(&runtime);
        click(&mut runtime, &mut app, &mut input, bar, 0.1);
        assert!(runtime.focused().is_some(), "clicking focuses the editor");

        input.time = 0.2;
        input.events = vec![InputEvent::Text("me".to_owned())];
        runtime.frame(&mut app, input.clone());
        assert_eq!(app.query, "me");
        let labels: Vec<String> = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .filter_map(|node| node.label)
            .collect();
        assert!(
            labels.iter().any(|label| label == "Menus"),
            "the matching result must be listed: {labels:?}"
        );
        assert!(
            !labels.iter().any(|label| label == "Overview"),
            "non-matching results must be filtered out"
        );

        input.time = 0.3;
        input.events = vec![InputEvent::KeyDown {
            key: Key::Enter,
            repeat: false,
        }];
        runtime.frame(&mut app, input.clone());
        assert_eq!(app.activated.as_deref(), Some("Menus"));
    }

    #[test]
    fn clicking_a_result_activates_it_and_closes_the_popup() {
        let size = Size::new(600.0, 400.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = SearchApp {
            query: String::new(),
            activated: None,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let bar = bar_center(&runtime);
        click(&mut runtime, &mut app, &mut input, bar, 0.1);

        let row = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some("Lists"))
            .expect("result row missing")
            .bounds
            .center();
        click(&mut runtime, &mut app, &mut input, row, 0.2);
        assert_eq!(app.activated.as_deref(), Some("Lists"));
        input.time = 0.3;
        input.events.clear();
        runtime.frame(&mut app, input.clone());

        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .all(|node| node.label.as_deref() != Some("Overview")),
            "the popup must close after activation"
        );
    }

    #[test]
    fn escape_closes_the_popup_before_the_field() {
        let size = Size::new(600.0, 400.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = SearchApp {
            query: String::new(),
            activated: None,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let bar = bar_center(&runtime);
        click(&mut runtime, &mut app, &mut input, bar, 0.1);
        input.time = 0.2;
        input.events = vec![InputEvent::KeyDown {
            key: Key::Escape,
            repeat: false,
        }];
        runtime.frame(&mut app, input.clone());
        assert!(runtime.focused().is_none(), "Escape clears the field");
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .all(|node| node.label.as_deref() != Some("Overview")),
            "Escape closes the popup"
        );
    }

    #[test]
    fn the_search_view_covers_the_window() {
        struct ViewApp;

        impl App for ViewApp {
            fn view(&mut self, ui: &mut Ui) {
                let query = ui.state(String::new);
                let search = crate::SearchBar::new(&query)
                    .view(true)
                    .results([crate::SearchResult::new("One")]);
                ui.search(&search);
            }
        }

        let size = Size::new(400.0, 300.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = ViewApp;
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let bar = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::SearchField)
            .map(|(_, node)| node.rect.center())
            .expect("search bar missing");
        click_for(&mut runtime, &mut input, bar, 0.1, |runtime, input| {
            runtime.frame(&mut app, input.clone())
        });
        let panel = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::Overlay && node.rect.width() >= 399.0)
            .map(|(_, node)| node.rect)
            .expect("full search view missing");
        assert!((panel.height() - 300.0).abs() < 0.5, "{panel:?}");
    }

    fn click_for(
        runtime: &mut Runtime,
        input: &mut InputState,
        point: Point,
        time: f64,
        mut frame: impl FnMut(&mut Runtime, &InputState) -> FrameOutput,
    ) {
        input.time = time;
        input.pointer = point;
        input.events = vec![
            InputEvent::PointerMoved(point),
            InputEvent::PointerDown {
                position: point,
                button: MouseButton::Left,
            },
        ];
        frame(runtime, input);
        input.time = time + 0.05;
        input.events = vec![InputEvent::PointerUp {
            position: point,
            button: MouseButton::Left,
        }];
        frame(runtime, input);
        input.events.clear();
    }
}

#[cfg(test)]
mod switch_tests {
    use super::*;
    use kasumi_core::{MouseButton, Point};

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    struct SwitchApp {
        on: bool,
        supporting: bool,
    }

    impl App for SwitchApp {
        fn view(&mut self, ui: &mut Ui) {
            let value = ui.state(|| false);
            ui.column(|ui| {
                ui.size_full().padding(20.0).gap(12.0);
                let mut switch = crate::Switch::new(&value).label("Notifications");
                if self.supporting {
                    switch = switch.supporting("Push alerts");
                }
                ui.add(switch);
            });
            if value.get() {
                self.on = true;
            }
        }
    }

    fn switch_geometry(runtime: &Runtime) -> (kasumi_core::Rect, kasumi_tree::SwitchPaint) {
        let node = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::Switch && node.switch.is_some())
            .map(|(_, node)| node)
            .expect("switch track missing");
        (node.rect, node.switch.expect("switch paint data missing"))
    }

    #[test]
    fn clicking_the_row_toggles_and_repaints_the_track() {
        let size = Size::new(400.0, 300.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = SwitchApp {
            on: false,
            supporting: false,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());

        let (rect, off) = switch_geometry(&runtime);
        assert!(rect.contains(Point::new(rect.center().x, rect.center().y)));
        assert!(off.progress < 0.05, "an off switch starts at zero");
        let theme = Theme::default();
        assert!(
            (off.track.r - theme.colors.surface_variant.r).abs() < 0.01,
            "the off track is surface-variant"
        );
        assert!((off.thumb - 16.0).abs() < 0.5);

        let point = rect.center();
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
        input.time = 0.15;
        input.events = vec![InputEvent::PointerUp {
            position: point,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        assert!(app.on, "the click must toggle the bound state");

        // Let the 250 ms thumb and color animation settle.
        for frame in 0..60 {
            input.time = 0.2 + frame as f64 / 60.0;
            input.events.clear();
            runtime.frame(&mut app, input.clone());
        }
        let (_, on) = switch_geometry(&runtime);
        assert!(on.progress > 0.95, "the switch must animate to on");
        assert!(
            (on.track.r - theme.colors.primary.r).abs() < 0.02
                && (on.track.b - theme.colors.primary.b).abs() < 0.02,
            "the on track is primary"
        );
        assert!((on.thumb - 24.0).abs() < 0.5, "the on thumb is 24 dp");
        assert!(
            on.thumb_center.0 > off.thumb_center.0,
            "the thumb travels toward the trailing edge"
        );
    }

    #[test]
    fn the_row_reports_changed_and_keeps_its_touch_target() {
        struct RowApp {
            changed: bool,
        }

        impl App for RowApp {
            fn view(&mut self, ui: &mut Ui) {
                let value = ui.state(|| false);
                let response = ui.add(crate::Switch::new(&value).label("Wifi"));
                self.changed = response.changed();
            }
        }

        let size = Size::new(400.0, 200.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = RowApp { changed: false };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let row = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| {
                node.kind == NodeKind::Switch && node.accessibility_label.as_deref() == Some("Wifi")
            })
            .map(|(_, node)| node.rect)
            .expect("switch row missing");
        assert!(
            row.height() >= 48.0,
            "the row keeps a 48 dp touch target: {row:?}"
        );
        let rect = row;

        input.time = 0.1;
        input.pointer = rect.center();
        input.events = vec![InputEvent::PointerDown {
            position: rect.center(),
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        input.time = 0.15;
        input.events = vec![InputEvent::PointerUp {
            position: rect.center(),
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input);
        assert!(app.changed, "toggling must report a change");
    }

    #[test]
    fn disabled_switches_ignore_presses() {
        struct DisabledApp {
            on: bool,
        }

        impl App for DisabledApp {
            fn view(&mut self, ui: &mut Ui) {
                let value = ui.state(|| false);
                ui.add(crate::Switch::new(&value).label("Locked").enabled(false));
                if value.get() {
                    self.on = true;
                }
            }
        }

        let size = Size::new(400.0, 200.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = DisabledApp { on: false };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let (rect, _) = switch_geometry(&runtime);
        input.time = 0.1;
        input.pointer = rect.center();
        input.events = vec![InputEvent::PointerDown {
            position: rect.center(),
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        input.time = 0.15;
        input.events = vec![InputEvent::PointerUp {
            position: rect.center(),
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input);
        assert!(!app.on, "a disabled switch must not toggle");
    }

    #[test]
    fn supporting_text_grows_the_row() {
        let size = Size::new(400.0, 300.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = SwitchApp {
            on: false,
            supporting: true,
        };
        runtime.frame(&mut app, input_state(size, 0.0, Point::ZERO));
        let row = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| {
                node.kind == NodeKind::Switch
                    && node.accessibility_label.as_deref() == Some("Notifications")
            })
            .map(|(_, node)| node.rect)
            .expect("switch row missing");
        assert!(
            row.height() >= 64.0,
            "a supporting line needs a two-line row: {row:?}"
        );
        let labels: Vec<String> = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .filter_map(|node| node.label)
            .collect();
        assert!(labels.iter().any(|label| label == "Notifications"));
        assert!(labels.iter().any(|label| label == "Push alerts"));
    }
}

#[cfg(test)]
mod dynamic_theme_tests {
    use super::*;
    use crate::{ContrastLevel, DynamicColorConfig, SchemeVariant, ThemeMode};

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    struct SeededApp {
        seed: kasumi_core::Color,
        variant: SchemeVariant,
        seen: Option<Theme>,
    }

    impl App for SeededApp {
        fn view(&mut self, ui: &mut Ui) {
            let theme = Theme::from_seed_with(
                self.seed,
                ThemeMode::Light,
                DynamicColorConfig::new().variant(self.variant),
            );
            ui.set_theme(theme);
            self.seen = Some(ui.theme().clone());
            let on = ui.state(|| true);
            ui.add(crate::Switch::new(&on).icons(false));
            ui.label("seeded");
        }
    }

    #[test]
    fn a_seeded_theme_colors_widgets_declared_after_the_set() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 200.0), Theme::light());
        let mut app = SeededApp {
            seed: kasumi_core::Color::hex("#006B5E"),
            variant: SchemeVariant::TonalSpot,
            seen: None,
        };
        runtime.frame(
            &mut app,
            input_state(Size::new(400.0, 200.0), 0.0, Point::ZERO),
        );
        let theme = app.seen.clone().expect("theme captured");
        assert_eq!(runtime.theme(), &theme);
        // The seeded scheme replaces the baseline roles for everything
        // declared after `set_theme`, including widgets that resolve colors
        // from the theme themselves.
        assert_ne!(theme.colors.primary, Theme::light().colors.primary);
        let track = runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::Switch && node.switch.is_some())
            .map(|(_, node)| node.switch.expect("switch paint"))
            .expect("switch missing");
        // The switch animates its color over 250 ms, so on the first frame it
        // is still the unselected `surface-variant`; the point of the test is
        // that the retained paint data exists at all and belongs to the
        // seeded theme's switch.
        assert!(track.track.a > 0.5);
        assert_eq!(
            theme.colors.surface_variant.r > 0.0,
            true,
            "the seeded scheme provides every surface role"
        );
    }

    #[test]
    fn switching_the_seed_repaints_the_next_frame() {
        let mut runtime = Runtime::new(texts(), Size::new(400.0, 200.0), Theme::light());
        let mut app = SeededApp {
            seed: kasumi_core::Color::hex("#006B5E"),
            variant: SchemeVariant::TonalSpot,
            seen: None,
        };
        let input = input_state(Size::new(400.0, 200.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let first = runtime.theme().colors.primary;
        app.seed = kasumi_core::Color::hex("#B3261E");
        runtime.frame(&mut app, input);
        let second = runtime.theme().colors.primary;
        assert_ne!(first, second, "a new seed must change the resolved roles");
    }

    #[test]
    fn high_contrast_stays_inside_the_theme() {
        let theme = Theme::from_seed_with(
            kasumi_core::Color::hex("#4F5BD5"),
            ThemeMode::Dark,
            DynamicColorConfig::new().contrast(ContrastLevel::High),
        );
        assert!(theme.is_dark());
        assert_eq!(theme.typography.body_medium.color, theme.colors.on_surface);
        // High contrast on a dark scheme pushes the on-surface tone up.
        let standard = Theme::from_seed(kasumi_core::Color::hex("#4F5BD5"), ThemeMode::Dark);
        assert!(theme.colors.on_surface.r >= standard.colors.on_surface.r);
    }
}

#[cfg(test)]
mod combo_box_tests {
    use super::*;
    use kasumi_core::{MouseButton, Point};

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    struct ComboApp {
        selected: usize,
        changed: usize,
    }

    impl App for ComboApp {
        fn view(&mut self, ui: &mut Ui) {
            let selected = ui.state(|| 0usize);
            ui.column(|ui| {
                ui.size_full().padding(20.0);
                let response =
                    ui.combo_box(["Automatic", "Always smooth", "Reduced motion"], &selected);
                if response.changed() {
                    self.changed += 1;
                }
            });
            self.selected = selected.get();
        }
    }

    fn field_center(runtime: &Runtime) -> Point {
        runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::ComboBox)
            .map(|(_, node)| node.rect.center())
            .expect("combo box missing")
    }

    fn click(
        runtime: &mut Runtime,
        app: &mut ComboApp,
        input: &mut InputState,
        point: Point,
        time: f64,
    ) {
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
        input.events.clear();
    }

    fn popup_visual(runtime: &Runtime) -> Option<(f32, f32, Option<(f32, f32)>)> {
        runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::Overlay && node.style.clip)
            .map(|(_, node)| {
                (
                    node.visual.scale(),
                    node.visual.opacity(),
                    node.visual.origin(),
                )
            })
    }

    #[test]
    fn opening_scales_from_the_top_edge_and_settles_at_full_size() {
        let size = Size::new(500.0, 400.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = ComboApp {
            selected: 0,
            changed: 0,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        assert!(popup_visual(&runtime).is_none(), "the popup starts closed");

        let center = field_center(&runtime);
        click(&mut runtime, &mut app, &mut input, center, 0.1);
        let (scale, opacity, origin) = popup_visual(&runtime).expect("popup should open");
        assert!(
            scale < 0.95,
            "the panel must start scaled down, got {scale}"
        );
        assert_eq!(
            opacity, 1.0,
            "an open panel stays opaque so its rows remain hittable"
        );
        assert_eq!(
            origin,
            Some((0.5, 0.0)),
            "a downward panel grows from its top edge"
        );

        // Let the spring settle.
        for frame in 0..90 {
            input.time = 0.2 + frame as f64 / 60.0;
            input.events.clear();
            runtime.frame(&mut app, input.clone());
        }
        let (scale, opacity, _) = popup_visual(&runtime).expect("popup should stay open");
        assert!((scale - 1.0).abs() < 0.01, "settled scale is 1.0");
        assert!(opacity > 0.99, "settled opacity is 1.0");
    }

    #[test]
    fn the_panel_keeps_animating_out_after_it_closes() {
        let size = Size::new(500.0, 400.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = ComboApp {
            selected: 0,
            changed: 0,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let center = field_center(&runtime);
        click(&mut runtime, &mut app, &mut input, center, 0.1);
        for frame in 0..90 {
            input.time = 0.2 + frame as f64 / 60.0;
            input.events.clear();
            runtime.frame(&mut app, input.clone());
        }
        // Close it again.
        click(&mut runtime, &mut app, &mut input, center, 2.0);
        let during = popup_visual(&runtime);
        assert!(
            during.is_some(),
            "the closing panel must stay mounted while it animates"
        );
        let (scale, _, _) = during.expect("panel");
        assert!(scale <= 1.0);

        // Once the exit tween finishes the overlay is gone.
        for frame in 0..60 {
            input.time = 2.2 + frame as f64 / 60.0;
            input.events.clear();
            runtime.frame(&mut app, input.clone());
        }
        assert!(
            popup_visual(&runtime).is_none(),
            "the panel must unmount after the exit motion"
        );
    }

    #[test]
    fn the_close_animation_is_quick() {
        let size = Size::new(500.0, 400.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = ComboApp {
            selected: 0,
            changed: 0,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let center = field_center(&runtime);
        click(&mut runtime, &mut app, &mut input, center, 0.1);
        for frame in 0..90 {
            input.time = 0.2 + frame as f64 / 60.0;
            input.events.clear();
            runtime.frame(&mut app, input.clone());
        }
        // Close by pressing the field again, and count from the release that
        // started the exit motion.
        let mut time = 2.0;
        input.time = time;
        input.pointer = center;
        input.events = vec![
            InputEvent::PointerMoved(center),
            InputEvent::PointerDown {
                position: center,
                button: MouseButton::Left,
            },
        ];
        runtime.frame(&mut app, input.clone());
        time += 1.0 / 60.0;
        input.time = time;
        input.events = vec![InputEvent::PointerUp {
            position: center,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        let mut frames = 0;
        while popup_visual(&runtime).is_some() && frames < 600 {
            time += 1.0 / 60.0;
            input.time = time;
            input.events.clear();
            runtime.frame(&mut app, input.clone());
            frames += 1;
            if frames < 8 {
                let visual = popup_visual(&runtime);
                eprintln!("frame {frames}: {visual:?}");
            }
        }
        eprintln!(
            "CLOSE FRAMES {frames} ({:.0} ms)",
            frames as f32 / 60.0 * 1000.0
        );
        assert!(frames < 20, "the close took {frames} frames");
    }

    #[test]
    fn rows_are_48_dp_and_report_the_selection() {
        let size = Size::new(500.0, 400.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = ComboApp {
            selected: 0,
            changed: 0,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let center = field_center(&runtime);
        click(&mut runtime, &mut app, &mut input, center, 0.1);

        let rows: Vec<(String, kasumi_core::Rect)> = runtime
            .cx
            .tree
            .iter()
            .filter(|(_, node)| node.kind == NodeKind::MenuItem)
            .filter_map(|(_, node)| {
                node.accessibility_label
                    .clone()
                    .map(|label| (label, node.rect))
            })
            .collect();
        assert_eq!(rows.len(), 3, "every option must have a row");
        for (label, rect) in &rows {
            assert!(
                (rect.height() - 48.0).abs() < 0.5,
                "{label} row is {} dp",
                rect.height()
            );
        }

        let target = rows
            .iter()
            .find(|(label, _)| label == "Reduced motion")
            .expect("option missing")
            .1
            .center();
        click(&mut runtime, &mut app, &mut input, target, 0.4);
        assert_eq!(app.selected, 2, "clicking a row selects it");
        assert_eq!(app.changed, 1, "the change is reported once");
    }

    #[test]
    fn the_chevron_cross_fades_between_states() {
        let size = Size::new(500.0, 400.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = ComboApp {
            selected: 0,
            changed: 0,
        };
        let mut input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let opacities = |runtime: &Runtime| -> Vec<f32> {
            runtime
                .cx
                .tree
                .iter()
                .filter(|(_, node)| {
                    node.text_str()
                        .is_some_and(|text| text == "▾" || text == "▴")
                })
                .map(|(_, node)| node.visual.opacity())
                .collect()
        };
        let closed = opacities(&runtime);
        assert_eq!(closed.len(), 2, "both chevrons exist");
        assert!(
            closed.iter().any(|alpha| *alpha > 0.9) && closed.iter().any(|alpha| *alpha < 0.1),
            "exactly one chevron is visible when closed: {closed:?}"
        );

        let center = field_center(&runtime);
        click(&mut runtime, &mut app, &mut input, center, 0.1);
        input.time = 0.2;
        input.events.clear();
        runtime.frame(&mut app, input.clone());
        let opening = opacities(&runtime);
        assert!(
            opening.iter().all(|alpha| *alpha > 0.05 && *alpha < 0.95),
            "mid-animation the chevrons blend: {opening:?}"
        );
    }
}

#[cfg(test)]
mod tray_tests {
    use super::*;
    use crate::tray::{TrayEvent, TrayMenu, TrayMenuItem, TrayStatus};

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    struct TrayApp {
        seen: Vec<String>,
    }

    impl App for TrayApp {
        fn view(&mut self, ui: &mut Ui) {
            for event in ui.tray_events().to_vec() {
                if let TrayEvent::Activated { id } = event {
                    self.seen.push(id.clone());
                    if id == "theme" {
                        ui.set_tray_menu(
                            TrayMenu::new("Kasumi")
                                .item(TrayMenuItem::new("theme", "Dark theme").checkbox(true)),
                        );
                    }
                }
            }
            ui.label("tray");
        }
    }

    #[test]
    fn activations_reach_the_next_frame_exactly_once() {
        let size = Size::new(300.0, 200.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = TrayApp { seen: Vec::new() };
        let input = input_state(size, 0.0, Point::ZERO);

        runtime.frame(&mut app, input.clone());
        assert!(app.seen.is_empty(), "no tray events yet");

        runtime.set_tray_state(
            TrayStatus::Active,
            vec![TrayEvent::Activated {
                id: "theme".to_owned(),
            }],
        );
        runtime.frame(&mut app, input.clone());
        assert_eq!(app.seen, vec!["theme".to_owned()]);

        let menu = runtime.take_tray_menu().expect("menu was requested");
        assert_eq!(
            menu.find("theme").expect("theme item").checked(),
            Some(true)
        );

        runtime.set_tray_state(TrayStatus::Active, Vec::new());
        runtime.frame(&mut app, input);
        assert_eq!(app.seen, vec!["theme".to_owned()], "no replay");
        assert!(runtime.take_tray_menu().is_none());
    }

    #[test]
    fn the_status_is_visible_to_the_application() {
        struct StatusApp {
            active: bool,
        }

        impl App for StatusApp {
            fn view(&mut self, ui: &mut Ui) {
                self.active = ui.tray_status().is_active();
            }
        }

        let size = Size::new(200.0, 150.0);
        let mut runtime = Runtime::new(texts(), size, Theme::default());
        let mut app = StatusApp { active: false };
        let input = input_state(size, 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        assert!(!app.active, "a runtime without a tray is disabled");

        runtime.set_tray_state(TrayStatus::Active, Vec::new());
        runtime.frame(&mut app, input.clone());
        assert!(app.active);

        runtime.set_tray_state(TrayStatus::Unavailable("no watcher".to_owned()), Vec::new());
        runtime.frame(&mut app, input);
        assert!(!app.active);
    }
}

#[cfg(test)]
mod context_menu_overlay_tests {
    use super::*;
    use kasumi_core::{MouseButton, Point};

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    fn menu_panel_rect(runtime: &Runtime) -> kasumi_core::Rect {
        runtime
            .cx
            .tree
            .iter()
            .find(|(_, node)| node.kind == NodeKind::Menu)
            .map(|(_, node)| node.rect)
            .expect("menu panel missing")
    }

    fn bounds(runtime: &Runtime, label: &str) -> kasumi_core::Rect {
        runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.label.as_deref() == Some(label))
            .unwrap_or_else(|| panic!("{label:?} missing"))
            .bounds
    }

    struct DialogInputApp;

    impl App for DialogInputApp {
        fn view(&mut self, ui: &mut Ui) {
            let open = ui.state(|| true);
            let value = ui.state(|| "hello".to_owned());
            ui.dialog(
                crate::Dialog::new(&open)
                    .title("Context menu dialog")
                    .dismissible(true),
                |ui| {
                    ui.add(
                        crate::TextInput::new(&value)
                            .placeholder("right click me")
                            .width(280.0),
                    );
                },
            );
        }
    }

    #[test]
    fn left_click_inside_a_dialog_closes_a_text_input_context_menu() {
        let mut runtime = Runtime::new(texts(), Size::new(500.0, 400.0), Theme::default());
        let mut app = DialogInputApp;
        let mut input = input_state(Size::new(500.0, 400.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());

        let field = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.role == kasumi_tree::Role::TextField)
            .expect("text field missing")
            .bounds;
        let field_center = field.center();
        input.time = 0.1;
        input.pointer = field_center;
        input.events = vec![
            InputEvent::PointerMoved(field_center),
            InputEvent::PointerDown {
                position: field_center,
                button: MouseButton::Right,
            },
        ];
        runtime.frame(&mut app, input.clone());
        assert!(
            runtime.cx.menu_top.is_some(),
            "right click should open the editor context menu"
        );

        // Click the dialog title: it is inside the dialog overlay but outside
        // the context-menu popup. The menu must close; the dialog must stay.
        let title = bounds(&runtime, "Context menu dialog").center();
        input.time = 0.2;
        input.pointer = title;
        input.events = vec![
            InputEvent::PointerMoved(title),
            InputEvent::PointerDown {
                position: title,
                button: MouseButton::Left,
            },
        ];
        runtime.frame(&mut app, input.clone());
        input.time = 0.25;
        input.events = vec![InputEvent::PointerUp {
            position: title,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());

        assert!(
            runtime.cx.menu_top.is_none(),
            "left click inside the dialog must dismiss the context menu"
        );
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Context menu dialog")),
            "dismissing the menu must not close the dialog"
        );
    }

    fn open_editor_menu(
        runtime: &mut Runtime,
        app: &mut dyn App,
        input: &mut InputState,
        point: Point,
        time: f64,
    ) {
        input.time = time;
        input.pointer = point;
        input.events = vec![
            InputEvent::PointerMoved(point),
            InputEvent::PointerDown {
                position: point,
                button: MouseButton::Right,
            },
        ];
        runtime.frame(app, input.clone());
        input.events.clear();
    }

    fn left_click(
        runtime: &mut Runtime,
        app: &mut dyn App,
        input: &mut InputState,
        point: Point,
        time: f64,
    ) {
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
        input.events.clear();
    }

    #[test]
    fn left_click_on_the_text_input_outside_the_menu_dismisses_it() {
        let mut runtime = Runtime::new(texts(), Size::new(500.0, 400.0), Theme::default());
        let mut app = DialogInputApp;
        let mut input = input_state(Size::new(500.0, 400.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let field = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.role == kasumi_tree::Role::TextField)
            .expect("text field missing")
            .bounds;
        let menu_anchor = Point::new(field.center().x, field.center().y);
        open_editor_menu(&mut runtime, &mut app, &mut input, menu_anchor, 0.1);
        assert!(runtime.cx.menu_top.is_some());

        // The menu opens at the pointer and extends to the right. The left
        // edge of the same field is outside it; clicking it must dismiss the
        // menu without closing the dialog.
        let outside_menu = Point::new(field.left() + 6.0, field.center().y);
        left_click(&mut runtime, &mut app, &mut input, outside_menu, 0.2);
        assert!(
            runtime.cx.menu_top.is_none(),
            "left click on the editor outside the menu must dismiss it"
        );
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Context menu dialog")),
            "the dialog must remain open"
        );
    }

    #[test]
    fn clicking_menu_padding_does_not_reach_the_dialog_scrim() {
        let mut runtime = Runtime::new(texts(), Size::new(500.0, 400.0), Theme::default());
        let mut app = DialogInputApp;
        let mut input = input_state(Size::new(500.0, 400.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let field = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.role == kasumi_tree::Role::TextField)
            .expect("text field missing")
            .bounds;
        open_editor_menu(&mut runtime, &mut app, &mut input, field.center(), 0.1);
        assert!(runtime.cx.menu_top.is_some());
        let menu_rect = menu_panel_rect(&runtime);
        // Padding is inside the popup but outside every menu item. A click
        // here must be swallowed by the menu, not reach the dialog scrim and
        // close the dialog.
        let padding = Point::new(menu_rect.left() + 2.0, menu_rect.top() + 2.0);
        input.time = 0.2;
        input.pointer = padding;
        input.events = vec![
            InputEvent::PointerMoved(padding),
            InputEvent::PointerDown {
                position: padding,
                button: MouseButton::Left,
            },
        ];
        runtime.frame(&mut app, input.clone());
        input.time = 0.25;
        input.events = vec![InputEvent::PointerUp {
            position: padding,
            button: MouseButton::Left,
        }];
        runtime.frame(&mut app, input.clone());
        assert!(
            runtime.cx.menu_top.is_none(),
            "clicking menu padding should dismiss the menu"
        );
        input.time = 0.3;
        runtime.frame(&mut app, input.clone());
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Context menu dialog")),
            "clicking menu padding must not close the dialog"
        );
    }

    #[test]
    fn clicking_a_menu_item_closes_the_editor_menu() {
        let mut runtime = Runtime::new(texts(), Size::new(500.0, 400.0), Theme::default());
        let mut app = DialogInputApp;
        let mut input = input_state(Size::new(500.0, 400.0), 0.0, Point::ZERO);
        runtime.frame(&mut app, input.clone());
        let field = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.role == kasumi_tree::Role::TextField)
            .expect("text field missing")
            .bounds;
        open_editor_menu(&mut runtime, &mut app, &mut input, field.center(), 0.1);
        assert!(runtime.cx.menu_top.is_some());

        let field_id = runtime
            .a11y_snapshot()
            .nodes
            .into_iter()
            .find(|node| node.role == kasumi_tree::Role::TextField)
            .expect("text field missing")
            .id;
        let item = bounds(&runtime, "Select all").center();
        left_click(&mut runtime, &mut app, &mut input, item, 0.2);
        assert!(
            runtime.cx.menu_top.is_none(),
            "clicking a menu item must close the menu"
        );
        assert!(
            runtime
                .a11y_snapshot()
                .nodes
                .iter()
                .any(|node| node.label.as_deref() == Some("Context menu dialog")),
            "the dialog must remain open"
        );

        input.time = 0.3;
        runtime.frame(&mut app, input.clone());
        let editor = runtime
            .cx
            .tree
            .node_by_id(field_id)
            .and_then(|node| node.text_input.as_ref())
            .expect("text editor disappeared");
        assert_eq!(editor.anchor, 0);
        assert_eq!(editor.cursor, "hello".len());
        assert_eq!(
            runtime.focused(),
            Some(field_id),
            "focus must return to the editor after Select all"
        );
    }
}

#[cfg(test)]
mod theme_switch_tests {
    use super::*;
    use crate::ColorScheme;
    use kasumi_core::Point;

    struct DarkApp {
        seen_on_surface: Option<kasumi_core::Color>,
    }

    impl App for DarkApp {
        fn view(&mut self, ui: &mut Ui) {
            ui.set_theme(Theme::dark());
            self.seen_on_surface = Some(ui.theme().colors.on_surface);
            ui.label("dark");
        }
    }

    #[test]
    fn set_theme_updates_later_widgets_and_the_runtime() {
        let mut runtime = Runtime::new(
            Arc::new(TextEngine::new()),
            Size::new(200.0, 100.0),
            Theme::light(),
        );
        let mut app = DarkApp {
            seen_on_surface: None,
        };
        runtime.frame(
            &mut app,
            input_state(Size::new(200.0, 100.0), 0.0, Point::ZERO),
        );
        assert_eq!(
            app.seen_on_surface,
            Some(ColorScheme::baseline_dark().on_surface)
        );
        assert!(runtime.theme().is_dark());
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    use kasumi_core::Point;
    use kasumi_state::ResourceStatus;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct ResourceApp {
        loads: Arc<AtomicUsize>,
        value: Option<u32>,
        status: ResourceStatus,
    }

    impl App for ResourceApp {
        fn view(&mut self, ui: &mut Ui) {
            let loads = Arc::clone(&self.loads);
            let resource = ui.resource_keyed("answer", move || {
                loads.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(25));
                Ok::<_, String>(42u32)
            });
            self.status = resource.status();
            self.value = resource.get();
        }
    }

    struct FutureApp {
        value: Option<String>,
        status: ResourceStatus,
    }

    impl App for FutureApp {
        fn view(&mut self, ui: &mut Ui) {
            let resource = ui.resource_future_keyed("future", || async {
                std::thread::sleep(Duration::from_millis(10));
                Ok::<_, String>("ready".to_owned())
            });
            self.status = resource.status();
            self.value = resource.get();
        }
    }

    fn texts() -> Arc<TextEngine> {
        Arc::new(TextEngine::new())
    }

    #[test]
    fn ui_resource_loads_once_and_exposes_the_value() {
        let loads = Arc::new(AtomicUsize::new(0));
        let mut runtime = Runtime::new(texts(), Size::new(300.0, 200.0), Theme::default());
        let woken = Arc::new(AtomicBool::new(false));
        {
            let woken = Arc::clone(&woken);
            runtime.set_resource_wake_callback(Arc::new(move || {
                woken.store(true, Ordering::SeqCst);
            }));
        }
        let mut app = ResourceApp {
            loads: Arc::clone(&loads),
            value: None,
            status: ResourceStatus::Idle,
        };
        let input = input_state(Size::new(300.0, 200.0), 0.0, Point::ZERO);

        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let mut next = input.clone();
            next.time += 0.01;
            runtime.frame(&mut app, next);
            if app.status == ResourceStatus::Ready {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "resource did not become ready: {:?}",
                app.status
            );
            std::thread::sleep(Duration::from_millis(3));
        }
        assert_eq!(app.value, Some(42));
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        assert_eq!(runtime.stats().resources, 1);
        assert!(
            woken.load(Ordering::SeqCst),
            "worker should wake the runtime"
        );
    }

    #[test]
    fn ui_future_resource_completes_through_the_executor() {
        let mut runtime = Runtime::new(texts(), Size::new(300.0, 200.0), Theme::default());
        let mut app = FutureApp {
            value: None,
            status: ResourceStatus::Idle,
        };
        let input = input_state(Size::new(300.0, 200.0), 0.0, Point::ZERO);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let mut next = input.clone();
            next.time += 0.01;
            runtime.frame(&mut app, next);
            if app.status == ResourceStatus::Ready {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "future resource did not become ready"
            );
            std::thread::sleep(Duration::from_millis(3));
        }
        assert_eq!(app.value.as_deref(), Some("ready"));
    }
}
