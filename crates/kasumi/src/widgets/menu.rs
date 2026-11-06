//! Popup menus with any number of nested submenus.
//!
//! A [`Menu`] is plain data: labels, shortcuts, check marks and child menus.
//! It carries no callbacks, so the same value can be rebuilt every frame and
//! compared cheaply. Activating an item returns its index path through the
//! menu tree as a [`MenuSelection`]; the application maps that path to its own
//! command.
//!
//! ```ignore
//! let menu = Menu::new()
//!     .item(MenuItem::new("Open").shortcut("Ctrl+O"))
//!     .submenu("Export", Menu::new()
//!         .item("PNG")
//!         .item("SVG"))
//!     .separator()
//!     .item(MenuItem::new("Quit"));
//!
//! if let Some(selection) = ui.menu(&trigger, menu) {
//!     match selection.path.as_slice() {
//!         [0] => open(),
//!         [1, 0] => export_png(),
//!         [1, 1] => export_svg(),
//!         [3] => quit(),
//!         _ => {}
//!     }
//! }
//! ```
//!
//! Menus are painted through a root overlay: a transparent full-window scrim
//! catches clicks that land outside the menu so they dismiss it instead of
//! reaching the page below. Submenus open on hover after
//! [`Menu::hover_delay`] or immediately on click, and cascade to the right of
//! their parent item, flipping to the left when the window edge is reached.

use std::time::Duration;

use kasumi_core::{
    Align, Color, Corners, CursorIcon, Edges, FlexDirection, Id, Inset, Length, Rect, Style,
    TextAlign, TextWrap, child_id, keyed_id,
};
use kasumi_tree::{MotionSpec, NodeKind, Sense, Spring};

use crate::ui::Ui;

/// Vertical padding of a menu panel, in logical pixels.
const MENU_PADDING: f32 = 8.0;
/// Height of one menu row.
const ROW_HEIGHT: f32 = 48.0;
/// Height reserved by a separator, including its margins.
const SEPARATOR_HEIGHT: f32 = 9.0;
/// Gap between a menu and its anchor.
const ANCHOR_GAP: f32 = 2.0;
/// Horizontal padding inside a row.
const ROW_PADDING: f32 = 16.0;
/// Width of the leading check-mark column and the trailing submenu arrow.
const ICON_COLUMN: f32 = 24.0;
/// Gap between a row's leading, label and trailing parts.
const ROW_GAP: f32 = 12.0;
const MIN_WIDTH: f32 = 112.0;
const MAX_WIDTH: f32 = 280.0;
/// Distance between a submenu and its parent item's trailing edge.
const SUBMENU_GAP: f32 = 2.0;
/// Default hover delay before a hovered submenu opens (Material uses 400 ms).
const DEFAULT_HOVER_DELAY: Duration = Duration::from_millis(400);

/// One selectable command in a [`Menu`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MenuItem {
    pub label: String,
    /// Optional right-aligned shortcut hint, for example `Ctrl+S`.
    pub shortcut: Option<String>,
    pub enabled: bool,
    /// Renders a leading check mark when set. `None` keeps the row a plain
    /// command with no selection affordance.
    pub checked: Option<bool>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            enabled: true,
            checked: None,
        }
    }

    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Marks the row as checkable. `true` paints the leading check mark.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    fn is_selectable(&self) -> bool {
        self.enabled
    }
}

impl From<&str> for MenuItem {
    fn from(label: &str) -> Self {
        Self::new(label)
    }
}

impl From<String> for MenuItem {
    fn from(label: String) -> Self {
        Self::new(label)
    }
}

/// A nested menu attached to one entry of its parent.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubMenu {
    pub label: String,
    pub enabled: bool,
    pub menu: Menu,
}

impl SubMenu {
    pub fn new(label: impl Into<String>, menu: Menu) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            menu,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// One entry of a [`Menu`].
#[derive(Debug, Clone, PartialEq)]
pub enum MenuEntry {
    Item(MenuItem),
    Separator,
    SubMenu(SubMenu),
}

impl MenuEntry {
    fn label(&self) -> Option<&str> {
        match self {
            Self::Item(item) => Some(&item.label),
            Self::SubMenu(menu) => Some(&menu.label),
            Self::Separator => None,
        }
    }

    fn is_enabled(&self) -> bool {
        match self {
            Self::Item(item) => item.is_selectable(),
            Self::SubMenu(menu) => menu.enabled,
            Self::Separator => false,
        }
    }
}

/// Where a popup menu is placed relative to its anchor.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum MenuPlacement {
    /// Below the anchor, leading edges aligned.
    #[default]
    BelowStart,
    /// Below the anchor, trailing edges aligned.
    BelowEnd,
    /// Below the anchor's pointer position, leading edges aligned. Used by
    /// context menus.
    AtPointer,
}

