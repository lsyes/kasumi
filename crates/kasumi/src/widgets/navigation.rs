//! Navigation drawer and rail.
//!
//! A [`NavigationDrawer`] is a declarative tree of destinations, sections and
//! nested groups. The same value renders as a permanent sidebar, a modal
//! drawer or a navigation rail, so an application describes its navigation
//! once and picks a presentation per breakpoint or input device.
//!
//! ```ignore
//! let nav = NavigationDrawer::new()
//!     .header(NavHeader::new("kasumi").subtitle("0.2.0"))
//!     .item(NavItem::new("overview", "Overview").icon("◉"))
//!     .section("Data", |section| {
//!         section
//!             .item(NavItem::new("lists", "Lists"))
//!             .item(NavItem::new("search", "Search"))
//!     })
//!     .selected("lists");
//!
//! if let Some(key) = ui.navigation(&nav).selected {
//!     open(key);
//! }
//! ```
//!
//! Nested destinations expand in place rather than shrinking rows with depth:
//! a parent row toggles its group and the group's children keep the flat
//! Material row metrics with one capped step of extra indentation.
//!
//! Visual treatment follows the M3 navigation drawer and rail specs: 56 dp
//! rows with a full-radius active indicator in `secondary-container`, icons
//! and labels in `on-secondary-container` when selected and
//! `on-surface-variant` otherwise, a 16 dp corner radius on the modal drawer's
//! trailing edge, and a 96 dp rail whose rows center their icon.

use kasumi_core::{
    Align, Color, Corners, CursorIcon, Edges, FlexDirection, Id, Justify, Length, Style, TextAlign,
    TextWrap, child_id, keyed_id,
};
use kasumi_tree::{NodeKind, Sense};

use crate::response::Response;
use crate::ui::{Ui, Widget};

/// Default width of an expanded drawer.
pub const DRAWER_WIDTH: f32 = 280.0;
/// Default width of a collapsed navigation rail.
pub const RAIL_WIDTH: f32 = 96.0;
/// Height of one destination row.
const ROW_HEIGHT: f32 = 56.0;
/// Horizontal inset of the active indicator inside a drawer.
const INDICATOR_INSET: f32 = 12.0;
/// Leading icon column.
const ICON_SIZE: f32 = 24.0;
/// Indentation added per nesting level.
const INDENT_STEP: f32 = 12.0;

/// How the navigation is presented.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum NavVariant {
    /// A permanent column that takes part in layout.
    #[default]
    Standard,
    /// A drawer-over-scrim presentation. The caller owns the scrim.
    Modal,
    /// A collapsed rail: icons only.
    Rail,
    /// An expanded rail: a narrow column whose rows keep their icon and label.
    ExpandedRail,
}

/// One navigation destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavItem {
    /// Stable identity. It is hashed into the node id and reported back when
    /// the destination is activated.
    pub key: String,
    pub label: String,
    /// Optional leading glyph. A short string keeps the framework free of an
    /// icon font; without one the label's first letter is used.
    pub icon: Option<String>,
    /// Trailing text, for example a count.
    pub badge: Option<String>,
    pub enabled: bool,
    pub children: Vec<NavItem>,
    /// Initial expanded state of the nested group. Retained state wins after
    /// the first frame.
    pub expanded: bool,
}

impl NavItem {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            icon: None,
            badge: None,
            enabled: true,
            children: Vec::new(),
            expanded: false,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn child(mut self, child: NavItem) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = NavItem>) -> Self {
        self.children.extend(children);
        self
    }

    /// Starts the nested group expanded.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Every key in this subtree, including its own.
    pub fn keys(&self) -> Vec<&str> {
        let mut out = vec![self.key.as_str()];
        for child in &self.children {
            out.extend(child.keys());
        }
        out
    }

    /// The keys from this item down to `key`, when the subtree contains it.
    pub fn path_to(&self, key: &str) -> Option<Vec<String>> {
        if self.key == key {
            return Some(vec![self.key.clone()]);
        }
        for child in &self.children {
            if let Some(mut path) = child.path_to(key) {
                path.insert(0, self.key.clone());
                return Some(path);
            }
        }
        None
    }
}

/// A titled group of destinations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavSection {
    pub title: String,
    pub items: Vec<NavItem>,
}

impl NavSection {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
        }
    }

    pub fn item(mut self, item: NavItem) -> Self {
        self.items.push(item);
        self
    }
}

