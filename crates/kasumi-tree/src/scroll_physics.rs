//! Chromium-style wheel scroll animation.
//!
//! This is a port of the smooth scrolling model used by iced's `Scrollable`
//! widget, which in turn models Chromium's scroll offset animation curve
//! (`cc/animation/scroll_offset_animation_curve.{h,cc}`).
//!
//! Wheel input moves a pending *target*; the visible offset eases towards it
//! with a cubic-Bézier ease-in-out curve. Each additional notch retargets the
//! running segment from its current position and velocity instead of
//! restarting it, so consecutive notches flow into one continuous animation.
//! The duration is inversely proportional to the remaining distance: short
//! scrolls get a softer animation and long scrolls get a snappier one.
//!
//! Unlike Firefox's `ScrollMode::Smooth` model there is no per-event duration
//! history and no explicit velocity integration. One segment is in flight at a
//! time, its position is a pure function of the frame timestamp, and therefore
//! its sampling cannot drift or oscillate when frame pacing changes.

use kasumi_core::Point;

/// The x control points of Chromium's wheel scrolling easing curve.
const SMOOTH_SCROLL_BEZIER_X1: f32 = 0.42;
const SMOOTH_SCROLL_BEZIER_X2: f32 = 0.58;

/// Chromium expresses scroll animation durations in units of 1/60 s.
const SMOOTH_SCROLL_DURATION_DIVISOR: f64 = 60.0;

/// The distance ramp for the duration, in logical pixels.
const SMOOTH_SCROLL_DURATION_RAMP_START: f32 = 120.0;
const SMOOTH_SCROLL_DURATION_RAMP_END: f32 = 480.0;

/// The shortest and longest animation durations, in `1 / 60` s units.
const SMOOTH_SCROLL_DURATION_MIN: f32 = 6.0;
const SMOOTH_SCROLL_DURATION_MAX: f32 = 12.0;

/// A new scroll starts one nominal frame before the input event so that the
/// first frame already shows movement instead of repeating the previous one.
const SMOOTH_SCROLL_FRAME_DELAY: f64 = 1.0 / 60.0;

/// Bounds the retargeted duration relative to the current velocity, which
/// keeps a small delta added at high speed from "rubber banding".
const SMOOTH_SCROLL_RETARGET_VELOCITY_BOUND: f32 = 2.5;

/// Clamp for the initial slope of a retargeted segment.
const SMOOTH_SCROLL_SLOPE_CLAMP: f32 = 1000.0;

/// Distances below this value are treated as already settled.
const EPSILON_DISTANCE: f32 = 0.01;

/// One in-flight smooth-scroll segment.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Segment {
    start: Point,
    /// Timestamp of the segment's start, in seconds.
    started: f64,
    /// Segment duration, in seconds.
    duration: f64,
    /// Initial slope (dy/dx) of the easing curve at its start.
    slope: f32,
}

impl Default for Segment {
    fn default() -> Self {
        Self {
            start: Point::ZERO,
            started: 0.0,
            duration: 0.0,
            slope: 0.0,
        }
    }
}

/// Retained smooth-scroll state for one scroll container.
///
/// The physics owns the authoritative rendered position and the pending
/// destination. `ScrollState` mirrors both for painting, hit testing and
/// scroll chaining, but only advances them through this type.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ScrollPhysics {
    position: Point,
    destination: Option<Point>,
    segment: Segment,
}

impl ScrollPhysics {
    /// Creates an idle physics state at the origin.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current, possibly fractional, scroll offset.
    pub fn position(&self) -> Point {
        self.position
    }

    /// The pending destination, or the current position while idle.
    pub fn target(&self) -> Point {
        self.destination.unwrap_or(self.position)
    }

    /// Whether a segment is currently in flight.
    pub fn is_animating(&self) -> bool {
        self.destination.is_some()
    }

    /// Stops any animation and places the offset at `position`.
    pub fn reset(&mut self, position: Point) {
        self.position = position;
        self.destination = None;
        self.segment = Segment::default();
    }

