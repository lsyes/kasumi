//! Material 3 lists.
//!
//! [`List`] is a declarative list of rows, section headers and dividers. Row
//! height follows the three Material list variants, rows can carry a leading
//! icon or avatar, a trailing icon or meta text, and selection is reported as
//! a row index instead of a callback.
//!
//! ```ignore
//! let list = List::new()
//!     .header("Recent")
//!     .row(ListRow::new("Design review").supporting("Today, 09:30"))
//!     .row(
//!         ListRow::new("Deploy")
//!             .supporting("main · 3 commits")
//!             .leading_icon("↑")
//!             .selected(index == selected),
//!     )
//!     .divider()
//!     .row(ListRow::new("Archive").leading_icon("▤"));
//!
//! if let Some(index) = ui.list(&list).clicked {
//!     open(index);
//! }
//! ```
//!
//! Long lists can be virtualized with [`List::virtualize`]. Because every
//! element has a known height, the visible window is computed from the prefix
//! offsets, so rows are laid out absolutely inside one scroll content node and
//! an arbitrary mixture of one-, two- and three-line rows still virtualizes
//! correctly. Off-screen rows release their retained nodes each frame.
//!
//! Visual treatment follows the M3 list tokens: 56/72/88 dp rows, 16 dp
//! horizontal padding, 12 dp between leading content and the text column,
//! `body-large` titles, `body-medium` supporting text, `label-small` meta
//! text, 1 dp `outline` dividers inset by 16 dp, and a `secondary-container`
//! container with `on-secondary-container` content for selected rows.

use kasumi_core::{
    Align, Color, Corners, CursorIcon, Edges, FlexDirection, Id, Justify, Length, Style, TextAlign,
    TextWrap, child_id,
};
use kasumi_tree::{NodeKind, Sense};

use crate::response::Response;
use crate::ui::{Ui, Widget};

/// Height of a one-line row.
pub const ONE_LINE_HEIGHT: f32 = 56.0;
/// Height of a two-line row.
pub const TWO_LINE_HEIGHT: f32 = 72.0;
/// Height of a three-line row.
pub const THREE_LINE_HEIGHT: f32 = 88.0;
/// Height of a section header.
pub const HEADER_HEIGHT: f32 = 40.0;
/// Height of a divider row, including its margins.
pub const DIVIDER_HEIGHT: f32 = 9.0;
/// Horizontal padding inside a row.
const ROW_PADDING: f32 = 16.0;
/// Gap between leading content and the text column.
const ROW_GAP: f32 = 12.0;
/// Diameter of a leading avatar.
const AVATAR_SIZE: f32 = 40.0;
/// Box size of a leading or trailing icon.
const ICON_SIZE: f32 = 24.0;

/// The three Material list row sizes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ListRowHeight {
    #[default]
    OneLine,
    TwoLine,
    ThreeLine,
}

impl ListRowHeight {
    pub fn height(self) -> f32 {
        match self {
            Self::OneLine => ONE_LINE_HEIGHT,
            Self::TwoLine => TWO_LINE_HEIGHT,
            Self::ThreeLine => THREE_LINE_HEIGHT,
        }
    }
}

/// Leading content of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListLeading {
    /// A 24 dp glyph.
    Icon(String),
    /// A 40 dp circular avatar with up to two characters.
    Avatar(String),
}

/// One list row.
#[derive(Debug, Clone, PartialEq)]
pub struct ListRow {
    pub title: String,
    /// Secondary line. Its presence implies a two-line row unless the height
    /// is set explicitly.
    pub supporting: Option<String>,
    pub leading: Option<ListLeading>,
    /// Trailing glyph, for example a chevron.
    pub trailing_icon: Option<String>,
    /// Trailing caption, for example a timestamp.
    pub trailing: Option<String>,
    pub height: ListRowHeight,
    pub selected: bool,
    pub enabled: bool,
}

