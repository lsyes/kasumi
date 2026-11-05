//! Kasumi: an immediate-mode surface over a retained tree.

#![forbid(unsafe_code)]

mod cx;
mod response;
pub mod runtime;
mod theme;
mod transition;
mod ui;
pub mod widgets;
mod window;

pub use response::{Response, ResponseFlags};
pub use runtime::{
    App, ClipboardAction, EmptyApp, FrameOutput, FrameStats, ImeState, Runtime, input_state,
};
pub use theme::Theme;
pub use transition::{Transition, TransitionState};
pub use ui::{Ui, Widget};
pub use widgets::{
    Button, ButtonVariant, Card, CardVariant, Checkbox, ComboBox, Dialog, Divider,
    DividerOrientation, Image, Label, ListView, Markdown, ProgressBar, QrCode, Radio, ScrollView,
    ScrollWindow, Slider, Spacer, Svg, TextInput, TransitionHandle,
};
pub use window::{WindowAction, WindowInfo, WindowRequest};

pub use kasumi_core as core;
pub use kasumi_core::WindowKey;
pub use kasumi_state::State;

/// Common imports for applications.
pub mod prelude {
    pub use crate::runtime::App;
    pub use crate::{
        Button, Card, CardVariant, Checkbox, ComboBox, Dialog, Divider, DividerOrientation, Image,
        Label, ListView, Markdown, ProgressBar, QrCode, Radio, Response, ScrollView, ScrollWindow,
        Slider, Spacer, State, Svg, TextInput, Theme, Transition, TransitionHandle, Ui, Widget,
        WindowAction, WindowInfo, WindowKey, WindowRequest,
    };
    pub use kasumi_core::{
        Align, Color, Corners, CursorIcon, Edges, FontFamily, FontWeight, GridTrack, Id, ImageData,
        ImageFit, Inset, Justify, Length, Size, TextAlign, TextWrap,
    };
    pub use kasumi_tree::{
        Animated, AnimationState, Easing, LerpValue, Motion, MotionSpec, Spring,
    };
}
