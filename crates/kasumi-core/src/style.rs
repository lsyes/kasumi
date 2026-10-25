//! The user-facing style description.
//!
//! `Style` is plain data: the UI layer writes it, the reconcile step compares
//! it, the layout layer converts it to taffy, and the paint layer reads its
//! visual fields. No handle, no lifetime, no interior mutability.

use crate::color::Color;
use crate::geometry::{Corners, Edges};
use crate::text::TextStyle;

/// Main axis direction for flex containers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

/// Cross-axis alignment and main-axis distribution.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
    #[default]
    Auto,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// A length relative to the parent or a fixed pixel value.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    Percent(f32),
}

impl Length {
    pub fn px(value: f32) -> Self {
        Self::Px(value)
    }

    pub fn percent(value: f32) -> Self {
        Self::Percent(value)
    }

    pub fn is_auto(self) -> bool {
        matches!(self, Self::Auto)
    }

    pub fn resolve(self, parent: Option<f32>) -> Option<f32> {
        match self {
            Self::Auto => None,
            Self::Px(value) => Some(value),
            Self::Percent(value) => parent.map(|parent| parent * value / 100.0),
        }
    }
}

/// What the mouse cursor should look like over a node.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CursorIcon {
    #[default]
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    Crosshair,
    NotAllowed,
    ResizeHorizontal,
    ResizeVertical,
}

/// Container layout algorithm.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutMode {
    #[default]
    Flex,
    Grid,
}

/// CSS-like placement mode. Absolute nodes are laid out relative to their
/// parent without taking space in normal flow.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Position {
    #[default]
    Relative,
    Absolute,
}

/// A grid track sizing rule.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum GridTrack {
    #[default]
    Auto,
    Px(f32),
    Percent(f32),
    Fr(f32),
    MinContent,
    MaxContent,
}

impl GridTrack {
    pub const fn px(value: f32) -> Self {
        Self::Px(value)
    }

    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }

    pub const fn fr(value: f32) -> Self {
        Self::Fr(value)
    }
}

/// Absolute-position offsets. Each edge defaults to `Auto`.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Inset {
    pub left: Length,
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
}

impl Inset {
    pub fn at(x: f32, y: f32) -> Self {
        Self {
            left: Length::Px(x),
            top: Length::Px(y),
            right: Length::Auto,
            bottom: Length::Auto,
        }
    }

    pub fn all(value: Length) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }
}

/// Layout plus visual description for one node.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    // Flex layout.
    pub direction: FlexDirection,
    pub wrap: bool,
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub padding: Edges,
    pub margin: Edges,
    pub gap: f32,
    pub align_items: Align,
    pub justify: Justify,
    pub align_self: Align,
    pub flex_grow: f32,
    pub flex_shrink: f32,

    // Grid. Tracks describe containers; spans describe this node as an item.
    pub layout: LayoutMode,
    pub grid_columns: Vec<GridTrack>,
    pub grid_rows: Vec<GridTrack>,
    pub grid_column_span: u16,
    pub grid_row_span: u16,

    // Positioning and overflow.
    pub position: Position,
    pub inset: Inset,
    pub scroll_x: bool,
    pub scroll_y: bool,

    // Painting.
    pub background: Color,
    pub border_color: Color,
    pub border_width: f32,
    pub radius: Corners,
    /// Drawn as an inset ring when the node has keyboard focus. Purely visual;
    /// does not participate in layout.
    pub focus_ring: Color,
    pub clip: bool,
    pub cursor: CursorIcon,

    // Text. Only meaningful for text nodes, but keeping it here means text
    // style changes participate in style comparison and layout invalidation.
    pub text: TextStyle,

    /// Hide the node from layout and painting. Unlike removing it, keeps its
    /// identity and state alive.
    pub hidden: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            direction: FlexDirection::Row,
            wrap: false,
            width: Length::Auto,
            height: Length::Auto,
            // Flex items default to a zero minimum in CSS terms. This lets
            // rows and columns shrink to their share of the window instead of
            // expanding to the min-content width of every descendant.
            min_width: Length::Px(0.0),
            min_height: Length::Px(0.0),
            max_width: Length::Auto,
            max_height: Length::Auto,
            padding: Edges::ZERO,
            margin: Edges::ZERO,
            gap: 0.0,
            align_items: Align::Auto,
            justify: Justify::Start,
            align_self: Align::Auto,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            layout: LayoutMode::Flex,
            grid_columns: Vec::new(),
            grid_rows: Vec::new(),
            grid_column_span: 1,
            grid_row_span: 1,
            position: Position::Relative,
            inset: Inset::default(),
            scroll_x: false,
            scroll_y: false,
            background: Color::TRANSPARENT,
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: Corners::ZERO,
            focus_ring: Color::TRANSPARENT,
            clip: false,
            cursor: CursorIcon::Default,
            text: TextStyle::default(),
            hidden: false,
        }
    }
}

