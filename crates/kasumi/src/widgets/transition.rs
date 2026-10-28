//! Presence and animation wrapper for any widget closure.

use std::hash::{Hash, Hasher};

use crate::transition::Transition;
use crate::ui::Ui;
use kasumi_core::{Id, Style, keyed_id};
use kasumi_tree::NodeKind;

/// A persistent transition scope which can retain an exiting subtree.
#[derive(Debug, Clone, Copy)]
pub struct TransitionHandle {
    id: Id,
}

impl TransitionHandle {
    pub fn id(self) -> Id {
        self.id
    }
}

impl Ui<'_> {
    /// Wraps content in a presence-aware transition. Use a stable key for each
    /// conditional piece of content; the wrapper retains the leaving subtree
    /// until its exit motion completes.
    pub fn transition<K: Hash>(
        &mut self,
        key: K,
        visible: bool,
        transition: Transition,
        f: impl FnOnce(&mut Ui),
    ) -> Option<TransitionHandle> {
        let expected_id = keyed_id(self.scope, &key);
        let now = self.time();

        if let Some(node) = self.cx.tree.find(expected_id) {
            let state = self.cx.transitions.state_mut(expected_id);
            state.seen_this_frame = true;
            state.spec = transition;
            if visible && state.closing {
                state.reopen(now, transition);
            } else if !visible && !state.closing {
                state.begin_exit(now);
            }
            let running = state.tick(now);
            let expired = !visible && !state.visible;
            let leaving = !visible;
            if let Some(tree_node) = self.cx.tree.get_mut(node) {
                state.apply(&mut tree_node.visual);
                tree_node.sense = if leaving || expired {
                    kasumi_tree::Sense::NONE
                } else {
                    kasumi_tree::Sense::HOVER
                };
            }
            if expired {
                self.cx.transitions.forget(expected_id);
                self.cx.tree.remove_node(node);
                return None;
            }
            if !leaving {
                self.cx.tree.declare_child(self.node, node);
            }
            self.cx.animations_running |= running;
            self.cx.repaint |= running || !visible;
            if visible {
                let mut child = Ui::new(self.cx, node, expected_id, Style::default());
                f(&mut child);
                child.commit_style();
            }
            return Some(TransitionHandle { id: expected_id });
        }

        if !visible {
            return None;
        }

        let id = expected_id;
        self.register_node_id(id, "transition");
        let node = self.cx.tree.ensure(id, NodeKind::Container);
        self.cx.tree.declare_child(self.node, node);
        let mut state = self.cx.transitions.state_mut(id).clone();
        state.enter(now, transition);
        let running = if state.initialized {
            state.tick(now)
        } else {
            false
        };
        if let Some(tree_node) = self.cx.tree.get_mut(node) {
            state.apply(&mut tree_node.visual);
        }
        {
            let stored = self.cx.transitions.state_mut(id);
            stored.clone_from(&state);
            stored.seen_this_frame = true;
        }
        self.cx.animations_running |= running;
        self.cx.repaint = true;

        self.cx.tree.set_style(
            node,
            Style {
                layout: kasumi_core::LayoutMode::Flex,
                ..Style::default()
            },
        );
        {
            let mut child = Ui::new(self.cx, node, id, Style::default());
            f(&mut child);
            child.commit_style();
        }
        Some(TransitionHandle { id })
    }

    pub fn is_transition_visible(&self, handle: TransitionHandle) -> bool {
        self.cx.transitions.contains(handle.id)
    }

    /// Starts an interpolated opacity turn when a keyed value changes. The
    /// caller applies the returned opacity to the content while replacing it.
    pub fn transition_switch<K: Hash, T: Hash + PartialEq + Copy>(
        &mut self,
        key: K,
        value: T,
        transition: Transition,
    ) -> f32 {
        let marker = kasumi_core::keyed_id(self.scope, &("switch", &key));
        let now = self.time();
        let state = self.cx.transitions.state_mut(marker);
        state.spec = transition;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);
        let value_hash = hasher.finish();
        let changed = state
            .switch_value
            .map(|previous| previous != value_hash)
            .unwrap_or(false);
        if changed || !state.initialized {
            state.initialized = true;
            state.switch_value = Some(value_hash);
            state.opacity.snap(0.0);
            state.opacity.set_target(1.0, transition.enter, now);
        }
        let running = state.tick(now);
        let opacity = state.opacity.value();
        let applied = state.initialized;
        self.cx.animations_running |= running;
        self.cx.repaint |= running;
        if applied { opacity } else { 1.0 }
    }
}