    /// Starts or retargets a smooth scroll toward `destination`.
    ///
    /// A fresh segment is backdated by one nominal frame so that the next
    /// drawn frame moves immediately. Retargeting an in-flight segment starts
    /// exactly at `now`, using the eased position and velocity of the old
    /// segment; backdating a retarget could otherwise move a frame backwards.
    pub fn scroll_to(&mut self, now: f64, destination: Point) {
        if self.destination == Some(destination) {
            return;
        }

        let current_progress = self.progress(now);
        let idle = self.destination.is_none() || current_progress >= 1.0;

        if idle {
            let start = self.position_at(now);
            let started = now - SMOOTH_SCROLL_FRAME_DELAY;
            self.start_segment(started, start, destination);
        } else {
            self.retarget_segment(now, destination);
        }
    }

    /// Retargets an in-flight segment without the fresh-segment frame delay.
    ///
    /// Used when layout changes move the destination (for example, when the
    /// content shrinks while an animation is running).
    pub fn retarget(&mut self, now: f64, destination: Point) {
        if self.destination == Some(destination) {
            return;
        }
        if self.destination.is_none() {
            self.start_segment(now, self.position, destination);
        } else {
            self.retarget_segment(now, destination);
        }
    }

    /// Advances the animation to `now`.
    ///
    /// Returns `true` while the segment is still in flight. Once it finishes,
    /// the position is set exactly to the destination so no fractional error
    /// accumulates at the edge of the content.
    pub fn step(&mut self, now: f64) -> bool {
        let Some(destination) = self.destination else {
            return false;
        };

        let progress = self.progress(now);
        if progress >= 1.0 {
            self.settle(destination);
            return false;
        }

        let eased = smooth_scroll_progress(progress as f32, self.segment.slope);
        self.position = lerp_point(self.segment.start, destination, eased);
        true
    }

    /// The current velocity in logical pixels per second.
    pub fn velocity(&self, now: f64) -> Point {
        let Some(destination) = self.destination else {
            return Point::ZERO;
        };

        let progress = self.progress(now);
        if progress >= 1.0 {
            return Point::ZERO;
        }

        let slope = smooth_scroll_curve_slope(progress as f32, self.segment.slope);
        let delta = destination - self.segment.start;
        let scale = slope as f64 / self.segment.duration.max(f64::EPSILON);

        Point::new(
            (delta.x as f64 * scale) as f32,
            (delta.y as f64 * scale) as f32,
        )
    }

    /// The current segment duration in milliseconds, or `0` while idle.
    pub fn duration_ms(&self) -> i64 {
        if self.destination.is_some() {
            (self.segment.duration * 1000.0).round() as i64
        } else {
            0
        }
    }

    /// Milliseconds left in the current segment.
    pub fn remaining_ms(&self, now: f64) -> i64 {
        if self.destination.is_none() {
            return 0;
        }
        ((self.segment.started + self.segment.duration - now).max(0.0) * 1000.0).round() as i64
    }

    fn progress(&self, now: f64) -> f64 {
        if self.destination.is_none() || self.segment.duration <= 0.0 {
            1.0
        } else {
            ((now - self.segment.started) / self.segment.duration).clamp(0.0, 1.0)
        }
    }

    fn position_at(&self, now: f64) -> Point {
        let Some(destination) = self.destination else {
            return self.position;
        };
        let eased = smooth_scroll_progress(self.progress(now) as f32, self.segment.slope);
        lerp_point(self.segment.start, destination, eased)
    }

    fn start_segment(&mut self, started: f64, start: Point, destination: Point) {
        let distance = (destination.x - start.x)
            .abs()
            .max((destination.y - start.y).abs());

        if distance < EPSILON_DISTANCE {
            self.settle(destination);
            return;
        }

        self.position = start;
        self.destination = Some(destination);
        self.segment = Segment {
            start,
            started,
            duration: smooth_scroll_duration(distance),
            slope: 0.0,
        };
    }

