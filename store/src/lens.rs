//! Lens trait for bidirectional data access.
//!
//! A lens provides both read and write access to a part of a larger structure.

use std::marker::PhantomData;

/// FNV-1a hash constants for 64-bit.
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Compute a hash of a string at compile time using FNV-1a.
///
/// This is used by the derive macro to generate deterministic path hashes
/// based on field names rather than TypeId.
pub const fn const_hash(s: &str) -> u64 {
    let bytes = s.as_bytes();
    let mut hash = FNV_OFFSET_BASIS;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    hash
}

/// Special hash value for identity lens (empty path segment).
/// This is a const so it can be used in pattern matching.
pub const IDENTITY_PATH_HASH: u64 = const_hash("");

/// A lens that can read and write a field of type `T` from a source of type `S`.
///
/// Lenses must be Copy and 'static so they can be stored in Bindings.
/// Use `#[derive(Lenses)]` to generate lens types for your structs.
pub trait Lens<S, T>: Copy + 'static {
    /// The path hash for this lens, computed from the field name.
    ///
    /// This is a const so it can be evaluated at compile time.
    /// Override this in generated lens types using `const_hash("field_name")`.
    const PATH_HASH: u64 = IDENTITY_PATH_HASH;

    /// Get a reference to the target field.
    fn get<'a>(&self, source: &'a S) -> &'a T;

    /// Get a mutable reference to the target field.
    fn get_mut<'a>(&self, source: &'a mut S) -> &'a mut T;

    /// Returns a hash used for path subscription normalization.
    ///
    /// This allows equivalent lens paths (e.g., `store.count()` vs `store.root().count()`)
    /// to share the same PathId by stripping identity lenses from the path.
    ///
    /// Default implementation returns `Self::PATH_HASH`.
    fn path_hash(&self) -> u64 {
        Self::PATH_HASH
    }
}

/// Composed lens that combines two lenses: S -> M -> T
///
/// The middle type M is part of the type signature to make the impl unambiguous.
pub struct ComposedLens<L1, L2, M> {
    first: L1,
    second: L2,
    _phantom: PhantomData<fn() -> M>,
}

impl<L1: Copy, L2: Copy, M> Clone for ComposedLens<L1, L2, M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<L1: Copy, L2: Copy, M> Copy for ComposedLens<L1, L2, M> {}

impl<L1, L2, M> ComposedLens<L1, L2, M> {
    pub fn new(first: L1, second: L2) -> Self {
        Self {
            first,
            second,
            _phantom: PhantomData,
        }
    }
}

impl<S: 'static, M: 'static, T: 'static, L1, L2> Lens<S, T> for ComposedLens<L1, L2, M>
where
    L1: Lens<S, M>,
    L2: Lens<M, T>,
{
    fn get<'a>(&self, source: &'a S) -> &'a T {
        self.second.get(self.first.get(source))
    }

    fn get_mut<'a>(&self, source: &'a mut S) -> &'a mut T {
        self.second.get_mut(self.first.get_mut(source))
    }

    /// If the first lens is an identity lens, strip it and return the second lens's path_hash.
    /// Otherwise, combine the path hashes of both lenses.
    /// This normalizes paths like `root().nested().value()` to match `nested().value()`.
    fn path_hash(&self) -> u64 {
        let first_hash = self.first.path_hash();

        if first_hash == IDENTITY_PATH_HASH {
            // First lens is identity, use the second lens's path
            self.second.path_hash()
        } else {
            // Combine the path hashes using FNV-1a continuation
            let second_hash = self.second.path_hash();
            // Use a simple combination that's order-dependent
            let mut hash = first_hash;
            hash ^= second_hash;
            hash = hash.wrapping_mul(FNV_PRIME);
            hash
        }
    }
}

/// Index lens for accessing elements of a Vec.
#[derive(Copy, Clone)]
pub struct IndexLens {
    index: usize,
}

impl IndexLens {
    pub fn new(index: usize) -> Self {
        Self { index }
    }
}