/// Declarative menu contents.
///
/// Values are cheap to rebuild each frame. See the [module docs](self) for the
/// selection-path convention.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    entries: Vec<MenuEntry>,
    placement: MenuPlacement,
    hover_delay: Duration,
    min_width: f32,
    max_width: f32,
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            placement: MenuPlacement::BelowStart,
            hover_delay: DEFAULT_HOVER_DELAY,
            min_width: MIN_WIDTH,
            max_width: MAX_WIDTH,
        }
    }

    pub fn item(mut self, item: impl Into<MenuItem>) -> Self {
        self.entries.push(MenuEntry::Item(item.into()));
        self
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(MenuEntry::Separator);
        self
    }

    pub fn submenu(mut self, label: impl Into<String>, menu: Menu) -> Self {
        self.entries
            .push(MenuEntry::SubMenu(SubMenu::new(label, menu)));
        self
    }

    pub fn push(mut self, entry: MenuEntry) -> Self {
        self.entries.push(entry);
        self
    }

    pub fn placement(mut self, placement: MenuPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// Delay before hovering a submenu parent row opens it. `Duration::ZERO`
    /// opens on hover immediately.
    pub fn hover_delay(mut self, delay: Duration) -> Self {
        self.hover_delay = delay;
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width.max(0.0);
        self
    }

    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = width.max(0.0);
        self
    }

    pub fn entries(&self) -> &[MenuEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn selectable_at(&self, index: usize) -> bool {
        self.entries
            .get(index)
            .is_some_and(|entry| entry.is_enabled())
    }

    fn has_shortcut(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry, MenuEntry::Item(item) if item.shortcut.is_some()))
    }

    fn has_checkmarks(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry, MenuEntry::Item(item) if item.checked.is_some()))
    }

    fn has_submenus(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry, MenuEntry::SubMenu(_)))
    }
}

/// The command the user activated, as a path of entry indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuSelection {
    /// Indices from the root menu down to the activated item.
    pub path: Vec<usize>,
    pub label: String,
}

impl MenuSelection {
    pub fn new(path: Vec<usize>, label: impl Into<String>) -> Self {
        Self {
            path,
            label: label.into(),
        }
    }

    /// The index inside the root menu.
    pub fn index(&self) -> Option<usize> {
        self.path.first().copied()
    }
}

/// Retained state of one open menu.
#[derive(Debug, Clone)]
pub(crate) struct MenuState {
    pub open: bool,
    /// Anchor rectangle. Context menus use a zero-sized rect at the pointer.
    pub anchor: Rect,
    pub placement: MenuPlacement,
    /// Indices of the submenus that are currently open, level by level.
    pub path: Vec<usize>,
    /// Keyboard highlight for every rendered level.
    pub active: Vec<Option<usize>>,
    /// Hovered row and the time the hover started, for delayed submenu open.
    pub hover: Option<(usize, usize, f64)>,
    /// Monotonic opening order; only the newest menu handles keyboard input.
    pub seq: u64,
}

impl MenuState {
    fn new(anchor: Rect, placement: MenuPlacement, seq: u64) -> Self {
        Self {
            open: true,
            anchor,
            placement,
            path: Vec::new(),
            active: vec![None],
            hover: None,
            seq,
        }
    }
}

struct LevelGeometry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// Entry index -> top offset inside the panel.
    offsets: Vec<f32>,
}

/// Computes the size of a menu level using the shaped width of its rows.
fn measure_level(ui: &Ui, menu: &Menu, min_width: f32, max_width: f32) -> (f32, f32) {
    let theme = ui.theme();
    let label_style = theme.typography.label_large.clone();
    let shortcut_style = theme.typography.label_small.clone();
    let leading = if menu.has_checkmarks() {
        ICON_COLUMN + ROW_GAP
    } else {
        0.0
    };
    let trailing = if menu.has_submenus() {
        ICON_COLUMN + ROW_GAP
    } else if menu.has_shortcut() {
        64.0 + ROW_GAP
    } else {
        0.0
    };
    let mut widest = 0.0f32;
    let mut height = MENU_PADDING * 2.0;
    for entry in &menu.entries {
        match entry {
            MenuEntry::Separator => height += SEPARATOR_HEIGHT,
            MenuEntry::Item(item) => {
                height += ROW_HEIGHT;
                let label = ui.texts().measure(&item.label, &label_style, None).width;
                let mut width = label + leading + trailing + ROW_PADDING * 2.0;
                if let Some(shortcut) = &item.shortcut {
                    width += ui.texts().measure(shortcut, &shortcut_style, None).width + ROW_GAP;
                }
                widest = widest.max(width);
            }
            MenuEntry::SubMenu(sub) => {
                height += ROW_HEIGHT;
                let label = ui.texts().measure(&sub.label, &label_style, None).width;
                widest = widest.max(label + leading + ICON_COLUMN + ROW_GAP + ROW_PADDING * 2.0);
            }
        }
    }
    (widest.clamp(min_width, max_width.max(min_width)), height)
}