impl Style {
    pub fn row() -> Self {
        Self {
            direction: FlexDirection::Row,
            align_items: Align::Center,
            ..Self::default()
        }
    }

    pub fn column() -> Self {
        Self {
            direction: FlexDirection::Column,
            ..Self::default()
        }
    }

    pub fn grid(columns: impl IntoIterator<Item = GridTrack>) -> Self {
        Self {
            layout: LayoutMode::Grid,
            grid_columns: columns.into_iter().collect(),
            align_items: Align::Stretch,
            ..Self::default()
        }
    }

    pub fn set_padding(&mut self, value: f32) -> &mut Self {
        self.padding = Edges::all(value);
        self
    }

    pub fn set_padding_xy(&mut self, x: f32, y: f32) -> &mut Self {
        self.padding = Edges::xy(x, y);
        self
    }

    pub fn set_gap(&mut self, value: f32) -> &mut Self {
        self.gap = value;
        self
    }

    pub fn set_width(&mut self, value: impl Into<Length>) -> &mut Self {
        self.width = value.into();
        self
    }

    pub fn set_height(&mut self, value: impl Into<Length>) -> &mut Self {
        self.height = value.into();
        self
    }

    pub fn set_size_full(&mut self) -> &mut Self {
        self.width = Length::Percent(100.0);
        self.height = Length::Percent(100.0);
        self
    }

    pub fn set_background(&mut self, color: Color) -> &mut Self {
        self.background = color;
        self
    }

    pub fn set_border(&mut self, width: f32, color: Color) -> &mut Self {
        self.border_width = width;
        self.border_color = color;
        self
    }

    pub fn set_radius(&mut self, radius: f32) -> &mut Self {
        self.radius = Corners::all(radius);
        self
    }

    pub fn set_align_items(&mut self, align: Align) -> &mut Self {
        self.align_items = align;
        self
    }

    pub fn set_justify(&mut self, justify: Justify) -> &mut Self {
        self.justify = justify;
        self
    }

    pub fn set_grow(&mut self, grow: f32) -> &mut Self {
        self.flex_grow = grow;
        self
    }

    pub fn set_text(&mut self, text: TextStyle) -> &mut Self {
        self.text = text;
        self
    }

    pub fn with_padding(mut self, value: f32) -> Self {
        self.set_padding(value);
        self
    }

    pub fn with_padding_xy(mut self, x: f32, y: f32) -> Self {
        self.set_padding_xy(x, y);
        self
    }

    pub fn with_gap(mut self, value: f32) -> Self {
        self.set_gap(value);
        self
    }

    pub fn with_width(mut self, value: impl Into<Length>) -> Self {
        self.set_width(value);
        self
    }

    pub fn with_height(mut self, value: impl Into<Length>) -> Self {
        self.set_height(value);
        self
    }

    pub fn with_size_full(mut self) -> Self {
        self.set_size_full();
        self
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub fn with_border(mut self, width: f32, color: Color) -> Self {
        self.set_border(width, color);
        self
    }

    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = Corners::all(radius);
        self
    }

    pub fn with_align_items(mut self, align: Align) -> Self {
        self.align_items = align;
        self
    }

    pub fn with_justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }

    pub fn with_grow(mut self, grow: f32) -> Self {
        self.flex_grow = grow;
        self
    }

    pub fn with_text(mut self, text: TextStyle) -> Self {
        self.text = text;
        self
    }

    pub fn with_focus_ring(mut self, color: Color) -> Self {
        self.focus_ring = color;
        self
    }

    pub fn with_cursor(mut self, cursor: CursorIcon) -> Self {
        self.cursor = cursor;
        self
    }

    pub fn with_layout(mut self, layout: LayoutMode) -> Self {
        self.layout = layout;
        self
    }

    pub fn with_grid_columns(mut self, columns: impl IntoIterator<Item = GridTrack>) -> Self {
        self.layout = LayoutMode::Grid;
        self.grid_columns = columns.into_iter().collect();
        self
    }

    pub fn with_grid_rows(mut self, rows: impl IntoIterator<Item = GridTrack>) -> Self {
        self.layout = LayoutMode::Grid;
        self.grid_rows = rows.into_iter().collect();
        self
    }

    pub fn with_grid_span(mut self, columns: u16, rows: u16) -> Self {
        self.grid_column_span = columns.max(1);
        self.grid_row_span = rows.max(1);
        self
    }

    pub fn with_absolute_position(mut self, inset: Inset) -> Self {
        self.position = Position::Absolute;
        self.inset = inset;
        self
    }

    pub fn with_scroll(mut self, x: bool, y: bool) -> Self {
        self.scroll_x = x;
        self.scroll_y = y;
        self.clip = x || y || self.clip;
        self
    }
}

impl From<f32> for Length {
    fn from(value: f32) -> Self {
        Self::Px(value)
    }
}
