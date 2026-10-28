//! The immediate-mode facade.
//!
//! Everything the user touches is `&mut Ui`. The retained tree, state table and
//! interaction bookkeeping stay inside `Cx`.

use std::hash::Hash;
use std::time::Duration;

use kasumi_core::{
    Align, Corners, CursorIcon, Edges, GridTrack, Id, InputState, Inset, Justify, Key, Length,
    Size, Style, TextContent, TextStyle, child_id, keyed_id, state_id,
};
use kasumi_paint::TextEngine;
use kasumi_state::State;
use kasumi_tree::{Animation, NodeId, NodeKind, Sense};

use crate::cx::Cx;
use crate::response::Response;
use crate::theme::Theme;
use crate::widgets::{
    Button, Checkbox, ComboBox, Divider, Image, Label, Markdown, ProgressBar, Radio, ScrollView,
    Slider, Spacer, Svg, TextInput,
};

/// A user-defined widget. The only required surface is `ui`, which is called
/// every frame during `view`.
pub trait Widget {
    type Output;
    fn ui(self, ui: &mut Ui) -> Self::Output;
}

/// The handle passed to `App::view`.
///
/// It borrows per-frame context and is intentionally not `Clone`; anything that
/// must outlive the frame is a `State<T>` handle instead.
pub struct Ui<'a> {
    pub(crate) cx: &'a mut Cx,
    pub(crate) node: NodeId,
    pub(crate) scope: Id,
    pub(crate) next_child: u32,
    pub(crate) next_state: u32,
    /// Style of the current container. Helpers buffer into this value; it is
    /// committed once when the container's closure returns, so a frame that
    /// writes the same style does not invalidate layout.
    pub(crate) style: Style,
}

impl<'a> Ui<'a> {
    pub(crate) fn new(cx: &'a mut Cx, node: NodeId, scope: Id, style: Style) -> Self {
        Self {
            cx,
            node,
            scope,
            next_child: 0,
            next_state: 0,
            style,
        }
    }

    /// Id of the current container. Animation and `Response` helpers use it.
    pub fn id(&self) -> Id {
        self.cx
            .tree
            .get(self.node)
            .map(|node| node.id)
            .unwrap_or(Id::ROOT)
    }

    pub fn frame(&self) -> u64 {
        self.cx.frame
    }

    pub fn time(&self) -> f64 {
        self.cx.input.time
    }

    pub fn viewport(&self) -> Size {
        self.cx.viewport()
    }

    pub fn theme(&self) -> &Theme {
        &self.cx.theme
    }

    /// Input state for this frame. The raw event list has already been
    /// consumed by the runtime; this is useful for pointer position and
    /// modifiers in custom widgets.
    pub fn input(&self) -> &InputState {
        &self.cx.input
    }

    pub fn texts(&self) -> &TextEngine {
        &self.cx.texts
    }

    /// True once for every key-down event in this frame.
    pub fn key_pressed(&self, key: &Key) -> bool {
        self.cx.keys.iter().any(|event| &event.key == key)
    }

    pub fn key_repeated(&self, key: &Key) -> bool {
        self.cx
            .keys
            .iter()
            .any(|event| &event.key == key && event.repeat)
    }

    pub fn key_modifiers(&self, key: &Key) -> Option<kasumi_core::Modifiers> {
        self.cx
            .keys
            .iter()
            .find(|event| &event.key == key)
            .map(|event| event.modifiers)
    }

    pub(crate) fn key_pressed_space_when(&self, id: Id) -> bool {
        self.cx.focused == Some(id) && self.key_pressed(&Key::Space)
    }

    pub fn request_repaint(&mut self) {
        self.cx.repaint = true;
    }

    // ------------------------------------------------------------------
    // Identity and reconciliation
    // ------------------------------------------------------------------

    pub(crate) fn next_id(&mut self, origin: &'static str) -> Id {
        let id = child_id(self.scope, self.next_child);
        self.next_child += 1;
        self.register_node_id(id, origin);
        id
    }

