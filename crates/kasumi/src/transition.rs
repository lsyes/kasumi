//! Presence-aware animated mounting.
//!
//! Retained motion specifications and presence-transition state.

use std::time::Duration;

use kasumi_core::Id;
use kasumi_tree::{Animated, AnimationState, Easing, MotionSpec, Spring};

const DEFAULT_EXIT_MS: u64 = 150;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    pub enter: MotionSpec,
    pub exit: MotionSpec,
    pub initial_opacity: f32,
    pub initial_translate: (f32, f32),
    pub initial_scale: f32,
    pub origin: Option<(f32, f32)>,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            enter: MotionSpec::tween(Duration::from_millis(180), Easing::EaseOutCubic),
            exit: MotionSpec::tween(
                Duration::from_millis(DEFAULT_EXIT_MS),
                Easing::EaseInOutCubic,
            ),
            initial_opacity: 0.0,
            initial_translate: (0.0, 0.0),
            initial_scale: 1.0,
            origin: None,
        }
    }
}

impl Transition {
    pub fn fade() -> Self {
        Self::default()
    }

    pub fn scale(factor: f32) -> Self {
        Self {
            initial_scale: factor.max(0.0),
            origin: Some((0.5, 0.5)),
            ..Self::default()
        }
    }

    pub fn slide(x: f32, y: f32) -> Self {
        Self {
            initial_translate: (x, y),
            ..Self::default()
        }
    }

    /// A popover enters from its top using a spring and exits with a tween.
    pub fn popover() -> Self {
        Self {
            enter: MotionSpec::spring(Spring::new(0.28, 0.8)),
            exit: MotionSpec::tween(Duration::from_millis(120), Easing::EaseInOutCubic),
            initial_opacity: 0.0,
            initial_translate: (0.0, -6.0),
            initial_scale: 0.97,
            origin: Some((0.5, 0.0)),
        }
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.enter = MotionSpec::tween(duration, self.enter.easing);
        self.exit = MotionSpec::tween(duration, self.exit.easing);
        self
    }

    pub fn enter(mut self, motion: MotionSpec) -> Self {
        self.enter = motion;
        self
    }

    pub fn exit(mut self, motion: MotionSpec) -> Self {
        self.exit = motion;
        self
    }

    pub fn opacity(mut self, initial: f32) -> Self {
        self.initial_opacity = initial.clamp(0.0, 1.0);
        self
    }

    pub fn origin(mut self, x: f32, y: f32) -> Self {
        self.origin = Some((x, y));
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitionState {
    pub visible: bool,
    pub closing: bool,
    pub initialized: bool,
    pub closing_since: Option<f64>,
    pub presence: Animated,
    pub opacity: Animated,
    pub translate: [Animated; 2],
    pub scale: Animated,
    pub spec: Transition,
    pub(crate) seen_this_frame: bool,
    pub(crate) switch_value: Option<u64>,
}

impl Default for TransitionState {
    fn default() -> Self {
        Self {
            visible: true,
            closing: false,
            initialized: false,
            closing_since: None,
            presence: Animated::new(1.0),
            opacity: Animated::new(1.0),
            translate: [Animated::new(0.0), Animated::new(0.0)],
            scale: Animated::new(1.0),
            spec: Transition::default(),
            seen_this_frame: false,
            switch_value: None,
        }
    }
}

impl TransitionState {
    pub(crate) fn enter(&mut self, now: f64, spec: Transition) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        self.spec = spec;
        self.visible = true;
        self.closing = false;
        self.closing_since = None;
        self.presence.snap(1.0);
        self.opacity.snap(spec.initial_opacity);
        self.opacity.set_target(1.0, spec.enter, now);
        self.translate[0].snap(spec.initial_translate.0);
        self.translate[1].snap(spec.initial_translate.1);
        self.translate[0].set_target(0.0, spec.enter, now);
        self.translate[1].set_target(0.0, spec.enter, now);
        self.scale.snap(spec.initial_scale);
        self.scale.set_target(1.0, spec.enter, now);
    }

    pub(crate) fn begin_exit(&mut self, now: f64) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.closing_since = Some(now);
        self.opacity.set_target(0.0, self.spec.exit, now);
        self.translate[0].set_target(self.spec.initial_translate.0, self.spec.exit, now);
        self.translate[1].set_target(self.spec.initial_translate.1, self.spec.exit, now);
        self.scale
            .set_target(self.spec.initial_scale, self.spec.exit, now);
        self.presence.set_target(0.0, self.spec.exit, now);
    }

    pub(crate) fn tick(&mut self, now: f64) -> bool {
        let running = self.opacity.is_running(now)
            || self.translate.iter().any(|value| value.is_running(now))
            || self.scale.is_running(now)
            || self.presence.is_running(now);
        if self.closing && !running {
            self.visible = false;
            self.closing = false;
            self.closing_since = None;
        }
        running
    }

    pub(crate) fn apply(&self, visual: &mut AnimationState) {
        visual.opacity = self.opacity;
        visual.translate = self.translate;
        visual.scale = self.scale;
        visual.presence = self.presence;
        visual.origin = self.spec.origin;
    }
}

#[derive(Default)]
pub(crate) struct TransitionStore {
    entries: std::collections::HashMap<Id, TransitionState>,
}

impl TransitionStore {
    pub(crate) fn state_mut(&mut self, id: Id) -> &mut TransitionState {
        self.entries.entry(id).or_default()
    }

    pub(crate) fn forget(&mut self, id: Id) {
        self.entries.remove(&id);
    }

    pub(crate) fn contains(&self, id: Id) -> bool {
        self.entries.contains_key(&id)
    }
}

impl TransitionState {
    pub(crate) fn reopen(&mut self, now: f64, transition: Transition) {
        self.closing = false;
        self.closing_since = None;
        self.visible = true;
        self.opacity.set_target(1.0, transition.enter, now);
        self.translate[0].set_target(0.0, transition.enter, now);
        self.translate[1].set_target(0.0, transition.enter, now);
        self.scale.set_target(1.0, transition.enter, now);
        self.presence.set_target(1.0, transition.enter, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasumi_tree::Motion;

    #[test]
    fn fade_retains_until_exit_finishes() {
        let mut state = TransitionState::default();
        state.enter(0.0, Transition::fade().duration(Duration::from_millis(100)));
        assert!(state.tick(0.0));
        assert_eq!(state.opacity.value(), 0.0);
        state.opacity.sample(0.1);
        state.begin_exit(0.1);
        assert!(state.tick(0.15));
        assert!(state.visible);
        state.tick(0.25);
        assert!(!state.visible);
    }

    #[test]
    fn spring_transition_is_available() {
        assert_eq!(Transition::popover().enter.kind, Motion::Spring);
    }
}
