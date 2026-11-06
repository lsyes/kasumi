//! Material 3 search bar and search view.
//!
//! [`SearchBar`] is a pill-shaped field with a leading magnifier, an optional
//! trailing action and an inline results popup. Typing filters the results
//! with a case-insensitive substring match; ↑/↓ move the highlight, `Enter`
//! activates it, and `Escape` closes the popup and then the field.
//!
//! ```ignore
//! let results = ["Overview", "Lists", "Menus", "Navigation"];
//! let search = SearchBar::new(&query)
//!     .placeholder("Search commands")
//!     .results(results.iter().map(|title| SearchResult::new(*title)));
//! let outcome = ui.search(&search);
//! if let Some(result) = outcome.activated {
//!     run_command(result);
//! }
//! ```
//!
//! `SearchBar::view(true)` renders the full-window Material *search view*
//! instead: a surface-container-high sheet over the page with the same field
//! docked at the top and the results as a list underneath. The view is a root
//! overlay that swallows clicks behind it, so it behaves like a modal without
//! needing a dialog.
//!
//! Layout follows the M3 search tokens: a 56 dp bar with a full-radius shape
//! and `surface-container-high` container, 24 dp leading and trailing icons,
//! `body-large` input text, 16 dp horizontal padding, and 56 dp result rows.
//! The bar's focus state uses the primary focus ring; the popup enters with
//! the same fade-and-scale motion as menus.

use kasumi_core::{
    Align, Color, Corners, CursorIcon, Edges, FlexDirection, Id, Inset, Justify, Key, Length,
    Style, TextAlign, TextWrap, child_id, keyed_id,
};
use kasumi_state::State;
use kasumi_tree::{MotionSpec, NodeKind, Sense, Spring};

use crate::response::Response;
use crate::ui::{Ui, Widget};

/// Height of the search bar.
pub const SEARCH_BAR_HEIGHT: f32 = 56.0;
/// Height of a result row.
pub const SEARCH_ROW_HEIGHT: f32 = 56.0;
/// Height of the full-screen search view header.
const SEARCH_VIEW_HEADER: f32 = 72.0;
/// Horizontal padding inside the bar.
const BAR_PADDING: f32 = 16.0;
/// Gap between the icons and the input.
const BAR_GAP: f32 = 8.0;
/// Width of the icon column.
const ICON_SIZE: f32 = 24.0;
/// Vertical padding around the inline results panel.
const PANEL_PADDING: f32 = 8.0;
/// Maximum number of rows the inline panel renders.
const MAX_RESULTS: usize = 8;
const MIN_PANEL_WIDTH: f32 = 240.0;

/// One search suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// Text shown in the row and reported back on activation.
    pub title: String,
    /// Optional secondary line.
    pub description: Option<String>,
    /// Optional leading glyph.
    pub icon: Option<String>,
}

impl SearchResult {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            icon: None,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query = query.to_lowercase();
        self.title.to_lowercase().contains(&query)
            || self
                .description
                .as_ref()
                .is_some_and(|text| text.to_lowercase().contains(&query))
    }
}

/// Outcome of a rendered search field.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResponse {
    /// Result activated with Enter or a click during this frame.
    pub activated: Option<String>,
    /// Current query after this frame.
    pub query: String,
    /// Retained response of the search bar.
    pub response: Response,
}

/// Declarative Material 3 search bar.
#[derive(Debug, Clone)]
pub struct SearchBar {
    query: State<String>,
    placeholder: String,
    results: Vec<SearchResult>,
    width: Length,
    view: bool,
    focused: Option<State<bool>>,
    show_clear: bool,
}