/// Branding at the top of a drawer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NavHeader {
    pub title: String,
    pub subtitle: Option<String>,
}

impl NavHeader {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
}

/// Outcome of a rendered navigation tree.
#[derive(Debug, Clone, PartialEq)]
pub struct NavResponse {
    /// Destination activated during this frame.
    pub selected: Option<String>,
    /// The retained response of the drawer container.
    pub response: Response,
}

/// Declarative navigation contents.
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationDrawer {
    entries: Vec<NavItem>,
    sections: Vec<NavSection>,
    header: Option<NavHeader>,
    footer: Vec<NavItem>,
    variant: NavVariant,
    width: Option<f32>,
    show_scrollbars: bool,
    selected: Option<String>,
}

impl Default for NavigationDrawer {
    fn default() -> Self {
        Self::new()
    }
}

impl NavigationDrawer {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            sections: Vec::new(),
            header: None,
            footer: Vec::new(),
            variant: NavVariant::Standard,
            width: None,
            // Material navigation drawers scroll without a persistent rail;
            // opt in with `scrollbars(true)` when the list is long enough that
            // the affordance helps.
            show_scrollbars: false,
            selected: None,
        }
    }

    pub fn header(mut self, header: NavHeader) -> Self {
        self.header = Some(header);
        self
    }

    pub fn item(mut self, item: NavItem) -> Self {
        self.entries.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = NavItem>) -> Self {
        self.entries.extend(items);
        self
    }

    /// Adds a titled section. The closure receives the empty section.
    pub fn section(
        mut self,
        title: impl Into<String>,
        f: impl FnOnce(NavSection) -> NavSection,
    ) -> Self {
        self.sections.push(f(NavSection::new(title)));
        self
    }

    /// A pinned destination rendered below the scrolling list.
    pub fn footer(mut self, item: NavItem) -> Self {
        self.footer.push(item);
        self
    }

    pub fn variant(mut self, variant: NavVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width.max(0.0));
        self
    }

    /// Paints the scrollbar rail. Off by default: Material navigation
    /// drawers scroll without a persistent thumb.
    pub fn scrollbars(mut self, show: bool) -> Self {
        self.show_scrollbars = show;
        self
    }

    /// Marks the current destination. Selected rows paint the active
    /// indicator, and their ancestors expand on the first frame.
    pub fn selected(mut self, key: impl Into<String>) -> Self {
        self.selected = Some(key.into());
        self
    }

    pub fn is_rail(&self) -> bool {
        matches!(self.variant, NavVariant::Rail)
    }

    /// The width this navigation resolves to.
    pub fn resolved_width(&self) -> f32 {
        self.width.unwrap_or(match self.variant {
            NavVariant::Standard | NavVariant::Modal => DRAWER_WIDTH,
            NavVariant::Rail => RAIL_WIDTH,
            NavVariant::ExpandedRail => 220.0,
        })
    }

    /// The keys of every destination, in declaration order.
    pub fn keys(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for item in self.entries.iter().chain(self.footer.iter()) {
            out.extend(item.keys());
        }
        for section in &self.sections {
            for item in &section.items {
                out.extend(item.keys());
            }
        }
        out
    }

    /// Whether a destination with `key` exists anywhere in this navigation.
    pub fn contains(&self, key: &str) -> bool {
        self.keys().contains(&key)
    }

    /// Expands the ancestor rows of the current selection.
    fn with_selection_expanded(mut self) -> Self {
        let Some(selected) = self.selected.clone() else {
            return self;
        };
        fn walk(items: &mut [NavItem], selected: &str) {
            for item in items.iter_mut() {
                if item
                    .children
                    .iter()
                    .any(|child| child.path_to(selected).is_some())
                {
                    item.expanded = true;
                }
                walk(&mut item.children, selected);
            }
        }
        walk(&mut self.entries, &selected);
        walk(&mut self.footer, &selected);
        for section in &mut self.sections {
            walk(&mut section.items, &selected);
        }
        self
    }
}

/// Per-frame state shared by the recursive row renderer.
struct Render<'a> {
    theme: &'a crate::Theme,
    rail: bool,
    selected: Option<&'a str>,
    activated: Option<String>,
}

impl Widget for NavigationDrawer {
    type Output = NavResponse;

