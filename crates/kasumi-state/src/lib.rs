//! Cross-frame application state.
//!
//! State lives in a type-erased, id-addressed table. Users only see cloneable
//! `State<T>` handles; the table, type checks and lifetime policy stay behind
//! that handle.

#![forbid(unsafe_code)]

use std::any::{Any, TypeId, type_name};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

use kasumi_core::Id;

/// Frames a value survives without being touched.
pub const DEFAULT_TTL_FRAMES: u64 = 600;

struct Entry {
    value: Box<dyn Any>,
    type_name: &'static str,
    path: Rc<str>,
    last_seen: u64,
    persistent: bool,
}

/// Type-erased storage shared by every `State<T>` handle.
pub struct StateStore {
    entries: RefCell<HashMap<Id, Entry>>,
    frame: Cell<u64>,
    ttl_frames: Cell<u64>,
    next_eviction: Cell<u64>,
    repaint: Cell<bool>,
}

impl Default for StateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StateStore {
    pub fn new() -> Self {
        Self {
            entries: RefCell::new(HashMap::new()),
            frame: Cell::new(0),
            ttl_frames: Cell::new(DEFAULT_TTL_FRAMES),
            next_eviction: Cell::new(u64::MAX),
            repaint: Cell::new(false),
        }
    }

    pub fn set_ttl_frames(&self, frames: u64) {
        if self.ttl_frames.get() != frames {
            self.ttl_frames.set(frames);
            self.next_eviction.set(0);
        }
    }

    pub fn request_repaint(&self) {
        self.repaint.set(true);
    }

    pub fn take_repaint(&self) -> bool {
        self.repaint.replace(false)
    }

    pub fn begin_frame(&self, frame: u64) {
        self.frame.set(frame);
    }

    /// Drops values whose TTL elapsed. Handles remain valid: accessing an
    /// evicted handle re-runs its initializer.
    pub fn end_frame(&self) {
        let frame = self.frame.get();
        if frame < self.next_eviction.get() {
            return;
        }
        let ttl = self.ttl_frames.get();
        let mut entries = self.borrow_entries_mut("evicting stale state");
        let mut next_eviction = u64::MAX;
        entries.retain(|id, entry| {
            if entry.persistent {
                return true;
            }
            let keep = frame.saturating_sub(entry.last_seen) <= ttl;
            if !keep && std::env::var_os("KASUMI_STATE_TRACE").is_some() {
                eprintln!(
                    "kasumi: evicted state {id} ({}, last used at frame {})",
                    entry.path, entry.last_seen
                );
            }
            if keep {
                next_eviction =
                    next_eviction.min(entry.last_seen.saturating_add(ttl).saturating_add(1));
            }
            keep
        });
        self.next_eviction.set(next_eviction);
    }

    pub fn entry_count(&self) -> usize {
        self.borrow_entries("reading state entry count").len()
    }

    fn borrow_entries(&self, context: &str) -> std::cell::Ref<'_, HashMap<Id, Entry>> {
        self.entries.try_borrow().unwrap_or_else(|_| {
            panic!(
                "kasumi: state store is already borrowed mutably while {context}. \
                 A `State` closure likely re-entered another state access; use a local \
                 variable inside closures instead."
            )
        })
    }

    fn borrow_entries_mut(&self, context: &str) -> std::cell::RefMut<'_, HashMap<Id, Entry>> {
        self.entries.try_borrow_mut().unwrap_or_else(|_| {
            panic!(
                "kasumi: state store is already borrowed while {context}. \
                 A `State` closure likely re-entered another state access; use a local \
                 variable inside closures instead."
            )
        })
    }

    /// Runs `f` with the entry for `id`, creating it from `init` when absent.
    /// All public `State` operations funnel through this function so that the
    /// borrow is released as early as possible.
    #[allow(clippy::too_many_arguments)]
    fn with_entry<R>(
        &self,
        id: Id,
        path: &Rc<str>,
        init: &dyn Fn() -> Box<dyn Any>,
        expected_type: &'static str,
        expected_id: TypeId,
        persistent: bool,
        context: &str,
        f: impl FnOnce(&mut Box<dyn Any>) -> R,
    ) -> R {
        let frame = self.frame.get();
        let mut entries = self.borrow_entries_mut(context);
        let entry = entries.entry(id).or_insert_with(|| Entry {
            value: init(),
            type_name: expected_type,
            path: path.clone(),
            last_seen: frame,
            persistent,
        });

        entry.last_seen = frame;
        if persistent {
            entry.persistent = true;
        }
        if !entry.persistent {
            let next = frame
                .saturating_add(self.ttl_frames.get())
                .saturating_add(1);
            if next < self.next_eviction.get() {
                self.next_eviction.set(next);
            }
        }

        if entry.value.as_ref().type_id() != expected_id {
            panic!(
                "kasumi: state type mismatch at {} (created at {}). Expected `{}`, found `{}`.",
                id, entry.path, expected_type, entry.type_name
            );
        }

        f(&mut entry.value)
    }
}