impl SearchBar {
    pub fn new(query: &State<String>) -> Self {
        Self {
            query: query.clone(),
            placeholder: "Search".to_owned(),
            results: Vec::new(),
            width: Length::Px(320.0),
            view: false,
            focused: None,
            show_clear: true,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn results(mut self, results: impl IntoIterator<Item = SearchResult>) -> Self {
        self.results = results.into_iter().collect();
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Binds an external open state. When the state flips to `true` the search
    /// field takes focus; `Escape` writes `false` once the popup is closed.
    pub fn focused(mut self, focused: &State<bool>) -> Self {
        self.focused = Some(focused.clone());
        self
    }

    /// Renders the full-window search view instead of the inline bar.
    pub fn view(mut self, view: bool) -> Self {
        self.view = view;
        self
    }

    pub fn clear_button(mut self, show: bool) -> Self {
        self.show_clear = show;
        self
    }

    /// Results matching the current query, up to the inline panel's limit.
    fn filtered(&self, query: &str) -> Vec<&SearchResult> {
        self.results
            .iter()
            .filter(|result| result.matches(query))
            .take(if self.view { usize::MAX } else { MAX_RESULTS })
            .collect()
    }
}

impl Widget for SearchBar {
    type Output = SearchResponse;

    fn ui(self, ui: &mut Ui) -> SearchResponse {
        let theme = ui.theme().clone();
        let id = ui.next_id("search bar");
        let node = ui.adopt(
            id,
            NodeKind::SearchField,
            Style {
                direction: FlexDirection::Row,
                align_items: Align::Center,
                width: self.width,
                height: Length::Px(SEARCH_BAR_HEIGHT),
                min_height: Length::Px(SEARCH_BAR_HEIGHT),
                padding: Edges::xy(4.0, 0.0),
                gap: BAR_GAP,
                background: theme.colors.surface_container_high,
                radius: Corners::all(theme.shape.full),
                ..Style::default()
            },
        );
        ui.cx.tree.set_accessibility_label(node, "Search");

        let response = ui.response_of(id);
        let panel_id = keyed_id(ui.scope, &("search-panel", id.raw()));
        // `popup_open` is the authoritative session flag. It is opened by a
        // press inside the pill or by an external `focused` state, and closed
        // by Escape, a selection, or a press outside both the bar and its
        // results. Nothing else may re-open it, which is what keeps a row that
        // still holds focus from resurrecting the panel after a selection.
        let mut open = ui
            .cx
            .tree
            .node_by_id(id)
            .map(|node| node.popup_open)
            .unwrap_or(false);
        let pressed_inside = ui
            .cx
            .active
            .is_some_and(|pressed| ui.cx.tree.is_within(pressed, id));
        if pressed_inside || response.clicked() {
            open = true;
        }
        if let Some(focused) = &self.focused
            && focused.get()
        {
            open = true;
        }
        if let Some(clicked) = ui.cx.clicked
            && !ui.cx.tree.is_within(clicked, id)
            && !ui.cx.tree.is_within(clicked, panel_id)
        {
            open = false;
            if let Some(state) = &self.focused
                && state.get()
            {
                state.set(false);
            }
        }

        // Leading magnifier.
        let magnifier_id = child_id(id, 0);
        ui.register_node_id(magnifier_id, "search icon");
        let magnifier = ui.cx.tree.ensure(magnifier_id, NodeKind::Text);
        ui.declare_child(node, magnifier);
        ui.cx.tree.set_a11y_hidden(magnifier, true);
        let mut glyph_style = theme.typography.headline_small.clone();
        glyph_style.size = 20.0;
        glyph_style.line_height = Some(22.0);
        glyph_style.color = theme.colors.on_surface_variant;
        glyph_style.wrap = TextWrap::None;
        glyph_style.align = TextAlign::Center;
        ui.set_style_on(
            magnifier,
            Style {
                text: glyph_style.clone(),
                width: Length::Px(ICON_SIZE),
                min_width: Length::Px(ICON_SIZE),
                flex_shrink: 0.0,
                align_self: Align::Center,
                margin: Edges::xy(12.0, 0.0),
                ..Style::default()
            },
        );
        ui.set_text_on(magnifier, "⌕".to_owned(), glyph_style);

        // The input is an embedded text editor. It shares its value with the
        // application through the same `State<String>`.
        let input_id = child_id(id, 1);
        ui.register_node_id(input_id, "search input");
        let input = ui.cx.tree.ensure(input_id, NodeKind::TextInput);
        ui.declare_child(node, input);
        let mut input_style = theme.typography.body_large.clone();
        input_style.color = theme.colors.on_surface;
        input_style.wrap = TextWrap::None;
        ui.set_style_on(
            input,
            Style {
                text: input_style.clone(),
                width: Length::Auto,
                height: Length::Px(SEARCH_BAR_HEIGHT - 16.0),
                min_width: Length::Px(0.0),
                flex_grow: 1.0,
                flex_basis: Length::Px(0.0),
                align_self: Align::Center,
                ..Style::default()
            },
        );
        ui.set_sense_on(input, Sense::click());
        ui.cx
            .tree
            .set_accessibility_label(input, self.placeholder.clone());

        // Clicking anywhere inside the pill focuses the editor, which is what
        // makes the popup open on the same press.
        let pressed_inside = ui
            .cx
            .active
            .is_some_and(|pressed| ui.cx.tree.is_within(pressed, id));
        if pressed_inside && ui.cx.focused != Some(input_id) {
            ui.cx.focused = Some(input_id);
            ui.request_repaint();
        }
        let bar_focused = ui
            .cx
            .focused
            .is_some_and(|focused| ui.cx.tree.is_within(focused, id));

        // Drive the embedded editor through the same path TextInput uses.
        let mut value = self.query.get();
        let mut editor = ui
            .cx
            .tree
            .node_by_id(input_id)
            .and_then(|node| node.text_input.clone())
            .unwrap_or_default();
        let focused = bar_focused;
        let mut changed = false;
        if focused {
            changed = crate::widgets::text_input::apply_edits(
                ui,
                &self.query,
                &mut value,
                &mut editor,
                theme.typography.body_large.size,
                true,
            );
        }
        let _ = changed;
        let shown = if value.is_empty() {
            self.placeholder.clone()
        } else {
            value.clone()
        };
        let mut shown_style = input_style.clone();
        if value.is_empty() {
            shown_style.color = theme.colors.on_surface_variant;
        }
        ui.set_text_on(input, shown, shown_style.clone());
        if let Some(node) = ui.cx.tree.get_mut(input) {
            node.text_input = Some(editor);
            node.changed = changed;
        }

        // Trailing clear affordance.
        let mut activated = None;
        let clear_id = child_id(id, 2);
        if self.show_clear && !value.is_empty() {
            ui.register_node_id(clear_id, "search clear");
            let clear = ui.cx.tree.ensure(clear_id, NodeKind::Button);
            ui.declare_child(node, clear);
            ui.cx.tree.set_accessibility_label(clear, "Clear search");
            ui.set_sense_on(clear, Sense::click());
            ui.set_style_on(
                clear,
                Style {
                    width: Length::Px(28.0),
                    height: Length::Px(28.0),
                    min_width: Length::Px(28.0),
                    justify: Justify::Center,
                    align_items: Align::Center,
                    radius: Corners::all(theme.shape.full),
                    state_layer: theme.colors.on_surface_variant,
                    cursor: CursorIcon::Pointer,
                    ..Style::default()
                },
            );
            let x_id = child_id(clear_id, 0);
            ui.register_node_id(x_id, "search clear glyph");
            let x = ui.cx.tree.ensure(x_id, NodeKind::Text);
            ui.declare_child(clear, x);
            ui.cx.tree.set_a11y_hidden(x, true);
            let mut style = theme.typography.title_small.clone();
            style.color = theme.colors.on_surface_variant;
            style.wrap = TextWrap::None;
            style.align = TextAlign::Center;
            ui.set_style_on(
                x,
                Style {
                    text: style.clone(),
                    align_self: Align::Center,
                    ..Style::default()
                },
            );
            ui.set_text_on(x, "✕".to_owned(), style);
            if ui.cx.response(clear_id).clicked() {
                self.query.set(String::new());
                ui.request_repaint();
            }
        }

        let query = self.query.get();
        let filtered: Vec<SearchResult> = self.filtered(&query).into_iter().cloned().collect();

        // Escape closes the session before the panel is declared, so the
        // closed frame leaves no rows behind for the accessibility tree.
        let escaping = ui.key_pressed(&Key::Escape);
        if escaping {
            open = false;
            ui.cx.focused = None;
            if let Some(state) = &self.focused {
                state.set(false);
            }
        }
        if let Some(tree_node) = ui.cx.tree.get_mut(node) {
            tree_node.popup_open = open;
        }
        if open {
            let index = ui
                .cx
                .tree
                .node_by_id(id)
                .map(|node| node.slider_value as usize)
                .unwrap_or(0);
            let mut index = index.min(filtered.len().saturating_sub(1));
            if focused && ui.key_pressed(&Key::ArrowDown) {
                index = (index + 1) % filtered.len();
                if let Some(tree_node) = ui.cx.tree.get_mut(node) {
                    tree_node.slider_value = index as f32;
                }
                ui.request_repaint();
            } else if focused && ui.key_pressed(&Key::ArrowUp) {
                index = index.saturating_sub(1);
                if let Some(tree_node) = ui.cx.tree.get_mut(node) {
                    tree_node.slider_value = index as f32;
                }
                ui.request_repaint();
            }
            let highlight = filtered.get(index).cloned();
            if focused
                && (ui.key_pressed(&Key::Enter) || ui.key_pressed(&Key::Space))
                && let Some(result) = highlight.clone()
            {
                activated = Some(result.title);
            }
            let selection = self.result_panel(ui, id, &filtered, index, self.view);
            if let Some(result) = selection {
                activated = Some(result);
                if let Some(tree_node) = ui.cx.tree.get_mut(node) {
                    tree_node.popup_open = false;
                }
                ui.cx.focused = None;
            } else if activated.is_none() {
                // Keyboard activation closes on the same frame as the popup
                // that was just rendered above.
                if let Some(tree_node) = ui.cx.tree.get_mut(node) {
                    tree_node.popup_open = open && ui.cx.focused.is_some();
                }
            }
        }

        if let Some(result) = &activated
            && let Some(focused_state) = &self.focused
        {
            let _ = result;
            focused_state.set(false);
        }

        SearchResponse {
            activated,
            query,
            response,
        }
    }
}

impl SearchBar {
    /// Renders the inline results panel or the full search view. Returns the
    /// row the user pressed.
    fn result_panel(
        &self,
        ui: &mut Ui,
        owner: Id,
        results: &[SearchResult],
        highlight: usize,
        full_view: bool,
    ) -> Option<String> {
        let theme = ui.theme().clone();
        let viewport = ui.viewport();
        let (x, y, width, max_height) = if full_view {
            (0.0, 0.0, viewport.width, viewport.height)
        } else {
            let rect = ui.cx.tree.visual_rect_of(owner);
            let width = rect.width().max(MIN_PANEL_WIDTH);
            let x = rect.left().min((viewport.width - width - 8.0).max(8.0));
            let y = rect.bottom() + 4.0;
            let max_height = (viewport.height - y - 8.0).max(SEARCH_ROW_HEIGHT);
            (x, y, width, max_height)
        };
        let rows_height = results.len() as f32 * SEARCH_ROW_HEIGHT + PANEL_PADDING * 2.0;
        let height = if full_view {
            viewport.height
        } else {
            rows_height.min(max_height)
        };
        let mut activated = None;
        let (_, panel_id) = ((), keyed_id(ui.scope, &("search-panel", owner.raw())));
        ui.overlay_at(
            ("search-panel", owner.raw()),
            "search panel",
            owner,
            Inset::at(x, y),
            Style {
                direction: FlexDirection::Column,
                width: Length::Px(width),
                height: Length::Px(height),
                padding: Edges::all(if full_view { 0.0 } else { PANEL_PADDING }),
                background: if full_view {
                    theme.colors.surface_container_high
                } else {
                    theme.surface_at_elevation(2)
                },
                radius: Corners::all(if full_view {
                    0.0
                } else {
                    theme.shape.extra_large
                }),
                clip: true,
                ..Style::default()
            },
            |ui| {
                if full_view {
                    // A scrim row and a title keep the view from looking like
                    // a floating menu.
                    ui.row(|ui| {
                        ui.width_full()
                            .height(SEARCH_VIEW_HEADER)
                            .padding_edges(Edges {
                                left: 16.0,
                                right: 16.0,
                                top: 0.0,
                                bottom: 0.0,
                            })
                            .align_items(Align::Center)
                            .gap(12.0);
                        ui.add(
                            crate::Label::new("Search")
                                .font_size(22.0)
                                .color(theme.colors.on_surface),
                        );
                        ui.spacer();
                        ui.add(
                            crate::Label::new(format!("{} results", results.len()))
                                .font_size(11.0)
                                .color(theme.colors.on_surface_variant)
                                .family(kasumi_core::FontFamily::Monospace),
                        );
                    });
                }
                let scale = ui.animate_on(
                    ui.cx.tree.root(),
                    "search-panel-open",
                    1.0,
                    MotionSpec::spring(Spring::new(0.2, 0.9)),
                );
                let _ = scale;
                for (index, result) in results.iter().enumerate() {
                    result_row(
                        ui,
                        owner,
                        result,
                        index,
                        index == highlight,
                        &theme,
                        &mut activated,
                    );
                }
            },
        );
        let _ = panel_id;
        activated
    }
}

fn result_row(
    ui: &mut Ui,
    owner: Id,
    result: &SearchResult,
    index: usize,
    highlighted: bool,
    theme: &crate::Theme,
    activated: &mut Option<String>,
) {
    let id = keyed_id(owner, &("search-result", index));
    ui.register_node_id(id, "search result");
    // The panel is an overlay, so the row is declared under it via `adopt`.
    let row = ui.adopt(
        id,
        NodeKind::MenuItem,
        Style {
            direction: FlexDirection::Row,
            align_items: Align::Center,
            width: Length::Percent(100.0),
            height: Length::Px(SEARCH_ROW_HEIGHT),
            min_height: Length::Px(SEARCH_ROW_HEIGHT),
            padding: Edges::xy(BAR_PADDING, 0.0),
            gap: 12.0,
            background: if highlighted {
                theme.colors.secondary_container
            } else {
                Color::TRANSPARENT
            },
            radius: Corners::all(theme.shape.extra_small),
            state_layer: theme.colors.on_surface,
            cursor: CursorIcon::Pointer,
            ..Style::default()
        },
    );
    ui.cx
        .tree
        .set_accessibility_label(row, result.title.clone());
    ui.set_sense_on(row, Sense::click());

    if let Some(icon) = &result.icon {
        let mut style = theme.typography.title_medium.clone();
        style.color = if highlighted {
            theme.colors.on_secondary_container
        } else {
            theme.colors.on_surface_variant
        };
        style.wrap = TextWrap::None;
        style.align = TextAlign::Center;
        let icon_id = child_id(id, 0);
        ui.register_node_id(icon_id, "search result icon");
        let glyph = ui.cx.tree.ensure(icon_id, NodeKind::Text);
        ui.declare_child(row, glyph);
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

    let column_id = child_id(id, 10);
    ui.register_node_id(column_id, "search result text");
    let column = ui.cx.tree.ensure(column_id, NodeKind::Container);
    ui.declare_child(row, column);
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
    title_style.color = if highlighted {
        theme.colors.on_secondary_container
    } else {
        theme.colors.on_surface
    };
    title_style.wrap = TextWrap::None;
    title_style.align = TextAlign::Start;
    let title_id = child_id(column_id, 0);
    ui.register_node_id(title_id, "search result title");
    let title = ui.cx.tree.ensure(title_id, NodeKind::Text);
    ui.declare_child(column, title);
    ui.set_style_on(
        title,
        Style {
            text: title_style.clone(),
            align_self: Align::Start,
            min_width: Length::Px(0.0),
            ..Style::default()
        },
    );
    ui.set_text_on(title, result.title.clone(), title_style);
    if let Some(description) = &result.description {
        let mut style = theme.typography.body_medium.clone();
        style.color = if highlighted {
            theme.colors.on_secondary_container
        } else {
            theme.colors.on_surface_variant
        };
        style.wrap = TextWrap::None;
        style.align = TextAlign::Start;
        let description_id = child_id(column_id, 1);
        ui.register_node_id(description_id, "search result description");
        let description_node = ui.cx.tree.ensure(description_id, NodeKind::Text);
        ui.declare_child(column, description_node);
        ui.set_style_on(
            description_node,
            Style {
                text: style.clone(),
                align_self: Align::Start,
                min_width: Length::Px(0.0),
                ..Style::default()
            },
        );
        ui.set_text_on(description_node, description.clone(), style);
    }

    if ui.cx.response(id).clicked() {
        *activated = Some(result.title.clone());
    }
}

impl Ui<'_> {
    /// Renders a search bar and reports the result the user activated.
    pub fn search(&mut self, search: &SearchBar) -> SearchResponse {
        search.clone().ui(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<SearchResult> {
        vec![
            SearchResult::new("Overview").description("The starting page"),
            SearchResult::new("Lists"),
            SearchResult::new("Menus").icon("≡"),
            SearchResult::new("Navigation"),
        ]
    }

    #[test]
    fn filtering_is_case_insensitive_and_matches_descriptions() {
        let state = std::rc::Rc::new(kasumi_state::StateStore::new());
        state.begin_frame(1);
        let query = State::new(state.clone(), Id::from_raw(7), "test".into(), || {
            "menu".to_owned()
        });
        let bar = SearchBar::new(&query).results(sample());
        let matched: Vec<&str> = bar
            .filtered("menu")
            .into_iter()
            .map(|result| result.title.as_str())
            .collect();
        assert_eq!(matched, vec!["Menus"]);
        let matched: Vec<&str> = bar
            .filtered("START")
            .into_iter()
            .map(|result| result.title.as_str())
            .collect();
        assert_eq!(matched, vec!["Overview"], "descriptions must match too");
        assert_eq!(bar.filtered("").len(), 4);
    }

    #[test]
    fn empty_query_lists_every_result() {
        let state = std::rc::Rc::new(kasumi_state::StateStore::new());
        state.begin_frame(1);
        let query = State::new(state.clone(), Id::from_raw(3), "test".into(), String::new);
        let bar = SearchBar::new(&query).results(sample());
        assert_eq!(bar.filtered("").len(), 4);
        assert_eq!(bar.filtered("zzz").len(), 0);
    }
}
