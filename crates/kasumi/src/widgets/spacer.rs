use kasumi_core::Style;
use kasumi_tree::NodeKind;

use crate::response::Response;
use crate::ui::{Ui, Widget};

/// A flexible empty node. Useful for pushing siblings apart in a row or
/// column.
#[derive(Debug, Default, Clone, Copy)]
pub struct Spacer;

impl Widget for Spacer {
    type Output = Response;

    fn ui(self, ui: &mut Ui) -> Response {
        let id = ui.next_id("spacer");
        let style = Style {
            flex_grow: 1.0,
            ..Style::default()
        };
        ui.adopt(id, NodeKind::Spacer, style);
        ui.response_of(id)
    }
}
