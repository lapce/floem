//! Testing utilities for Floem UI applications.
//!
//! This crate provides a test harness for testing Floem views without
//! creating an actual window.
//!
//! # Example
//!
//! ```rust,ignore
//! use floem_test::prelude::*;
//!
//! #[test]
//! fn test_button_click() {
//!     let tracker = ClickTracker::new();
//!
//!     let view = tracker.track(
//!         Empty::new().style(|s| s.size(100.0, 100.0))
//!     );
//!
//!     let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);
//!     harness.click(50.0, 50.0);
//!
//!     assert!(tracker.was_clicked());
//! }
//! ```
//!
//! # Testing Z-Index / Layered Views
//!
//! ```rust,ignore
//! use floem_test::prelude::*;
//!
//! #[test]
//! fn test_z_index_click_order() {
//!     let tracker = ClickTracker::new();
//!
//!     // Create overlapping views with different z-indices
//!     let view = layers((
//!         tracker.track_named("back", Empty::new().z_index(1)),
//!         tracker.track_named("front", Empty::new().z_index(10)),
//!     ));
//!
//!     let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);
//!     harness.click(50.0, 50.0);
//!
//!     // Front (z-index 10) should receive the click
//!     assert_eq!(tracker.clicked_names(), vec!["front"]);
//! }
//! ```

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use floem::prelude::*;
use floem::views::{stack_from_iter, Decorators};

// Re-export the test harness from floem
pub use floem::test_harness::*;

// Re-export commonly used floem types for convenience
pub use floem::ViewId;

/// Prelude module for convenient imports in tests.
pub mod prelude {
    pub use super::{layer, layers, ClickTracker, TestHarnessExt};
    pub use floem::prelude::*;
    pub use floem::test_harness::*;
    pub use floem::views::{stack, stack_from_iter, Container, Decorators, Empty};
    pub use floem::ViewId;
}

/// Tracks click events on views for testing.
///
/// This helper makes it easy to verify which views received click events
/// and in what order.
///
/// # Example
///
/// ```rust,ignore
/// let tracker = ClickTracker::new();
///
/// let view = tracker.track(my_view);
/// // ... click the view ...
/// assert!(tracker.was_clicked());
/// ```
#[derive(Clone, Default)]
pub struct ClickTracker {
    clicks: Rc<RefCell<Vec<Option<String>>>>,
    count: Rc<Cell<usize>>,
}

impl ClickTracker {
    /// Create a new click tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap a view to track when it receives clicks.
    ///
    /// The view will have `on_click_stop` added to it.
    pub fn track<V: IntoView>(&self, view: V) -> impl IntoView + use<V> {
        let clicks = self.clicks.clone();
        let count = self.count.clone();
        view.into_view().on_click_stop(move |_| {
            clicks.borrow_mut().push(None);
            count.set(count.get() + 1);
        })
    }

    /// Wrap a view to track when it receives clicks, with a name for identification.
    ///
    /// The view will have `on_click_stop` added to it.
    pub fn track_named<V: IntoView>(&self, name: &str, view: V) -> impl IntoView + use<V> {
        let clicks = self.clicks.clone();
        let count = self.count.clone();
        let name = name.to_string();
        view.into_view().on_click_stop(move |_| {
            clicks.borrow_mut().push(Some(name.clone()));
            count.set(count.get() + 1);
        })
    }

    /// Returns true if any tracked view was clicked.
    pub fn was_clicked(&self) -> bool {
        self.count.get() > 0
    }

    /// Returns the number of clicks recorded.
    pub fn click_count(&self) -> usize {
        self.count.get()
    }

    /// Returns the names of clicked views in order.
    ///
    /// Views tracked without names will be represented as `None`.
    pub fn clicked(&self) -> Vec<Option<String>> {
        self.clicks.borrow().clone()
    }

    /// Returns just the names of clicked views (ignoring unnamed ones).
    pub fn clicked_names(&self) -> Vec<String> {
        self.clicks
            .borrow()
            .iter()
            .filter_map(|n| n.clone())
            .collect()
    }

    /// Reset the tracker, clearing all recorded clicks.
    pub fn reset(&self) {
        self.clicks.borrow_mut().clear();
        self.count.set(0);
    }
}

/// Create a single layer (absolute positioned view that fills its container).
///
/// This is useful for creating overlapping views for z-index testing.
///
/// # Example
///
/// ```rust,ignore
/// let view = layer(Empty::new().z_index(5));
/// ```
pub fn layer(view: impl IntoView) -> impl IntoView {
    view.into_view()
        .style(|s| s.absolute().inset(0.0).size_full())
}

/// Create a stack of overlapping layers.
///
/// Each child view is positioned absolutely to fill the container,
/// making them overlap. This is useful for testing z-index behavior.
///
/// # Example
///
/// ```rust,ignore
/// let view = layers((
///     Empty::new().z_index(1),  // Back layer
///     Empty::new().z_index(10), // Front layer
/// ));
/// ```
pub fn layers<VT: ViewTuple + 'static>(children: VT) -> impl IntoView {
    // Convert each child to a layer with absolute positioning
    let children_iter = children.into_views().into_iter().map(|v| {
        v.style(|s| s.absolute().inset(0.0).size_full())
    });

    stack_from_iter(children_iter).style(|s| s.size_full())
}

/// Extension trait for TestHarness with convenient test methods.
pub trait TestHarnessExt {
    /// Create a new test harness with a specified size.
    fn new_with_size(view: impl IntoView, width: f64, height: f64) -> TestHarness;
}

impl TestHarnessExt for TestHarness {
    /// Create a new test harness with a specified size.
    ///
    /// This is a convenience method that combines `new()` and `set_size()`.
    fn new_with_size(view: impl IntoView, width: f64, height: f64) -> TestHarness {
        let mut harness = TestHarness::new(view);
        harness.set_size(width, height);
        harness
    }
}
