//! Kasumi: an immediate-mode surface over a retained tree.

#![forbid(unsafe_code)]

mod cx;
mod response;
pub mod runtime;
mod theme;
mod ui;
pub mod widgets;

pub use response::{Response, ResponseFlags};
pub use runtime::{
    App, ClipboardAction, EmptyApp, FrameOutput, FrameStats, ImeState, Runtime, input_state,
};
pub use theme::Theme;
pub use ui::{Ui, Widget};
pub use widgets::{
    Button, ButtonVariant, Checkbox, ComboBox, Divider, DividerOrientation, Image, Label, Markdown,
    ProgressBar, QrCode, Radio, ScrollView, Slider, Spacer, Svg, TextInput,
};

pub use kasumi_core as core;
pub use kasumi_state::State;

/// Common imports for applications.
pub mod prelude {
    pub use crate::runtime::App;
    pub use crate::{
        Button, Checkbox, ComboBox, Divider, DividerOrientation, Image, Label, Markdown,
        ProgressBar, QrCode, Radio, Response, ScrollView, Slider, Spacer, State, Svg, TextInput,
        Theme, Ui, Widget,
    };
    pub use kasumi_core::{
        Align, Color, Corners, CursorIcon, Edges, FontFamily, FontWeight, GridTrack, Id, ImageData,
        ImageFit, Inset, Justify, Length, Size, TextAlign, TextWrap,
    };
}