    fn ui(self, ui: &mut Ui) -> NavResponse {
        let drawer = self.with_selection_expanded();
        let theme = ui.theme().clone();
        let width = drawer.resolved_width();
        let variant = drawer.variant;
        let rail = variant == NavVariant::Rail;
        let modal = variant == NavVariant::Modal;
        let id = ui.next_id("navigation drawer");

        let mut context = Render {
            theme: &theme,
            rail,
            selected: drawer.selected.as_deref(),
            activated: None,
        };

        let style = Style {
            direction: FlexDirection::Column,
            width: Length::Px(width),
            min_width: Length::Px(width),
            height: if modal {
                Length::Px(ui.viewport().height)
            } else {
                Length::Percent(100.0)
            },
            flex_shrink: 0.0,
            padding: Edges::xy(12.0, 16.0),
            gap: theme.spacing.sm,
            background: if modal {
                theme.colors.surface_container_low
            } else {
                theme.colors.surface
            },
            radius: if modal {
                Corners {
                    top_left: 0.0,
                    top_right: theme.shape.large,
                    bottom_right: theme.shape.large,
                    bottom_left: 0.0,
                }
            } else {
                Corners::ZERO
            },
            ..Style::default()
        };
        let node = ui.adopt(id, NodeKind::NavigationDrawer, style);
        ui.cx.tree.set_accessibility_label(node, "Navigation");

        if let Some(header) = &drawer.header {
            header.ui_into(ui, node, id, &context);
        }

        // The scrolling list owns wheel scrolling and the scrollbar.
        let list_id = child_id(id, 20);
        ui.register_node_id(list_id, "navigation list");
        let list = ui.cx.tree.ensure(list_id, NodeKind::Container);
        ui.declare_child(node, list);
        ui.set_style_on(
            list,
            Style {
                direction: FlexDirection::Column,
                width: Length::Percent(100.0),
                flex_grow: 1.0,
                flex_basis: Length::Px(0.0),
                min_height: Length::Px(0.0),
                gap: 2.0,
                clip: true,
                scroll_y: true,
                padding: Edges::xy(0.0, 4.0),
                ..Style::default()
            },
        );
        ui.set_sense_on(list, Sense::hover().union(Sense::FOCUS));
        if let Some(tree_node) = ui.cx.tree.get_mut(list) {
            tree_node.show_scrollbars = drawer.show_scrollbars;
        }

        for item in &drawer.entries {
            row(ui, list, list_id, item, &mut context, 0);
        }
        for section in &drawer.sections {
            section_label(ui, list, list_id, &section.title, &context);
            for item in &section.items {
                row(ui, list, list_id, item, &mut context, 0);
            }
        }

        if !drawer.footer.is_empty() {
            divider_rule(ui, node, id, 21);
            for item in &drawer.footer {
                row(ui, node, id, item, &mut context, 0);
            }
        }

        NavResponse {
            selected: context.activated.take(),
            response: ui.response_of(id),
        }
    }
}

impl NavHeader {
    fn ui_into(
        &self,
        ui: &mut Ui,
        parent: kasumi_tree::NodeId,
        parent_id: Id,
        context: &Render<'_>,
    ) {
        let theme = context.theme;
        let id = child_id(parent_id, 30);
        ui.register_node_id(id, "navigation header");
        let node = ui.cx.tree.ensure(id, NodeKind::Container);
        ui.declare_child(parent, node);
        ui.set_style_on(
            node,
            Style {
                direction: FlexDirection::Row,
                align_items: Align::Center,
                width: Length::Percent(100.0),
                height: Length::Px(56.0),
                min_height: Length::Px(56.0),
                padding: Edges::xy(4.0, 0.0),
                gap: 10.0,
                ..Style::default()
            },
        );
        let mark_id = child_id(id, 0);
        ui.register_node_id(mark_id, "navigation mark");
        let mark = ui.cx.tree.ensure(mark_id, NodeKind::Container);
        ui.declare_child(node, mark);
        ui.set_style_on(
            mark,
            Style {
                width: Length::Px(34.0),
                height: Length::Px(34.0),
                min_width: Length::Px(34.0),
                flex_shrink: 0.0,
                radius: Corners::all(theme.shape.medium),
                background: theme.colors.primary,
                ..Style::default()
            },
        );
        if context.rail {
            return;
        }
        let title_id = child_id(id, 1);
        ui.register_node_id(title_id, "navigation header title");
        let title = ui.cx.tree.ensure(title_id, NodeKind::Container);
        ui.declare_child(node, title);
        ui.set_style_on(
            title,
            Style {
                direction: FlexDirection::Column,
                flex_grow: 1.0,
                min_width: Length::Px(0.0),
                ..Style::default()
            },
        );
        let mut title_style = theme.typography.title_medium.clone();
        title_style.color = theme.colors.on_surface;
        text(
            ui,
            title,
            title_id,
            0,
            &self.title,
            title_style,
            "navigation title",
        );
        if let Some(subtitle) = &self.subtitle {
            let mut subtitle_style = theme.typography.label_small.clone();
            subtitle_style.color = theme.colors.outline;
            text(
                ui,
                title,
                title_id,
                1,
                subtitle,
                subtitle_style,
                "navigation subtitle",
            );
        }
    }
}

