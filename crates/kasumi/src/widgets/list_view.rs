use kasumi_core::Length;

use crate::response::Response;
use crate::ui::{Ui, Widget};
use crate::widgets::ScrollView;

/// A fixed-height, virtualized vertical list.
///
/// Only rows intersecting the visible scroll window (plus a configurable
/// overscan) are declared each frame. Row identity is derived from its index,
/// so scrolling does not recreate widgets that stay visible and off-screen
/// rows release their node payload immediately.
///
/// ```ignore
/// ui.add(
///     ListView::new(rows.len(), 48.0, |ui, index| {
///         ui.row(|ui| {
///             ui.width_full().padding_xy(16.0, 0.0).align_items(Align::Center);
///             ui.label(&rows[index].title);
///         });
///     })
///     .size(Length::Percent(100.0), 320.0),
/// );
/// ```
///
/// Rows are laid out absolutely inside a single scroll content node, which has
/// the full list height. As a consequence, local widget state inside a row
/// disappears when the row scrolls out of range; lift anything that must
/// survive into [`crate::State`].
pub struct ListView<F> {
    rows: usize,
    row_height: f32,
    overscan: f32,
    width: Length,
    height: Length,
    grow: f32,
    smooth: bool,
    show_scrollbars: bool,
    render: F,
}

impl<F> ListView<F>
where
    F: FnMut(&mut Ui, usize),
{
    /// `rows` is the total item count; `row_height` is the fixed logical
    /// extent of every row.
    pub fn new(rows: usize, row_height: f32, render: F) -> Self {
        Self {
            rows,
            row_height: row_height.max(1.0),
            overscan: 240.0,
            width: Length::Percent(100.0),
            height: Length::Auto,
            grow: 0.0,
            smooth: true,
            show_scrollbars: true,
            render,
        }
    }

    /// Extra rendered space above and below the viewport, in logical pixels.
    pub fn overscan(mut self, overscan: f32) -> Self {
        self.overscan = overscan.max(0.0);
        self
    }

    pub fn size(mut self, width: impl Into<Length>, height: impl Into<Length>) -> Self {
        self.width = width.into();
        self.height = height.into();
        self
    }

    /// Lets the list consume the remaining space of its parent.
    pub fn grow(mut self, grow: f32) -> Self {
        self.grow = grow.max(0.0);
        self
    }

    pub fn smooth(mut self, smooth: bool) -> Self {
        self.smooth = smooth;
        self
    }

    pub fn scrollbars(mut self, show: bool) -> Self {
        self.show_scrollbars = show;
        self
    }

    pub fn row_count(&self) -> usize {
        self.rows
    }

    /// Total virtual extent in logical pixels.
    pub fn content_height(&self) -> f32 {
        self.rows as f32 * self.row_height
    }
}

impl<F> Widget for ListView<F>
where
    F: FnMut(&mut Ui, usize),
{
    type Output = Response;

    fn ui(mut self, ui: &mut Ui) -> Response {
        let rows = self.rows;
        let row_height = self.row_height;
        let render = &mut self.render;
        let view = ScrollView::vertical()
            .size(self.width, self.height)
            .grow(self.grow)
            .smooth(self.smooth)
            .scrollbars(self.show_scrollbars)
            .overscan(self.overscan)
            .virtual_rows(rows, row_height);
        ui.virtual_scroll(view, |ui, window| {
            for index in window.row_range(row_height, rows) {
                ui.keyed(index, |ui| {
                    ui.column(|ui| {
                        ui.absolute(0.0, index as f32 * row_height)
                            .width(Length::Percent(100.0))
                            .height(row_height);
                        render(ui, index);
                    });
                });
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_height_tracks_rows() {
        let list = ListView::new(10_000, 32.0, |_: &mut Ui, _| {}).grow(1.0);
        assert_eq!(list.row_count(), 10_000);
        assert_eq!(list.content_height(), 320_000.0);
    }

    #[test]
    fn zero_height_is_clamped() {
        let list = ListView::new(3, 0.0, |_: &mut Ui, _| {});
        assert_eq!(list.content_height(), 3.0);
    }
}
