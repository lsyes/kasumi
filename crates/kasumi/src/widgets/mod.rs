//! Built-in widgets. They are ordinary types implementing [`crate::Widget`], not
//! special cases in the reconciler.

mod button;
mod cache;
mod card;
mod combo_box;
mod dialog;
mod editor_ops;
mod feedback;
mod form_controls;
pub mod highlight;
mod image;
mod label;
mod list;
mod list_view;
mod markdown;
mod menu;
mod navigation;
mod qr_code;
mod scroll_view;
mod search;
mod slider;
mod spacer;
mod svg;
mod switch;
mod text_editor;
mod text_input;
mod transition;

pub use button::{Button, ButtonVariant};
pub use card::{Card, CardVariant};
pub use combo_box::ComboBox;
pub use dialog::Dialog;
pub use feedback::{Divider, DividerOrientation, ProgressBar};
pub use form_controls::{Checkbox, Radio};
pub use highlight::{
    Highlighter, Language, PlainHighlighter, SyntaxColors, SyntaxHighlighter, Token, TokenKind,
};
pub use image::Image;
pub use label::Label;
pub use list::{
    DIVIDER_HEIGHT, HEADER_HEIGHT, List, ListElement, ListLeading, ListResponse, ListRow,
    ListRowHeight, ONE_LINE_HEIGHT, THREE_LINE_HEIGHT, TWO_LINE_HEIGHT, simple_list,
};
pub use list_view::ListView;
pub use markdown::Markdown;
pub(crate) use menu::MenuState;
pub use menu::{
    Menu, MenuButton, MenuButtonOutput, MenuEntry, MenuItem, MenuPlacement, MenuSelection, SubMenu,
};
pub use navigation::{
    DRAWER_WIDTH, NavHeader, NavItem, NavResponse, NavSection, NavVariant, NavigationDrawer,
    RAIL_WIDTH,
};
pub use qr_code::QrCode;
pub use scroll_view::{ScrollView, ScrollWindow};
pub use search::{SEARCH_BAR_HEIGHT, SEARCH_ROW_HEIGHT, SearchBar, SearchResponse, SearchResult};
pub use slider::Slider;
pub use spacer::Spacer;
pub use svg::Svg;
pub use switch::{
    STATE_LAYER, Switch, SwitchGeometry, THUMB_OFF, THUMB_ON, THUMB_PRESSED, TOUCH_HEIGHT,
    TRACK_HEIGHT, TRACK_WIDTH,
};
pub use text_editor::{EditorStatus, TextEditor};
pub use text_input::TextInput;
pub use transition::TransitionHandle;
