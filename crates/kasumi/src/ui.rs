//! The immediate-mode facade.
//!
//! Everything the user touches is `&mut Ui`. The retained tree, state table and
//! interaction bookkeeping stay inside `Cx`.

use std::future::Future;
use std::hash::Hash;
use std::time::Duration;

use kasumi_core::{
    Align, Corners, CursorIcon, Edges, GridTrack, Id, InputState, Inset, Justify, Key, Length,
    Point, Size, Style, TextContent, TextStyle, Vec2, WindowKey, child_id, keyed_id, state_id,
};
use kasumi_paint::TextEngine;
use kasumi_state::{Resource, State};
use kasumi_tree::{Easing, MotionSpec, NodeId, NodeKind, Sense};

use crate::cx::Cx;
use crate::response::Response;
use crate::theme::Theme;
use crate::widgets::{
    Button, Card, Checkbox, ComboBox, Dialog, Divider, Image, Label, Markdown, ProgressBar, Radio,
    ScrollView, ScrollWindow, Slider, Spacer, Svg, TextInput,
};
use crate::window::{WindowAction, WindowInfo, WindowRequest};

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
    pub(crate) next_resource: u32,
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
            next_resource: 0,
            style,
        }
    }

    /// Marks the current container as a click target.
    ///
    /// Call this at the start of a container closure. The response returned by
    /// the container then reports `hovered`, `pressed`, `clicked`, `focused`
    /// and `context_clicked` like a button, while the caller keeps full layout
    /// control over the children. The container also gets a pointer cursor and
    /// the theme focus ring.
    ///
    /// ```ignore
    /// let response = ui.row(|ui| {
    ///     ui.clickable().padding_xy(12.0, 0.0).height(36.0);
    ///     ui.label("Row");
    /// });
    /// if response.clicked() {
    ///     // select
    /// }
    /// ```
    pub fn clickable(&mut self) -> &mut Self {
        let node = self.node;
        let focus_ring = self.cx.theme.colors.primary;
        self.cx.tree.set_sense(node, Sense::click());
        self.style.cursor = CursorIcon::Pointer;
        self.style.focus_ring = focus_ring;
        self
    }

    /// Whether the current container was hovered on the previous frame. Only
    /// meaningful inside a container that called [`Ui::clickable`].
    pub fn is_hovered(&self) -> bool {
        self.cx.hovered == Some(self.scope)
    }

    /// Whether the current container is being pressed this frame.
    pub fn is_pressed(&self) -> bool {
        self.cx.active == Some(self.scope) && self.cx.pointer_down
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

    /// The frame delta clamped for stable animation integration.
    pub fn delta_time(&self) -> f32 {
        self.cx.delta_time
    }

    pub fn viewport(&self) -> Size {
        self.cx.viewport()
    }

    /// Description of the window currently being rendered.
    pub fn window(&self) -> &WindowInfo {
        &self.cx.window
    }

    /// System-tray activations the backend delivered since the last frame.
    ///
    /// Empty when the application declared no tray. Entries are cleared when
    /// the next frame starts, so a frame sees each activation exactly once.
    pub fn tray_events(&self) -> &[crate::tray::TrayEvent] {
        &self.cx.tray_events
    }

    /// The tray's current status for this frame.
    pub fn tray_status(&self) -> crate::tray::TrayStatus {
        self.cx.tray_status.clone()
    }

    /// Asks the backend to replace the tray menu after this frame.
    pub fn set_tray_menu(&mut self, menu: crate::tray::TrayMenu) {
        self.cx.tray_menu = Some(menu);
        self.cx.repaint = true;
    }

    /// Every live window this frame, sorted by [`WindowKey`].
    ///
    /// The current window is always included. Newly opened windows appear on a
    /// later frame, after the backend has created them.
    pub fn windows(&self) -> &[WindowInfo] {
        &self.cx.windows
    }

    /// Asks the backend to open a new window after this frame.
    pub fn open_window(&mut self, request: WindowRequest) {
        self.cx.window_actions.push(WindowAction::Open(request));
    }

    /// Asks the backend to close `window`. Closing the current window ends its
    /// frame loop after presentation; the rest of this `view` still runs.
    pub fn close_window(&mut self, window: WindowKey) {
        self.cx.window_actions.push(WindowAction::Close(window));
    }

    pub fn focus_window(&mut self, window: WindowKey) {
        self.cx.window_actions.push(WindowAction::Focus(window));
    }

    pub fn set_window_title(&mut self, window: WindowKey, title: impl Into<String>) {
        self.cx.window_actions.push(WindowAction::SetTitle {
            window,
            title: title.into(),
        });
    }

    pub fn theme(&self) -> &Theme {
        &self.cx.theme
    }

    /// Replaces the active theme for this window.
    ///
    /// Widgets declared later in this frame use the new tokens immediately;
    /// nodes declared before the call keep the old styles until the next
    /// frame. The call requests that follow-up frame.
    pub fn set_theme(&mut self, theme: Theme) {
        self.cx.theme = theme;
        self.cx.repaint = true;
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

    /// Moves keyboard focus to `id`.
    ///
    /// Applications use this to focus a search field or dialog input they just
    /// declared, without waiting for a pointer press. The widget must be a
    /// focusable node (text input/editor, button, …).
    pub fn focus(&mut self, id: Id) {
        if self.cx.focused != Some(id) {
            self.cx.focused = Some(id);
            self.cx.repaint = true;
        }
    }

    /// Direct access to the retained visual transform for the current
    /// container. Call this during the container closure to combine transitions
    /// with custom effects.
    pub fn visual(&mut self) -> Option<&mut kasumi_tree::AnimationState> {
        self.cx.tree.get_mut(self.node).map(|node| &mut node.visual)
    }

    /// Sets the opacity used for this node and its descendants.
    pub fn opacity(&mut self, opacity: f32) -> &mut Self {
        if let Some(node) = self.cx.tree.get_mut(self.node) {
            node.visual.opacity.snap(opacity.clamp(0.0, 1.0));
        }
        self
    }

    pub fn scale(&mut self, scale: f32) -> &mut Self {
        if let Some(node) = self.cx.tree.get_mut(self.node) {
            node.visual.scale.snap(scale.max(0.0));
        }
        self
    }

    pub fn translate(&mut self, x: f32, y: f32) -> &mut Self {
        if let Some(node) = self.cx.tree.get_mut(self.node) {
            node.visual.translate[0].snap(x);
            node.visual.translate[1].snap(y);
        }
        self
    }

    pub fn transform_origin(&mut self, x: f32, y: f32) -> &mut Self {
        if let Some(node) = self.cx.tree.get_mut(self.node) {
            node.visual.origin = Some((x, y));
        }
        self
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
            next_resource: 0,
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
                next_resource: 0,
                style,
            };
            f(&mut child);
            let style = child.style;
            child.cx.tree.set_style(node, style);
        }
        self.cx.response(id)
    }

    fn scroll_outer_style(config: ScrollView) -> Style {
        Style {
            width: config.width,
            height: config.height,
            min_width: Length::Px(0.0),
            min_height: Length::Px(0.0),
            flex_grow: config.grow,
            flex_basis: if config.grow > 0.0 {
                Length::Px(0.0)
            } else {
                Length::Auto
            },
            align_self: Align::Start,
            align_items: Align::Stretch,
            clip: true,
            scroll_x: config.horizontal,
            scroll_y: config.vertical,
            ..Style::default()
        }
    }

    // The scroll axis must not be compressed by the viewport: content is laid
    // out at its natural extent and simply overflows along the scroll axis.
    // The cross axis stretches to the viewport for vertical-only scroll views
    // so percentage-sized children resolve against the visible width rather
    // than the max-content width of one subtree.
    fn scroll_inner_style(config: ScrollView) -> Style {
        Style {
            direction: config.direction(),
            width: if config.vertical && !config.horizontal {
                Length::Percent(100.0)
            } else {
                Length::Auto
            },
            height: if config.vertical {
                Length::Auto
            } else {
                Length::Percent(100.0)
            },
            min_width: Length::Px(0.0),
            min_height: Length::Px(0.0),
            align_items: Align::Auto,
            align_self: Align::Start,
            flex_grow: if config.vertical { 0.0 } else { 1.0 },
            flex_shrink: 0.0,
            ..Style::default()
        }
    }

    /// A scroll container with retained, Chromium-style wheel scrolling.
    pub fn scroll(&mut self, config: ScrollView, f: impl FnOnce(&mut Ui)) -> Response {
        let id = self.next_id("scroll view");
        let node = self.cx.tree.ensure(id, NodeKind::Container);
        self.cx.tree.declare_child(self.node, node);
        self.cx
            .tree
            .set_style(node, Self::scroll_outer_style(config));

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
                next_resource: 0,
                style: Self::scroll_inner_style(config),
            };
            f(&mut child);
            let mut style = child.style;
            style.min_width = Length::Px(0.0);
            style.min_height = Length::Px(0.0);
            style.flex_shrink = 0.0;
            child.cx.tree.set_style(content, style);
            child
                .cx
                .tree
                .set_sense(node, Sense::hover().union(Sense::FOCUS));
        }
        if let Some(node) = self.cx.tree.get_mut(node) {
            node.scroll.smooth = config.smooth;
            node.show_scrollbars = config.show_scrollbars;
        }
        self.cx.response(id)
    }

    /// A scroll container that lays out only the visible part of a declared
    /// content extent.
    ///
    /// Pass a [`ScrollView`] configured with
    /// [`ScrollView::virtual_content`] or [`ScrollView::virtual_rows`]. The
    /// closure receives the retained scroll offset and the viewport measured
    /// on the previous frame, and must only declare children that intersect
    /// that window. Position them yourself (usually absolutely) inside the
    /// content origin.
    ///
    /// This is the primitive behind [`crate::ListView`]; use that widget for
    /// fixed-height rows.
    ///
    /// ```ignore
    /// let view = ScrollView::vertical()
    ///     .grow(1.0)
    ///     .virtual_content(Size::new(0.0, 100_000.0));
    /// ui.virtual_scroll(view, |ui, window| {
    ///     for index in window.row_range(40.0, 2500) {
    ///         ui.keyed(index, |ui| {
    ///             ui.row(|ui| {
    ///                 ui.absolute(0.0, window.row_top(index, 40.0))
    ///                     .width_full()
    ///                     .height(40.0);
    ///             });
    ///         });
    ///     }
    /// });
    /// ```
    pub fn virtual_scroll(
        &mut self,
        config: ScrollView,
        f: impl FnOnce(&mut Ui, ScrollWindow),
    ) -> Response {
        let content_size = config.virtual_content.unwrap_or_else(|| {
            panic!(
                "kasumi: Ui::virtual_scroll requires ScrollView::virtual_content or \
                 ScrollView::virtual_rows; otherwise no scroll extent can be known"
            )
        });
        let id = self.next_id("virtual scroll view");
        let previous = self
            .cx
            .tree
            .node_by_id(id)
            .map(|node| (node.rect.size, node.scroll.offset));
        let node = self.cx.tree.ensure(id, NodeKind::Container);
        self.cx.tree.declare_child(self.node, node);
        self.cx
            .tree
            .set_style(node, Self::scroll_outer_style(config));

        let outer = self.viewport();
        let (viewport, offset) = match previous {
            Some((size, offset)) if size.width > 0.5 && size.height > 0.5 => (size, offset),
            _ => (
                Size::new(
                    config
                        .width
                        .resolve(Some(outer.width))
                        .unwrap_or(outer.width),
                    config
                        .height
                        .resolve(Some(outer.height))
                        .unwrap_or(outer.height),
                ),
                Point::ZERO,
            ),
        };

        let content_id = child_id(id, 0);
        self.register_node_id(content_id, "virtual scroll content");
        let content = self.cx.tree.ensure(content_id, NodeKind::Container);
        self.cx.tree.declare_child(node, content);

        let mut inner_style = Self::scroll_inner_style(config);
        inner_style.align_items = Align::Auto;
        if config.vertical && !config.horizontal {
            inner_style.width = Length::Percent(100.0);
            inner_style.height = Length::Px(content_size.height.max(0.0));
        } else if config.horizontal && !config.vertical {
            inner_style.width = Length::Px(content_size.width.max(0.0));
            inner_style.height = Length::Percent(100.0);
        } else {
            inner_style.width = Length::Px(content_size.width.max(0.0));
            inner_style.height = Length::Px(content_size.height.max(0.0));
        }
        let declared_width = inner_style.width;
        let declared_height = inner_style.height;

        {
            let mut child = Ui {
                cx: &mut *self.cx,
                node: content,
                scope: content_id,
                next_child: 0,
                next_state: 0,
                next_resource: 0,
                style: inner_style,
            };
            f(
                &mut child,
                ScrollWindow {
                    offset,
                    viewport,
                    content: content_size,
                    overscan: config.overscan,
                },
            );
            let mut style = child.style;
            // The virtual extent is authoritative: a closure that calls
            // `height_full` on the content must not collapse the scrollbar.
            style.width = declared_width;
            style.height = declared_height;
            style.min_width = Length::Px(0.0);
            style.min_height = Length::Px(0.0);
            style.flex_shrink = 0.0;
            child.cx.tree.set_style(content, style);
            child
                .cx
                .tree
                .set_sense(node, Sense::hover().union(Sense::FOCUS));
        }
        if let Some(node) = self.cx.tree.get_mut(node) {
            node.scroll.smooth = config.smooth;
            node.show_scrollbars = config.show_scrollbars;
        }
        self.cx.response(id)
    }

    /// Scrolls the container behind `response` to an absolute position.
    ///
    /// The position is clamped to the retained scroll range and any in-flight
    /// smooth animation is cancelled. Returns true when the offset changed.
    pub fn scroll_to(&mut self, response: &Response, position: Point) -> bool {
        let changed = self
            .cx
            .tree
            .scroll_to(response.id(), position, self.cx.input.time);
        if changed {
            self.cx.repaint = true;
        }
        changed
    }

    /// Adds a smooth scroll delta to the container behind `response`.
    pub fn scroll_by(&mut self, response: &Response, delta: Vec2) -> bool {
        let changed = self
            .cx
            .tree
            .scroll_by(response.id(), delta, self.cx.input.time);
        if changed {
            self.cx.repaint = true;
            self.cx.animations_running = true;
        }
        changed
    }

    /// Declares a themed card container and runs its content closure.
    ///
    /// The card is a column with padding and a gap; `config` selects the
    /// elevated, filled or outlined Material treatment. A clickable card gets
    /// a pointer cursor, keyboard focus and a previous-frame hover tint.
    ///
    /// ```ignore
    /// ui.card(Card::outlined().padding(20.0), |ui| {
    ///     ui.add(Label::new("Storage").font_size(16.0).semibold());
    ///     ui.add(Label::new("42 GB used").font_size(13.0));
    /// });
    /// ```
    pub fn card(&mut self, config: Card, f: impl FnOnce(&mut Ui)) -> Response {
        let id = self.next_id("card");
        let theme = self.theme().clone();
        let mut style = config.style(&theme);
        if config.clickable {
            style.cursor = CursorIcon::Pointer;
        }
        let node = self.cx.tree.ensure(id, NodeKind::Container);
        self.cx.tree.declare_child(self.node, node);
        self.cx.tree.set_style(node, style.clone());
        if config.clickable {
            self.cx.tree.set_sense(node, Sense::click());
        }
        {
            let mut child = Ui {
                cx: &mut *self.cx,
                node,
                scope: id,
                next_child: 0,
                next_state: 0,
                next_resource: 0,
                style,
            };
            f(&mut child);
            let style = child.style;
            child.cx.tree.set_style(node, style);
        }
        self.cx.response(id)
    }

    /// Shows a modal dialog above all content while its open state is true.
    ///
    /// The scrim covers the whole window and blocks pointer and wheel input
    /// from reaching the page below. When `dismissible` is set, Escape and a
    /// click on the scrim write `false` to the dialog's state. Focus trapping
    /// is intentionally left to the application; render focusable content in
    /// the body and manage focus with `State` if the flow requires it.
    ///
    /// Returns the dialog card's response, or `None` while closed.
    pub fn dialog(&mut self, config: Dialog, body: impl FnOnce(&mut Ui)) -> Option<Response> {
        if !config.open.get() {
            return None;
        }
        let viewport = self.viewport();
        let theme = self.theme().clone();
        let open = config.open.clone();
        let dismissible = config.dismissible;
        let scrim = config
            .scrim
            .unwrap_or_else(|| theme.colors.scrim.with_alpha(0.32));
        let width = config.resolved_width(viewport);
        let padding = config.padding;
        let title = config.title.clone();
        let description = config.description.clone();
        let key = config.open.id();
        let mut card_response = None;
        self.overlay_at(
            ("dialog", key.raw()),
            "dialog",
            self.id(),
            Inset::at(0.0, 0.0),
            Style {
                direction: kasumi_core::FlexDirection::Column,
                align_items: Align::Center,
                justify: Justify::Center,
                width: Length::Percent(100.0),
                height: Length::Percent(100.0),
                ..Style::default()
            },
            |ui| {
                let scrim_response = ui.container(|ui| {
                    ui.absolute(0.0, 0.0)
                        .width(viewport.width)
                        .height(viewport.height)
                        .background(scrim);
                    // The scrim blocks the page but must not enter tab order.
                    ui.set_sense_on(ui.node, Sense::hover().union(Sense::CLICK));
                    ui.cx.tree.set_a11y_hidden(ui.node, true);
                });
                if dismissible && scrim_response.clicked() {
                    open.set(false);
                }
                if let Some(title) = title.clone() {
                    ui.cx.tree.set_accessibility_label(ui.node, title.clone());
                }
                let mut title_style = theme.typography.headline_small.clone();
                title_style.color = theme.colors.on_surface;
                let mut description_style = theme.typography.body_medium.clone();
                description_style.color = theme.colors.on_surface_variant;
                let card = ui.card(
                    Card::new()
                        .padding(padding)
                        .gap(theme.spacing.lg)
                        .width(width)
                        .max_width(width)
                        .radius(theme.shape.extra_large)
                        .background(theme.colors.surface_container_high),
                    |ui| {
                        if let Some(title) = &title {
                            ui.add(
                                Label::new(title.clone())
                                    .style(title_style.clone())
                                    .wrap(kasumi_core::TextWrap::Word),
                            );
                        }
                        if let Some(description) = &description {
                            ui.add(
                                Label::new(description.clone()).style(description_style.clone()),
                            );
                        }
                        body(ui);
                    },
                );
                if dismissible && ui.key_pressed(&Key::Escape) {
                    open.set(false);
                }
                card_response = Some(card);
            },
        );
        card_response
    }

    /// Controls whether the nearest scroll viewport on the current container
    /// paints a scrollbar. Scrolling and hit testing remain available.
    pub fn hide_scrollbars(&mut self) -> &mut Self {
        if let Some(node) = self.cx.tree.get_mut(self.node)
            && (node.style.scroll_x || node.style.scroll_y)
        {
            node.show_scrollbars = false;
        }
        self
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
        let id = {
            let _ = _owner;
            keyed_id(self.scope, &key)
        };
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
                next_resource: 0,
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

    /// A multi-line editor bound to `State<String>`. Returns the response and
    /// the current caret/selection status; see [`crate::TextEditor`].
    pub fn text_editor(&mut self, value: &State<String>) -> (Response, crate::EditorStatus) {
        crate::TextEditor::new(value).ui(self)
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
        let width = (text.chars().count() as f32 * theme.typography.body_small.size * 0.6 + 16.0)
            .clamp(48.0, 320.0);
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
                height: Length::Px(26.0),
                padding: Edges::xy(8.0, 0.0),
                align_items: Align::Center,
                background: theme.colors.inverse_surface,
                radius: Corners::all(theme.shape.extra_small),
                ..Style::default()
            },
            |ui| {
                let mut style = theme.typography.body_small.clone();
                style.color = theme.colors.inverse_on_surface;
                ui.add(Label::new(text).style(style));
            },
        );
    }

    // ------------------------------------------------------------------
    // State
    // ------------------------------------------------------------------

    /// Returns a cloneable handle to a value that survives frames.
    ///
    /// The slot is declared once per frame, so it stays alive while this call
    /// runs even if the value itself is only read occasionally. Values whose
    /// declaring code stops running are evicted after the state TTL; use
    /// [`Ui::state_persistent`] to opt out of eviction entirely.
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
        self.cx.state.touch(id);
        State::new(self.cx.state.clone(), id, path, init)
    }

    /// Like [`Ui::state`], but exempt from TTL eviction.
    pub fn state_persistent<T: 'static>(&mut self, init: impl Fn() -> T + 'static) -> State<T> {
        self.state(init).persistent()
    }

    /// Loads an asynchronous resource, starting the loader on first use.
    ///
    /// The handle is cached for this position (or key) for as long as it is
    /// declared, so later frames return the same value without re-running the
    /// loader. Loaders execute on a shared background executor, never on the
    /// UI thread. `E` is flattened to a displayable [`kasumi_state::ResourceError`]
    /// so it can cross the worker boundary.
    ///
    /// ```ignore
    /// let users = ui.resource(|| api::fetch_users().map_err(|error| error.to_string()));
    /// if users.is_loading() && !users.has_value() {
    ///     ui.label("Loading…");
    /// } else if let Some(users) = users.get() {
    ///     // render
    /// }
    /// ```
    ///
    /// Position-based identity follows call order. Use
    /// [`Ui::resource_keyed`] for conditional resources, lists and anything
    /// that can be reordered.
    pub fn resource<T, F, E>(&mut self, loader: F) -> Resource<T>
    where
        T: Send + 'static,
        F: Fn() -> Result<T, E> + Send + Sync + 'static,
        E: std::fmt::Display + 'static,
    {
        let (id, path) = self.next_resource_slot();
        self.cx.resources.get_or_insert(id, path, loader)
    }

    /// Keyed variant of [`Ui::resource`]. The cache key is stable when sibling
    /// widgets are added, removed or reordered.
    pub fn resource_keyed<K, T, F, E>(&mut self, key: K, loader: F) -> Resource<T>
    where
        K: Hash,
        T: Send + 'static,
        F: Fn() -> Result<T, E> + Send + Sync + 'static,
        E: std::fmt::Display + 'static,
    {
        let (id, path) = self.keyed_resource_slot(&key);
        self.cx.resources.get_or_insert(id, path, loader)
    }

    /// Future-backed variant of [`Ui::resource`].
    ///
    /// The future is polled to completion on the shared executor. A future that
    /// depends on an external reactor must bring that runtime with it.
    pub fn resource_future<T, F, Fut, E>(&mut self, factory: F) -> Resource<T>
    where
        T: Send + 'static,
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + 'static,
        E: std::fmt::Display + 'static,
    {
        let (id, path) = self.next_resource_slot();
        self.cx.resources.get_or_insert_future(id, path, factory)
    }

    /// Keyed variant of [`Ui::resource_future`].
    pub fn resource_future_keyed<K, T, F, Fut, E>(&mut self, key: K, factory: F) -> Resource<T>
    where
        K: Hash,
        T: Send + 'static,
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + 'static,
        E: std::fmt::Display + 'static,
    {
        let (id, path) = self.keyed_resource_slot(&key);
        self.cx.resources.get_or_insert_future(id, path, factory)
    }

    fn next_resource_slot(&mut self) -> (Id, String) {
        let index = self.next_resource;
        self.next_resource += 1;
        let id = kasumi_core::resource_id(self.scope, index);
        (id, format!("{:016x}::resource[{index}]", self.scope.raw()))
    }

    fn keyed_resource_slot<K: Hash>(&self, key: &K) -> (Id, String) {
        let id = keyed_id(self.scope, &("resource", key));
        (
            id,
            format!("{:016x}::resource[{}]", self.scope.raw(), id.short()),
        )
    }

    // ------------------------------------------------------------------
    // Animation
    // ------------------------------------------------------------------

    /// Animates a scalar attached to the current container and returns its
    /// current value. The first call for a node starts from `0.0`; a change of
    /// target restarts the interpolation from the value visible now. Easing is
    /// fixed to smoothstep.
    pub fn animate(&mut self, target: f32, duration: Duration) -> f32 {
        self.animate_with(
            "animate",
            target,
            MotionSpec::tween(duration, Easing::EaseOutCubic),
        )
    }

    /// Animates a named scalar channel on the current container.
    pub fn animate_with(
        &mut self,
        key: impl Into<Box<str>>,
        target: f32,
        motion: MotionSpec,
    ) -> f32 {
        self.animate_on(self.node, key, target, motion)
    }

    /// Animates a named scalar channel on a specific retained node.
    ///
    /// Widgets must use their own node rather than the parent container: two
    /// controls sharing a parent would otherwise share (and endlessly
    /// retarget) the same animation channel.
    pub(crate) fn animate_on(
        &mut self,
        node_id: kasumi_tree::NodeId,
        key: impl Into<Box<str>>,
        target: f32,
        motion: MotionSpec,
    ) -> f32 {
        let now = self.cx.input.time;
        let Some(node) = self.cx.tree.get_mut(node_id) else {
            return target;
        };
        let key = key.into();
        let (value, running) = node.animate_channel(&key, target, motion, now);
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

    /// Sets the flex-shrink factor. Containers default to `1`.
    pub fn flex_shrink(&mut self, shrink: f32) -> &mut Self {
        self.edit_style(|style| style.flex_shrink = shrink.max(0.0));
        self
    }

    pub fn grow(&mut self, grow: f32) -> &mut Self {
        self.edit_style(|style| {
            style.set_grow(grow);
        })
    }

    /// Sets the flex base size. Combine with [`Ui::grow`] to distribute free
    /// space strictly by the grow ratios instead of by content width.
    pub fn basis(&mut self, basis: impl Into<Length>) -> &mut Self {
        self.edit_style(|style| {
            style.set_flex_basis(basis);
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

    /// Like [`Self::set_text_on`] but attaches syntax-highlight ranges.
    pub(crate) fn set_text_on_with_spans(
        &mut self,
        node: NodeId,
        text: String,
        style: TextStyle,
        spans: Vec<kasumi_core::TextSpan>,
    ) {
        self.cx
            .tree
            .set_text(node, TextContent::new(text, style).with_spans(spans));
    }

    pub(crate) fn set_sense_on(&mut self, node: NodeId, sense: Sense) {
        self.cx.tree.set_sense(node, sense);
    }

    pub(crate) fn response_of(&self, id: Id) -> Response {
        self.cx.response(id)
    }
}
