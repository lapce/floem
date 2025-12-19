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
use floem::views::{Decorators, stack_from_iter};

// Re-export the test harness from floem
pub use floem::test_harness::*;

// Re-export commonly used floem types for convenience
pub use floem::ViewId;

/// Prelude module for convenient imports in tests.
pub mod prelude {
    pub use super::{ClickTracker, ScrollTracker, TestHarnessExt, layer, layers};
    pub use floem::ViewId;
    pub use floem::prelude::*;
    pub use floem::test_harness::*;
    pub use floem::views::{Container, Decorators, Empty, Scroll, stack, stack_from_iter};
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
    double_clicks: Rc<RefCell<Vec<Option<String>>>>,
    double_click_count: Rc<Cell<usize>>,
    secondary_clicks: Rc<RefCell<Vec<Option<String>>>>,
    secondary_click_count: Rc<Cell<usize>>,
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

    /// Wrap a view to track when it receives clicks, with a name for identification.
    ///
    /// The view will have `on_click_cont` added to it, allowing the event to bubble.
    pub fn track_named_cont<V: IntoView>(&self, name: &str, view: V) -> impl IntoView + use<V> {
        let clicks = self.clicks.clone();
        let count = self.count.clone();
        let name = name.to_string();
        view.into_view().on_click_cont(move |_| {
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
        self.double_clicks.borrow_mut().clear();
        self.double_click_count.set(0);
        self.secondary_clicks.borrow_mut().clear();
        self.secondary_click_count.set(0);
    }

    /// Wrap a view to track when it receives double clicks, with a name for identification.
    ///
    /// The view will have `on_double_click_stop` added to it.
    pub fn track_double_click<V: IntoView>(&self, name: &str, view: V) -> impl IntoView + use<V> {
        let clicks = self.double_clicks.clone();
        let count = self.double_click_count.clone();
        let name = name.to_string();
        view.into_view().on_double_click_stop(move |_| {
            clicks.borrow_mut().push(Some(name.clone()));
            count.set(count.get() + 1);
        })
    }

    /// Returns the number of double clicks recorded.
    pub fn double_click_count(&self) -> usize {
        self.double_click_count.get()
    }

    /// Returns the names of double-clicked views in order.
    pub fn double_clicked_names(&self) -> Vec<String> {
        self.double_clicks
            .borrow()
            .iter()
            .filter_map(|n| n.clone())
            .collect()
    }

    /// Wrap a view to track when it receives secondary (right) clicks, with a name.
    ///
    /// The view will have `on_secondary_click_stop` added to it.
    pub fn track_secondary_click<V: IntoView>(
        &self,
        name: &str,
        view: V,
    ) -> impl IntoView + use<V> {
        let clicks = self.secondary_clicks.clone();
        let count = self.secondary_click_count.clone();
        let name = name.to_string();
        view.into_view().on_secondary_click_stop(move |_| {
            clicks.borrow_mut().push(Some(name.clone()));
            count.set(count.get() + 1);
        })
    }

    /// Returns the number of secondary (right) clicks recorded.
    pub fn secondary_click_count(&self) -> usize {
        self.secondary_click_count.get()
    }

    /// Returns the names of secondary-clicked views in order.
    pub fn secondary_clicked_names(&self) -> Vec<String> {
        self.secondary_clicks
            .borrow()
            .iter()
            .filter_map(|n| n.clone())
            .collect()
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
    let children_iter = children
        .into_views()
        .into_iter()
        .map(|v| v.style(|s| s.absolute().inset(0.0).size_full()));

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

/// Tracks scroll events on Scroll views for testing.
///
/// This helper records viewport changes from scroll events, making it easy
/// to verify scroll behavior in tests.
///
/// # Example
///
/// ```rust,ignore
/// let scroll_tracker = ScrollTracker::new();
///
/// let content = Empty::new().style(|s| s.size(200.0, 400.0));
/// let scroll_view = scroll_tracker.track(Scroll::new(content));
///
/// let mut harness = TestHarness::new_with_size(scroll_view, 100.0, 100.0);
/// harness.scroll_vertical(50.0, 50.0, 50.0);
///
/// let viewport = scroll_tracker.last_viewport().unwrap();
/// assert!(viewport.y0 > 0.0, "Should have scrolled down");
/// ```
/// Kurbo types re-exported for convenience.
pub use floem::kurbo::{Point, Rect};

#[derive(Clone, Default)]
pub struct ScrollTracker {
    viewports: Rc<RefCell<Vec<Rect>>>,
}

impl ScrollTracker {
    /// Create a new scroll tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap a Scroll view to track its viewport changes.
    pub fn track(&self, scroll: floem::views::Scroll) -> floem::views::Scroll {
        let viewports = self.viewports.clone();
        scroll.on_scroll(move |viewport| {
            viewports.borrow_mut().push(viewport);
        })
    }

    /// Returns true if any scroll events have been recorded.
    pub fn has_scrolled(&self) -> bool {
        !self.viewports.borrow().is_empty()
    }

    /// Returns the number of scroll events recorded.
    pub fn scroll_count(&self) -> usize {
        self.viewports.borrow().len()
    }

    /// Returns the last recorded viewport, if any.
    pub fn last_viewport(&self) -> Option<Rect> {
        self.viewports.borrow().last().copied()
    }

    /// Returns all recorded viewports in order.
    pub fn viewports(&self) -> Vec<Rect> {
        self.viewports.borrow().clone()
    }

    /// Returns the current scroll position (top-left of viewport).
    pub fn scroll_position(&self) -> Option<Point> {
        self.last_viewport().map(|v| v.origin())
    }

    /// Reset the tracker, clearing all recorded viewports.
    pub fn reset(&self) {
        self.viewports.borrow_mut().clear();
    }
}
