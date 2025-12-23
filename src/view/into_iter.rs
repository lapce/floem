//! Trait for converting various types into an iterator of views.
//!
//! The [`IntoViewIter`] trait provides a unified way to convert arrays, tuples,
//! vectors, slices, and iterators into an iterator of views.
//!
//! # Supported Types
//!
//! | Type | Example |
//! |------|---------|
//! | Arrays `[V; N]` | `[label("a"), label("b")]` |
//! | Tuples (1-16) | `(label("a"), button("b"), "text")` |
//! | `Vec<V>` | `vec![label("a"), label("b")]` |
//! | Slices `&[V]` | `&items[..]` (requires `V: Clone`) |
//! | Empty `()` | `()` |
//! | Iterator | `iter.map(label).collect::<Vec<_>>()` |
//!
//! # Example
//!
//! ```rust
//! use floem::prelude::*;
//!
//! // Using the trait to iterate over children
//! fn process_children(children: impl IntoViewIter) {
//!     for view in children.into_view_iter() {
//!         // Each `view` is an AnyView (Box<dyn View>)
//!         println!("View id: {:?}", view.id());
//!     }
//! }
//!
//! // Can be called with various types:
//! // process_children([label("a"), label("b")]);                  // array
//! // process_children((label("a"), button("b")));                 // tuple
//! // process_children(vec![label("a"), label("b")]);              // vec
//! // process_children(items.iter().map(label).collect::<Vec<_>>()); // iterator
//! ```

use super::{AnyView, IntoView};

/// Trait for types that can be converted into an iterator of views.
///
/// Returns an iterator to enable lazy view construction and avoid intermediate allocations.
pub trait IntoViewIter {
    /// Converts this type into an iterator of boxed views.
    fn into_view_iter(self) -> impl Iterator<Item = AnyView>;
}

// =============================================================================
// Arrays: [V; N] where V: IntoView (homogeneous types, compile-time size)
// =============================================================================

impl<V: IntoView, const N: usize> IntoViewIter for [V; N] {
    fn into_view_iter(self) -> impl Iterator<Item = AnyView> {
        self.into_iter().map(|v| v.into_any())
    }
}

// =============================================================================
// Vec<V> where V: IntoView (homogeneous types, dynamic size)
// =============================================================================

impl<V: IntoView> IntoViewIter for Vec<V> {
    fn into_view_iter(self) -> impl Iterator<Item = AnyView> {
        self.into_iter().map(|v| v.into_any())
    }
}

// =============================================================================
// Slices: &[V] where V: IntoView + Clone
// =============================================================================

impl<V: IntoView + Clone> IntoViewIter for &[V] {
    fn into_view_iter(self) -> impl Iterator<Item = AnyView> {
        self.iter().cloned().map(|v| v.into_any())
    }
}

// =============================================================================
// Empty tuple
// =============================================================================

impl IntoViewIter for () {
    fn into_view_iter(self) -> impl Iterator<Item = AnyView> {
        std::iter::empty()
    }
}

// =============================================================================
// Tuples via macro (heterogeneous types)
// =============================================================================

macro_rules! impl_into_view_iter_for_tuple {
    ($($t:ident),+; $($idx:tt),+) => {
        impl<$($t: IntoView),+> IntoViewIter for ($($t,)+) {
            fn into_view_iter(self) -> impl Iterator<Item = AnyView> {
                [$(self.$idx.into_any()),+].into_iter()
            }
        }
    };
}

impl_into_view_iter_for_tuple!(A; 0);
impl_into_view_iter_for_tuple!(A, B; 0, 1);
impl_into_view_iter_for_tuple!(A, B, C; 0, 1, 2);
impl_into_view_iter_for_tuple!(A, B, C, D; 0, 1, 2, 3);
impl_into_view_iter_for_tuple!(A, B, C, D, E; 0, 1, 2, 3, 4);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F; 0, 1, 2, 3, 4, 5);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F, G; 0, 1, 2, 3, 4, 5, 6);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F, G, H; 0, 1, 2, 3, 4, 5, 6, 7);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F, G, H, I; 0, 1, 2, 3, 4, 5, 6, 7, 8);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F, G, H, I, J; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F, G, H, I, J, K; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14);
impl_into_view_iter_for_tuple!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);

#[cfg(test)]
mod tests {
    use super::*;

    // Type-level tests to ensure the trait is implemented correctly
    fn _assert_into_view_iter<T: IntoViewIter>() {}

    fn _test_impls() {
        // Arrays
        _assert_into_view_iter::<[(); 0]>();
        _assert_into_view_iter::<[(); 1]>();
        _assert_into_view_iter::<[(); 5]>();

        // Tuples
        _assert_into_view_iter::<()>();
        _assert_into_view_iter::<((),)>();
        _assert_into_view_iter::<((), ())>();
        _assert_into_view_iter::<((), (), ())>();

        // Vec
        _assert_into_view_iter::<Vec<()>>();

        // Slices
        _assert_into_view_iter::<&[()]>();

        // Vec<AnyView>
        _assert_into_view_iter::<Vec<AnyView>>();
    }
}