    fn retarget_segment(&mut self, now: f64, destination: Point) {
        let start = self.position_at(now);
        let velocity = self.animated_slope_velocity(now);
        let delta = destination - start;

        // Chromium retargets from the signed dimension with the largest
        // magnitude.
        let max_dimension = if delta.x.abs() > delta.y.abs() {
            delta.x
        } else {
            delta.y
        };

        // A short new delta at high velocity would otherwise get a long
        // duration and feel like rubber banding. Only a bound with the same
        // sign as the new motion applies; a reversal starts a normal segment.
        let mut duration = smooth_scroll_duration(max_dimension.abs());
        if velocity.abs() > 0.01 {
            let bound = SMOOTH_SCROLL_RETARGET_VELOCITY_BOUND * max_dimension / velocity;
            if bound > 0.0 {
                duration = duration.min(bound as f64);
            }
        }

        if max_dimension.abs() < EPSILON_DISTANCE || duration < 0.001 {
            self.settle(destination);
            return;
        }

        let slope = (velocity as f64 * duration / max_dimension as f64).clamp(
            -SMOOTH_SCROLL_SLOPE_CLAMP as f64,
            SMOOTH_SCROLL_SLOPE_CLAMP as f64,
        ) as f32;

        self.position = start;
        self.destination = Some(destination);
        self.segment = Segment {
            start,
            started: now,
            duration,
            slope,
        };
    }

    /// The signed velocity along the segment's dominant dimension.
    fn animated_slope_velocity(&self, now: f64) -> f32 {
        let Some(destination) = self.destination else {
            return 0.0;
        };

        let progress = self.progress(now);
        if progress >= 1.0 {
            return 0.0;
        }

        let delta = destination - self.segment.start;
        let max_dimension = if delta.x.abs() > delta.y.abs() {
            delta.x
        } else {
            delta.y
        };

        smooth_scroll_curve_slope(progress as f32, self.segment.slope) * max_dimension
            / self.segment.duration.max(f64::EPSILON) as f32
    }

    fn settle(&mut self, destination: Point) {
        self.position = destination;
        self.destination = None;
        self.segment = Segment::default();
    }
}

fn smooth_scroll_duration(distance: f32) -> f64 {
    let slope = (SMOOTH_SCROLL_DURATION_MIN - SMOOTH_SCROLL_DURATION_MAX)
        / (SMOOTH_SCROLL_DURATION_RAMP_END - SMOOTH_SCROLL_DURATION_RAMP_START);
    let offset = SMOOTH_SCROLL_DURATION_MAX - SMOOTH_SCROLL_DURATION_RAMP_START * slope;

    (offset + distance.clamp(0.0, SMOOTH_SCROLL_DURATION_RAMP_END) * slope)
        .clamp(SMOOTH_SCROLL_DURATION_MIN, SMOOTH_SCROLL_DURATION_MAX) as f64
        / SMOOTH_SCROLL_DURATION_DIVISOR
}

fn smooth_scroll_progress(time: f32, slope: f32) -> f32 {
    if time <= 0.0 {
        return 0.0;
    }
    if time >= 1.0 {
        return 1.0;
    }

    let s = bezier_solve(time);
    let y1 = SMOOTH_SCROLL_BEZIER_X1 * slope;
    let os = 1.0 - s;

    // Cubic Bézier with control points (0.42, y1), (0.58, 1), (1, 1).
    3.0 * y1 * s * os * os + 3.0 * s * s * os + s * s * s
}

fn smooth_scroll_curve_slope(time: f32, slope: f32) -> f32 {
    let s = if time <= 0.0 {
        0.0
    } else if time >= 1.0 {
        1.0
    } else {
        bezier_solve(time)
    };

    let x1 = SMOOTH_SCROLL_BEZIER_X1;
    let x2 = SMOOTH_SCROLL_BEZIER_X2;
    let y1 = x1 * slope;
    let os = 1.0 - s;

    let x_prime = 3.0 * x1 * os * (1.0 - 3.0 * s) + 3.0 * x2 * s * (2.0 - 3.0 * s) + 3.0 * s * s;
    let y_prime = 3.0 * y1 * os * (1.0 - 3.0 * s) + 3.0 * s * (2.0 - 3.0 * s) + 3.0 * s * s;

    y_prime / x_prime
}