    pub(crate) fn register_node_id(&mut self, id: Id, origin: &'static str) {
        #[cfg(debug_assertions)]
        if let Some(previous) = self.cx.seen_ids.insert(id, origin) {
            panic!(
                "kasumi: duplicate element id {id} in frame {}: already declared by `{previous}`, \
                 declared again by `{origin}`. Two widgets produced the same id; wrap conditional \
                 or reordered children in `ui.keyed(key, ..)`.",
                self.cx.frame
            );
        }
        #[cfg(not(debug_assertions))]
        let _ = (id, origin);
    }

    pub(crate) fn adopt(&mut self, id: Id, kind: NodeKind, style: Style) -> NodeId {
        let node = self.cx.tree.ensure(id, kind);
        self.cx.tree.declare_child(self.node, node);
        self.cx.tree.set_style(node, style);
        node
    }

    /// Groups children under a stable key so their ids survive reordering.
    pub fn keyed<K: Hash>(&mut self, key: K, f: impl FnOnce(&mut Ui)) {
        let scope = keyed_id(self.scope, &key);
        let mut child = Ui {
            cx: &mut *self.cx,
            node: self.node,
            scope,
            next_child: 0,
            next_state: 0,
            style: self.style.clone(),
        };
        f(&mut child);
        self.style = child.style;
    }

    /// Adds a widget. This is the builder counterpart to the `ui.label` /
    /// `ui.button` sugar.
    pub fn add<W: Widget>(&mut self, widget: W) -> W::Output {
        widget.ui(self)
    }

    // ------------------------------------------------------------------
    // Containers
    // ------------------------------------------------------------------

    /// A vertical flex container.
    pub fn column(&mut self, f: impl FnOnce(&mut Ui)) -> Response {
        self.container_with("column", NodeKind::Container, Style::column(), f)
    }

    /// A horizontal flex container.
    pub fn row(&mut self, f: impl FnOnce(&mut Ui)) -> Response {
        self.container_with("row", NodeKind::Container, Style::row(), f)
    }

    /// A neutral flex container with absolutely no layout defaults.
    pub fn container(&mut self, f: impl FnOnce(&mut Ui)) -> Response {
        self.container_with("container", NodeKind::Container, Style::default(), f)
    }

    /// A centered column, useful for empty states and dialogs.
    pub fn center(&mut self, f: impl FnOnce(&mut Ui)) -> Response {
        self.container_with(
            "center",
            NodeKind::Container,
            Style {
                direction: kasumi_core::FlexDirection::Column,
                align_items: Align::Center,
                justify: Justify::Center,
                ..Style::default()
            },
            f,
        )
    }

    /// A wrapping row.
    pub fn wrap_row(&mut self, f: impl FnOnce(&mut Ui)) -> Response {
        self.container_with(
            "wrap row",
            NodeKind::Container,
            Style {
                wrap: true,
                ..Style::row()
            },
            f,
        )
    }

    /// A grid with equal fractional columns.
    pub fn grid(&mut self, columns: usize, f: impl FnOnce(&mut Ui)) -> Response {
        self.grid_with(std::iter::repeat_n(GridTrack::fr(1.0), columns.max(1)), f)
    }

    /// A grid with explicit tracks.
    pub fn grid_with(
        &mut self,
        columns: impl IntoIterator<Item = GridTrack>,
        f: impl FnOnce(&mut Ui),
    ) -> Response {
        let mut style = Style::grid(columns);
        style.align_items = Align::Stretch;
        self.container_with("grid", NodeKind::Container, style, f)
    }

    /// Layers all direct children in one grid cell.
    pub fn stack(&mut self, f: impl FnOnce(&mut Ui)) -> Response {
        self.container_with("stack", NodeKind::Stack, Style::default(), f)
    }

    fn container_with(
        &mut self,
        origin: &'static str,
        kind: NodeKind,
        style: Style,
        f: impl FnOnce(&mut Ui),
    ) -> Response {
        let id = self.next_id(origin);
        let node = self.cx.tree.ensure(id, kind);
        self.cx.tree.declare_child(self.node, node);
        {
            let mut child = Ui {
                cx: &mut *self.cx,
                node,
                scope: id,
                next_child: 0,
                next_state: 0,
                style,
            };
            f(&mut child);
            let style = child.style;
            child.cx.tree.set_style(node, style);
        }
        self.cx.response(id)
    }

