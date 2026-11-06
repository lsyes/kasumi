//! Kasumi: an immediate-mode surface over a retained tree.

#![forbid(unsafe_code)]

mod cx;
pub mod dynamic_color;
mod response;
pub mod runtime;
mod theme;
mod transition;
mod ui;
pub mod widgets;
mod window;

pub use dynamic_color::{
    ContrastLevel, DynamicColorConfig, DynamicScheme, SchemeVariant, argb_from_color,
    argb_to_color, seed_from_image,
};
pub use response::{Response, ResponseFlags};
pub use runtime::{
    App, ClipboardAction, EmptyApp, FrameOutput, FrameStats, ImeState, Runtime, input_state,
};
pub use theme::{
    ColorScheme, DurationScale, ElevationScale, InteractionState, MotionEasing, MotionScale,
    ShapeScale, SpacingScale, StateLayerOpacity, Theme, ThemeMode, Typography,
};
pub use transition::{Transition, TransitionState};
pub use ui::{Ui, Widget};
pub use widgets::{
    Button, ButtonVariant, Card, CardVariant, Checkbox, ComboBox, Dialog, Divider,
    DividerOrientation, Image, Label, List, ListElement, ListLeading, ListResponse, ListRow,
    ListRowHeight, ListView, Markdown, Menu, MenuButton, MenuButtonOutput, MenuEntry, MenuItem,
    MenuPlacement, MenuSelection, NavHeader, NavItem, NavResponse, NavSection, NavVariant,
    NavigationDrawer, ProgressBar, QrCode, Radio, ScrollView, ScrollWindow, SearchBar,
    SearchResponse, SearchResult, Slider, Spacer, SubMenu, Svg, Switch, TextInput,
    TransitionHandle,
};
pub use window::{WindowAction, WindowInfo, WindowRequest};

pub use kasumi_core as core;
pub use kasumi_core::WindowKey;
pub use kasumi_state::{Resource, ResourceError, ResourceStatus, State};

/// Common imports for applications.
pub mod prelude {
    pub use crate::runtime::App;
    pub use crate::{
        Button, Card, CardVariant, Checkbox, ColorScheme, ComboBox, Dialog, Divider,
        DividerOrientation, Image, Label, List, ListElement, ListLeading, ListRow, ListRowHeight,
        ListView, Markdown, Menu, MenuButton, MenuEntry, MenuItem, MenuPlacement, MenuSelection,
        NavHeader, NavItem, NavResponse, NavSection, NavVariant, NavigationDrawer, ProgressBar,
        QrCode, Radio, Resource, ResourceError, ResourceStatus, Response, ScrollView, ScrollWindow,
        SearchBar, SearchResponse, SearchResult, Slider, Spacer, State, SubMenu, Svg, Switch,
        TextInput, Theme, ThemeMode, Transition, TransitionHandle, Ui, Widget, WindowAction,
        WindowInfo, WindowKey, WindowRequest,
    };
    pub use crate::{ContrastLevel, DynamicColorConfig, DynamicScheme, SchemeVariant};
    pub use kasumi_core::{
        Align, Color, Corners, CursorIcon, Edges, FontFamily, FontWeight, GridTrack, Id, ImageData,
        ImageFit, Inset, Justify, Length, Size, TextAlign, TextWrap,
    };
    pub use kasumi_tree::{
        Animated, AnimationState, Easing, LerpValue, Motion, MotionSpec, Spring,
    };
}
