//! Identity types for application windows.
//!
//! The core crate stays backend-agnostic: backends assign these keys and the
//! runtime uses them in window requests and `Ui::windows` snapshots.

/// Stable, backend-independent identity of an application window.
///
/// The primary window uses [`WindowKey::MAIN`] when it is created through the
/// single-window entry point. Backends may assign any value for additional
/// windows, but a key is never reused within one event-loop session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowKey(pub u64);

impl WindowKey {
    pub const MAIN: WindowKey = WindowKey(1);

    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl Default for WindowKey {
    fn default() -> Self {
        Self::MAIN
    }
}

impl std::fmt::Display for WindowKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "window:{}", self.0)
    }
}
