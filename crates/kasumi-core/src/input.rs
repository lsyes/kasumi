//! Input state for one frame.
//!
//! The backend translates window events into a fresh `InputState` per frame.
//! The runtime consumes the events, updates persistent interaction state, and
//! discards the list before calling `view`.

use crate::geometry::{Point, Size, Vec2};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
    Other(u16),
}

/// Named keys the framework understands. Printable text arrives separately as
/// `InputEvent::Text`, which keeps IME and keyboard layout handling out of the
/// widget layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Tab,
    BackTab,
    Enter,
    Escape,
    Backspace,
    Delete,
    Home,
    End,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    PageUp,
    PageDown,
    Space,
    /// A printable key, independent of the text produced. Shortcut handling
    /// uses this because Ctrl/Command combinations do not produce `Text`.
    Character(String),
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    PointerMoved(Point),
    PointerDown {
        position: Point,
        button: MouseButton,
    },
    PointerUp {
        position: Point,
        button: MouseButton,
    },
    PointerLeft,
    Scroll(Vec2),
    KeyDown {
        key: Key,
        repeat: bool,
    },
    KeyUp {
        key: Key,
    },
    Text(String),
    /// Text read from the platform clipboard by the backend.
    Paste(String),
    /// Clipboard text explicitly routed to a text input. Used by context-menu
    /// paste so focus moving to the menu item cannot redirect the insertion.
    PasteTo {
        target: crate::Id,
        text: String,
    },
    /// Active IME composition. The cursor offsets are byte offsets into
    /// `text`, when the input method provides them.
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    ModifiersChanged(Modifiers),
    FocusChanged(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputState {
    /// Seconds since application start, used for animations.
    pub time: f64,
    pub viewport: Size,
    pub scale_factor: f32,
    pub pointer: Point,
    pub pointer_inside: bool,
    pub modifiers: Modifiers,
    pub events: Vec<InputEvent>,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            time: 0.0,
            viewport: Size::new(800.0, 600.0),
            scale_factor: 1.0,
            pointer: Point::ZERO,
            pointer_inside: true,
            modifiers: Modifiers::default(),
            events: Vec::new(),
        }
    }
}

impl InputState {
    /// Takes the events collected for this frame. Called once by the runtime.
    pub fn take_events(&mut self) -> Vec<InputEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn is_shift_down(&self) -> bool {
        self.modifiers.shift
    }
}