fn level_geometry(
    ui: &Ui,
    menu: &Menu,
    parent_x: f32,
    parent_y: f32,
    viewport: kasumi_core::Size,
) -> LevelGeometry {
    let (width, height) = measure_level(ui, menu, menu.min_width, menu.max_width);
    let x = parent_x.clamp(4.0, (viewport.width - width - 4.0).max(4.0));
    let mut y = parent_y;
    if y + height > viewport.height - 4.0 {
        y = (viewport.height - height - 4.0).max(4.0);
    }
    let mut offsets = Vec::with_capacity(menu.entries.len());
    let mut offset = MENU_PADDING;
    for entry in &menu.entries {
        offsets.push(offset);
        offset += match entry {
            MenuEntry::Separator => SEPARATOR_HEIGHT,
            _ => ROW_HEIGHT,
        };
    }
    LevelGeometry {
        x,
        y,
        width,
        height,
        offsets,
    }
}

impl Ui<'_> {
    /// Declares the popup menu attached to `anchor` and returns the command the
    /// user activated during this frame, if any.
    ///
    /// The menu is retained under `anchor.id()`. Open it with
    /// [`Ui::open_menu`] (or [`crate::MenuButton`]) and close it with
    /// [`Ui::close_menu`]; clicking the scrim outside the menu or pressing
    /// Escape closes it automatically.
    pub fn menu(&mut self, anchor: &crate::Response, menu: Menu) -> Option<MenuSelection> {
        let owner = anchor.id();
        let rect = anchor.rect();
        let placement = menu.placement;
        self.menu_at_owner(owner, rect, placement, menu)
    }

    /// Declares a context menu owned by `owner`, opened at the pointer when the
    /// owner reports a right click.
    pub fn context_menu(&mut self, owner: &crate::Response, menu: Menu) -> Option<MenuSelection> {
        let id = owner.id();
        if owner.context_clicked() {
            let pointer = self.cx.input.pointer;
            self.open_menu_at(
                id,
                Rect::new(pointer, kasumi_core::Size::ZERO),
                MenuPlacement::AtPointer,
            );
        }
        self.menu_at_owner(id, owner.rect(), MenuPlacement::AtPointer, menu)
    }

    /// Opens the menu retained under `owner`. Call before declaring the menu.
    pub fn open_menu(&mut self, owner: Id) {
        self.open_menu_at(owner, Rect::ZERO, MenuPlacement::BelowStart);
    }

    /// Opens the menu retained under `owner` at an explicit rectangle.
    pub fn open_menu_at(&mut self, owner: Id, anchor: Rect, placement: MenuPlacement) {
        let seq = self.cx.menu_seq.wrapping_add(1);
        self.cx.menu_seq = seq;
        let state = self
            .cx
            .menus
            .entry(owner)
            .or_insert_with(|| MenuState::new(anchor, placement, seq));
        state.open = true;
        state.anchor = anchor;
        state.placement = placement;
        state.path.clear();
        state.active.clear();
        state.active.push(None);
        state.hover = None;
        state.seq = seq;
        self.cx.menu_top = Some(owner);
        self.request_repaint();
    }

    /// Closes the menu retained under `owner` if it is open.
    pub fn close_menu(&mut self, owner: Id) {
        let Some(state) = self.cx.menus.get_mut(&owner) else {
            return;
        };
        if !state.open {
            return;
        }
        state.open = false;
        state.path.clear();
        state.hover = None;
        if self.cx.menu_top == Some(owner) {
            self.cx.menu_top = None;
        }
        self.request_repaint();
    }

    /// Whether the menu retained under `owner` is open.
    pub fn is_menu_open(&self, owner: Id) -> bool {
        self.cx.menus.get(&owner).is_some_and(|state| state.open)
    }

    fn menu_at_owner(
        &mut self,
        owner: Id,
        anchor: Rect,
        placement: MenuPlacement,
        menu: Menu,
    ) -> Option<MenuSelection> {
        let seq = self.cx.menu_seq;
        let state = self.cx.menus.get_mut(&owner);
        if state.is_none() || menu.is_empty() {
            return None;
        }
        // Keep the retained anchor in sync with the live layout unless the menu
        // was opened explicitly at a point (context menu) or by pointer.
        if let Some(state) = self.cx.menus.get_mut(&owner) {
            if state.open && placement != MenuPlacement::AtPointer {
                state.anchor = anchor;
            }
        }
        if !self.cx.menus.get(&owner).is_some_and(|state| state.open) {
            return None;
        }
        let top = self.cx.menu_top == Some(owner);
        if top && self.cx.menus.get(&owner).is_some_and(|state| state.open) {
            self.menu_keyboard(owner, &menu);
        }
        self.render_menu(owner, &menu, seq)
    }

    fn menu_keyboard(&mut self, owner: Id, menu: &Menu) {
        let Some(state) = self.cx.menus.get(&owner) else {
            return;
        };
        let depth = state.path.len();
        let active = state.active.get(depth).copied().flatten();
        let mut activate = false;
        let mut open_child = false;
        let mut close_level = false;
        let mut close_all = false;
        let mut step: Option<isize> = None;
        if self.key_pressed(&kasumi_core::Key::ArrowDown) {
            step = Some(1);
        } else if self.key_pressed(&kasumi_core::Key::ArrowUp) {
            step = Some(-1);
        } else if self.key_pressed(&kasumi_core::Key::ArrowRight) {
            open_child = true;
        } else if self.key_pressed(&kasumi_core::Key::ArrowLeft) {
            close_level = true;
        } else if self.key_pressed(&kasumi_core::Key::Enter)
            || self.key_pressed(&kasumi_core::Key::Space)
        {
            activate = true;
        } else if self.key_pressed(&kasumi_core::Key::Escape) {
            close_all = true;
        }
        let Some(state) = self.cx.menus.get_mut(&owner) else {
            return;
        };
        if close_all {
            state.open = false;
            state.path.clear();
            state.hover = None;
            self.cx.menu_top = None;
            self.request_repaint();
            return;
        }
        if let Some(delta) = step {
            let next = next_selectable(menu, state.active.get(depth).copied().flatten(), delta);
            if state.active.len() <= depth {
                state.active.resize(depth + 1, None);
            }
            state.active[depth] = next;
            state.hover = None;
            self.request_repaint();
            return;
        }
        if close_level {
            if state.path.pop().is_some() {
                state.active.truncate(state.path.len() + 1);
                state.hover = None;
                self.request_repaint();
            } else {
                state.open = false;
                state.hover = None;
                self.cx.menu_top = None;
                self.request_repaint();
            }
            return;
        }
        let Some(active) = activate
            .then_some(active)
            .flatten()
            .or_else(|| open_child.then_some(active).flatten())
        else {
            return;
        };
        let Some(entry) = menu.entries.get(active) else {
            return;
        };
        if !entry.is_enabled() {
            return;
        }
        match entry {
            MenuEntry::SubMenu(sub) => {
                state.path.truncate(depth);
                state.path.push(active);
                state.active.truncate(depth + 1);
                state.active.push(None);
                state.hover = None;
                let _ = sub;
                self.request_repaint();
            }
            MenuEntry::Item(_) if activate => {
                let selection = MenuSelection::new(
                    state
                        .path
                        .iter()
                        .copied()
                        .chain(std::iter::once(active))
                        .collect(),
                    entry.label().unwrap_or_default().to_owned(),
                );
                state.open = false;
                state.path.clear();
                state.hover = None;
                self.cx.menu_top = None;
                self.cx.pending_menu_selection = Some(selection);
                self.request_repaint();
            }
            _ => {}
        }
    }

    fn render_menu(&mut self, owner: Id, menu: &Menu, seq: u64) -> Option<MenuSelection> {
        let theme = self.theme().clone();
        let viewport = self.viewport();
        let states = self.cx.menus.get(&owner).cloned()?;
        let open = states.open;
        let path = states.path.clone();
        let active = states.active.clone();
        let anchor = states.anchor;
        let placement = states.placement;

        // Root placement.
        let (root_x, root_y) = match placement {
            MenuPlacement::AtPointer => (anchor.left() + 2.0, anchor.top() + 2.0),
            MenuPlacement::BelowStart => (anchor.left(), anchor.bottom() + ANCHOR_GAP),
            MenuPlacement::BelowEnd => (anchor.right(), anchor.bottom() + ANCHOR_GAP),
        };
        let mut levels: Vec<LevelGeometry> = Vec::new();
        let mut current_menu = menu;
        let mut x = root_x;
        let mut y = root_y;
        for depth in 0..=path.len() {
            let geometry = level_geometry(self, current_menu, x, y, viewport);
            // A right-aligned menu places its trailing edge at the anchor.
            x = geometry.x;
            y = geometry.y;
            levels.push(geometry);
            let Some(index) = path.get(depth).copied() else {
                break;
            };
            let Some(MenuEntry::SubMenu(sub)) = current_menu.entries.get(index) else {
                break;
            };
            let parent = levels.last().expect("the level was just pushed");
            if let Some(offset) = parent.offsets.get(index) {
                x = parent.x + parent.width + SUBMENU_GAP;
                y = parent.y + offset - MENU_PADDING;
            }
            current_menu = &sub.menu;
        }
        // Shift the root when a trailing-aligned menu would leave the window.
        if placement == MenuPlacement::BelowEnd
            && let Some(root) = levels.first()
        {
            let shift = root.x + root.width - anchor.right();
            for level in &mut levels {
                level.x -= shift;
            }
        }

        // Declare the overlay group and its scrim.
        let group_id = keyed_id(self.scope, &("menu-group", owner.raw()));
        self.register_node_id(group_id, "menu group");
        let root_node = self.cx.tree.root();
        let group = self.cx.tree.ensure(group_id, NodeKind::Overlay);
        self.declare_child(root_node, group);
        self.set_style_on(
            group,
            Style {
                position: kasumi_core::Position::Absolute,
                inset: Inset::at(0.0, 0.0),
                width: Length::Px(viewport.width),
                height: Length::Px(viewport.height),
                ..Style::default()
            },
        );

        let scrim_id = child_id(group_id, 0);
        self.register_node_id(scrim_id, "menu scrim");
        let scrim = self.cx.tree.ensure(scrim_id, NodeKind::Container);
        self.declare_child(group, scrim);
        self.set_style_on(
            scrim,
            Style {
                position: kasumi_core::Position::Absolute,
                inset: Inset::at(0.0, 0.0),
                width: Length::Px(viewport.width),
                height: Length::Px(viewport.height),
                ..Style::default()
            },
        );
        self.set_sense_on(scrim, Sense::hover().union(Sense::CLICK));
        let scrim_response = self.cx.response(scrim_id);

        let mut selection: Option<MenuSelection> = None;
        let mut hovered_row: Option<(usize, usize)> = None;
        let mut clicked_row: Option<(usize, usize)> = None;
        let mut next_path = path.clone();
        let mut current_menu = menu;
        for depth in 0..levels.len() {
            let geometry = &levels[depth];
            let siblings = self.cx.menus.get(&owner).map(|state| state.path.clone());
            let prefix: Vec<usize> = siblings
                .unwrap_or_default()
                .into_iter()
                .take(depth)
                .collect();
            let panel_id = keyed_id(group_id, &("menu-level", &prefix));
            let panel_origin: &'static str = if depth == 0 {
                "menu panel"
            } else {
                "submenu panel"
            };
            self.register_node_id(panel_id, panel_origin);
            let panel = self.cx.tree.ensure(panel_id, NodeKind::Menu);
            self.declare_child(group, panel);
            let progress = self.animate_on(
                panel,
                "menu-level",
                1.0,
                MotionSpec::spring(Spring::new(0.2, 0.9)),
            );
            self.set_style_on(
                panel,
                Style {
                    position: kasumi_core::Position::Absolute,
                    inset: Inset::at(geometry.x, geometry.y),
                    width: Length::Px(geometry.width),
                    height: Length::Px(geometry.height),
                    direction: FlexDirection::Column,
                    background: theme.surface_at_elevation(2),
                    border_color: theme.colors.outline_variant.fade(0.5),
                    border_width: 1.0,
                    radius: Corners::all(theme.shape.extra_small),
                    clip: true,
                    ..Style::default()
                },
            );
            // The panel is interactive on its first frame, so it stays fully
            // opaque and only the scale animates in from 80%: that is the
            // Material 3 menu entrance.
            if let Some(node) = self.cx.tree.get_mut(panel) {
                let progress = progress.clamp(0.0, 1.0);
                node.visual.scale = kasumi_tree::Animated::new(0.8 + 0.2 * progress);
                node.visual.origin = Some((0.5, 0.0));
            }

            for (index, entry) in current_menu.entries.iter().enumerate() {
                let offset = geometry.offsets[index];
                let row_id = child_id(panel_id, index as u32);
                if matches!(entry, MenuEntry::Separator) {
                    self.register_node_id(row_id, "menu separator");
                    let separator = self.cx.tree.ensure(row_id, NodeKind::Container);
                    self.declare_child(panel, separator);
                    self.cx.tree.set_a11y_hidden(separator, true);
                    self.set_style_on(
                        separator,
                        Style {
                            position: kasumi_core::Position::Absolute,
                            inset: Inset::at(8.0, offset + SEPARATOR_HEIGHT / 2.0),
                            width: Length::Px((geometry.width - 16.0).max(1.0)),
                            height: Length::Px(1.0),
                            background: theme.colors.outline_variant,
                            ..Style::default()
                        },
                    );
                    continue;
                }
                let enabled = entry.is_enabled();
                let is_submenu = matches!(entry, MenuEntry::SubMenu(_));
                let checked = match entry {
                    MenuEntry::Item(item) => item.checked,
                    _ => None,
                };
                let selected = match entry {
                    MenuEntry::Item(item) => item.checked == Some(true),
                    _ => false,
                };
                let active_here = active.get(depth).copied().flatten() == Some(index);
                let label = entry.label().unwrap_or_default();
                let shortcut = match entry {
                    MenuEntry::Item(item) => item.shortcut.clone(),
                    _ => None,
                };
                let hovered = self.cx.hovered == Some(row_id) && enabled;
                if hovered {
                    hovered_row = Some((depth, index));
                }
                let row_background = if selected {
                    theme.colors.secondary_container
                } else {
                    Color::TRANSPARENT
                };
                let foreground = if !enabled {
                    theme.colors.on_surface.fade(theme.state.disabled_content)
                } else if selected {
                    theme.colors.on_secondary_container
                } else {
                    theme.colors.on_surface
                };
                self.register_node_id(row_id, "menu item");
                let row = self.cx.tree.ensure(row_id, NodeKind::MenuItem);
                self.declare_child(panel, row);
                self.set_sense_on(
                    row,
                    if enabled {
                        Sense::hover().union(Sense::CLICK)
                    } else {
                        Sense::hover()
                    },
                );
                self.cx.tree.set_enabled(row, enabled);
                self.cx.tree.set_accessibility_label(row, label.to_owned());
                self.set_style_on(
                    row,
                    Style {
                        position: kasumi_core::Position::Absolute,
                        inset: Inset::at(4.0, offset),
                        width: Length::Px((geometry.width - 8.0).max(1.0)),
                        height: Length::Px(ROW_HEIGHT),
                        direction: FlexDirection::Row,
                        align_items: Align::Center,
                        padding: Edges::xy(ROW_PADDING - 4.0, 0.0),
                        gap: ROW_GAP,
                        background: if active_here && hovered {
                            row_background
                        } else if active_here {
                            theme.colors.on_surface.fade(0.12)
                        } else {
                            row_background
                        },
                        radius: Corners::all(theme.shape.extra_small),
                        state_layer: if enabled {
                            if selected {
                                theme.colors.on_secondary_container
                            } else {
                                theme.colors.on_surface
                            }
                        } else {
                            Color::TRANSPARENT
                        },
                        cursor: if enabled {
                            CursorIcon::Pointer
                        } else {
                            CursorIcon::Default
                        },
                        ..Style::default()
                    },
                );
                let response = self.cx.response(row_id);
                if response.clicked() {
                    clicked_row = Some((depth, index));
                }

                // Leading check mark column.
                let check_id = child_id(row_id, 0);
                self.register_node_id(check_id, "menu check");
                let check_node = self.cx.tree.ensure(check_id, NodeKind::Text);
                self.declare_child(row, check_node);
                self.cx.tree.set_a11y_hidden(check_node, true);
                let mut check_style = theme.typography.label_large.clone();
                check_style.color = foreground;
                check_style.wrap = TextWrap::None;
                self.set_style_on(
                    check_node,
                    Style {
                        text: check_style.clone(),
                        width: Length::Px(ICON_COLUMN),
                        min_width: Length::Px(ICON_COLUMN),
                        flex_shrink: 0.0,
                        align_self: Align::Center,
                        ..Style::default()
                    },
                );
                self.set_text_on(
                    check_node,
                    if checked == Some(true) { "✓" } else { "" }.to_owned(),
                    check_style,
                );

                // Label.
                let label_id = child_id(row_id, 1);
                self.register_node_id(label_id, "menu label");
                let label_node = self.cx.tree.ensure(label_id, NodeKind::Text);
                self.declare_child(row, label_node);
                let mut label_style = theme.typography.label_large.clone();
                label_style.color = foreground;
                label_style.wrap = TextWrap::None;
                label_style.align = TextAlign::Start;
                self.set_style_on(
                    label_node,
                    Style {
                        text: label_style.clone(),
                        flex_grow: 1.0,
                        min_width: Length::Px(0.0),
                        align_self: Align::Center,
                        ..Style::default()
                    },
                );
                self.set_text_on(label_node, label.to_owned(), label_style);

                // Shortcut or submenu arrow.
                let trailing_id = child_id(row_id, 2);
                self.register_node_id(trailing_id, "menu trailing");
                let trailing_node = self.cx.tree.ensure(trailing_id, NodeKind::Text);
                self.declare_child(row, trailing_node);
                self.cx.tree.set_a11y_hidden(trailing_node, true);
                let mut trailing_style = if is_submenu {
                    theme.typography.label_large.clone()
                } else {
                    theme.typography.label_small.clone()
                };
                trailing_style.color = if enabled {
                    theme.colors.on_surface_variant
                } else {
                    theme.colors.on_surface.fade(theme.state.disabled_content)
                };
                trailing_style.wrap = TextWrap::None;
                trailing_style.align = TextAlign::End;
                self.set_style_on(
                    trailing_node,
                    Style {
                        text: trailing_style.clone(),
                        flex_shrink: 0.0,
                        align_self: Align::Center,
                        ..Style::default()
                    },
                );
                let trailing = if is_submenu {
                    "›".to_owned()
                } else {
                    shortcut.unwrap_or_default()
                };
                self.set_text_on(trailing_node, trailing, trailing_style);
            }

            // Descend into the next open submenu.
            if let Some(index) = path.get(depth).copied()
                && let Some(MenuEntry::SubMenu(sub)) = current_menu.entries.get(index)
            {
                current_menu = &sub.menu;
            }
        }

        // Hover opens submenus after the configured delay and closes levels
        // that the pointer left.
        if open {
            let now = self.time();
            let delay = menu.hover_delay;
            if let Some((depth, index)) = hovered_row {
                let is_submenu = matches!(
                    level_menu(menu, &path, depth).and_then(|m| m.entries.get(index)),
                    Some(MenuEntry::SubMenu(_))
                );
                if is_submenu {
                    let already = path.len() > depth && path[depth] == index;
                    if !already {
                        let started = self
                            .cx
                            .menus
                            .get(&owner)
                            .and_then(|state| state.hover)
                            .filter(|(d, i, _)| *d == depth && *i == index)
                            .map(|(_, _, time)| time)
                            .unwrap_or(now);
                        if delay.is_zero() || now - started >= delay.as_secs_f64() {
                            next_path.truncate(depth);
                            next_path.push(index);
                            if let Some(state) = self.cx.menus.get_mut(&owner) {
                                state.active.truncate(depth + 1);
                                state.active.resize(depth + 1, None);
                                if let Some(slot) = state.active.get_mut(depth) {
                                    *slot = Some(index);
                                }
                            }
                        } else {
                            self.request_repaint();
                        }
                        if let Some(state) = self.cx.menus.get_mut(&owner) {
                            state.hover = Some((depth, index, started));
                        }
                    }
                } else if path.len() > depth && path[depth] != index {
                    next_path.truncate(depth);
                }
            } else if let Some(hovered) = self.cx.hovered {
                // The pointer left the menu entirely: cancel pending opens but
                // keep already open levels.
                let inside = (0..levels.len()).any(|depth| {
                    let prefix: Vec<usize> = path.iter().copied().take(depth).collect();
                    let panel_id = keyed_id(group_id, &("menu-level", &prefix));
                    self.cx
                        .tree
                        .node_by_id(panel_id)
                        .is_some_and(|node| node.rect.contains(self.cx.input.pointer))
                        || self.cx.tree.is_within(hovered, panel_id)
                });
                if !inside && let Some(state) = self.cx.menus.get_mut(&owner) {
                    state.hover = None;
                }
            }

            // Click handling.
            if let Some((depth, index)) = clicked_row {
                let entry = level_menu(menu, &path, depth).and_then(|m| m.entries.get(index));
                match entry {
                    Some(MenuEntry::SubMenu(_)) => {
                        if path.len() > depth && path[depth] == index {
                            next_path.truncate(depth);
                        } else {
                            next_path.truncate(depth);
                            next_path.push(index);
                        }
                        if let Some(state) = self.cx.menus.get_mut(&owner) {
                            state.active.truncate(depth + 1);
                            state.active.resize(depth + 1, None);
                            if let Some(slot) = state.active.get_mut(depth) {
                                *slot = Some(index);
                            }
                        }
                    }
                    Some(MenuEntry::Item(item)) => {
                        let full: Vec<usize> = path
                            .iter()
                            .copied()
                            .take(depth)
                            .chain(std::iter::once(index))
                            .collect();
                        selection = Some(MenuSelection::new(full, item.label.clone()));
                        next_path.clear();
                        if let Some(state) = self.cx.menus.get_mut(&owner) {
                            state.open = false;
                            state.path.clear();
                            state.hover = None;
                        }
                        self.cx.menu_top = None;
                    }
                    _ => {}
                }
            }
            if next_path != path || selection.is_some() {
                if let Some(state) = self.cx.menus.get_mut(&owner) {
                    state.path = next_path;
                }
                self.request_repaint();
            }
            if scrim_response.clicked() && selection.is_none() {
                self.close_menu(owner);
            }
        }

        self.cx.menus.entry(owner).and_modify(|state| {
            state.seq = seq;
        });
        let selection = selection.or_else(|| self.cx.pending_menu_selection.take());
        if selection.is_some()
            && self
                .cx
                .tree
                .node_by_id(owner)
                .is_some_and(|node| node.enabled && node.sense.is_focusable())
        {
            // Pointer-driven focus moved to the pressed row. Returning it to
            // the owner keeps text editing alive after Cut/Copy/Select all.
            self.cx.focused = Some(owner);
        }
        selection
    }
}

