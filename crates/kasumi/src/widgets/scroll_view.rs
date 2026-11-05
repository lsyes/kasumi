use kasumi_core::{FlexDirection, Length, Size};

/// Configuration for a scrollable viewport.
#[derive(Debug, Clone, Copy)]
pub struct ScrollView {
    pub horizontal: bool,
    pub vertical: bool,
    pub smooth: bool,
    pub width: Length,
    pub height: Length,
    pub grow: f32,
    pub show_scrollbars: bool,
    /// Full content extent declared without laying out off-screen children.
    ///
    /// Set by [`ScrollView::virtual_content`] or [`ScrollView::virtual_rows`]
    /// and consumed by [`crate::Ui::virtual_scroll`]. A caller that passes this
    /// configuration to [`crate::Ui::scroll`] gets the normal eager layout.
    pub virtual_content: Option<Size>,
    /// Extra space, in logical pixels, rendered above and below the visible
    /// window by virtualized widgets. This hides the one-frame latency of
    /// retained viewport measurements while scrolling quickly.
    pub overscan: f32,
}

impl ScrollView {
    pub fn vertical() -> Self {
        Self {
            horizontal: false,
            vertical: true,
            smooth: true,
            width: Length::Percent(100.0),
            height: Length::Auto,
            grow: 0.0,
            show_scrollbars: true,
            virtual_content: None,
            overscan: 240.0,
        }
    }

    pub fn horizontal() -> Self {
        Self {
            horizontal: true,
            vertical: false,
            smooth: true,
            width: Length::Auto,
            height: Length::Percent(100.0),
            grow: 0.0,
            show_scrollbars: true,
            virtual_content: None,
            overscan: 240.0,
        }
    }

    pub fn both() -> Self {
        Self {
            horizontal: true,
            vertical: true,
            smooth: true,
            width: Length::Percent(100.0),
            height: Length::Percent(100.0),
            grow: 0.0,
            show_scrollbars: true,
            virtual_content: None,
            overscan: 240.0,
        }
    }

    pub fn smooth(mut self, smooth: bool) -> Self {
        self.smooth = smooth;
        self
    }

    pub fn size(mut self, width: impl Into<Length>, height: impl Into<Length>) -> Self {
        self.width = width.into();
        self.height = height.into();
        self
    }

    /// Lets the viewport consume remaining space in the parent flex layout.
    pub fn grow(mut self, grow: f32) -> Self {
        self.grow = grow.max(0.0);
        self
    }

    pub fn scrollbars(mut self, show: bool) -> Self {
        self.show_scrollbars = show;
        self
    }

    /// Declares the complete content size without building off-screen nodes.
    ///
    /// The next call to [`crate::Ui::virtual_scroll`] uses this extent for the
    /// scrollbar, the scroll range and layout; only the children declared by
    /// the closure exist in the tree.
    pub fn virtual_content(mut self, size: impl Into<Size>) -> Self {
        self.virtual_content = Some(size.into());
        self
    }

    /// Convenience for fixed-height virtual lists: `rows * row_height`.
    pub fn virtual_rows(mut self, rows: usize, row_height: f32) -> Self {
        self.virtual_content = Some(Size::new(0.0, rows as f32 * row_height.max(0.0)));
        self
    }

    /// Extra rendered space around the visible window, in logical pixels.
    pub fn overscan(mut self, overscan: f32) -> Self {
        self.overscan = overscan.max(0.0);
        self
    }

    pub fn direction(self) -> FlexDirection {
        if self.horizontal && !self.vertical {
            FlexDirection::Row
        } else {
            FlexDirection::Column
        }
    }
}

/// Geometry available while declaring a virtualized scroll view's children.
///
/// `offset` is the retained scroll position and `viewport` is the size of the
/// scroll container from the previous layout pass. On the first frame the
/// viewport falls back to the explicitly configured size (or the window when
/// that size is automatic), so list widgets render an initial window instead
/// of producing an empty frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollWindow {
    pub offset: kasumi_core::Point,
    pub viewport: Size,
    pub content: Size,
    /// Overscan inherited from the scroll view configuration.
    pub overscan: f32,
}

impl ScrollWindow {
    /// The visible row range for a fixed row extent, including overscan.
    ///
    /// The returned range is clamped to `0..row_count`, making it safe to use
    /// directly in indexing loops.
    pub fn row_range(&self, row_extent: f32, row_count: usize) -> std::ops::Range<usize> {
        if row_count == 0 || row_extent <= f32::EPSILON {
            return 0..0;
        }
        let first = ((self.offset.y - self.overscan) / row_extent).floor();
        let last = ((self.offset.y + self.viewport.height + self.overscan) / row_extent).ceil();
        let start = first.max(0.0).min(row_count as f32) as usize;
        let end = last.max(0.0).min(row_count as f32) as usize;
        let end = end.max(start).min(row_count);
        start..end
    }

    /// Vertical distance from the content origin to the top of a row.
    pub fn row_top(&self, index: usize, row_extent: f32) -> f32 {
        index as f32 * row_extent.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_range_clamps_and_includes_overscan() {
        let window = ScrollWindow {
            offset: kasumi_core::Point::new(0.0, 500.0),
            viewport: Size::new(300.0, 200.0),
            content: Size::new(300.0, 10_000.0),
            overscan: 100.0,
        };
        assert_eq!(window.row_range(50.0, 1000), 8..16);
        assert_eq!(window.row_range(50.0, 1000).len(), 8);
        assert_eq!(window.row_range(50.0, 10), 8..10);
        assert_eq!(window.row_range(50.0, 0), 0..0);
        assert_eq!(window.row_range(0.0, 10), 0..0);
    }

    #[test]
    fn first_frame_range_does_not_overflow() {
        let window = ScrollWindow {
            offset: kasumi_core::Point::ZERO,
            viewport: Size::new(300.0, 200.0),
            content: Size::new(300.0, 90.0),
            overscan: 100.0,
        };
        assert_eq!(window.row_range(30.0, 3), 0..3);
        assert_eq!(window.row_top(2, 30.0), 60.0);
    }
}
