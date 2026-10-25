//! Interaction result returned by widgets.

use kasumi_core::{Id, Point, Rect};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ResponseFlags(u16);

impl ResponseFlags {
    pub const HOVERED: ResponseFlags = ResponseFlags(1 << 0);
    pub const PRESSED: ResponseFlags = ResponseFlags(1 << 1);
    pub const CLICKED: ResponseFlags = ResponseFlags(1 << 2);
    pub const RELEASED: ResponseFlags = ResponseFlags(1 << 3);
    pub const FOCUSED: ResponseFlags = ResponseFlags(1 << 4);
    pub const CHANGED: ResponseFlags = ResponseFlags(1 << 5);
    pub const DRAGGED: ResponseFlags = ResponseFlags(1 << 6);
    pub const CONTEXT_CLICKED: ResponseFlags = ResponseFlags(1 << 7);

    pub fn contains(self, other: ResponseFlags) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: ResponseFlags) {
        self.0 |= other.0;
    }
}

/// What happened to one widget this frame.
///
/// `rect` is the geometry from the previous layout pass, because layout for the
/// current frame cannot run until `view` returns. Controls with one-frame
/// delayed hit testing are an explicit property of the immediate surface; see
/// the README for the mitigation path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Response {
    id: Id,
    rect: Rect,
    flags: ResponseFlags,
    scroll_offset: Point,
}

impl Response {
    pub fn new(id: Id, rect: Rect, flags: ResponseFlags) -> Self {
        Self {
            id,
            rect,
            flags,
            scroll_offset: Point::ZERO,
        }
    }

    pub fn id(&self) -> Id {
        self.id
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn flags(&self) -> ResponseFlags {
        self.flags
    }

    pub fn hovered(&self) -> bool {
        self.flags.contains(ResponseFlags::HOVERED)
    }

    pub fn pressed(&self) -> bool {
        self.flags.contains(ResponseFlags::PRESSED)
    }

    pub fn clicked(&self) -> bool {
        self.flags.contains(ResponseFlags::CLICKED)
    }

    pub fn released(&self) -> bool {
        self.flags.contains(ResponseFlags::RELEASED)
    }

    pub fn focused(&self) -> bool {
        self.flags.contains(ResponseFlags::FOCUSED)
    }

    pub fn changed(&self) -> bool {
        self.flags.contains(ResponseFlags::CHANGED)
    }

    pub fn dragged(&self) -> bool {
        self.flags.contains(ResponseFlags::DRAGGED)
    }

    pub fn context_clicked(&self) -> bool {
        self.flags.contains(ResponseFlags::CONTEXT_CLICKED)
    }

    pub fn scroll_offset(&self) -> Point {
        self.scroll_offset
    }

    pub(crate) fn with_scroll_offset(mut self, offset: Point) -> Self {
        self.scroll_offset = offset;
        self
    }

    pub(crate) fn insert_flag(&mut self, flag: ResponseFlags) {
        self.flags.insert(flag);
    }
}
