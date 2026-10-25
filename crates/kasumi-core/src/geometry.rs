//! Geometry primitives shared by every layer.
//!
//! All values are in logical pixels unless a type or function says otherwise.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

/// A two-dimensional point.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn splat(value: f32) -> Self {
        Self::new(value, value)
    }

    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }

    pub fn min(self, other: Self) -> Self {
        Self::new(self.x.min(other.x), self.y.min(other.y))
    }

    pub fn max(self, other: Self) -> Self {
        Self::new(self.x.max(other.x), self.y.max(other.y))
    }

    pub fn round(self) -> Self {
        Self::new(self.x.round(), self.y.round())
    }
}

/// A two-dimensional vector, used for deltas rather than positions.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl Add for Vec2 {
    type Output = Vec2;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

/// A width/height pair.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn splat(value: f32) -> Self {
        Self::new(value, value)
    }

    pub fn max(self, other: Self) -> Self {
        Self::new(self.width.max(other.width), self.height.max(other.height))
    }

    pub fn min(self, other: Self) -> Self {
        Self::new(self.width.min(other.width), self.height.min(other.height))
    }

    pub fn round(self) -> Self {
        Self::new(self.width.round(), self.height.round())
    }

    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

impl Add for Size {
    type Output = Size;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.width + rhs.width, self.height + rhs.height)
    }
}

impl Sub for Size {
    type Output = Size;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.width - rhs.width, self.height - rhs.height)
    }
}

/// An axis-aligned rectangle. `origin` is the top-left corner.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        origin: Point::ZERO,
        size: Size::ZERO,
    };

    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub const fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    pub fn from_min_max(min: Point, max: Point) -> Self {
        Self::new(min, Size::new(max.x - min.x, max.y - min.y))
    }

    pub fn x(self) -> f32 {
        self.origin.x
    }

    pub fn y(self) -> f32 {
        self.origin.y
    }

    pub fn width(self) -> f32 {
        self.size.width
    }

    pub fn height(self) -> f32 {
        self.size.height
    }

    pub fn left(self) -> f32 {
        self.origin.x
    }

    pub fn top(self) -> f32 {
        self.origin.y
    }

    pub fn right(self) -> f32 {
        self.origin.x + self.size.width
    }

    pub fn bottom(self) -> f32 {
        self.origin.y + self.size.height
    }

    pub fn center(self) -> Point {
        Point::new(
            self.origin.x + self.size.width * 0.5,
            self.origin.y + self.size.height * 0.5,
        )
    }

    pub fn is_empty(self) -> bool {
        self.size.is_empty()
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.left()
            && point.x < self.right()
            && point.y >= self.top()
            && point.y < self.bottom()
    }

    pub fn intersects(self, other: Self) -> bool {
        self.left() < other.right()
            && other.left() < self.right()
            && self.top() < other.bottom()
            && other.top() < self.bottom()
    }

    pub fn intersect(self, other: Self) -> Self {
        let min = self.origin.max(other.origin);
        let max = Point::new(
            self.right().min(other.right()),
            self.bottom().min(other.bottom()),
        );
        if max.x <= min.x || max.y <= min.y {
            Self::ZERO
        } else {
            Self::from_min_max(min, max)
        }
    }

    pub fn inset(self, edges: Edges) -> Self {
        Self::new(
            Point::new(self.origin.x + edges.left, self.origin.y + edges.top),
            Size::new(
                (self.size.width - edges.left - edges.right).max(0.0),
                (self.size.height - edges.top - edges.bottom).max(0.0),
            ),
        )
    }

    pub fn translate(self, offset: Vec2) -> Self {
        Self::new(
            Point::new(self.origin.x + offset.x, self.origin.y + offset.y),
            self.size,
        )
    }

    pub fn round(self) -> Self {
        // Snap to device-independent pixel boundaries so that adjacent
        // backgrounds do not leave sub-pixel seams.
        let min = self.origin.round();
        let max = Point::new(self.right().round(), self.bottom().round());
        Self::from_min_max(min, max)
    }
}

/// Independent top/right/bottom/left values.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub const ZERO: Edges = Edges {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn xy(x: f32, y: f32) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }

    pub const fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub const fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

/// Independent corner radii, ordered clockwise from the top left.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Corners {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl Corners {
    pub const ZERO: Corners = Corners {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };

    pub const fn all(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }

    pub const fn top(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }
    }

    pub fn max(self) -> f32 {
        self.top_left
            .max(self.top_right)
            .max(self.bottom_right)
            .max(self.bottom_left)
    }

    pub fn to_array(self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }
}

impl Neg for Vec2 {
    type Output = Vec2;

    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y)
    }
}

impl Add for Point {
    type Output = Point;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Add<Vec2> for Rect {
    type Output = Rect;

    fn add(self, rhs: Vec2) -> Self::Output {
        self.translate(rhs)
    }
}

impl Mul<f32> for Size {
    type Output = Size;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.width * rhs, self.height * rhs)
    }
}

impl Div<f32> for Size {
    type Output = Size;

    fn div(self, rhs: f32) -> Self::Output {
        Self::new(self.width / rhs, self.height / rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_and_containment() {
        let a = Rect::from_xywh(0.0, 0.0, 100.0, 50.0);
        let b = Rect::from_xywh(50.0, 25.0, 100.0, 50.0);
        assert_eq!(a.intersect(b), Rect::from_xywh(50.0, 25.0, 50.0, 25.0));
        assert!(a.contains(Point::new(0.0, 0.0)));
        assert!(!a.contains(Point::new(100.0, 0.0)));
    }

    #[test]
    fn empty_intersection() {
        let a = Rect::from_xywh(0.0, 0.0, 10.0, 10.0);
        let b = Rect::from_xywh(20.0, 20.0, 10.0, 10.0);
        assert!(a.intersect(b).is_empty());
    }
}