impl<T: 'static> Lens<Vec<T>, T> for IndexLens {
    // Base hash for index lens
    const PATH_HASH: u64 = const_hash("[index]");

    fn get<'a>(&self, source: &'a Vec<T>) -> &'a T {
        &source[self.index]
    }

    fn get_mut<'a>(&self, source: &'a mut Vec<T>) -> &'a mut T {
        &mut source[self.index]
    }

    /// Each index gets a unique path hash by mixing the index into the base hash.
    /// This enables per-item fine-grained reactivity.
    fn path_hash(&self) -> u64 {
        let mut hash = const_hash("[index]");
        hash ^= self.index as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        hash
    }
}

/// Key lens for accessing elements of a HashMap by key.
///
/// Note: The key must be Clone because we store it in the lens.
#[derive(Clone)]
pub struct KeyLens<K> {
    key: K,
}

impl<K: Copy> Copy for KeyLens<K> {}

impl<K> KeyLens<K> {
    pub fn new(key: K) -> Self {
        Self { key }
    }
}

impl<K, V> Lens<std::collections::HashMap<K, V>, V> for KeyLens<K>
where
    K: std::hash::Hash + Eq + Copy + 'static,
    V: 'static,
{
    // Base hash for key lens
    const PATH_HASH: u64 = const_hash("[key]");

    fn get<'a>(&self, source: &'a std::collections::HashMap<K, V>) -> &'a V {
        source
            .get(&self.key)
            .expect("KeyLens: key not found in HashMap")
    }

    fn get_mut<'a>(&self, source: &'a mut std::collections::HashMap<K, V>) -> &'a mut V {
        source
            .get_mut(&self.key)
            .expect("KeyLens: key not found in HashMap")
    }

    /// Each key gets a unique path hash by mixing the key's hash into the base hash.
    /// This enables per-key fine-grained reactivity.
    fn path_hash(&self) -> u64 {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.key.hash(&mut hasher);
        let key_hash = hasher.finish();

        let mut hash = const_hash("[key]");
        hash ^= key_hash;
        hash = hash.wrapping_mul(FNV_PRIME);
        hash
    }
}

/// KeyLens implementation for IndexMap - O(1) access by key.
impl<K, V> Lens<indexmap::IndexMap<K, V>, V> for KeyLens<K>
where
    K: std::hash::Hash + Eq + Copy + 'static,
    V: 'static,
{
    // Base hash for key lens (same as HashMap)
    const PATH_HASH: u64 = const_hash("[key]");

    fn get<'a>(&self, source: &'a indexmap::IndexMap<K, V>) -> &'a V {
        source
            .get(&self.key)
            .expect("KeyLens: key not found in IndexMap")
    }

    fn get_mut<'a>(&self, source: &'a mut indexmap::IndexMap<K, V>) -> &'a mut V {
        source
            .get_mut(&self.key)
            .expect("KeyLens: key not found in IndexMap")
    }

    /// Each key gets a unique path hash by mixing the key's hash into the base hash.
    /// This enables per-key fine-grained reactivity.
    fn path_hash(&self) -> u64 {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.key.hash(&mut hasher);
        let key_hash = hasher.finish();

        let mut hash = const_hash("[key]");
        hash ^= key_hash;
        hash = hash.wrapping_mul(FNV_PRIME);
        hash
    }
}

/// Identity lens that returns the source as-is.
pub struct IdentityLens<T>(PhantomData<fn() -> T>);

impl<T> Clone for IdentityLens<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for IdentityLens<T> {}

impl<T> Default for IdentityLens<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: 'static> Lens<T, T> for IdentityLens<T> {
    // Identity lens uses the empty string hash so it can be detected and stripped
    const PATH_HASH: u64 = IDENTITY_PATH_HASH;

    fn get<'a>(&self, source: &'a T) -> &'a T {
        source
    }

    fn get_mut<'a>(&self, source: &'a mut T) -> &'a mut T {
        source
    }
}
