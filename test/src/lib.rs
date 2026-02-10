//! Testing utilities for Floem UI applications.
//!
//! This crate provides a headless harness for testing Floem views without
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
//!     let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
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
//!     let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
//!     harness.click(50.0, 50.0);
//!
//!     // Front (z-index 10) should receive the click
//!     assert_eq!(tracker.clicked_names(), vec!["front"]);
//! }
//! ```

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use floem::kurbo::Vec2;
use floem::prelude::*;
use floem::views::scroll::ScrollChanged;
use floem::views::{Decorators, Stack};

// Re-export the headless harness from floem
pub use floem::headless::*;

// Re-export commonly used floem types for convenience
pub use floem::ViewId;

/// Prelude module for convenient imports in tests.
pub mod prelude {
    pub use super::{ClickTracker, PointerCaptureTracker, ScrollTracker, layer, layers};
    pub use floem::ViewId;
    pub use floem::event::PointerId;
    pub use floem::headless::*;
    pub use floem::prelude::*;
    pub use floem::views::{Container, Decorators, Empty, Scroll, Stack};
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
        view.into_view().action(move || {
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
        view.into_view().action(move || {
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
        view.into_view()
            .on_event_cont(listener::Click, move |_, _| {
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
        view.into_view()
            .on_event_stop(listener::DoubleClick, move |_, _| {
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
        view.into_view()
            .on_event_stop(listener::SecondaryClick, move |_, _| {
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

    Stack::from_iter(children_iter).style(|s| s.size_full())
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
/// let mut harness = HeadlessHarness::new_with_size(scroll_view, 100.0, 100.0);
/// harness.scroll_vertical(50.0, 50.0, 50.0);
///
/// let viewport = scroll_tracker.last_viewport().unwrap();
/// assert!(viewport.y0 > 0.0, "Should have scrolled down");
/// ```
/// Kurbo types re-exported for convenience.
pub use floem::kurbo::{Point, Rect};

#[derive(Clone, Default)]
pub struct ScrollTracker {
    offsets: Rc<RefCell<Vec<Vec2>>>,
}

impl ScrollTracker {
    /// Create a new scroll tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap a Scroll view to track its viewport changes.
    pub fn track(&self, scroll: floem::views::Scroll) -> floem::views::Scroll {
        let viewports = self.offsets.clone();
        scroll.on_event_stop(ScrollChanged::listener(), move |_cx, state| {
            viewports.borrow_mut().push(state.offset);
        })
    }

    /// Returns true if any scroll events have been recorded.
    pub fn has_scrolled(&self) -> bool {
        !self.offsets.borrow().is_empty()
    }

    /// Returns the number of scroll events recorded.
    pub fn scroll_count(&self) -> usize {
        self.offsets.borrow().len()
    }

    /// Returns the last recorded viewport, if any.
    pub fn last_offset(&self) -> Option<Vec2> {
        self.offsets.borrow().last().copied()
    }

    /// Returns all recorded viewports in order.
    pub fn offsets(&self) -> Vec<Vec2> {
        self.offsets.borrow().clone()
    }

    /// Returns the current scroll position (top-left of viewport).
    pub fn scroll_position(&self) -> Option<Point> {
        self.last_offset().map(|v| v.to_point())
    }

    /// Reset the tracker, clearing all recorded viewports.
    pub fn reset(&self) {
        self.offsets.borrow_mut().clear();
    }
}

/// Type alias for pointer event tracking with optional pointer ID.
type PointerEventLog = Rc<RefCell<Vec<(String, Option<floem::event::PointerId>)>>>;

/// Tracks pointer capture events on views for testing.
///
/// This helper makes it easy to verify which views received GotPointerCapture
/// and LostPointerCapture events.
///
/// # Example
///
/// ```rust,ignore
/// let tracker = PointerCaptureTracker::new();
///
/// let view = tracker.track("my_view", my_view);
/// // ... set pointer capture ...
/// assert!(tracker.got_capture_count() > 0);
/// ```
#[derive(Clone, Default)]
pub struct PointerCaptureTracker {
    got_captures: Rc<RefCell<Vec<(String, floem::event::PointerId)>>>,
    lost_captures: Rc<RefCell<Vec<(String, floem::event::PointerId)>>>,
    pointer_downs: PointerEventLog,
    pointer_moves: PointerEventLog,
    pointer_ups: PointerEventLog,
}

impl PointerCaptureTracker {
    /// Create a new pointer capture tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap a view to track pointer capture events with a name.
    pub fn track<V: IntoView>(&self, name: &str, view: V) -> impl IntoView + use<V> {
        let got_captures = self.got_captures.clone();
        let lost_captures = self.lost_captures.clone();
        let pointer_downs = self.pointer_downs.clone();
        let pointer_moves = self.pointer_moves.clone();
        let pointer_ups = self.pointer_ups.clone();
        let name = name.to_string();

        let name_got = name.clone();
        let name_lost = name.clone();
        let name_down = name.clone();
        let name_move = name.clone();
        let name_up = name.clone();

        view.into_view()
            .on_event(listener::GotPointerCapture, move |_cx, drag_token| {
                got_captures
                    .borrow_mut()
                    .push((name_got.clone(), drag_token.pointer_id()));
                floem::event::EventPropagation::Continue
            })
            .on_event(listener::LostPointerCapture, move |_cx, pointer_id| {
                lost_captures
                    .borrow_mut()
                    .push((name_lost.clone(), *pointer_id));
                floem::event::EventPropagation::Continue
            })
            .on_event(listener::PointerDown, move |_cx, pe| {
                pointer_downs
                    .borrow_mut()
                    .push((name_down.clone(), pe.pointer.pointer_id));
                floem::event::EventPropagation::Continue
            })
            .on_event(listener::PointerMove, move |_cx, pu| {
                pointer_moves
                    .borrow_mut()
                    .push((name_move.clone(), pu.pointer.pointer_id));
                floem::event::EventPropagation::Continue
            })
            .on_event(listener::PointerUp, move |_cx, pe| {
                pointer_ups
                    .borrow_mut()
                    .push((name_up.clone(), pe.pointer.pointer_id));
                floem::event::EventPropagation::Continue
            })
    }

    /// Returns the number of GotPointerCapture events recorded.
    pub fn got_capture_count(&self) -> usize {
        self.got_captures.borrow().len()
    }

    /// Returns the number of LostPointerCapture events recorded.
    pub fn lost_capture_count(&self) -> usize {
        self.lost_captures.borrow().len()
    }

    /// Returns the names of views that got pointer capture, in order.
    pub fn got_capture_names(&self) -> Vec<String> {
        self.got_captures
            .borrow()
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Returns the names of views that lost pointer capture, in order.
    pub fn lost_capture_names(&self) -> Vec<String> {
        self.lost_captures
            .borrow()
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Returns the names of views that received PointerDown events, in order.
    pub fn pointer_down_names(&self) -> Vec<String> {
        self.pointer_downs
            .borrow()
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Returns the names of views that received PointerMove events, in order.
    pub fn pointer_move_names(&self) -> Vec<String> {
        self.pointer_moves
            .borrow()
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Returns the names of views that received PointerUp events, in order.
    pub fn pointer_up_names(&self) -> Vec<String> {
        self.pointer_ups
            .borrow()
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Reset the tracker, clearing all recorded events.
    pub fn reset(&self) {
        self.got_captures.borrow_mut().clear();
        self.lost_captures.borrow_mut().clear();
        self.pointer_downs.borrow_mut().clear();
        self.pointer_moves.borrow_mut().clear();
        self.pointer_ups.borrow_mut().clear();
    }
}