impl ListRow {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            supporting: None,
            leading: None,
            trailing_icon: None,
            trailing: None,
            height: ListRowHeight::OneLine,
            selected: false,
            enabled: true,
        }
    }

    pub fn supporting(mut self, text: impl Into<String>) -> Self {
        self.supporting = Some(text.into());
        self
    }

    pub fn leading_icon(mut self, icon: impl Into<String>) -> Self {
        self.leading = Some(ListLeading::Icon(icon.into()));
        self
    }

    pub fn leading_avatar(mut self, text: impl Into<String>) -> Self {
        self.leading = Some(ListLeading::Avatar(text.into()));
        self
    }

    pub fn trailing_icon(mut self, icon: impl Into<String>) -> Self {
        self.trailing_icon = Some(icon.into());
        self
    }

    pub fn trailing(mut self, text: impl Into<String>) -> Self {
        self.trailing = Some(text.into());
        self
    }

    pub fn height(mut self, height: ListRowHeight) -> Self {
        self.height = height;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The height this row resolves to. A supporting line promotes an
    /// unspecified one-line row to two-line, which is what the Material
    /// guidelines call for.
    pub fn resolved_height(&self) -> f32 {
        if self.height == ListRowHeight::OneLine && self.supporting.is_some() {
            TWO_LINE_HEIGHT
        } else {
            self.height.height()
        }
    }
}

impl From<&str> for ListRow {
    fn from(title: &str) -> Self {
        Self::new(title)
    }
}

impl From<String> for ListRow {
    fn from(title: String) -> Self {
        Self::new(title)
    }
}

/// One element of a list.
#[derive(Debug, Clone, PartialEq)]
pub enum ListElement {
    Row(ListRow),
    Header(String),
    Divider,
}

impl ListElement {
    fn height(&self) -> f32 {
        match self {
            Self::Row(row) => row.resolved_height(),
            Self::Header(_) => HEADER_HEIGHT,
            Self::Divider => DIVIDER_HEIGHT,
        }
    }
}

impl From<ListRow> for ListElement {
    fn from(row: ListRow) -> Self {
        Self::Row(row)
    }
}

impl From<&str> for ListElement {
    fn from(title: &str) -> Self {
        Self::Row(ListRow::new(title))
    }
}

impl From<String> for ListElement {
    fn from(title: String) -> Self {
        Self::Row(ListRow::new(title))
    }
}

/// Result of rendering a list.
#[derive(Debug, Clone, PartialEq)]
pub struct ListResponse {
    /// Index of the activated row, counting rows only. Headers and dividers
    /// are skipped, so the value matches [`List::row_count`] indexing rather
    /// than the element index.
    pub clicked: Option<usize>,
    /// The retained response of the list container.
    pub response: Response,
}

/// Declarative Material 3 list.
#[derive(Debug, Clone, PartialEq)]
pub struct List {
    elements: Vec<ListElement>,
    width: Length,
    height: Length,
    grow: f32,
    virtualize: bool,
    overscan: f32,
    show_scrollbars: bool,
    smooth: bool,
}

impl Default for List {
    fn default() -> Self {
        Self::new()
    }
}