fn divider_rule(ui: &mut Ui, parent: kasumi_tree::NodeId, parent_id: Id, slot: u32) {
    let theme = ui.theme().clone();
    let id = child_id(parent_id, slot);
    ui.register_node_id(id, "navigation divider");
    let node = ui.cx.tree.ensure(id, NodeKind::Container);
    ui.declare_child(parent, node);
    ui.set_style_on(
        node,
        Style {
            width: Length::Percent(100.0),
            height: Length::Px(1.0),
            min_height: Length::Px(1.0),
            background: theme.colors.outline_variant,
            margin: Edges::xy(0.0, 4.0),
            ..Style::default()
        },
    );
}

fn section_label(
    ui: &mut Ui,
    parent: kasumi_tree::NodeId,
    parent_id: Id,
    title: &str,
    context: &Render<'_>,
) {
    if context.rail {
        // A rail separates sections with a rule instead of a heading.
        let id = keyed_id(parent_id, &("nav-section-rule", title));
        ui.register_node_id(id, "navigation section rule");
        let node = ui.cx.tree.ensure(id, NodeKind::Spacer);
        ui.declare_child(parent, node);
        ui.set_style_on(
            node,
            Style {
                width: Length::Percent(100.0),
                height: Length::Px(1.0),
                min_height: Length::Px(1.0),
                background: context.theme.colors.outline_variant,
                margin: Edges::xy(16.0, 8.0),
                ..Style::default()
            },
        );
        return;
    }
    let id = keyed_id(parent_id, &("nav-section", title));
    ui.register_node_id(id, "navigation section");
    let node = ui.cx.tree.ensure(id, NodeKind::Container);
    ui.declare_child(parent, node);
    ui.set_style_on(
        node,
        Style {
            width: Length::Percent(100.0),
            height: Length::Px(28.0),
            min_height: Length::Px(28.0),
            padding: Edges {
                left: 16.0,
                right: 16.0,
                top: 16.0,
                bottom: 0.0,
            },
            align_items: Align::Center,
            ..Style::default()
        },
    );
    let mut label_style = context.theme.typography.title_small.clone();
    label_style.color = context.theme.colors.on_surface_variant;
    text(
        ui,
        node,
        id,
        0,
        title,
        label_style,
        "navigation section title",
    );
}