/// Solves `x(s) = time` for the easing curve's x-axis.
///
/// The control points keep x strictly increasing, so bisection always
/// converges and does not need a Newton fallback.
fn bezier_solve(time: f32) -> f32 {
    let x1 = SMOOTH_SCROLL_BEZIER_X1;
    let x2 = SMOOTH_SCROLL_BEZIER_X2;

    let x = |s: f32| {
        let os = 1.0 - s;
        3.0 * x1 * s * os * os + 3.0 * x2 * s * s * os + s * s * s
    };

    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..40 {
        let mid = (low + high) * 0.5;
        if x(mid) < time {
            low = mid;
        } else {
            high = mid;
        }
    }

    (low + high) * 0.5
}

fn lerp_point(start: Point, end: Point, progress: f32) -> Point {
    Point::new(
        start.x + (end.x - start.x) * progress,
        start.y + (end.y - start.y) * progress,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrolling_physics(destination: Point, now: f64) -> ScrollPhysics {
        let mut physics = ScrollPhysics::new();
        physics.scroll_to(now, destination);
        physics
    }

    #[test]
    fn one_notch_uses_the_chromium_duration() {
        let physics = scrolling_physics(Point::new(0.0, 120.0), 0.0);
        assert!(physics.is_animating());
        assert_eq!(physics.duration_ms(), 200);
    }

    #[test]
    fn longer_distances_use_shorter_durations() {
        let short = scrolling_physics(Point::new(0.0, 120.0), 0.0);
        let long = scrolling_physics(Point::new(0.0, 480.0), 0.0);
        assert_eq!(short.duration_ms(), 200);
        assert_eq!(long.duration_ms(), 100);
    }

    #[test]
    fn first_frame_already_moves() {
        let mut physics = scrolling_physics(Point::new(0.0, 120.0), 0.0);
        assert!(physics.step(0.0));
        assert!(physics.position().y > 0.0);
        assert!(physics.position().y < 120.0);
    }

    #[test]
    fn settles_exactly_on_the_destination() {
        let mut physics = scrolling_physics(Point::new(0.0, 120.0), 0.0);
        assert!(!physics.step(1.0));
        assert_eq!(physics.position(), Point::new(0.0, 120.0));
        assert!(!physics.is_animating());
    }

    #[test]
    fn retargeting_is_continuous_and_monotonic() {
        let mut physics = scrolling_physics(Point::new(0.0, 120.0), 0.0);
        assert!(physics.step(0.05));
        let before = physics.position();

        physics.scroll_to(0.05, Point::new(0.0, 240.0));
        let after = physics.position();
        assert!(
            (after.y - before.y).abs() < 0.5,
            "retarget jumped from {before:?} to {after:?}"
        );

        let mut previous = after.y;
        for frame in 1..=60 {
            physics.step(0.05 + frame as f64 / 60.0);
            assert!(
                physics.position().y + 0.001 >= previous,
                "retarget moved backwards at frame {frame}"
            );
            previous = physics.position().y;
        }
        assert_eq!(physics.position().y, 240.0);
    }

    #[test]
    fn rapid_notches_accumulate_without_losing_distance() {
        let mut physics = ScrollPhysics::new();
        for notch in 1..=4 {
            physics.scroll_to(notch as f64 * 0.016, Point::new(0.0, notch as f32 * 120.0));
            physics.step(notch as f64 * 0.016);
        }
        assert!(physics.target().y >= 480.0);
        assert!(physics.is_animating());
    }

    #[test]
    fn step_is_frame_rate_independent() {
        let mut sixty = scrolling_physics(Point::new(0.0, 240.0), 0.0);
        let mut thirty = scrolling_physics(Point::new(0.0, 240.0), 0.0);

        for frame in 0..=6 {
            sixty.step(frame as f64 / 60.0);
        }
        for frame in 0..=3 {
            thirty.step(frame as f64 / 30.0);
        }

        assert!((sixty.position().y - thirty.position().y).abs() < 0.01);
    }

    #[test]
    fn reset_cancels_the_pending_animation() {
        let mut physics = scrolling_physics(Point::new(0.0, 120.0), 0.0);
        physics.reset(Point::new(0.0, 40.0));
        assert!(!physics.is_animating());
        assert_eq!(physics.position(), Point::new(0.0, 40.0));
        assert_eq!(physics.target(), Point::new(0.0, 40.0));
    }
}