impl List {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            width: Length::Percent(100.0),
            height: Length::Auto,
            grow: 0.0,
            virtualize: false,
            overscan: 240.0,
            show_scrollbars: true,
            smooth: true,
        }
    }

    pub fn row(mut self, row: impl Into<ListRow>) -> Self {
        self.elements.push(ListElement::Row(row.into()));
        self
    }

    /// Adds an arbitrary element, which may also be a bare title string.
    pub fn element(mut self, element: impl Into<ListElement>) -> Self {
        self.elements.push(element.into());
        self
    }

    pub fn header(mut self, title: impl Into<String>) -> Self {
        self.elements.push(ListElement::Header(title.into()));
        self
    }

    pub fn divider(mut self) -> Self {
        self.elements.push(ListElement::Divider);
        self
    }

    pub fn size(mut self, width: impl Into<Length>, height: impl Into<Length>) -> Self {
        self.width = width.into();
        self.height = height.into();
        self
    }

    /// Lets the list consume the remaining space of its parent.
    pub fn grow(mut self, grow: f32) -> Self {
        self.grow = grow.max(0.0);
        self
    }

    /// Declares only the visible elements. Use this for long lists; short
    /// lists are cheaper to declare eagerly because they keep local widget
    /// state across frames.
    pub fn virtualize(mut self, virtualize: bool) -> Self {
        self.virtualize = virtualize;
        self
    }

    pub fn overscan(mut self, overscan: f32) -> Self {
        self.overscan = overscan.max(0.0);
        self
    }

    pub fn scrollbars(mut self, show: bool) -> Self {
        self.show_scrollbars = show;
        self
    }

    pub fn smooth(mut self, smooth: bool) -> Self {
        self.smooth = smooth;
        self
    }

    /// Total content height including headers and dividers.
    pub fn content_height(&self) -> f32 {
        self.elements.iter().map(ListElement::height).sum()
    }

    /// Number of rows, ignoring headers and dividers.
    pub fn row_count(&self) -> usize {
        self.elements
            .iter()
            .filter(|element| matches!(element, ListElement::Row(_)))
            .count()
    }

    /// Offsets of every element inside the content node.
    fn offsets(&self) -> Vec<f32> {
        let mut offsets = Vec::with_capacity(self.elements.len());
        let mut offset = 0.0;
        for element in &self.elements {
            offsets.push(offset);
            offset += element.height();
        }
        offsets
    }

    /// The element range intersecting the `[top, bottom)` window.
    ///
    /// `offsets` must be the prefix offsets produced by [`List::offsets`].
    /// Both bounds are found by binary search, so the cost is logarithmic in
    /// the element count even when the rows have different heights.
    fn visible_range(&self, offsets: &[f32], top: f32, bottom: f32) -> std::ops::Range<usize> {
        if self.elements.is_empty() {
            return 0..0;
        }
        // First element whose bottom edge is past the window's top edge.
        let mut low = 0usize;
        let mut high = offsets.len();
        while low < high {
            let middle = (low + high) / 2;
            if offsets[middle] + self.elements[middle].height() <= top {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let first = low.min(offsets.len());
        // One past the last element whose top edge is above the window's
        // bottom edge.
        let last = offsets.partition_point(|offset| *offset < bottom);
        first..last.max(first)
    }
}

impl Widget for List {
    type Output = ListResponse;

    fn ui(self, ui: &mut Ui) -> ListResponse {
        let theme = ui.theme().clone();
        let viewport = ui.viewport();
        let content_height = self.content_height().max(1.0);
        let offsets = self.offsets();

        if !self.virtualize {
            let list_id = ui.next_id("list");
            let node = ui.adopt(
                list_id,
                NodeKind::Container,
                Style {
                    direction: FlexDirection::Column,
                    width: self.width,
                    height: self.height,
                    flex_grow: self.grow,
                    flex_basis: if self.grow > 0.0 {
                        Length::Px(0.0)
                    } else {
                        Length::Auto
                    },
                    min_width: Length::Px(0.0),
                    min_height: Length::Px(0.0),
                    ..Style::default()
                },
            );
            let mut clicked = None;
            let mut row_index = 0usize;
            for (index, element) in self.elements.iter().enumerate() {
                let row = matches!(element, ListElement::Row(_)).then(|| row_index);
                if row.is_some() {
                    row_index += 1;
                }
                element_ui(ui, node, list_id, element, index, row, &theme, &mut clicked);
            }
            return ListResponse {
                clicked,
                response: ui.response_of(list_id),
            };
        }

        // Virtualized: one scroll viewport whose content is the declared
        // extent, with only the intersecting elements declared.
        let view = crate::ScrollView::vertical()
            .size(self.width, self.height)
            .grow(self.grow)
            .smooth(self.smooth)
            .scrollbars(self.show_scrollbars)
            .overscan(self.overscan)
            .virtual_content(kasumi_core::Size::new(0.0, content_height));
        let mut clicked = None;
        let response = ui.virtual_scroll(view, |ui, window| {
            let top = (window.offset.y - window.overscan).max(0.0);
            let bottom = window.offset.y + window.viewport.height + window.overscan;
            let range = self.visible_range(&offsets, top, bottom);
            let mut row_index = self
                .elements
                .iter()
                .take(range.start)
                .filter(|element| matches!(element, ListElement::Row(_)))
                .count();
            for index in range {
                let element = &self.elements[index];
                let offset = offsets[index];
                let row = matches!(element, ListElement::Row(_)).then(|| row_index);
                if row.is_some() {
                    row_index += 1;
                }
                // `ui.keyed` only swaps the id scope; the declared child is a
                // fresh container keyed by the element index so two rows in
                // the same window never share a node.
                ui.keyed(index, |ui| {
                    ui.column(|ui| {
                        ui.absolute(0.0, offset)
                            .width(Length::Percent(100.0))
                            .height(element.height());
                        element_ui(
                            ui,
                            ui.node,
                            ui.scope,
                            element,
                            index,
                            row,
                            &theme,
                            &mut clicked,
                        );
                    });
                });
            }
        });
        let _ = viewport;
        ListResponse { clicked, response }
    }
}

fn element_ui(
    ui: &mut Ui,
    parent: kasumi_tree::NodeId,
    parent_id: Id,
    element: &ListElement,
    index: usize,
    row_index: Option<usize>,
    theme: &crate::Theme,
    clicked: &mut Option<usize>,
) {
    match element {
        ListElement::Divider => {
            let id = child_id(parent_id, index as u32);
            ui.register_node_id(id, "list divider");
            let node = ui.cx.tree.ensure(id, NodeKind::Container);
            ui.declare_child(parent, node);
            ui.cx.tree.set_a11y_hidden(node, true);
            ui.set_style_on(
                node,
                Style {
                    width: Length::Percent(100.0),
                    height: Length::Px(1.0),
                    min_height: Length::Px(1.0),
                    margin: Edges::xy(16.0, 4.0),
                    background: theme.colors.outline,
                    ..Style::default()
                },
            );
        }
        ListElement::Header(title) => {
            let id = child_id(parent_id, index as u32);
            ui.register_node_id(id, "list header");
            let node = ui.cx.tree.ensure(id, NodeKind::Container);
            ui.declare_child(parent, node);
            ui.set_style_on(
                node,
                Style {
                    width: Length::Percent(100.0),
                    height: Length::Px(HEADER_HEIGHT),
                    min_height: Length::Px(HEADER_HEIGHT),
                    padding: Edges::xy(ROW_PADDING, 0.0),
                    align_items: Align::Center,
                    ..Style::default()
                },
            );
            let mut label_style = theme.typography.title_small.clone();
            label_style.color = theme.colors.on_surface_variant;
            text(
                ui,
                node,
                id,
                0,
                title,
                label_style,
                TextAlign::Start,
                "list header title",
            );
        }
        ListElement::Row(row) => {
            row_ui(ui, parent, parent_id, row, index, row_index, theme, clicked);
        }
    }
}

fn row_ui(
    ui: &mut Ui,
    parent: kasumi_tree::NodeId,
    parent_id: Id,
    row: &ListRow,
    index: usize,
    row_index: Option<usize>,
    theme: &crate::Theme,
    clicked: &mut Option<usize>,
) {
    let height = row.resolved_height();
    let id = child_id(parent_id, index as u32);
    ui.register_node_id(id, "list row");
    let node = ui.cx.tree.ensure(id, NodeKind::Container);
    ui.declare_child(parent, node);
    ui.cx.tree.set_enabled(node, row.enabled);
    ui.cx.tree.set_accessibility_label(node, row.title.clone());
    ui.set_sense_on(
        node,
        if row.enabled {
            Sense::click()
        } else {
            Sense::NONE
        },
    );
    let foreground = if !row.enabled {
        theme.colors.on_surface.fade(theme.state.disabled_content)
    } else if row.selected {
        theme.colors.on_secondary_container
    } else {
        theme.colors.on_surface
    };
    let supporting_color = if !row.enabled {
        theme.colors.on_surface.fade(theme.state.disabled_content)
    } else if row.selected {
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
            height: Length::Px(height),
            min_height: Length::Px(height),
            padding: Edges::xy(ROW_PADDING, 0.0),
            gap: ROW_GAP,
            background: if row.selected {
                theme.colors.secondary_container
            } else {
                Color::TRANSPARENT
            },
            radius: if row.selected {
                Corners::all(theme.shape.small)
            } else {
                Corners::ZERO
            },
            state_layer: if row.enabled {
                if row.selected {
                    theme.colors.on_secondary_container
                } else {
                    theme.colors.on_surface
                }
            } else {
                Color::TRANSPARENT
            },
            cursor: if row.enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            },
            focus_ring: theme.colors.primary,
            ..Style::default()
        },
    );

    // Leading slot: icon, avatar or nothing.
    if let Some(leading) = &row.leading {
        let icon_id = child_id(id, 0);
        ui.register_node_id(icon_id, "list leading");
        match leading {
            ListLeading::Icon(glyph) => {
                let icon_node = ui.cx.tree.ensure(icon_id, NodeKind::Text);
                ui.declare_child(node, icon_node);
                ui.cx.tree.set_a11y_hidden(icon_node, true);
                let mut style = theme.typography.title_medium.clone();
                style.color = supporting_color;
                style.wrap = TextWrap::None;
                style.align = TextAlign::Center;
                ui.set_style_on(
                    icon_node,
                    Style {
                        text: style.clone(),
                        width: Length::Px(ICON_SIZE),
                        min_width: Length::Px(ICON_SIZE),
                        flex_shrink: 0.0,
                        align_self: Align::Center,
                        ..Style::default()
                    },
                );
                ui.set_text_on(icon_node, glyph.clone(), style);
            }
            ListLeading::Avatar(initials) => {
                let avatar = ui.cx.tree.ensure(icon_id, NodeKind::Container);
                ui.declare_child(node, avatar);
                ui.set_style_on(
                    avatar,
                    Style {
                        width: Length::Px(AVATAR_SIZE),
                        height: Length::Px(AVATAR_SIZE),
                        min_width: Length::Px(AVATAR_SIZE),
                        flex_shrink: 0.0,
                        justify: Justify::Center,
                        align_items: Align::Center,
                        background: if row.selected {
                            theme.colors.primary
                        } else {
                            theme.colors.primary_container
                        },
                        radius: Corners::all(AVATAR_SIZE / 2.0),
                        ..Style::default()
                    },
                );
                let label_id = child_id(icon_id, 0);
                ui.register_node_id(label_id, "list avatar label");
                let label = ui.cx.tree.ensure(label_id, NodeKind::Text);
                ui.declare_child(avatar, label);
                ui.cx.tree.set_a11y_hidden(label, true);
                let mut style = theme.typography.title_small.clone();
                style.color = if row.selected {
                    theme.colors.on_primary
                } else {
                    theme.colors.on_primary_container
                };
                style.wrap = TextWrap::None;
                ui.set_style_on(
                    label,
                    Style {
                        text: style.clone(),
                        align_self: Align::Center,
                        ..Style::default()
                    },
                );
                ui.set_text_on(label, initials.clone(), style);
            }
        }
    }

    // Text column.
    let column_id = child_id(id, 10);
    ui.register_node_id(column_id, "list text");
    let column = ui.cx.tree.ensure(column_id, NodeKind::Container);
    ui.declare_child(node, column);
    ui.set_style_on(
        column,
        Style {
            direction: FlexDirection::Column,
            justify: Justify::Center,
            align_items: Align::Start,
            flex_grow: 1.0,
            flex_basis: Length::Px(0.0),
            min_width: Length::Px(0.0),
            gap: 2.0,
            ..Style::default()
        },
    );
    let mut title_style = theme.typography.body_large.clone();
    title_style.color = foreground;
    text(
        ui,
        column,
        column_id,
        0,
        &row.title,
        title_style,
        TextAlign::Start,
        "list title",
    );
    if let Some(supporting) = &row.supporting {
        let mut style = theme.typography.body_medium.clone();
        style.color = supporting_color;
        text(
            ui,
            column,
            column_id,
            1,
            supporting,
            style,
            TextAlign::Start,
            "list supporting",
        );
    }

    // Trailing slot: meta text and/or a glyph.
    if let Some(trailing) = &row.trailing {
        let mut style = theme.typography.label_small.clone();
        style.color = supporting_color;
        style.wrap = TextWrap::None;
        let trailing_id = child_id(id, 20);
        ui.register_node_id(trailing_id, "list trailing text");
        let meta = ui.cx.tree.ensure(trailing_id, NodeKind::Text);
        ui.declare_child(node, meta);
        ui.set_style_on(
            meta,
            Style {
                text: style.clone(),
                align_self: Align::Center,
                flex_shrink: 0.0,
                ..Style::default()
            },
        );
        ui.set_text_on(meta, trailing.clone(), style);
    }
    if let Some(icon) = &row.trailing_icon {
        let mut style = theme.typography.title_medium.clone();
        style.color = supporting_color;
        style.wrap = TextWrap::None;
        style.align = TextAlign::End;
        let icon_id = child_id(id, 21);
        ui.register_node_id(icon_id, "list trailing icon");
        let glyph = ui.cx.tree.ensure(icon_id, NodeKind::Text);
        ui.declare_child(node, glyph);
        ui.cx.tree.set_a11y_hidden(glyph, true);
        ui.set_style_on(
            glyph,
            Style {
                text: style.clone(),
                width: Length::Px(ICON_SIZE),
                min_width: Length::Px(ICON_SIZE),
                flex_shrink: 0.0,
                align_self: Align::Center,
                ..Style::default()
            },
        );
        ui.set_text_on(glyph, icon.clone(), style);
    }

    if row.enabled
        && ui.cx.response(id).clicked()
        && let Some(row_index) = row_index
    {
        *clicked = Some(row_index);
    }
}

