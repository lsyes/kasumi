//! Core data types shared by the Kasumi crates.
//!
//! This crate deliberately has no dependencies. It contains the vocabulary
//! that crosses layer boundaries: ids, geometry, colors, styles, input events
//! and draw-list primitives.

#![forbid(unsafe_code)]

pub mod color;
pub mod draw;
pub mod geometry;
pub mod id;
pub mod image;
pub mod input;
pub mod style;
pub mod text;

pub use color::Color;
pub use draw::{DrawItem, DrawList, GlyphPrimitive, ImagePrimitive, RectPrimitive};
pub use geometry::{Corners, Edges, Point, Rect, Size, Vec2};
pub use id::{Id, child_id, hash_str, keyed_id, mix, state_id};
pub use image::{ImageData, ImageFit};
pub use input::{InputEvent, InputState, Key, Modifiers, MouseButton};
pub use style::{
    Align, CursorIcon, FlexDirection, GridTrack, Inset, Justify, LayoutMode, Length, Position,
    Style,
};
pub use text::{FontFaceId, FontFamily, FontWeight, TextAlign, TextContent, TextStyle, TextWrap};