/// The menu at `depth` along `path`. Depth 0 is the root.
fn level_menu<'m>(root: &'m Menu, path: &[usize], depth: usize) -> Option<&'m Menu> {
    let mut menu = root;
    for index in path.iter().take(depth) {
        match menu.entries.get(*index) {
            Some(MenuEntry::SubMenu(sub)) => menu = &sub.menu,
            _ => return None,
        }
    }
    Some(menu)
}

/// Moves the keyboard highlight to the next selectable row, skipping
/// separators, disabled rows and submenu rows that cannot open.
fn next_selectable(menu: &Menu, current: Option<usize>, delta: isize) -> Option<usize> {
    let len = menu.entries.len();
    if len == 0 {
        return None;
    }
    let mut index = match current {
        Some(index) if index < len => index,
        _ => {
            if delta >= 0 {
                len - 1
            } else {
                0
            }
        }
    };
    for _ in 0..len {
        index = if delta >= 0 {
            (index + 1) % len
        } else {
            (index + len - 1) % len
        };
        if menu.selectable_at(index) {
            return Some(index);
        }
    }
    None
}

/// The button that opens a [`Menu`] when clicked.
///
/// ```ignore
/// let opened = ui.add(MenuButton::new("File", file_menu));
/// if let Some(selection) = opened.selection { /* dispatch */ }
/// ```
pub struct MenuButton {
    label: String,
    menu: Menu,
    variant: crate::ButtonVariant,
    full_width: bool,
}

