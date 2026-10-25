//! Built-in widgets. They are ordinary types implementing [`crate::Widget`], not
//! special cases in the reconciler.

mod button;
mod cache;
mod combo_box;
mod feedback;
mod form_controls;
mod image;
mod label;
mod markdown;
mod qr_code;
mod scroll_view;
mod slider;
mod spacer;
mod svg;
mod text_input;

pub use button::{Button, ButtonVariant};
pub use combo_box::ComboBox;
pub use feedback::{Divider, DividerOrientation, ProgressBar};
pub use form_controls::{Checkbox, Radio};
pub use image::Image;
pub use label::Label;
pub use markdown::Markdown;
pub use qr_code::QrCode;
pub use scroll_view::ScrollView;
pub use slider::Slider;
pub use spacer::Spacer;
pub use svg::Svg;
pub use text_input::TextInput;
