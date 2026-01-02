//! Path tracking for fine-grained reactivity.
//!
//! Each Binding has a PathId that identifies its location in the state tree.
//! When a field is updated, we notify subscribers of that specific path.
//!
//! PathIds are based on normalized lens path hashes, so equivalent paths
//! (e.g., `store.count()` vs `store.root().count()`) share the same PathId.

use std::any::TypeId;
use std::hash::{Hash, Hasher};

/// Identifier for a path in the state tree.
///
/// PathIds are determined by the normalized lens path hash, ensuring that bindings
/// to the same logical path share a PathId regardless of how they're created.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PathId(u64);

impl PathId {
    /// Create a PathId based on a lens type.
    ///
    /// Bindings with the same lens type get the same PathId.
    pub fn for_lens<L: 'static>() -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        TypeId::of::<L>().hash(&mut hasher);
        PathId(hasher.finish())
    }

    /// Create a PathId from a hash value directly.
    ///
    /// This is used with `Lens::path_hash()` for normalized paths.
    pub fn from_hash(hash: u64) -> Self {
        PathId(hash)
    }

    /// The root path (uses the unit type as a sentinel).
    pub fn root() -> Self {
        PathId::for_lens::<()>()
    }
}