/// Result of a [`MenuButton`].
pub struct MenuButtonOutput {
    pub response: crate::Response,
    pub selection: Option<MenuSelection>,
}

impl MenuButton {
    pub fn new(label: impl Into<String>, menu: Menu) -> Self {
        Self {
            label: label.into(),
            menu,
            variant: crate::ButtonVariant::Outlined,
            full_width: false,
        }
    }

    pub fn variant(mut self, variant: crate::ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }
}

impl crate::Widget for MenuButton {
    type Output = MenuButtonOutput;

    fn ui(self, ui: &mut Ui) -> Self::Output {
        let mut button = crate::Button::new(self.label).variant(self.variant);
        if self.full_width {
            button = button.full_width();
        }
        let response = ui.add(button);
        if response.clicked() {
            ui.open_menu(response.id());
        }
        let selection = ui.menu(&response, self.menu);
        MenuButtonOutput {
            response,
            selection,
        }
    }
}

#[allow(dead_code)]
fn _assert_state_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<MenuSelection>();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested() -> Menu {
        Menu::new()
            .item(MenuItem::new("Open"))
            .submenu(
                "Export",
                Menu::new()
                    .item("PNG")
                    .item(MenuItem::new("SVG").checked(true))
                    .submenu("Slides", Menu::new().item("16:9")),
            )
            .separator()
            .item(MenuItem::new("Quit").shortcut("Ctrl+Q"))
    }

    #[test]
    fn nested_menus_keep_entry_indices() {
        let menu = nested();
        assert_eq!(menu.entries().len(), 4);
        let MenuEntry::SubMenu(export) = &menu.entries()[1] else {
            panic!("expected a submenu");
        };
        assert_eq!(export.label, "Export");
        assert_eq!(export.menu.entries().len(), 3);
        assert!(level_menu(&menu, &[1, 2], 2).is_some());
        assert!(level_menu(&menu, &[0], 1).is_none());
    }

    #[test]
    fn keyboard_navigation_skips_separators_and_disabled_rows() {
        let menu = Menu::new()
            .item("One")
            .separator()
            .item(MenuItem::new("Two").enabled(false))
            .item("Three");
        assert_eq!(next_selectable(&menu, None, 1), Some(0));
        assert_eq!(next_selectable(&menu, Some(0), 1), Some(3));
        assert_eq!(next_selectable(&menu, Some(3), 1), Some(0));
        assert_eq!(next_selectable(&menu, Some(0), -1), Some(3));
    }

    #[test]
    fn selection_reports_the_full_path() {
        let selection = MenuSelection::new(vec![1, 2], "16:9");
        assert_eq!(selection.index(), Some(1));
        assert_eq!(selection.label, "16:9");
    }
}