    /// A scroll container with retained, spring-based smooth scrolling.
    pub fn scroll(&mut self, config: ScrollView, f: impl FnOnce(&mut Ui)) -> Response {
        let id = self.next_id("scroll view");
        let outer_style = Style {
            width: config.width,
            height: config.height,
            min_width: Length::Px(0.0),
            min_height: Length::Px(0.0),
            flex_grow: config.grow,
            align_self: Align::Start,
            align_items: Align::Stretch,
            clip: true,
            scroll_x: config.horizontal,
            scroll_y: config.vertical,
            ..Style::default()
        };
        let inner_style = Style {
            direction: config.direction(),
            width: Length::Auto,
            height: if config.vertical {
                Length::Auto
            } else {
                Length::Percent(100.0)
            },
            align_items: Align::Auto,
            align_self: Align::Start,
            flex_grow: if config.vertical { 0.0 } else { 1.0 },
            ..Style::default()
        };
        let node = self.cx.tree.ensure(id, NodeKind::Container);
        self.cx.tree.declare_child(self.node, node);
        self.cx.tree.set_style(node, outer_style);

        // The viewport owns retained scroll state. Its content child inherits
        // the natural size of the caller's widgets, like Iced's `bounds` and
        // `content` split.
        let content_id = child_id(id, 0);
        self.register_node_id(content_id, "scroll content");
        let content = self.cx.tree.ensure(content_id, NodeKind::Container);
        self.cx.tree.declare_child(node, content);
        {
            let mut child = Ui {
                cx: &mut *self.cx,
                node: content,
                scope: content_id,
                next_child: 0,
                next_state: 0,
                style: inner_style,
            };
            f(&mut child);
            let mut style = child.style;
            style.min_height = Length::Px(0.0);
            child.cx.tree.set_style(content, style);
            child
                .cx
                .tree
                .set_sense(node, Sense::hover().union(Sense::FOCUS));
        }
        if let Some(node) = self.cx.tree.get_mut(node) {
            node.scroll.smooth = config.smooth;
        }
        self.cx.response(id)
    }

    /// An overlay positioned relative to the root. It is declared after the
    /// current subtree, so it paints above ordinary page content.
    pub(crate) fn overlay_at<K: Hash>(
        &mut self,
        key: K,
        origin: &'static str,
        _owner: Id,
        inset: Inset,
        style: Style,
        f: impl FnOnce(&mut Ui),
    ) -> (Response, Id) {
        let id = keyed_id(self.scope, &key);
        self.register_node_id(id, origin);
        let root = self.cx.tree.root();
        let node = self.cx.tree.ensure(id, NodeKind::Overlay);
        self.cx.tree.declare_child(root, node);
        let mut style = style;
        style.position = kasumi_core::Position::Absolute;
        style.inset = inset;
        {
            let mut child = Ui {
                cx: &mut *self.cx,
                node,
                scope: id,
                next_child: 0,
                next_state: 0,
                style,
            };
            f(&mut child);
            let style = child.style;
            child.cx.tree.set_style(node, style);
        }
        (self.cx.response(id), id)
    }

    // ------------------------------------------------------------------
    // Widgets
    // ------------------------------------------------------------------

    pub fn label(&mut self, text: impl Into<String>) -> Response {
        Label::new(text).ui(self)
    }

    pub fn button(&mut self, text: impl Into<String>) -> Response {
        Button::new(text).ui(self)
    }

    pub fn spacer(&mut self) -> Response {
        Spacer.ui(self)
    }

    /// A single-line text field bound to `State<String>`.
    pub fn text_input(&mut self, value: &State<String>) -> Response {
        TextInput::new(value).ui(self)
    }

    pub fn image(&mut self, image: Image) -> Response {
        image.ui(self)
    }

    pub fn svg(&mut self, svg: Svg) -> Response {
        svg.ui(self)
    }

    pub fn markdown(&mut self, markdown: impl Into<String>) -> Response {
        Markdown::new(markdown).ui(self)
    }

    pub fn checkbox(&mut self, label: impl Into<String>, checked: &State<bool>) -> Response {
        Checkbox::new(label, checked).ui(self)
    }