/// A cloneable handle to one typed value in a `StateStore`.
pub struct State<T: 'static> {
    store: Rc<StateStore>,
    id: Id,
    path: Rc<str>,
    init: Rc<dyn Fn() -> T>,
    persistent: bool,
    _marker: PhantomData<fn() -> T>,
}

impl<T: 'static> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            id: self.id,
            path: self.path.clone(),
            init: self.init.clone(),
            persistent: self.persistent,
            _marker: PhantomData,
        }
    }
}

impl<T: 'static> fmt::Debug for State<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("id", &self.id)
            .field("path", &self.path)
            .field("persistent", &self.persistent)
            .finish()
    }
}

impl<T: 'static> State<T> {
    /// Used by the UI layer to attach a handle to an id. The initializer is
    /// retained so an evicted value can be recreated.
    #[doc(hidden)]
    pub fn new(
        store: Rc<StateStore>,
        id: Id,
        path: Rc<str>,
        init: impl Fn() -> T + 'static,
    ) -> Self {
        Self {
            store,
            id,
            path,
            init: Rc::new(init),
            persistent: false,
            _marker: PhantomData,
        }
    }

    pub fn id(&self) -> Id {
        self.id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// Keeps the value alive even when it is not read for a while.
    pub fn persistent(mut self) -> Self {
        self.persistent = true;
        self
    }

    /// Marks the existing entry persistent immediately.
    pub fn set_persistent(&self) {
        self.store.with_entry(
            self.id,
            &self.path,
            &|| Box::new((self.init)()),
            type_name::<T>(),
            TypeId::of::<T>(),
            true,
            "marking state persistent",
            |_| (),
        );
    }

    fn with_boxed<R>(&self, context: &str, f: impl FnOnce(&mut Box<dyn Any>) -> R) -> R {
        self.store.with_entry(
            self.id,
            &self.path,
            &|| Box::new((self.init)()),
            type_name::<T>(),
            TypeId::of::<T>(),
            self.persistent,
            context,
            f,
        )
    }

    /// Clones the current value.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(T::clone)
    }

    /// Borrows the value without cloning.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.with_boxed("reading state", |value| {
            let typed = value.downcast_ref::<T>().unwrap_or_else(|| {
                panic!(
                    "kasumi: state type mismatch at {} (created at {}). Expected `{}`.",
                    self.id,
                    self.path,
                    type_name::<T>()
                )
            });
            f(typed)
        })
    }

    /// Replaces the value and requests a repaint.
    pub fn set(&self, next: T) {
        self.with_boxed("writing state", |value| {
            let typed = value.downcast_mut::<T>().unwrap_or_else(|| {
                panic!(
                    "kasumi: state type mismatch at {} (created at {}). Expected `{}`.",
                    self.id,
                    self.path,
                    type_name::<T>()
                )
            });
            *typed = next;
        });
        self.store.request_repaint();
    }

    /// Mutates the value in place and requests a repaint.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        self.with_boxed("updating state", |value| {
            let typed = value.downcast_mut::<T>().unwrap_or_else(|| {
                panic!(
                    "kasumi: state type mismatch at {} (created at {}). Expected `{}`.",
                    self.id,
                    self.path,
                    type_name::<T>()
                )
            });
            f(typed);
        });
        self.store.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Rc<StateStore> {
        let store = Rc::new(StateStore::new());
        store.begin_frame(1);
        store
    }

    #[test]
    fn typed_get_set_and_update() {
        let store = store();
        let state = State::new(store.clone(), Id::from_raw(7), "counter".into(), || 0i32);
        assert_eq!(state.get(), 0);
        state.set(4);
        state.update(|value| *value += 1);
        assert_eq!(state.get(), 5);
        assert!(store.take_repaint());
    }

    #[test]
    fn ttl_eviction_reinitializes() {
        let store = store();
        store.set_ttl_frames(2);
        let state = State::new(store.clone(), Id::from_raw(9), "ephemeral".into(), || 1u8);
        assert_eq!(state.get(), 1);
        state.set(9);
        for frame in 2..=6 {
            store.begin_frame(frame);
            store.end_frame();
        }
        assert_eq!(state.get(), 1);
    }

    #[test]
    fn persistent_survives_ttl() {
        let store = store();
        store.set_ttl_frames(1);
        let state =
            State::new(store.clone(), Id::from_raw(11), "sticky".into(), || 3u8).persistent();
        state.set(8);
        for frame in 2..=8 {
            store.begin_frame(frame);
            store.end_frame();
        }
        assert_eq!(state.get(), 8);
    }

    #[test]
    #[should_panic(expected = "state type mismatch")]
    fn wrong_type_panics() {
        let store = store();
        let a = State::new(store.clone(), Id::from_raw(13), "a".into(), || 1u32);
        let b = State::new(store.clone(), Id::from_raw(13), "b".into(), || "text");
        let _ = a.get();
        let _ = b.get();
    }
}
