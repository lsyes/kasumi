//! The retained node tree.

#![forbid(unsafe_code)]

mod a11y;
mod arena;
mod node;
mod tree;

pub use a11y::{A11yNode, A11ySnapshot, Role};
pub use arena::{Arena, NodeId};
pub use node::{
    Animation, Node, NodeKind, ScrollState, Sense, TextEdit, TextEditSnapshot, TextInputState,
    TextMotion,
};
pub use tree::{ScrollAxis, ScrollbarGeometry, Tree};