#[allow(clippy::too_many_arguments)]
fn text(
    ui: &mut Ui,
    parent: kasumi_tree::NodeId,
    parent_id: Id,
    index: u32,
    content: &str,
    style: kasumi_core::TextStyle,
    align: TextAlign,
    origin: &'static str,
) {
    let id = child_id(parent_id, index);
    ui.register_node_id(id, origin);
    let node = ui.cx.tree.ensure(id, NodeKind::Text);
    ui.declare_child(parent, node);
    let mut style = style;
    style.align = align;
    style.wrap = TextWrap::None;
    ui.set_style_on(
        node,
        Style {
            text: style.clone(),
            align_self: Align::Start,
            width: Length::Auto,
            min_width: Length::Px(0.0),
            ..Style::default()
        },
    );
    ui.set_text_on(node, content.to_owned(), style);
}

impl Ui<'_> {
    /// Renders a Material 3 list and reports the activated row.
    pub fn list(&mut self, list: &List) -> ListResponse {
        list.clone().ui(self)
    }
}

/// Convenience for a short list of clickable titles.
pub fn simple_list(titles: impl IntoIterator<Item = impl Into<String>>) -> List {
    let mut list = List::new();
    for title in titles {
        list = list.row(ListRow::new(title.into()));
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> List {
        List::new()
            .header("Recent")
            .row(ListRow::new("One"))
            .row(ListRow::new("Two").supporting("with a second line"))
            .divider()
            .row(ListRow::new("Three").height(ListRowHeight::ThreeLine))
    }

    #[test]
    fn content_height_uses_the_three_row_variants() {
        let list = sample();
        assert_eq!(list.row_count(), 3);
        assert_eq!(
            list.content_height(),
            HEADER_HEIGHT + ONE_LINE_HEIGHT + TWO_LINE_HEIGHT + DIVIDER_HEIGHT + THREE_LINE_HEIGHT
        );
    }

    #[test]
    fn supporting_text_promotes_a_one_line_row() {
        let row = ListRow::new("Title").supporting("detail");
        assert_eq!(row.resolved_height(), TWO_LINE_HEIGHT);
        let explicit = row.height(ListRowHeight::ThreeLine);
        assert_eq!(explicit.resolved_height(), THREE_LINE_HEIGHT);
        assert_eq!(ListRow::new("Title").resolved_height(), ONE_LINE_HEIGHT);
    }

    #[test]
    fn virtual_range_covers_the_viewport_and_skips_far_elements() {
        let mut list = List::new();
        for index in 0..100 {
            list = list.row(ListRow::new(format!("row {index}")));
        }
        let offsets = list.offsets();
        let top = 10.0 * ONE_LINE_HEIGHT;
        let bottom = 12.0 * ONE_LINE_HEIGHT;
        let range = list.visible_range(&offsets, top, bottom);
        assert!(
            range.contains(&10),
            "the first visible row must be included"
        );
        assert!(range.contains(&11), "the last visible row must be included");
        assert!(
            !range.contains(&9) && !range.contains(&12),
            "rows outside the window must not be declared"
        );
        assert!(range.len() < 100, "the window must stay bounded");
    }

    #[test]
    fn mixed_heights_still_binary_search_correctly() {
        let list = sample();
        let offsets = list.offsets();
        // The three-line row is the last element and starts after 40+56+72+9.
        let last = offsets[4];
        assert_eq!(last, 40.0 + 56.0 + 72.0 + 9.0);
        let range = list.visible_range(&offsets, last, last + 10.0);
        assert_eq!(range, 4..5);
    }
}