    pub fn radio<T: Clone + PartialEq + 'static>(
        &mut self,
        label: impl Into<String>,
        value: T,
        selected: &State<T>,
    ) -> Response {
        Radio::new(label, value, selected).ui(self)
    }

    pub fn slider(&mut self, value: &State<f32>, min: f32, max: f32) -> Response {
        Slider::new(value, min, max).ui(self)
    }

    pub fn combo_box(
        &mut self,
        options: impl IntoIterator<Item = impl Into<String>>,
        selected: &State<usize>,
    ) -> Response {
        ComboBox::new(options, selected).ui(self)
    }

    pub fn divider(&mut self) -> Response {
        Divider::horizontal().ui(self)
    }

    pub fn progress(&mut self, value: &State<f32>) -> Response {
        ProgressBar::new(value).ui(self)
    }

    /// Shows a lightweight tooltip overlay while `owner` is hovered.
    pub fn tooltip(&mut self, owner: &Response, text: impl Into<String>) {
        if !owner.hovered() {
            return;
        }
        let text = text.into();
        if text.is_empty() {
            return;
        }
        let theme = self.theme().clone();
        let viewport = self.viewport();
        let width =
            (text.chars().count() as f32 * theme.font_size * 0.58 + 20.0).clamp(48.0, 320.0);
        let pointer = self.cx.input.pointer;
        let x = (pointer.x + 14.0).min((viewport.width - width - 6.0).max(6.0));
        let y = (pointer.y + 18.0).min((viewport.height - 34.0).max(6.0));
        self.overlay_at(
            ("tooltip", owner.id().raw()),
            "tooltip",
            owner.id(),
            Inset::at(x, y),
            Style {
                width: Length::Px(width),
                height: Length::Px(28.0),
                padding: Edges::xy(9.0, 0.0),
                align_items: Align::Center,
                background: theme.panel,
                border_color: theme.border_strong,
                border_width: 1.0,
                radius: Corners::all(theme.radius_small),
                ..Style::default()
            },
            |ui| {
                ui.add(
                    Label::new(text)
                        .font_size(theme.font_size_small)
                        .color(theme.text_muted),
                );
            },
        );
    }

    /// Attaches a retained context menu to a widget response. Call this every
    /// frame next to the owning widget; the menu declares its own root overlay
    /// only while open.
    pub fn context_menu(&mut self, owner: &Response, f: impl FnOnce(&mut Ui)) {
        if owner.context_clicked() {
            let position = self.cx.input.pointer;
            self.cx.context_menu = Some(crate::cx::ContextMenuState {
                owner: owner.id(),
                position,
                root: None,
            });
        }
        let Some(menu) = self.cx.context_menu else {
            return;
        };
        if menu.owner != owner.id() {
            return;
        }

        let width = 196.0;
        let viewport = self.viewport();
        let x = menu.position.x.min((viewport.width - width - 8.0).max(8.0));
        let y = menu.position.y.min((viewport.height - 48.0).max(8.0));
        let theme = self.theme().clone();
        let (_, popup_id) = self.overlay_at(
            ("context-menu", owner.id().raw()),
            "context menu",
            owner.id(),
            Inset::at(x, y),
            Style {
                direction: kasumi_core::FlexDirection::Column,
                width: Length::Px(width),
                max_height: Length::Px((viewport.height - y - 8.0).max(48.0)),
                padding: Edges::all(6.0),
                gap: 2.0,
                background: theme.panel,
                border_color: theme.border_strong,
                border_width: 1.0,
                radius: Corners::all(theme.radius_medium),
                ..Style::default()
            },
            f,
        );
        if let Some(menu) = self.cx.context_menu.as_mut() {
            menu.root = Some(popup_id);
        }
        // The item has already run its action during `f`. Closing after the
        // subtree is declared keeps that action in this frame while ensuring
        // the popup is absent from the next one.
        if let Some(clicked) = self.cx.clicked
            && self.cx.tree.is_within(clicked, popup_id)
        {
            self.cx.context_menu = None;
            self.cx.repaint = true;
        }
        if self.key_pressed(&Key::Escape) {
            self.cx.context_menu = None;
        }
    }

    // ------------------------------------------------------------------
    // State
    // ------------------------------------------------------------------

    /// Returns a cloneable handle to a value that survives frames.
    pub fn state<T: 'static>(&mut self, init: impl Fn() -> T + 'static) -> State<T> {
        let index = self.next_state;
        self.next_state += 1;
        let id = state_id(self.scope, index);
        let path = self
            .cx
            .state_paths
            .entry(id)
            .or_insert_with(|| format!("{:016x}::state[{index}]", self.scope.raw()).into())
            .clone();
        State::new(self.cx.state.clone(), id, path, init)
    }

    /// Like [`Ui::state`], but exempt from TTL eviction.
    pub fn state_persistent<T: 'static>(&mut self, init: impl Fn() -> T + 'static) -> State<T> {
        self.state(init).persistent()
    }

    // ------------------------------------------------------------------
    // Animation
    // ------------------------------------------------------------------

    /// Animates a scalar attached to the current container and returns its
    /// current value. The first call for a node starts from `0.0`; a change of
    /// target restarts the interpolation from the value visible now. Easing is
    /// fixed to smoothstep.
    pub fn animate(&mut self, target: f32, duration: Duration) -> f32 {
        let now = self.cx.input.time;
        let seconds = duration.as_secs_f64();
        let Some(node) = self.cx.tree.get_mut(self.node) else {
            return target;
        };
        let running;
        let value = match node.animation.as_mut() {
            Some(animation) => {
                if (animation.to - target).abs() > 0.000_1 {
                    let from = animation.sample(now);
                    animation.from = from;
                    animation.to = target;
                    animation.start_time = now;
                    animation.duration = seconds;
                    animation.value = from;
                }
                let value = animation.sample(now);
                running = animation.is_running(now);
                value
            }
            None => {
                let mut animation = Animation {
                    from: 0.0,
                    to: target,
                    start_time: now,
                    duration: seconds,
                    value: 0.0,
                };
                let value = animation.sample(now);
                running = animation.is_running(now);
                node.animation = Some(animation);
                value
            }
        };
        if running {
            self.cx.repaint = true;
        }
        self.cx.animations_running |= running;
        value
    }

    // ------------------------------------------------------------------
    // Current-container style helpers
    // ------------------------------------------------------------------

    fn edit_style(&mut self, f: impl FnOnce(&mut Style)) -> &mut Self {
        f(&mut self.style);
        self
    }

    /// Commits the buffered root style. Called by the runtime after `view`.
    pub(crate) fn commit_style(&mut self) {
        let style = std::mem::take(&mut self.style);
        self.cx.tree.set_style(self.node, style);
    }

    pub fn background(&mut self, color: kasumi_core::Color) -> &mut Self {
        self.edit_style(|style| {
            style.set_background(color);
        })
    }

    pub fn border(&mut self, width: f32, color: kasumi_core::Color) -> &mut Self {
        self.edit_style(|style| {
            style.set_border(width, color);
        })
    }

    pub fn radius(&mut self, radius: f32) -> &mut Self {
        self.edit_style(|style| {
            style.set_radius(radius);
        })
    }

    pub fn corners(&mut self, corners: Corners) -> &mut Self {
        self.edit_style(|style| style.radius = corners)
    }

    pub fn padding(&mut self, value: f32) -> &mut Self {
        self.edit_style(|style| {
            style.set_padding(value);
        })
    }

    pub fn padding_xy(&mut self, x: f32, y: f32) -> &mut Self {
        self.edit_style(|style| {
            style.set_padding_xy(x, y);
        })
    }

    pub fn padding_edges(&mut self, edges: Edges) -> &mut Self {
        self.edit_style(|style| style.padding = edges)
    }

    pub fn margin(&mut self, value: f32) -> &mut Self {
        self.edit_style(|style| style.margin = Edges::all(value))
    }

    pub fn gap(&mut self, value: f32) -> &mut Self {
        self.edit_style(|style| {
            style.set_gap(value);
        })
    }

    pub fn width(&mut self, width: impl Into<Length>) -> &mut Self {
        self.edit_style(|style| {
            style.set_width(width.into());
        })
    }

    pub fn height(&mut self, height: impl Into<Length>) -> &mut Self {
        self.edit_style(|style| {
            style.set_height(height.into());
        })
    }

    pub fn min_width(&mut self, width: impl Into<Length>) -> &mut Self {
        self.edit_style(|style| style.min_width = width.into())
    }

    pub fn min_height(&mut self, height: impl Into<Length>) -> &mut Self {
        self.edit_style(|style| style.min_height = height.into())
    }

    pub fn max_width(&mut self, width: impl Into<Length>) -> &mut Self {
        self.edit_style(|style| style.max_width = width.into())
    }

    pub fn size_full(&mut self) -> &mut Self {
        self.edit_style(|style| {
            style.set_size_full();
        })
    }

    pub fn width_full(&mut self) -> &mut Self {
        self.edit_style(|style| style.width = Length::Percent(100.0))
    }

    pub fn height_full(&mut self) -> &mut Self {
        self.edit_style(|style| style.height = Length::Percent(100.0))
    }

    pub fn align_items(&mut self, align: Align) -> &mut Self {
        self.edit_style(|style| {
            style.set_align_items(align);
        })
    }

    pub fn align_self(&mut self, align: Align) -> &mut Self {
        self.edit_style(|style| style.align_self = align)
    }

    pub fn justify(&mut self, justify: Justify) -> &mut Self {
        self.edit_style(|style| {
            style.set_justify(justify);
        })
    }

    pub fn grow(&mut self, grow: f32) -> &mut Self {
        self.edit_style(|style| {
            style.set_grow(grow);
        })
    }

    pub fn wrap(&mut self, wrap: bool) -> &mut Self {
        self.edit_style(|style| style.wrap = wrap)
    }

    pub fn grid_columns(&mut self, columns: impl IntoIterator<Item = GridTrack>) -> &mut Self {
        self.edit_style(|style| {
            style.layout = kasumi_core::LayoutMode::Grid;
            style.grid_columns = columns.into_iter().collect();
        })
    }

    pub fn grid_span(&mut self, columns: u16, rows: u16) -> &mut Self {
        self.edit_style(|style| {
            style.grid_column_span = columns.max(1);
            style.grid_row_span = rows.max(1);
        })
    }

    pub fn absolute(&mut self, x: f32, y: f32) -> &mut Self {
        self.edit_style(|style| {
            style.position = kasumi_core::Position::Absolute;
            style.inset = Inset::at(x, y);
        })
    }

    pub fn scrollable(&mut self, horizontal: bool, vertical: bool) -> &mut Self {
        self.edit_style(|style| {
            style.scroll_x = horizontal;
            style.scroll_y = vertical;
            style.clip = horizontal || vertical || style.clip;
        })
    }

    pub fn clip(&mut self) -> &mut Self {
        self.edit_style(|style| style.clip = true)
    }

    pub fn cursor(&mut self, cursor: CursorIcon) -> &mut Self {
        self.edit_style(|style| style.cursor = cursor)
    }

    pub fn focus_ring(&mut self, color: kasumi_core::Color) -> &mut Self {
        self.edit_style(|style| style.focus_ring = color)
    }

    pub fn hidden(&mut self, hidden: bool) -> &mut Self {
        self.edit_style(|style| style.hidden = hidden)
    }

    // ------------------------------------------------------------------
    // Internal helpers used by widgets
    // ------------------------------------------------------------------

    pub(crate) fn declare_child(&mut self, parent: NodeId, child: NodeId) {
        self.cx.tree.declare_child(parent, child);
    }

    pub(crate) fn set_style_on(&mut self, node: NodeId, style: Style) {
        self.cx.tree.set_style(node, style);
    }

    pub(crate) fn set_text_on(&mut self, node: NodeId, text: String, style: TextStyle) {
        self.cx.tree.set_text(node, TextContent::new(text, style));
    }

    pub(crate) fn set_sense_on(&mut self, node: NodeId, sense: Sense) {
        self.cx.tree.set_sense(node, sense);
    }

    pub(crate) fn response_of(&self, id: Id) -> Response {
        self.cx.response(id)
    }
}
