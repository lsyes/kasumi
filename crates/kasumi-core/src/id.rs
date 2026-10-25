//! Stable, position-derived element identifiers.
//!
//! An `Id` is produced by hashing the path of its parent scope, its position
//! among siblings, and an optional user key. It is never allocated, never
//! recycled and can be copied freely.

use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

/// Opaque identity of an element or a state slot.
///
/// The zero value is reserved for the implicit root node.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(pub(crate) u64);

impl Id {
    pub const ROOT: Id = Id(0);

    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    pub fn is_root(self) -> bool {
        self.0 == 0
    }

    /// A short form suitable for logs and debug overlays.
    pub fn short(self) -> String {
        format!("{:016x}", self.0)
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({:016x})", self.0)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// Mixes a parent path with a local discriminator.
///
/// This is a splitmix64-style avalanche. It is deterministic across builds and
/// platforms because it does not depend on `RandomState`.
pub const fn mix(parent: u64, value: u64) -> u64 {
    let mut z = parent
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(value)
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Derives a child id from a parent and a dense sibling index.
pub fn child_id(parent: Id, index: u32) -> Id {
    Id(mix(parent.0, u64::from(index) + 1))
}

/// Derives an id for a state slot. State ids use a separate namespace from
/// node ids so that adding or removing a state call does not shift widget ids.
pub fn state_id(parent: Id, index: u32) -> Id {
    Id(mix(parent.0 ^ 0x5354_4154_455F_4E53, u64::from(index) + 1))
}

/// Derives a scoped id from a parent and an arbitrary application key.
pub fn keyed_id<K: Hash + ?Sized>(parent: Id, key: &K) -> Id {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    Id(mix(parent.0 ^ 0xA5A5_A5A5_A5A5_A5A5, hasher.finish()))
}

/// Stable hash of a string, used for labels and debug scopes.
pub fn hash_str(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn children_are_dense_and_distinct() {
        let parent = Id::from_raw(0x1234);
        let ids: Vec<_> = (0..32).map(|i| child_id(parent, i)).collect();
        let unique: std::collections::HashSet<_> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len());
        assert!(ids.iter().all(|id| *id != parent));
    }

    #[test]
    fn keyed_ids_do_not_depend_on_position() {
        let parent = Id::from_raw(99);
        assert_eq!(keyed_id(parent, &"alpha"), keyed_id(parent, &"alpha"));
        assert_ne!(keyed_id(parent, &"alpha"), keyed_id(parent, &"beta"));
    }
}
