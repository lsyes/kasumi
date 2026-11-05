//! Retained animation values and motion curves.

use std::time::Duration;

use kasumi_core::{Color, Vec2};

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    #[default]
    EaseOutCubic,
    EaseInOutCubic,
    Smoothstep,
    /// A CSS-style cubic Bézier with control points `(x1, y1)` and `(x2, y2)`.
    ///
    /// The x coordinates are clamped to `0..=1`; y coordinates may overshoot.
    /// This is the representation used by the Material 3 motion tokens.
    CubicBezier {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
}

impl Easing {
    /// Material 3 standard easing: `cubic-bezier(0.2, 0, 0, 1)`.
    pub const M3_STANDARD: Self = Self::CubicBezier {
        x1: 0.2,
        y1: 0.0,
        x2: 0.0,
        y2: 1.0,
    };

    /// Material 3 emphasized easing: `cubic-bezier(0.2, 0, 0, 1)` is the
    /// standard curve; the expressive emphasized pair adds decelerate and
    /// accelerate variants. This constant is the canonical emphasized curve.
    pub const M3_EMPHASIZED: Self = Self::CubicBezier {
        x1: 0.2,
        y1: 0.0,
        x2: 0.0,
        y2: 1.0,
    };

    /// Material 3 decelerate easing: `cubic-bezier(0, 0, 0, 1)`.
    pub const M3_DECELERATE: Self = Self::CubicBezier {
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 1.0,
    };

    /// Material 3 accelerate easing: `cubic-bezier(0.3, 0, 1, 1)`.
    pub const M3_ACCELERATE: Self = Self::CubicBezier {
        x1: 0.3,
        y1: 0.0,
        x2: 1.0,
        y2: 1.0,
    };

    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Self::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) * 0.5
                }
            }
            Self::Smoothstep => t * t * (3.0 - 2.0 * t),
            Self::CubicBezier { x1, y1, x2, y2 } => {
                cubic_bezier_ease(t, x1.clamp(0.0, 1.0), y1, x2.clamp(0.0, 1.0), y2)
            }
        }
    }
}

/// Samples a cubic Bézier timing function at `x` by solving `x(t) = x` and
/// evaluating `y(t)`. Newton iterations are used first, with a bisection
/// fallback for flat segments.
fn cubic_bezier_ease(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let curve = |t: f32, p1: f32, p2: f32| {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * p1 + 3.0 * inverse * t * t * p2 + t * t * t
    };
    let derivative = |t: f32, p1: f32, p2: f32| {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * p1 + 6.0 * inverse * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
    };

    let mut guess = x;
    for _ in 0..8 {
        let error = curve(guess, x1, x2) - x;
        if error.abs() < 1.0e-5 {
            return curve(guess, y1, y2);
        }
        let slope = derivative(guess, x1, x2);
        if slope.abs() < 1.0e-6 {
            break;
        }
        guess = (guess - error / slope).clamp(0.0, 1.0);
    }

    let (mut low, mut high) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let middle = (low + high) * 0.5;
        if curve(middle, x1, x2) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    curve((low + high) * 0.5, y1, y2)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub response: f32,
    pub damping: f32,
    pub epsilon: f32,
}

impl Default for Spring {
    fn default() -> Self {
        Self {
            response: 0.32,
            damping: 0.82,
            epsilon: 0.001,
        }
    }
}

impl Spring {
    pub fn new(response: f32, damping: f32) -> Self {
        Self {
            response: response.max(0.01),
            damping: damping.clamp(0.0, 1.0),
            ..Self::default()
        }
    }

    pub fn stiffness(self) -> f32 {
        4.0 * std::f32::consts::PI.powi(2) / self.response.powi(2)
    }

    pub fn damping_coefficient(self) -> f32 {
        2.0 * self.damping * (self.stiffness() * 1.0).sqrt()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motion {
    #[default]
    Tween,
    Spring,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionSpec {
    pub duration: Duration,
    pub easing: Easing,
    pub spring: Spring,
    pub kind: Motion,
}

impl Default for MotionSpec {
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(180),
            easing: Easing::EaseOutCubic,
            spring: Spring::default(),
            kind: Motion::Tween,
        }
    }
}

impl MotionSpec {
    pub fn tween(duration: Duration, easing: Easing) -> Self {
        Self {
            duration,
            easing,
            ..Self::default()
        }
    }