fn row(
    ui: &mut Ui,
    container: kasumi_tree::NodeId,
    container_id: Id,
    item: &NavItem,
    context: &mut Render<'_>,
    depth: usize,
) {
    let theme = context.theme;
    let selected = context.selected == Some(item.key.as_str());
    let rail = context.rail;
    // Keyed so a reordered list does not reparent retained animation state.
    let id = keyed_id(container_id, &("nav-item", item.key.as_str()));
    let has_children = item.has_children();
    let expanded = has_children
        && ui
            .cx
            .tree
            .node_by_id(id)
            .map(|node| node.popup_open)
            .unwrap_or(item.expanded);
    // Indentation is applied by the group container, so every level adds
    // exactly one step even when a level is collapsed.
    let indent = 0.0f32;

    ui.register_node_id(id, "navigation item");
    let node = ui.cx.tree.ensure(id, NodeKind::NavigationItem);
    ui.declare_child(container, node);
    ui.cx.tree.set_enabled(node, item.enabled);
    ui.cx.tree.set_accessibility_label(node, item.label.clone());
    ui.set_sense_on(
        node,
        if item.enabled {
            Sense::click()
        } else {
            Sense::NONE
        },
    );
    let foreground = if !item.enabled {
        theme.colors.on_surface.fade(theme.state.disabled_content)
    } else if selected {
        theme.colors.on_secondary_container
    } else {
        theme.colors.on_surface_variant
    };
    ui.set_style_on(
        node,
        Style {
            direction: FlexDirection::Row,
            align_items: Align::Center,
            width: Length::Percent(100.0),
            height: Length::Px(ROW_HEIGHT),
            min_height: Length::Px(ROW_HEIGHT),
            max_height: Length::Px(ROW_HEIGHT),
            flex_shrink: 0.0,
            padding: Edges {
                left: if rail { 0.0 } else { INDICATOR_INSET + indent },
                right: if rail { 0.0 } else { 16.0 },
                top: 0.0,
                bottom: 0.0,
            },
            gap: 12.0,
            justify: if rail {
                Justify::Center
            } else {
                Justify::Start
            },
            background: if selected {
                theme.colors.secondary_container
            } else {
                Color::TRANSPARENT
            },
            radius: Corners::all(theme.shape.full),
            state_layer: if item.enabled {
                if selected {
                    theme.colors.on_secondary_container
                } else {
                    theme.colors.on_surface
                }
            } else {
                Color::TRANSPARENT
            },
            cursor: if item.enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            },
            focus_ring: theme.colors.primary,
            ..Style::default()
        },
    );

    let icon_text = item
        .icon
        .clone()
        .filter(|icon| !icon.is_empty())
        .unwrap_or_else(|| {
            item.label
                .chars()
                .next()
                .map(|first| first.to_uppercase().to_string())
                .unwrap_or_default()
        });
    let icon_id = child_id(id, 0);
    ui.register_node_id(icon_id, "navigation icon");
    let icon = ui.cx.tree.ensure(icon_id, NodeKind::Text);
    ui.declare_child(node, icon);
    ui.cx.tree.set_a11y_hidden(icon, true);
    let mut icon_style = theme.typography.title_medium.clone();
    icon_style.color = foreground;
    icon_style.wrap = TextWrap::None;
    icon_style.align = TextAlign::Center;
    ui.set_style_on(
        icon,
        Style {
            text: icon_style.clone(),
            width: Length::Px(ICON_SIZE),
            min_width: Length::Px(ICON_SIZE),
            flex_shrink: 0.0,
            align_self: Align::Center,
            ..Style::default()
        },
    );
    ui.set_text_on(icon, icon_text, icon_style);

    if !rail {
        let label_id = child_id(id, 1);
        ui.register_node_id(label_id, "navigation label");
        let label = ui.cx.tree.ensure(label_id, NodeKind::Text);
        ui.declare_child(node, label);
        let mut label_style = theme.typography.label_large.clone();
        label_style.color = foreground;
        label_style.wrap = TextWrap::None;
        label_style.align = TextAlign::Start;
        ui.set_style_on(
            label,
            Style {
                text: label_style.clone(),
                flex_grow: 1.0,
                min_width: Length::Px(0.0),
                align_self: Align::Center,
                ..Style::default()
            },
        );
        let label_text = match &item.badge {
            Some(badge) => format!("{}   {badge}", item.label),
            None => item.label.clone(),
        };
        ui.set_text_on(label, label_text, label_style);

        if has_children {
            let chevron_id = child_id(id, 2);
            ui.register_node_id(chevron_id, "navigation chevron");
            let chevron = ui.cx.tree.ensure(chevron_id, NodeKind::Text);
            ui.declare_child(node, chevron);
            ui.cx.tree.set_a11y_hidden(chevron, true);
            let mut chevron_style = theme.typography.label_large.clone();
            chevron_style.color = theme.colors.on_surface_variant;
            chevron_style.wrap = TextWrap::None;
            chevron_style.align = TextAlign::End;
            let open = ui.animate_on(
                node,
                "nav-expanded",
                if expanded { 1.0 } else { 0.0 },
                kasumi_tree::MotionSpec::spring(kasumi_tree::Spring::new(0.2, 0.9)),
            );
            ui.set_style_on(
                chevron,
                Style {
                    text: chevron_style.clone(),
                    flex_shrink: 0.0,
                    align_self: Align::Center,
                    ..Style::default()
                },
            );
            ui.set_text_on(
                chevron,
                if open >= 0.5 { "▾" } else { "▸" }.to_owned(),
                chevron_style,
            );
        }
    }

    let response = ui.cx.response(id);
    let mut expanded = expanded;
    if response.clicked() && item.enabled {
        if has_children {
            expanded = !expanded;
            if let Some(tree_node) = ui.cx.tree.get_mut(node) {
                tree_node.popup_open = expanded;
            }
            ui.request_repaint();
        } else {
            context.activated = Some(item.key.clone());
        }
    }

    if has_children && expanded {
        // The group is declared as a sibling of the parent row so both keep
        // the flat list metrics. It only exists while expanded, and the row
        // keeps its retained expanded bit for the next frame.
        let group_id = child_id(id, 3);
        ui.register_node_id(group_id, "navigation group");
        let group = ui.cx.tree.ensure(group_id, NodeKind::Container);
        ui.declare_child(container, group);
        ui.set_style_on(
            group,
            Style {
                direction: FlexDirection::Column,
                width: Length::Percent(100.0),
                gap: 2.0,
                // One step of leading inset per level, so a nested row's
                // indicator starts further right than its parent's.
                padding: if rail {
                    Edges::ZERO
                } else {
                    Edges {
                        left: INDENT_STEP,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }
                },
                ..Style::default()
            },
        );
        for child in &item.children {
            row(ui, group, group_id, child, context, depth + 1);
        }
        let _ = depth;
    }
}

