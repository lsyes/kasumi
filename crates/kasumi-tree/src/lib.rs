//! The retained node tree.

#![forbid(unsafe_code)]

mod a11y;
mod animation;
mod arena;
mod node;
mod scroll_physics;
pub mod switch_paint;
mod tree;

pub use a11y::{A11yNode, A11ySnapshot, Role};
pub use animation::{Animated, AnimationState, Easing, LerpValue, Motion, MotionSpec, Spring};
pub use arena::{Arena, NodeId};
pub use node::{
    Animation, Node, NodeKind, ScrollState, Sense, TextEdit, TextEditSnapshot, TextInputState,
    TextMotion,
};
pub use scroll_physics::ScrollPhysics;
pub use switch_paint::SwitchPaint;
pub use tree::{ScrollAxis, ScrollKind, ScrollbarGeometry, Tree};