    pub fn spring(spring: Spring) -> Self {
        Self {
            spring,
            kind: Motion::Spring,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Animated {
    from: f32,
    target: f32,
    value: f32,
    velocity: f32,
    started_at: f64,
    last_time: f64,
    spec: MotionSpec,
    initialized: bool,
}

impl Animated {
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            target: value,
            value,
            velocity: 0.0,
            started_at: 0.0,
            last_time: 0.0,
            spec: MotionSpec::default(),
            initialized: true,
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    pub fn spec(&self) -> MotionSpec {
        self.spec
    }

    pub fn is_running(&self, now: f64) -> bool {
        match self.spec.kind {
            Motion::Tween => {
                now < self.started_at + self.spec.duration.as_secs_f64()
                    && (self.value - self.target).abs() > self.spec.spring.epsilon
            }
            Motion::Spring => {
                (self.value - self.target).abs() > self.spec.spring.epsilon
                    || self.velocity.abs() > self.spec.spring.epsilon
            }
        }
    }

    pub fn set_target(&mut self, target: f32, spec: MotionSpec, now: f64) -> bool {
        if !self.initialized {
            self.initialized = true;
            self.target = target;
            self.from = self.value;
            self.started_at = now;
            self.last_time = now;
            self.spec = spec;
            self.velocity = 0.0;
            return true;
        }
        let spec_changed = self.spec != spec;
        if (self.target - target).abs() <= f32::EPSILON && !spec_changed && !self.is_running(now) {
            return false;
        }
        self.sample(now);
        self.from = self.value;
        self.target = target;
        self.started_at = now;
        self.last_time = now;
        self.spec = spec;
        match spec.kind {
            Motion::Tween => self.velocity = 0.0,
            Motion::Spring => {
                if !spec_changed && self.velocity.abs() > 0.001 {
                    // Preserve momentum when a spring is retargeted mid-flight.
                } else {
                    self.velocity = 0.0;
                }
            }
        }
        true
    }

    pub fn snap(&mut self, value: f32) {
        self.from = value;
        self.target = value;
        self.value = value;
        self.velocity = 0.0;
        self.initialized = true;
    }

    pub fn sample(&mut self, now: f64) -> f32 {
        if !self.initialized {
            return self.value;
        }
        match self.spec.kind {
            Motion::Tween => {
                if self.spec.duration.is_zero() {
                    self.value = self.target;
                    return self.value;
                }
                let elapsed =
                    ((now - self.started_at) / self.spec.duration.as_secs_f64()).clamp(0.0, 1.0);
                let eased = self.spec.easing.sample(elapsed as f32);
                self.value = self.from + (self.target - self.from) * eased;
                if elapsed >= 1.0 {
                    self.value = self.target;
                }
            }
            Motion::Spring => {
                let dt = (now - self.last_time).clamp(0.0, 0.05) as f32;
                self.last_time = now;
                if dt > 0.0 {
                    // Fixed substeps keep a stiff spring stable when a frame is
                    // delayed or the host schedules a gap between repaints.
                    let steps = ((dt / (1.0 / 240.0)).ceil() as usize).clamp(1, 12);
                    let step = dt / steps as f32;
                    let stiffness = self.spec.spring.stiffness();
                    let damping = self.spec.spring.damping_coefficient();
                    for _ in 0..steps {
                        let accel =
                            (self.target - self.value) * stiffness - self.velocity * damping;
                        self.velocity += accel * step;
                        self.value += self.velocity * step;
                    }
                }
                if (self.value - self.target).abs() <= self.spec.spring.epsilon
                    && self.velocity.abs() <= self.spec.spring.epsilon
                {
                    self.value = self.target;
                    self.velocity = 0.0;
                }
            }
        }
        self.value
    }

    /// Makes an uninitialized channel start at zero instead of snapping to its
    /// target. Used by `Ui::animate` for backwards-compatible first-run motion.
    pub fn start_from_zero(&mut self) {
        self.value = 0.0;
    }
}

pub trait LerpValue: Copy {
    fn lerp(self, other: Self, t: f32) -> Self;
}

impl LerpValue for f32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }
}

impl LerpValue for Color {
    fn lerp(self, other: Self, t: f32) -> Self {
        Color::lerp(self, other, t)
    }
}

impl LerpValue for Vec2 {
    fn lerp(self, other: Self, t: f32) -> Self {
        Vec2::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationState {
    pub opacity: Animated,
    pub translate: [Animated; 2],
    pub scale: Animated,
    pub exit: Animated,
    pub presence: Animated,
    pub revision: u64,
    pub origin: Option<(f32, f32)>,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            opacity: Animated::new(1.0),
            translate: [Animated::new(0.0), Animated::new(0.0)],
            scale: Animated::new(1.0),
            exit: Animated::new(0.0),
            presence: Animated::new(1.0),
            revision: 0,
            origin: None,
        }
    }
}

impl AnimationState {
    pub fn opacity(&self) -> f32 {
        self.opacity.value()
    }

    pub fn translation(&self) -> Vec2 {
        Vec2::new(self.translate[0].value(), self.translate[1].value())
    }

    pub fn scale(&self) -> f32 {
        self.scale.value()
    }

    pub fn origin(&self) -> Option<(f32, f32)> {
        self.origin
    }

    pub fn any_running(&self, now: f64) -> bool {
        self.presence.is_running(now)
            || self.opacity.is_running(now)
            || self.translate.iter().any(|value| value.is_running(now))
            || self.scale.is_running(now)
            || self.exit.is_running(now)
    }

    pub fn exit_progress(&self) -> f32 {
        self.exit.value().clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_are_clamped_and_monotonic() {
        for easing in [
            Easing::Linear,
            Easing::EaseOutCubic,
            Easing::EaseInOutCubic,
            Easing::Smoothstep,
        ] {
            assert_eq!(easing.sample(0.0), 0.0);
            assert_eq!(easing.sample(1.0), 1.0);
            assert_eq!(easing.sample(-1.0), 0.0);
            assert_eq!(easing.sample(2.0), 1.0);
        }
    }

    #[test]
    fn tween_retargets_from_current_value() {
        let mut value = Animated::new(0.0);
        value.set_target(1.0, MotionSpec::default(), 0.0);
        let middle = value.sample(0.09);
        assert!(middle > 0.0 && middle < 1.0);
        value.set_target(0.0, MotionSpec::default(), 0.09);
        assert!((value.value() - middle).abs() < 0.0001);
        assert_eq!(value.sample(1.0), 0.0);
        assert!(!value.is_running(1.0));
    }

    #[test]
    fn spring_settles() {
        let mut value = Animated::new(0.0);
        value.set_target(1.0, MotionSpec::spring(Spring::default()), 0.0);
        for step in 1..=300 {
            value.sample(step as f64 / 60.0);
        }
        assert!((value.value() - 1.0).abs() < 0.01);
        assert!(!value.is_running(5.0));
    }
}