fn text(
    ui: &mut Ui,
    parent: kasumi_tree::NodeId,
    parent_id: Id,
    index: u32,
    content: &str,
    style: kasumi_core::TextStyle,
    origin: &'static str,
) {
    let id = child_id(parent_id, index);
    ui.register_node_id(id, origin);
    let node = ui.cx.tree.ensure(id, NodeKind::Text);
    ui.declare_child(parent, node);
    ui.set_style_on(
        node,
        Style {
            text: style.clone(),
            align_self: Align::Center,
            ..Style::default()
        },
    );
    ui.set_text_on(node, content.to_owned(), style);
}

impl Ui<'_> {
    /// Renders a navigation drawer and reports the destination the user
    /// activated. Pass the current key through [`NavigationDrawer::selected`].
    pub fn navigation(&mut self, navigation: &NavigationDrawer) -> NavResponse {
        navigation.clone().ui(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn navigation() -> NavigationDrawer {
        NavigationDrawer::new()
            .header(NavHeader::new("kasumi").subtitle("0.2.0"))
            .item(NavItem::new("overview", "Overview").icon("◉"))
            .section("Data", |section| {
                section
                    .item(NavItem::new("lists", "Lists"))
                    .item(NavItem::new("search", "Search"))
            })
            .item(
                NavItem::new("settings", "Settings")
                    .child(NavItem::new("theme", "Theme"))
                    .expanded(true),
            )
    }

    #[test]
    fn keys_cover_items_sections_and_children() {
        let nav = navigation();
        assert_eq!(
            nav.keys(),
            vec!["overview", "settings", "theme", "lists", "search"]
        );
        assert!(nav.contains("theme"));
        assert!(!nav.contains("missing"));
    }

    #[test]
    fn path_to_reports_nested_ancestors() {
        let item = NavItem::new("settings", "Settings")
            .child(NavItem::new("theme", "Theme").child(NavItem::new("dark", "Dark")));
        assert_eq!(
            item.path_to("dark"),
            Some(vec![
                "settings".to_owned(),
                "theme".to_owned(),
                "dark".to_owned()
            ])
        );
        assert_eq!(item.path_to("missing"), None);
    }

    #[test]
    fn variant_widths_follow_the_spec() {
        assert_eq!(NavigationDrawer::new().resolved_width(), DRAWER_WIDTH);
        assert_eq!(
            NavigationDrawer::new()
                .variant(NavVariant::Rail)
                .resolved_width(),
            RAIL_WIDTH
        );
        assert_eq!(NavigationDrawer::new().width(320.0).resolved_width(), 320.0);
    }

    #[test]
    fn selection_auto_expands_ancestors() {
        let nav = NavigationDrawer::new()
            .item(NavItem::new("settings", "Settings").child(NavItem::new("theme", "Theme")));
        let expanded = nav.clone().selected("theme").with_selection_expanded();
        let settings = &expanded.entries[0];
        assert!(settings.expanded);
        assert!(!nav.entries[0].expanded);
    }
}
