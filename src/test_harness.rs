//! Test harness for headless UI testing.
//!
//! This module provides utilities for testing Floem views without creating
//! an actual window. It allows simulating user interactions and verifying
//! view behavior.
//!
//! # Example
//!
//! ```rust,ignore
//! use floem::test_harness::TestHarness;
//! use floem::views::*;
//!
//! let harness = TestHarness::new(
//!     stack((
//!         button("Behind").z_index(1),
//!         button("In Front").z_index(10),
//!     ))
//! );
//!
//! // Simulate a click at (50, 50)
//! let result = harness.pointer_down(50.0, 50.0);
//! ```

use peniko::kurbo::{Point, Size};

use crate::context::InteractionState;
use crate::event::Event;
use crate::id::ViewId;
use crate::style::{Style, StyleSelector};
use crate::view::IntoView;
use crate::window_handle::WindowHandle;

/// Result of an event dispatch operation.
#[derive(Debug, Clone)]
pub struct EventResult {
    /// Whether the event was handled (propagation stopped).
    pub handled: bool,
    /// Whether a pointer event was consumed.
    pub consumed: bool,
}

/// A test harness for headless UI testing.
///
/// TestHarness manages a view tree and provides methods to simulate
/// user interactions without creating an actual window.
///
/// Internally, it uses a headless `WindowHandle` to ensure that test behavior
/// matches real window behavior, including the full `process_update()` cycle
/// for style recalculation and layout.
pub struct TestHarness {
    window_handle: WindowHandle,
}

impl TestHarness {
    /// Create a new test harness with the given root view.
    ///
    /// The view will be set up with default size (800x600) and scale (1.0).
    pub fn new(view: impl IntoView) -> Self {
        Self::new_with_size(view, 800.0, 600.0)
    }

    /// Create a new test harness with the given root view and window size.
    pub fn new_with_size(view: impl IntoView, width: f64, height: f64) -> Self {
        let size = Size::new(width, height);
        let window_handle = WindowHandle::new_headless(view, size, 1.0);

        Self { window_handle }
    }

    /// Set the window size and rebuild layout.
    pub fn set_size(&mut self, width: f64, height: f64) -> &mut Self {
        self.window_handle.size(Size::new(width, height));
        self
    }

    /// Set the window scale factor.
    pub fn set_scale(&mut self, scale: f64) -> &mut Self {
        self.window_handle.scale(scale);
        self
    }

    /// Run all passes: style → layout → compute_layout.
    ///
    /// This must be called after any changes to view styles or structure
    /// for events to be dispatched correctly.
    ///
    /// Note: When using the headless WindowHandle, this is typically called
    /// automatically via `process_update()` after event dispatch.
    pub fn rebuild(&mut self) {
        self.window_handle.process_update_no_paint();
    }

    /// Get the root view ID.
    pub fn root_id(&self) -> ViewId {
        self.window_handle.id
    }

    /// Dispatch an event to the view tree.
    ///
    /// This uses the full WindowHandle event dispatch and processing,
    /// including the `process_update()` cycle that handles:
    /// - Update messages
    /// - Style recalculation
    /// - Layout passes
    pub fn dispatch_event(&mut self, event: Event) -> EventResult {
        self.window_handle.event(event);

        // Note: WindowHandle::event() doesn't return propagation info,
        // so we return a basic result. The important thing is that the
        // full process_update() cycle runs.
        EventResult {
            handled: true,
            consumed: false,
        }
    }

    /// Simulate a pointer down event at the given position.
    pub fn pointer_down(&mut self, x: f64, y: f64) -> EventResult {
        self.dispatch_event(create_pointer_down(x, y))
    }

    /// Simulate a pointer up event at the given position.
    pub fn pointer_up(&mut self, x: f64, y: f64) -> EventResult {
        self.dispatch_event(create_pointer_up(x, y))
    }

    /// Simulate a pointer move event to the given position.
    pub fn pointer_move(&mut self, x: f64, y: f64) -> EventResult {
        self.dispatch_event(create_pointer_move(x, y))
    }

    /// Simulate a click (pointer down + pointer up) at the given position.
    pub fn click(&mut self, x: f64, y: f64) -> EventResult {
        self.pointer_down(x, y);
        self.pointer_up(x, y)
    }

    /// Simulate a double click at the given position.
    pub fn double_click(&mut self, x: f64, y: f64) -> EventResult {
        self.dispatch_event(create_pointer_down_with_count(x, y, 2));
        self.dispatch_event(create_pointer_up_with_count(x, y, 2))
    }

    /// Simulate a secondary (right) click at the given position.
    pub fn secondary_click(&mut self, x: f64, y: f64) -> EventResult {
        self.dispatch_event(create_secondary_pointer_down(x, y));
        self.dispatch_event(create_secondary_pointer_up(x, y))
    }

    /// Simulate a scroll wheel event at the given position.
    ///
    /// `delta_x` and `delta_y` are the scroll amounts in pixels.
    /// These are raw scroll deltas - the scroll view negates them internally.
    /// Typically: negative `delta_y` = scroll down (see lower content).
    pub fn scroll(&mut self, x: f64, y: f64, delta_x: f64, delta_y: f64) -> EventResult {
        self.dispatch_event(create_scroll_event(x, y, delta_x, delta_y))
    }

    /// Simulate scrolling down (to see lower content).
    ///
    /// `amount` is how many pixels to scroll down (positive value).
    pub fn scroll_down(&mut self, x: f64, y: f64, amount: f64) -> EventResult {
        // Negative delta because scroll view negates it
        self.scroll(x, y, 0.0, -amount)
    }

    /// Simulate scrolling up (to see higher content).
    ///
    /// `amount` is how many pixels to scroll up (positive value).
    pub fn scroll_up(&mut self, x: f64, y: f64, amount: f64) -> EventResult {
        // Positive delta because scroll view negates it
        self.scroll(x, y, 0.0, amount)
    }

    /// Simulate scrolling right (to see content further right).
    ///
    /// `amount` is how many pixels to scroll right (positive value).
    pub fn scroll_right(&mut self, x: f64, y: f64, amount: f64) -> EventResult {
        self.scroll(x, y, -amount, 0.0)
    }

    /// Simulate scrolling left (to see content further left).
    ///
    /// `amount` is how many pixels to scroll left (positive value).
    pub fn scroll_left(&mut self, x: f64, y: f64, amount: f64) -> EventResult {
        self.scroll(x, y, amount, 0.0)
    }

    /// Simulate a line-based scroll wheel event at the given position.
    ///
    /// `lines_x` and `lines_y` are the number of lines to scroll.
    /// This is converted to pixel delta using a default line height of 20 pixels.
    pub fn scroll_lines(&mut self, x: f64, y: f64, lines_x: f32, lines_y: f32) -> EventResult {
        self.dispatch_event(create_scroll_lines_event(x, y, lines_x, lines_y))
    }

    /// Find the view at the given position (hit test).
    pub fn view_at(&self, x: f64, y: f64) -> Option<ViewId> {
        hit_test(self.root_id(), Point::new(x, y))
    }

    /// Check if a view is currently in the "clicking" state (pointer down but not up).
    pub fn is_clicking(&self, id: ViewId) -> bool {
        self.window_handle.window_state.is_clicking(&id)
    }

    /// Check if a view is currently hovered.
    pub fn is_hovered(&self, id: ViewId) -> bool {
        self.window_handle.window_state.is_hovered(&id)
    }

    /// Check if a view is currently focused.
    pub fn is_focused(&self, id: ViewId) -> bool {
        self.window_handle.window_state.is_focused(&id)
    }

    /// Check if a view is currently active.
    pub fn is_active(&self, id: ViewId) -> bool {
        self.window_handle.window_state.is_active(&id)
    }

    /// Get the current interaction state for a view.
    ///
    /// This returns the same state used during style computation to determine
    /// which style selectors (hover, active, focused, etc.) apply to the view.
    pub fn get_interaction_state(&self, id: ViewId) -> InteractionState {
        InteractionState {
            is_selected: id.is_selected(),
            is_hovered: self.window_handle.window_state.is_hovered(&id),
            is_disabled: id.is_disabled(),
            is_focused: self.window_handle.window_state.is_focused(&id),
            is_clicking: self.window_handle.window_state.is_clicking(&id),
            is_dark_mode: self.window_handle.window_state.is_dark_mode(),
            is_file_hover: self.window_handle.window_state.is_file_hover(&id),
            using_keyboard_navigation: self.window_handle.window_state.keyboard_navigation,
        }
    }

    /// Check if a view has styles defined for the given selector.
    ///
    /// For example, `has_style_for_selector(id, StyleSelector::Active)` returns true
    /// if the view has an `:active` style defined.
    pub fn has_style_for_selector(&mut self, id: ViewId, selector: StyleSelector) -> bool {
        self.window_handle
            .window_state
            .has_style_for_sel(id, selector)
    }

    /// Get the computed style for a view.
    ///
    /// This returns a clone of the fully computed style after all style passes have run.
    /// Use this to verify that style changes (e.g., from :active or :hover selectors)
    /// are being applied correctly.
    ///
    /// # Example
    /// ```rust,ignore
    /// use floem::style::Background;
    ///
    /// harness.pointer_down(50.0, 50.0);
    /// let style = harness.get_computed_style(id);
    /// let bg = style.get(Background);
    /// assert!(bg.is_some(), "Background should be set when :active");
    /// ```
    pub fn get_computed_style(&self, id: ViewId) -> Style {
        id.state().borrow().computed_style.clone()
    }

    /// Trigger a style recalculation pass.
    ///
    /// This runs the full process_update cycle which includes style recalculation.
    /// With the headless WindowHandle, this is typically called automatically
    /// after event dispatch, but can be called manually if needed.
    pub fn recompute_styles(&mut self) {
        self.window_handle.process_update_no_paint();
    }

    /// Request style recalculation for views with Active selector, then recompute.
    ///
    /// This simulates the full pointer-up flow:
    /// 1. Request style update for views with Active selector
    /// 2. Clear clicking state
    /// 3. Run style recalculation via process_update
    pub fn process_pointer_up_styles(&mut self) {
        // Request style update for views that have Active selector
        for id in self.window_handle.window_state.clicking.clone() {
            if self
                .window_handle
                .window_state
                .has_style_for_sel(id, StyleSelector::Active)
            {
                id.request_style_recursive();
            }
        }

        // Clicking state should already be cleared by dispatch_event,
        // but clear it here too for safety
        self.window_handle.window_state.clicking.clear();

        // Run full update cycle
        self.window_handle.process_update_no_paint();
    }

    /// Check if a repaint was requested.
    ///
    /// This is useful for verifying that style changes trigger repaints.
    pub fn paint_requested(&self) -> bool {
        self.window_handle.window_state.request_paint
    }

    /// Clear the paint request flag.
    ///
    /// Call this before an operation to then check if it triggered a repaint.
    pub fn clear_paint_request(&mut self) {
        self.window_handle.window_state.request_paint = false;
    }

    /// Check if a view has pending style changes.
    pub fn has_pending_style_change(&self, id: ViewId) -> bool {
        use crate::view::ChangeFlags;
        id.state()
            .borrow()
            .requested_changes
            .contains(ChangeFlags::STYLE)
    }

    /// Get the viewport rectangle for a view, if one is set.
    ///
    /// This is typically set on children of scroll views to indicate
    /// which portion of the child is currently visible.
    pub fn get_viewport(&self, id: ViewId) -> Option<peniko::kurbo::Rect> {
        id.state().borrow().viewport
    }

    /// Get the layout rectangle for a view.
    ///
    /// This is the rectangle in window coordinates where the view is positioned.
    pub fn get_layout_rect(&self, id: ViewId) -> peniko::kurbo::Rect {
        id.state().borrow().layout_rect
    }

    /// Get the size of a view from its layout.
    pub fn get_size(&self, id: ViewId) -> Option<Size> {
        id.get_size()
    }

    /// Get the content rectangle for a view (excluding padding/borders).
    pub fn get_content_rect(&self, id: ViewId) -> peniko::kurbo::Rect {
        id.get_content_rect()
    }
}

/// Create a pointer down event at the given position.
fn create_pointer_down(x: f64, y: f64) -> Event {
    create_pointer_down_with_count(x, y, 1)
}

/// Create a pointer down event at the given position with a specific click count.
fn create_pointer_down_with_count(x: f64, y: f64, count: u8) -> Event {
    use ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
    };

    Event::Pointer(PointerEvent::Down(PointerButtonEvent {
        state: ui_events::pointer::PointerState {
            position: dpi::PhysicalPosition::new(x, y),
            count,
            ..Default::default()
        },
        button: Some(PointerButton::Primary),
        pointer: PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        },
    }))
}

/// Create a pointer up event at the given position.
fn create_pointer_up(x: f64, y: f64) -> Event {
    create_pointer_up_with_count(x, y, 1)
}

/// Create a pointer up event at the given position with a specific click count.
fn create_pointer_up_with_count(x: f64, y: f64, count: u8) -> Event {
    use ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
    };

    Event::Pointer(PointerEvent::Up(PointerButtonEvent {
        state: ui_events::pointer::PointerState {
            position: dpi::PhysicalPosition::new(x, y),
            count,
            ..Default::default()
        },
        button: Some(PointerButton::Primary),
        pointer: PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        },
    }))
}

/// Create a secondary (right) pointer down event at the given position.
fn create_secondary_pointer_down(x: f64, y: f64) -> Event {
    use ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
    };

    Event::Pointer(PointerEvent::Down(PointerButtonEvent {
        state: ui_events::pointer::PointerState {
            position: dpi::PhysicalPosition::new(x, y),
            count: 1,
            ..Default::default()
        },
        button: Some(PointerButton::Secondary),
        pointer: PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        },
    }))
}

/// Create a secondary (right) pointer up event at the given position.
fn create_secondary_pointer_up(x: f64, y: f64) -> Event {
    use ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
    };

    Event::Pointer(PointerEvent::Up(PointerButtonEvent {
        state: ui_events::pointer::PointerState {
            position: dpi::PhysicalPosition::new(x, y),
            count: 1,
            ..Default::default()
        },
        button: Some(PointerButton::Secondary),
        pointer: PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        },
    }))
}

/// Create a pointer move event to the given position.
fn create_pointer_move(x: f64, y: f64) -> Event {
    use ui_events::pointer::{PointerEvent, PointerId, PointerInfo, PointerType, PointerUpdate};

    Event::Pointer(PointerEvent::Move(PointerUpdate {
        pointer: PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        },
        current: ui_events::pointer::PointerState {
            position: dpi::PhysicalPosition::new(x, y),
            count: 0,
            ..Default::default()
        },
        coalesced: Vec::new(),
        predicted: Vec::new(),
    }))
}

/// Create a scroll event with pixel delta at the given position.
fn create_scroll_event(x: f64, y: f64, delta_x: f64, delta_y: f64) -> Event {
    use dpi::PhysicalPosition;
    use ui_events::ScrollDelta;
    use ui_events::pointer::{
        PointerEvent, PointerId, PointerInfo, PointerScrollEvent, PointerType,
    };

    Event::Pointer(PointerEvent::Scroll(PointerScrollEvent {
        state: ui_events::pointer::PointerState {
            position: PhysicalPosition::new(x, y),
            count: 0,
            ..Default::default()
        },
        delta: ScrollDelta::PixelDelta(PhysicalPosition::new(delta_x, delta_y)),
        pointer: PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        },
    }))
}

/// Create a scroll event with line delta at the given position.
fn create_scroll_lines_event(x: f64, y: f64, lines_x: f32, lines_y: f32) -> Event {
    use dpi::PhysicalPosition;
    use ui_events::ScrollDelta;
    use ui_events::pointer::{
        PointerEvent, PointerId, PointerInfo, PointerScrollEvent, PointerType,
    };

    Event::Pointer(PointerEvent::Scroll(PointerScrollEvent {
        state: ui_events::pointer::PointerState {
            position: PhysicalPosition::new(x, y),
            count: 0,
            ..Default::default()
        },
        delta: ScrollDelta::LineDelta(lines_x, lines_y),
        pointer: PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Mouse,
        },
    }))
}

/// Perform a hit test to find the view at the given point.
/// Uses CSS stacking context semantics to find the topmost view.
fn hit_test(view_id: ViewId, point: Point) -> Option<ViewId> {
    if view_id.is_hidden() {
        return None;
    }

    let state = view_id.state();
    let layout_rect = state.borrow().layout_rect;

    if !layout_rect.contains(point) {
        return None;
    }

    // Check children using stacking context semantics (reverse order for top-first)
    let items = crate::context::collect_stacking_context_items(view_id);
    for item in items.iter().rev() {
        if item.view_id.is_hidden() {
            continue;
        }

        let child_state = item.view_id.state();
        let child_rect = child_state.borrow().layout_rect;

        if child_rect.contains(point) {
            // If this item creates a stacking context, recursively hit test its children
            if item.creates_context {
                if let Some(hit) = hit_test(item.view_id, point) {
                    return Some(hit);
                }
            } else {
                // Non-stacking-context items: the item itself is a hit candidate
                // Its children are already in our flat list
                return Some(item.view_id);
            }
        }
    }

    Some(view_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::UnitExt;
    use crate::view::HasViewId;
    use crate::views::{Decorators, Empty, stack};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn test_harness_creation() {
        let harness = TestHarness::new(Empty::new());
        assert!(!harness.root_id().is_hidden());
    }

    #[test]
    fn test_click_handler_basic() {
        let clicked = Rc::new(Cell::new(false));
        let clicked_clone = clicked.clone();

        let view = Empty::new()
            .style(|s| s.size(100.0, 100.0))
            .on_click_stop(move |_| {
                clicked_clone.set(true);
            });

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        // Simulate click
        harness.click(50.0, 50.0);

        assert!(clicked.get(), "Click handler should have been called");
    }

    #[test]
    fn test_z_index_click_ordering() {
        // Test that views with higher z-index receive clicks first
        let clicked_z1 = Rc::new(Cell::new(false));
        let clicked_z10 = Rc::new(Cell::new(false));

        let clicked_z1_clone = clicked_z1.clone();
        let clicked_z10_clone = clicked_z10.clone();

        let view = stack((
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
                .on_click_stop(move |_| {
                    clicked_z1_clone.set(true);
                }),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10))
                .on_click_stop(move |_| {
                    clicked_z10_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        // Click in the center where both views overlap
        harness.click(50.0, 50.0);

        // z-index 10 should have been clicked, z-index 1 should not
        assert!(
            clicked_z10.get(),
            "View with z-index 10 should receive the click"
        );
        assert!(
            !clicked_z1.get(),
            "View with z-index 1 should NOT receive the click (blocked by z-index 10)"
        );
    }

    #[test]
    fn test_stacking_context_child_escapes() {
        // Test CSS stacking context escaping: a child with high z-index inside a
        // non-stacking-context parent should "escape" and receive clicks before
        // siblings with lower z-index.
        //
        // Structure:
        //   Root
        //   ├── Wrapper (no z-index, no stacking context)
        //   │   └── Child (z-index: 10)  <-- should receive click!
        //   └── Sibling (z-index: 5)
        //
        // Child should receive the click because it escapes Wrapper's non-stacking-context
        // and competes directly with Sibling.

        let clicked_child = Rc::new(Cell::new(false));
        let clicked_sibling = Rc::new(Cell::new(false));

        let clicked_child_clone = clicked_child.clone();
        let clicked_sibling_clone = clicked_sibling.clone();

        let view = stack((
            // Wrapper without z-index (no stacking context)
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10))
                .on_click_stop(move |_| {
                    clicked_child_clone.set(true);
                }),))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
            // Sibling with z-index 5
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_sibling_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        // Child (z=10) should receive click, not Sibling (z=5)
        assert!(
            clicked_child.get(),
            "Child with z-index 10 should receive click (escaped from non-stacking-context parent)"
        );
        assert!(
            !clicked_sibling.get(),
            "Sibling with z-index 5 should NOT receive click"
        );
    }

    #[test]
    fn test_stacking_context_bounds_children() {
        // Test CSS stacking context bounding: a child with high z-index inside a
        // stacking-context parent should be BOUNDED and NOT receive clicks before
        // siblings with higher z-index than the parent.
        //
        // Structure:
        //   Root
        //   ├── Parent (z-index: 1, creates stacking context)
        //   │   └── Child (z-index: 100)  <-- bounded within Parent!
        //   └── Sibling (z-index: 5)  <-- should receive click!
        //
        // Sibling should receive the click because Child is bounded within Parent,
        // and Sibling (z=5) > Parent (z=1).

        let clicked_child = Rc::new(Cell::new(false));
        let clicked_sibling = Rc::new(Cell::new(false));

        let clicked_child_clone = clicked_child.clone();
        let clicked_sibling_clone = clicked_sibling.clone();

        let view = stack((
            // Parent with z-index 1 (creates stacking context)
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
                .on_click_stop(move |_| {
                    clicked_child_clone.set(true);
                }),))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1)),
            // Sibling with z-index 5
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_sibling_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        // Sibling (z=5) should receive click because Child is bounded within Parent (z=1)
        assert!(
            clicked_sibling.get(),
            "Sibling with z-index 5 should receive click (Parent z=1 < Sibling z=5)"
        );
        assert!(
            !clicked_child.get(),
            "Child with z-index 100 should NOT receive click (bounded within Parent z=1)"
        );
    }

    #[test]
    fn test_stacking_context_complex_interleaving() {
        // Complex test with multiple escaping children interleaving with siblings
        //
        // Structure:
        //   Root
        //   ├── A (no z-index, no stacking context)
        //   │   ├── A1 (z-index: 3)
        //   │   └── A2 (z-index: 7)  <-- should receive click!
        //   ├── B (z-index: 5)
        //   └── C (z-index: 6)
        //
        // Event order (reverse of paint): A2 (7), C (6), B (5), A1 (3), A (0)
        // A2 should receive the click.

        let clicked_a1 = Rc::new(Cell::new(false));
        let clicked_a2 = Rc::new(Cell::new(false));
        let clicked_b = Rc::new(Cell::new(false));
        let clicked_c = Rc::new(Cell::new(false));

        let clicked_a1_clone = clicked_a1.clone();
        let clicked_a2_clone = clicked_a2.clone();
        let clicked_b_clone = clicked_b.clone();
        let clicked_c_clone = clicked_c.clone();

        let view = stack((
            // A (no stacking context)
            stack((
                Empty::new()
                    .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(3))
                    .on_click_stop(move |_| {
                        clicked_a1_clone.set(true);
                    }),
                Empty::new()
                    .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(7))
                    .on_click_stop(move |_| {
                        clicked_a2_clone.set(true);
                    }),
            ))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
            // B (z-index: 5)
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_b_clone.set(true);
                }),
            // C (z-index: 6)
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(6))
                .on_click_stop(move |_| {
                    clicked_c_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        // A2 (z=7) should receive click - highest z-index among all participants
        assert!(
            clicked_a2.get(),
            "A2 with z-index 7 should receive click (escaped and highest)"
        );
        assert!(!clicked_c.get(), "C should NOT receive click");
        assert!(!clicked_b.get(), "B should NOT receive click");
        assert!(!clicked_a1.get(), "A1 should NOT receive click");
    }

    #[test]
    fn test_stacking_context_negative_z_index() {
        // Test negative z-index: views with negative z-index are painted first
        // and receive events last.
        //
        // Structure:
        //   Root
        //   ├── A (z-index: -1)
        //   ├── B (no z-index, effectively 0)  <-- should receive click!
        //   └── C (z-index: -5)
        //
        // B (z=0) should receive the click because it's highest.

        let clicked_a = Rc::new(Cell::new(false));
        let clicked_b = Rc::new(Cell::new(false));
        let clicked_c = Rc::new(Cell::new(false));

        let clicked_a_clone = clicked_a.clone();
        let clicked_b_clone = clicked_b.clone();
        let clicked_c_clone = clicked_c.clone();

        let view = stack((
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(-1))
                .on_click_stop(move |_| {
                    clicked_a_clone.set(true);
                }),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
                .on_click_stop(move |_| {
                    clicked_b_clone.set(true);
                }),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(-5))
                .on_click_stop(move |_| {
                    clicked_c_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            clicked_b.get(),
            "B (z=0) should receive click - highest z-index"
        );
        assert!(!clicked_a.get(), "A (z=-1) should NOT receive click");
        assert!(!clicked_c.get(), "C (z=-5) should NOT receive click");
    }

    #[test]
    fn test_stacking_context_transform_creates_context() {
        // Test that transform creates a stacking context, bounding children.
        //
        // Structure:
        //   Root
        //   ├── Parent (transform: scale 101%, creates stacking context)
        //   │   └── Child (z-index: 100)  <-- bounded within Parent!
        //   └── Sibling (z-index: 5)  <-- should receive click!
        //
        // Sibling should receive click because Parent has transform (creates context),
        // bounding Child, and Parent itself has z=0 < Sibling z=5.

        let clicked_child = Rc::new(Cell::new(false));
        let clicked_sibling = Rc::new(Cell::new(false));

        let clicked_child_clone = clicked_child.clone();
        let clicked_sibling_clone = clicked_sibling.clone();

        let view = stack((
            // Parent with non-identity transform (creates stacking context even without z-index)
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
                .on_click_stop(move |_| {
                    clicked_child_clone.set(true);
                }),))
            .style(|s| {
                s.absolute()
                    .inset(0.0)
                    .size(100.0, 100.0)
                    .scale(101.pct()) // Non-identity transform creates stacking context
            }),
            // Sibling with z-index 5
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_sibling_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        // Sibling (z=5) should receive click because Parent (with transform, z=0) bounds Child
        assert!(
            clicked_sibling.get(),
            "Sibling (z=5) should receive click - Parent's transform creates stacking context"
        );
        assert!(
            !clicked_child.get(),
            "Child should NOT receive click - bounded by Parent's transform stacking context"
        );
    }

    #[test]
    fn test_stacking_context_deeply_nested_escape() {
        // Test deeply nested escaping: a child nested multiple levels deep can still
        // escape if no ancestor creates a stacking context.
        //
        // Structure:
        //   Root
        //   ├── Level1 (no z-index)
        //   │   └── Level2 (no z-index)
        //   │       └── Level3 (no z-index)
        //   │           └── DeepChild (z-index: 10)  <-- should receive click!
        //   └── Sibling (z-index: 5)
        //
        // DeepChild escapes all the way up and competes with Sibling.

        let clicked_deep = Rc::new(Cell::new(false));
        let clicked_sibling = Rc::new(Cell::new(false));

        let clicked_deep_clone = clicked_deep.clone();
        let clicked_sibling_clone = clicked_sibling.clone();

        let view = stack((
            // Level1
            stack((
                // Level2
                stack((
                    // Level3
                    stack((Empty::new()
                        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10))
                        .on_click_stop(move |_| {
                            clicked_deep_clone.set(true);
                        }),))
                    .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
                ))
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
            ))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
            // Sibling
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_sibling_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            clicked_deep.get(),
            "DeepChild (z=10) should receive click - escaped through 3 levels"
        );
        assert!(
            !clicked_sibling.get(),
            "Sibling (z=5) should NOT receive click"
        );
    }

    #[test]
    fn test_stacking_context_dom_order_tiebreaker() {
        // Test DOM order as tiebreaker: when multiple views have the same z-index,
        // the later one in DOM order should receive events first (painted last).
        //
        // Structure:
        //   Root
        //   ├── First (z-index: 5)
        //   ├── Second (z-index: 5)
        //   └── Third (z-index: 5)  <-- should receive click! (last in DOM)

        let clicked_first = Rc::new(Cell::new(false));
        let clicked_second = Rc::new(Cell::new(false));
        let clicked_third = Rc::new(Cell::new(false));

        let clicked_first_clone = clicked_first.clone();
        let clicked_second_clone = clicked_second.clone();
        let clicked_third_clone = clicked_third.clone();

        let view = stack((
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_first_clone.set(true);
                }),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_second_clone.set(true);
                }),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_third_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            clicked_third.get(),
            "Third should receive click - last in DOM order with same z-index"
        );
        assert!(!clicked_second.get(), "Second should NOT receive click");
        assert!(!clicked_first.get(), "First should NOT receive click");
    }

    #[test]
    fn test_stacking_context_mixed_contexts() {
        // Test mixed stacking contexts: some views create context, some don't.
        //
        // Structure:
        //   Root
        //   ├── NoContext (no z-index)
        //   │   └── Escaper (z-index: 8)  <-- escapes!
        //   ├── WithContext (z-index: 3, creates context)
        //   │   └── Bounded (z-index: 100)  <-- bounded by WithContext
        //   └── TopLevel (z-index: 6)
        //
        // Event order: Escaper (8), TopLevel (6), WithContext (3) -> Bounded (100)
        // Escaper should receive click.

        let clicked_escaper = Rc::new(Cell::new(false));
        let clicked_bounded = Rc::new(Cell::new(false));
        let clicked_top = Rc::new(Cell::new(false));

        let clicked_escaper_clone = clicked_escaper.clone();
        let clicked_bounded_clone = clicked_bounded.clone();
        let clicked_top_clone = clicked_top.clone();

        let view = stack((
            // NoContext wrapper
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(8))
                .on_click_stop(move |_| {
                    clicked_escaper_clone.set(true);
                }),))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
            // WithContext (z-index creates stacking context)
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
                .on_click_stop(move |_| {
                    clicked_bounded_clone.set(true);
                }),))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(3)),
            // TopLevel
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(6))
                .on_click_stop(move |_| {
                    clicked_top_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            clicked_escaper.get(),
            "Escaper (z=8) should receive click - escaped and highest"
        );
        assert!(
            !clicked_top.get(),
            "TopLevel (z=6) should NOT receive click"
        );
        assert!(
            !clicked_bounded.get(),
            "Bounded (z=100) should NOT receive click - bounded by WithContext (z=3)"
        );
    }

    #[test]
    fn test_stacking_context_partial_overlap() {
        // Test partial overlap: click coordinates matter for hit testing.
        //
        // Structure:
        //   Root (200x100)
        //   ├── Left (0-100, z-index: 5)
        //   └── Right (100-200, z-index: 10)
        //
        // Click at (50, 50) should hit Left.
        // Click at (150, 50) should hit Right.

        let clicked_left = Rc::new(Cell::new(false));
        let clicked_right = Rc::new(Cell::new(false));

        let clicked_left_clone = clicked_left.clone();
        let clicked_right_clone = clicked_right.clone();

        let view = stack((
            Empty::new()
                .style(|s| {
                    s.absolute()
                        .inset_left(0.0)
                        .inset_top(0.0)
                        .size(100.0, 100.0)
                        .z_index(5)
                })
                .on_click_stop(move |_| {
                    clicked_left_clone.set(true);
                }),
            Empty::new()
                .style(|s| {
                    s.absolute()
                        .inset_left(100.0)
                        .inset_top(0.0)
                        .size(100.0, 100.0)
                        .z_index(10)
                })
                .on_click_stop(move |_| {
                    clicked_right_clone.set(true);
                }),
        ))
        .style(|s| s.size(200.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 200.0, 100.0);

        // Click left side
        harness.click(50.0, 50.0);

        assert!(
            clicked_left.get(),
            "Left should receive click at (50, 50)"
        );
        assert!(
            !clicked_right.get(),
            "Right should NOT receive click at (50, 50)"
        );

        // Reset
        clicked_left.set(false);
        clicked_right.set(false);

        // Click right side
        harness.click(150.0, 50.0);

        assert!(
            clicked_right.get(),
            "Right should receive click at (150, 50)"
        );
        assert!(
            !clicked_left.get(),
            "Left should NOT receive click at (150, 50)"
        );
    }

    #[test]
    fn test_stacking_context_pointer_events_none() {
        // Test that views with pointer_events_none are skipped in event dispatch.
        //
        // Structure:
        //   Root
        //   ├── Top (z-index: 10, pointer_events_none)  <-- skipped!
        //   └── Bottom (z-index: 5)  <-- should receive click!

        let clicked_top = Rc::new(Cell::new(false));
        let clicked_bottom = Rc::new(Cell::new(false));

        let clicked_top_clone = clicked_top.clone();
        let clicked_bottom_clone = clicked_bottom.clone();

        let view = stack((
            Empty::new()
                .style(|s| {
                    s.absolute()
                        .inset(0.0)
                        .size(100.0, 100.0)
                        .z_index(10)
                        .pointer_events_none()
                })
                .on_click_stop(move |_| {
                    clicked_top_clone.set(true);
                }),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_bottom_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            !clicked_top.get(),
            "Top (pointer_events_none) should NOT receive click"
        );
        assert!(
            clicked_bottom.get(),
            "Bottom should receive click when Top has pointer_events_none"
        );
    }

    #[test]
    fn test_stacking_context_hidden_view() {
        // Test that hidden views are skipped in event dispatch.
        //
        // Structure:
        //   Root
        //   ├── Hidden (z-index: 10, display: none)  <-- skipped!
        //   └── Visible (z-index: 5)  <-- should receive click!

        let clicked_hidden = Rc::new(Cell::new(false));
        let clicked_visible = Rc::new(Cell::new(false));

        let clicked_hidden_clone = clicked_hidden.clone();
        let clicked_visible_clone = clicked_visible.clone();

        let view = stack((
            Empty::new()
                .style(|s| {
                    s.absolute()
                        .inset(0.0)
                        .size(100.0, 100.0)
                        .z_index(10)
                        .display(taffy::Display::None)
                })
                .on_click_stop(move |_| {
                    clicked_hidden_clone.set(true);
                }),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_visible_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            !clicked_hidden.get(),
            "Hidden view should NOT receive click"
        );
        assert!(
            clicked_visible.get(),
            "Visible view should receive click when other is hidden"
        );
    }

    #[test]
    fn test_stacking_context_hidden_parent_hides_children() {
        // Test that children of hidden views don't receive events.
        //
        // Structure:
        //   Root
        //   ├── HiddenParent (z-index: 10, display: none)
        //   │   └── Child (z-index: 100)  <-- should NOT receive click (parent hidden)
        //   └── Visible (z-index: 5)  <-- should receive click!

        let clicked_child = Rc::new(Cell::new(false));
        let clicked_visible = Rc::new(Cell::new(false));

        let clicked_child_clone = clicked_child.clone();
        let clicked_visible_clone = clicked_visible.clone();

        let view = stack((
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
                .on_click_stop(move |_| {
                    clicked_child_clone.set(true);
                }),))
            .style(|s| {
                s.absolute()
                    .inset(0.0)
                    .size(100.0, 100.0)
                    .z_index(10)
                    .display(taffy::Display::None)
            }),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    clicked_visible_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            !clicked_child.get(),
            "Child of hidden parent should NOT receive click"
        );
        assert!(
            clicked_visible.get(),
            "Visible sibling should receive click"
        );
    }

    #[test]
    fn test_stacking_context_hidden_in_escaped_context() {
        // Test hidden view that would otherwise escape to parent stacking context.
        //
        // Structure:
        //   Root
        //   ├── Wrapper (no z-index, no stacking context)
        //   │   ├── Hidden (z-index: 10, display: none)  <-- would escape, but hidden
        //   │   └── Visible (z-index: 5)  <-- escapes, should receive click!
        //   └── Sibling (z-index: 7)
        //
        // Hidden (z=10) would beat Sibling (z=7), but it's hidden.
        // Sibling (z=7) should receive click (beats Visible z=5).

        let clicked_hidden = Rc::new(Cell::new(false));
        let clicked_visible = Rc::new(Cell::new(false));
        let clicked_sibling = Rc::new(Cell::new(false));

        let h_clone = clicked_hidden.clone();
        let v_clone = clicked_visible.clone();
        let s_clone = clicked_sibling.clone();

        let view = stack((
            stack((
                Empty::new()
                    .style(|s| {
                        s.absolute()
                            .inset(0.0)
                            .size(100.0, 100.0)
                            .z_index(10)
                            .display(taffy::Display::None)
                    })
                    .on_click_stop(move |_| {
                        h_clone.set(true);
                    }),
                Empty::new()
                    .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                    .on_click_stop(move |_| {
                        v_clone.set(true);
                    }),
            ))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(7))
                .on_click_stop(move |_| {
                    s_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(!clicked_hidden.get(), "Hidden should NOT receive click");
        assert!(
            !clicked_visible.get(),
            "Visible (z=5) should NOT receive click (Sibling z=7 wins)"
        );
        assert!(
            clicked_sibling.get(),
            "Sibling (z=7) should receive click (Hidden is skipped)"
        );
    }

    #[test]
    fn test_stacking_context_hidden_does_not_bubble() {
        // Test that events don't bubble through hidden ancestors.
        //
        // Structure:
        //   Root
        //   └── HiddenParent (display: none, with on_click)
        //       └── Child (z-index: 5, with on_click)
        //
        // Neither should receive the click (parent is hidden).

        use crate::event::EventPropagation;

        let clicked_parent = Rc::new(Cell::new(false));
        let clicked_child = Rc::new(Cell::new(false));

        let p_clone = clicked_parent.clone();
        let c_clone = clicked_child.clone();

        let view = stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click(move |_| {
                c_clone.set(true);
                EventPropagation::Continue
            }),))
        .style(|s| s.size(100.0, 100.0).display(taffy::Display::None))
        .on_click(move |_| {
            p_clone.set(true);
            EventPropagation::Continue
        });

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            !clicked_child.get(),
            "Child of hidden parent should NOT receive click"
        );
        assert!(
            !clicked_parent.get(),
            "Hidden parent should NOT receive click"
        );
    }

    #[test]
    fn test_stacking_context_nested_contexts() {
        // Test nested stacking contexts: a stacking context inside another stacking context.
        //
        // Structure:
        //   Root
        //   ├── Outer (z-index: 5, creates context)
        //   │   └── Inner (z-index: 3, creates context)
        //   │       └── DeepChild (z-index: 100)  <-- bounded by Inner, which is bounded by Outer
        //   └── Sibling (z-index: 6)  <-- should receive click!
        //
        // Sibling (z=6) > Outer (z=5), so Sibling wins.

        let clicked_deep = Rc::new(Cell::new(false));
        let clicked_sibling = Rc::new(Cell::new(false));

        let clicked_deep_clone = clicked_deep.clone();
        let clicked_sibling_clone = clicked_sibling.clone();

        let view = stack((
            // Outer stacking context
            stack((
                // Inner stacking context
                stack((Empty::new()
                    .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
                    .on_click_stop(move |_| {
                        clicked_deep_clone.set(true);
                    }),))
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(3)),
            ))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5)),
            // Sibling
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(6))
                .on_click_stop(move |_| {
                    clicked_sibling_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            clicked_sibling.get(),
            "Sibling (z=6) should receive click - higher than Outer (z=5)"
        );
        assert!(
            !clicked_deep.get(),
            "DeepChild should NOT receive click - doubly bounded by nested contexts"
        );
    }

    #[test]
    fn test_stacking_context_sibling_isolation() {
        // Test that sibling stacking contexts are isolated from each other.
        //
        // Structure:
        //   Root
        //   ├── ContextA (z-index: 5, creates context)
        //   │   └── ChildA (z-index: 100)  <-- bounded by ContextA
        //   └── ContextB (z-index: 10, creates context)  <-- should receive click!
        //       └── ChildB (z-index: 1)  <-- bounded by ContextB
        //
        // ContextB (z=10) > ContextA (z=5), so ContextB's subtree gets events first.
        // Within ContextB, ChildB (z=1) is the only option.

        let clicked_child_a = Rc::new(Cell::new(false));
        let clicked_child_b = Rc::new(Cell::new(false));

        let clicked_child_a_clone = clicked_child_a.clone();
        let clicked_child_b_clone = clicked_child_b.clone();

        let view = stack((
            // ContextA (z=5)
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
                .on_click_stop(move |_| {
                    clicked_child_a_clone.set(true);
                }),))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5)),
            // ContextB (z=10)
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
                .on_click_stop(move |_| {
                    clicked_child_b_clone.set(true);
                }),))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10)),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            clicked_child_b.get(),
            "ChildB should receive click - ContextB (z=10) > ContextA (z=5)"
        );
        assert!(
            !clicked_child_a.get(),
            "ChildA (z=100) should NOT receive click - bounded by ContextA (z=5)"
        );
    }

    #[test]
    fn test_stacking_context_event_bubbling() {
        // Test event bubbling with stacking context: when a child with z-index
        // handles an event and returns Continue, the event should bubble up
        // to its DOM parent (even if the parent has lower z-index).
        //
        // Structure:
        //   Root
        //   └── Parent (no z-index, with on_click returning Continue)
        //       └── Child (z-index: 5, with on_click returning Continue)
        //
        // Both should receive the click due to bubbling.

        use crate::event::EventPropagation;

        let clicked_parent = Rc::new(Cell::new(false));
        let clicked_child = Rc::new(Cell::new(false));

        let clicked_parent_clone = clicked_parent.clone();
        let clicked_child_clone = clicked_child.clone();

        let view = stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click(move |_| {
                clicked_child_clone.set(true);
                EventPropagation::Continue
            }),))
        .style(|s| s.size(100.0, 100.0))
        .on_click(move |_| {
            clicked_parent_clone.set(true);
            EventPropagation::Continue
        });

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(clicked_child.get(), "Child should receive click first");
        assert!(
            clicked_parent.get(),
            "Parent should receive click via bubbling"
        );
    }

    #[test]
    fn test_stacking_context_bubbling_stops_on_stop() {
        // Test that bubbling stops when a handler returns Stop.
        //
        // Structure:
        //   Root
        //   └── Parent (no z-index, with on_click returning Continue)
        //       └── Child (z-index: 5, with on_click_stop)
        //
        // Only Child should receive the click (bubbling stops).

        let clicked_parent = Rc::new(Cell::new(false));
        let clicked_child = Rc::new(Cell::new(false));

        let clicked_parent_clone = clicked_parent.clone();
        let clicked_child_clone = clicked_child.clone();

        let view = stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_child_clone.set(true);
            }),))
        .style(|s| s.size(100.0, 100.0))
        .on_click_stop(move |_| {
            clicked_parent_clone.set(true);
        });

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(clicked_child.get(), "Child should receive click");
        assert!(
            !clicked_parent.get(),
            "Parent should NOT receive click (bubbling stopped)"
        );
    }

    #[test]
    fn test_stacking_context_deep_bubbling() {
        // Test event bubbling through multiple ancestor levels (no stacking contexts).
        //
        // Structure:
        //   Root
        //   └── GrandParent (no z-index, with on_click returning Continue)
        //       └── Parent (no z-index, with on_click returning Continue)
        //           └── Child (z-index: 5, with on_click returning Continue)
        //
        // All three should receive the click due to bubbling.

        use crate::event::EventPropagation;

        let clicked_grandparent = Rc::new(Cell::new(false));
        let clicked_parent = Rc::new(Cell::new(false));
        let clicked_child = Rc::new(Cell::new(false));

        let gp_clone = clicked_grandparent.clone();
        let p_clone = clicked_parent.clone();
        let c_clone = clicked_child.clone();

        let view = stack((stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click(move |_| {
                c_clone.set(true);
                EventPropagation::Continue
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
        .on_click(move |_| {
            p_clone.set(true);
            EventPropagation::Continue
        }),))
        .style(|s| s.size(100.0, 100.0))
        .on_click(move |_| {
            gp_clone.set(true);
            EventPropagation::Continue
        });

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(clicked_child.get(), "Child should receive click first");
        assert!(
            clicked_parent.get(),
            "Parent should receive click via bubbling"
        );
        assert!(
            clicked_grandparent.get(),
            "GrandParent should receive click via bubbling"
        );
    }

    #[test]
    fn test_stacking_context_bubbling_across_stacking_contexts() {
        // Test event bubbling through nested stacking contexts (like web browser).
        //
        // In web: events bubble through DOM ancestors regardless of stacking contexts.
        // Each ancestor with z-index creates its own stacking context, but bubbling
        // still follows the DOM tree.
        //
        // Structure:
        //   Root
        //   └── GrandParent (z-index: 1, creates stacking context)
        //       └── Parent (z-index: 2, creates stacking context)
        //           └── Child (z-index: 3, with on_click returning Continue)
        //
        // Event goes to Child, then bubbles to Parent, then GrandParent.

        use crate::event::EventPropagation;

        let clicked_grandparent = Rc::new(Cell::new(false));
        let clicked_parent = Rc::new(Cell::new(false));
        let clicked_child = Rc::new(Cell::new(false));

        let gp_clone = clicked_grandparent.clone();
        let p_clone = clicked_parent.clone();
        let c_clone = clicked_child.clone();

        let view = stack((stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(3))
            .on_click(move |_| {
                c_clone.set(true);
                EventPropagation::Continue
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(2))
        .on_click(move |_| {
            p_clone.set(true);
            EventPropagation::Continue
        }),))
        .style(|s| s.size(100.0, 100.0).z_index(1))
        .on_click(move |_| {
            gp_clone.set(true);
            EventPropagation::Continue
        });

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(clicked_child.get(), "Child should receive click first");
        assert!(
            clicked_parent.get(),
            "Parent should receive click via bubbling (matches web)"
        );
        assert!(
            clicked_grandparent.get(),
            "GrandParent should receive click via bubbling (matches web)"
        );
    }

    #[test]
    fn test_stacking_context_multiple_escaped_children() {
        // Test multiple escaped children competing: highest z-index wins.
        //
        // Structure:
        //   Root
        //   └── Wrapper (no stacking context)
        //       ├── Child1 (z-index: 3)
        //       ├── Child2 (z-index: 7)
        //       ├── Child3 (z-index: 5)
        //       └── Child4 (z-index: 7)  <-- should receive click! (same z as Child2, but later in DOM)
        //
        // All escape, Child4 wins (z=7, last in DOM order).

        let clicked = [
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(false)),
        ];

        let c0 = clicked[0].clone();
        let c1 = clicked[1].clone();
        let c2 = clicked[2].clone();
        let c3 = clicked[3].clone();

        let view = stack((stack((
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(3))
                .on_click_stop(move |_| c0.set(true)),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(7))
                .on_click_stop(move |_| c1.set(true)),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| c2.set(true)),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(7))
                .on_click_stop(move |_| c3.set(true)),
        ))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            !clicked[0].get(),
            "Child1 (z=3) should NOT receive click"
        );
        assert!(
            !clicked[1].get(),
            "Child2 (z=7) should NOT receive click - Child4 is later in DOM"
        );
        assert!(
            !clicked[2].get(),
            "Child3 (z=5) should NOT receive click"
        );
        assert!(
            clicked[3].get(),
            "Child4 (z=7, last in DOM) should receive click"
        );
    }

    #[test]
    fn test_stacking_context_explicit_z_index_zero() {
        // Test explicit z-index: 0 creates stacking context (bounds children).
        //
        // Structure:
        //   Root
        //   ├── Parent (z-index: 0, creates stacking context!)
        //   │   └── Child (z-index: 100)  <-- bounded by Parent!
        //   └── Sibling (z-index: 1)  <-- should receive click!
        //
        // Parent has explicit z-index: 0 which creates stacking context.
        // Sibling (z=1) > Parent (z=0), so Sibling wins.

        let clicked_child = Rc::new(Cell::new(false));
        let clicked_sibling = Rc::new(Cell::new(false));

        let clicked_child_clone = clicked_child.clone();
        let clicked_sibling_clone = clicked_sibling.clone();

        let view = stack((
            // Parent with explicit z-index: 0
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
                .on_click_stop(move |_| {
                    clicked_child_clone.set(true);
                }),))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(0)),
            // Sibling
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
                .on_click_stop(move |_| {
                    clicked_sibling_clone.set(true);
                }),
        ))
        .style(|s| s.size(100.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        harness.click(50.0, 50.0);

        assert!(
            clicked_sibling.get(),
            "Sibling (z=1) should receive click - higher than Parent (z=0)"
        );
        assert!(
            !clicked_child.get(),
            "Child should NOT receive click - bounded by Parent's explicit z-index: 0"
        );
    }

    #[test]
    fn test_active_style_triggers_paint() {
        use crate::peniko::color::palette::css;
        use crate::style::Background;

        let view = Empty::new().style(|s| s.size(100.0, 100.0).active(|s| s.background(css::RED)));
        let id = view.view_id();

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        // Check that view has Active selector
        assert!(
            harness.has_style_for_selector(id, crate::style::StyleSelector::Active),
            "View should have Active selector"
        );

        // Check computed background color before clicking - should be None
        let bg_before = harness.get_computed_style(id).get(Background);
        assert!(
            bg_before.is_none(),
            "Background should be None before clicking"
        );

        // Pointer down
        harness.pointer_down(50.0, 50.0);

        // Check clicking state
        assert!(
            harness.is_clicking(id),
            "Should be clicking after pointer down"
        );

        // Check computed background color after clicking - should be RED
        let bg_after = harness.get_computed_style(id).get(Background);
        assert!(
            bg_after.is_some(),
            "Background should be set after pointer down on view with :active style"
        );
    }

    #[test]
    fn test_style_request_on_clicking() {
        use crate::peniko::color::palette::css;

        let view = Empty::new().style(|s| s.size(100.0, 100.0).active(|s| s.background(css::RED)));
        let id = view.view_id();

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        // Verify selector is detected
        assert!(
            harness.has_style_for_selector(id, crate::style::StyleSelector::Active),
            "View should have Active selector"
        );

        // Check clicking state before
        assert!(!harness.is_clicking(id), "Should not be clicking initially");

        // Pointer down
        harness.pointer_down(50.0, 50.0);

        // Check clicking state after
        assert!(
            harness.is_clicking(id),
            "Should be clicking after pointer down"
        );

        // Check if has_active is still true after clicking
        assert!(
            harness.has_style_for_selector(id, crate::style::StyleSelector::Active),
            "View should still have Active selector after clicking"
        );
    }

    #[test]
    fn test_nested_stack_click_no_z_index() {
        // Reproduces the counter example structure:
        // Nested stacks with clickable elements, NO z-index set.
        //
        // Structure:
        //   Root Stack (flex column)
        //   └── Inner Stack (flex row)
        //       ├── Button1 (clickable)
        //       └── Button2 (clickable)
        //
        // This mimics the counter example where buttons are inside nested
        // tuple-based stacks without any z-index.

        let clicked_button1 = Rc::new(Cell::new(false));
        let clicked_button2 = Rc::new(Cell::new(false));

        let clicked_button1_clone = clicked_button1.clone();
        let clicked_button2_clone = clicked_button2.clone();

        let view = stack((stack((
            Empty::new()
                .style(|s| s.size(50.0, 50.0))
                .on_click_stop(move |_| {
                    clicked_button1_clone.set(true);
                }),
            Empty::new()
                .style(|s| s.size(50.0, 50.0))
                .on_click_stop(move |_| {
                    clicked_button2_clone.set(true);
                }),
        ))
        .style(|s| s.flex_row()),))
        .style(|s| s.size(100.0, 100.0).flex_col());

        let mut harness = TestHarness::new_with_size(view, 100.0, 100.0);

        // Click on button1 (should be at x=25, y=25)
        harness.click(25.0, 25.0);

        assert!(
            clicked_button1.get(),
            "Button1 should receive click in nested stack without z-index"
        );
        assert!(
            !clicked_button2.get(),
            "Button2 should NOT receive click (not at click position)"
        );
    }

    #[test]
    fn test_counter_example_structure() {
        // More closely mirrors the actual counter example structure:
        // Outer tuple with multiple children, inner tuple with 3 buttons.
        //
        // Structure (from counter example):
        //   Root (flex col)
        //   ├── Label "Value: ..."
        //   ├── Spacer
        //   └── Button Row (flex row by default as tuple)
        //       ├── "Increment" button
        //       ├── "Decrement" button
        //       └── "Reset" button

        let clicked_increment = Rc::new(Cell::new(false));
        let clicked_decrement = Rc::new(Cell::new(false));
        let clicked_reset = Rc::new(Cell::new(false));

        let inc = clicked_increment.clone();
        let dec = clicked_decrement.clone();
        let rst = clicked_reset.clone();

        // Mimics the counter example tuple structure
        let view = (
            // Value label at top
            Empty::new().style(|s| s.size(200.0, 30.0)),
            // Spacer
            Empty::new().style(|s| s.size(200.0, 10.0)),
            // Button row
            (
                Empty::new()
                    .style(|s| s.size(60.0, 30.0))
                    .on_click_stop(move |_| {
                        inc.set(true);
                    }),
                Empty::new()
                    .style(|s| s.size(60.0, 30.0))
                    .on_click_stop(move |_| {
                        dec.set(true);
                    }),
                Empty::new()
                    .style(|s| s.size(60.0, 30.0))
                    .on_click_stop(move |_| {
                        rst.set(true);
                    }),
            ),
        )
            .style(|s| s.size(200.0, 100.0).flex_col().items_center().justify_center());

        let mut harness = TestHarness::new_with_size(view, 200.0, 100.0);

        // Click on the increment button
        harness.click(30.0, 55.0);

        assert!(
            clicked_increment.get(),
            "Increment button should receive click"
        );
        assert!(
            !clicked_decrement.get(),
            "Decrement button should NOT receive click"
        );
        assert!(!clicked_reset.get(), "Reset button should NOT receive click");
    }

    #[test]
    fn test_scroll_view_creates_stacking_context() {
        // Verify that scroll views create stacking contexts.
        // This prevents their children from being collected by parent's stacking context,
        // which would cause double-painting (once at original coords, once at scroll offset).
        use crate::views::scroll::Scroll;

        let content = Empty::new().style(|s| s.size(200.0, 500.0)); // Tall content
        let scroll_view = Scroll::new(content).style(|s| s.size(200.0, 100.0));
        let scroll_id = scroll_view.view_id();

        let view = stack((scroll_view,)).style(|s| s.size(200.0, 100.0));
        let root_id = view.view_id();

        let _harness = TestHarness::new_with_size(view, 200.0, 100.0);

        // The scroll view sets viewport on its child, which should make the scroll view
        // create a stacking context (because it has a child with viewport)
        let scroll_state = scroll_id.state();
        let creates_context = scroll_state.borrow().stacking_info.creates_context;
        assert!(
            creates_context,
            "Scroll view should create stacking context (has child with viewport)"
        );

        // Verify the scroll view's children are NOT in the root's stacking context
        let root_items = crate::context::collect_stacking_context_items(root_id);
        let scroll_child_ids: Vec<_> = scroll_id.children().into_iter().collect();
        let has_scroll_children = root_items
            .iter()
            .any(|item| scroll_child_ids.contains(&item.view_id));
        assert!(
            !has_scroll_children,
            "Scroll view's children should NOT appear in parent's stacking context"
        );
    }

    #[test]
    fn test_scroll_view_click_after_scroll() {
        // Test that clicks work correctly after scrolling
        use crate::views::scroll::Scroll;

        let clicked = Rc::new(Cell::new(false));
        let clicked_clone = clicked.clone();

        // Create content with a clickable button at y=150 (below initial viewport)
        let button = Empty::new()
            .style(|s| s.size(100.0, 50.0).margin_top(150.0))
            .on_click_stop(move |_| {
                clicked_clone.set(true);
            });
        let button_id = button.view_id();

        let content = stack((button,)).style(|s| s.size(200.0, 500.0).flex_col());
        let scroll_view = Scroll::new(content).style(|s| s.size(200.0, 100.0));

        let view = stack((scroll_view,)).style(|s| s.size(200.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 200.0, 100.0);

        // Button is at y=150 in content coordinates, but viewport is y=0 to y=100
        // So button is not visible yet. Let's scroll down to see it.

        // Scroll down by 100px so the button (at y=150) is now at visual y=50
        harness.scroll_down(50.0, 50.0, 100.0);

        // Debug: print button's layout_rect after scroll
        let button_rect = button_id.layout_rect();
        let button_state = button_id.state();
        let button_viewport = button_state.borrow().viewport;
        let button_layout = button_id.get_layout();
        eprintln!("Button layout_rect after scroll: {:?}", button_rect);
        eprintln!(
            "Button viewport: {:?}, layout.location: {:?}",
            button_viewport,
            button_layout.map(|l| (l.location.x, l.location.y))
        );

        // Check should_send for the button
        eprintln!("Hit test at (50, 75): {:?}", harness.view_at(50.0, 75.0));

        // The button should now be visible at approximately y=50 (150 - 100 scroll)
        // Click in the middle of where the button should be visually
        harness.click(50.0, 75.0);

        assert!(
            clicked.get(),
            "Button should receive click after scrolling (clicked at visual position)"
        );
    }

    #[test]
    fn test_clip_aware_hit_testing_clipped_content() {
        // Test that clicks on content outside the scroll view's visible area
        // don't trigger click handlers (clip-aware hit testing)
        use crate::views::scroll::Scroll;

        let clicked = Rc::new(Cell::new(false));
        let clicked_clone = clicked.clone();

        // Create a button at y=150 (below the scroll view's 100px height)
        let button = Empty::new()
            .style(|s| s.size(100.0, 50.0).margin_top(150.0))
            .on_click_stop(move |_| {
                clicked_clone.set(true);
            });

        let content = stack((button,)).style(|s| s.size(200.0, 500.0).flex_col());
        let scroll_view = Scroll::new(content).style(|s| s.size(200.0, 100.0));
        let view = stack((scroll_view,)).style(|s| s.size(200.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 200.0, 100.0);

        // The button is at y=150 in content coordinates.
        // Without scrolling, the viewport shows y=0 to y=100.
        // The button is NOT visible (it's below the viewport).

        // Try to click at the button's content position (y=175, middle of button).
        // This click should NOT reach the button because it's clipped!
        // The click at window y=175 is outside the scroll view's bounds (0-100).
        // But even if we tried to click at y=75 (inside scroll view),
        // the button is not there visually.

        // Click at y=175 (where the button would be without clipping)
        // This is outside the scroll view entirely, so won't hit anything.
        harness.click(50.0, 175.0);
        assert!(
            !clicked.get(),
            "Click outside scroll view bounds should not hit clipped content"
        );
    }

    #[test]
    fn test_clip_aware_hit_testing_visible_content() {
        // Test that clicks on content inside the scroll view's visible area
        // do trigger click handlers
        use crate::views::scroll::Scroll;

        let clicked = Rc::new(Cell::new(false));
        let clicked_clone = clicked.clone();

        // Create a button at y=25 (inside the scroll view's 100px height)
        let button = Empty::new()
            .style(|s| s.size(100.0, 50.0).margin_top(25.0))
            .on_click_stop(move |_| {
                clicked_clone.set(true);
            });

        let content = stack((button,)).style(|s| s.size(200.0, 500.0).flex_col());
        let scroll_view = Scroll::new(content).style(|s| s.size(200.0, 100.0));
        let view = stack((scroll_view,)).style(|s| s.size(200.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 200.0, 100.0);

        // The button is at y=25 to y=75 in content coordinates.
        // The viewport shows y=0 to y=100, so the button IS visible.

        // Click in the middle of the button (y=50)
        harness.click(50.0, 50.0);
        assert!(
            clicked.get(),
            "Click inside scroll view on visible content should trigger handler"
        );
    }

    #[test]
    fn test_clip_aware_hit_testing_after_scroll_clipped() {
        // Test that after scrolling, content that moves out of view
        // no longer receives clicks
        use crate::views::scroll::Scroll;

        let button1_clicked = Rc::new(Cell::new(false));
        let button1_clicked_clone = button1_clicked.clone();

        let button2_clicked = Rc::new(Cell::new(false));
        let button2_clicked_clone = button2_clicked.clone();

        // Button1 at top (y=10), Button2 further down (y=150)
        let button1 = Empty::new()
            .style(|s| s.size(100.0, 30.0).margin_top(10.0))
            .on_click_stop(move |_| {
                button1_clicked_clone.set(true);
            });

        let button2 = Empty::new()
            .style(|s| s.size(100.0, 30.0).margin_top(110.0)) // 10 + 30 + 110 = 150
            .on_click_stop(move |_| {
                button2_clicked_clone.set(true);
            });

        let content = stack((button1, button2)).style(|s| s.size(200.0, 500.0).flex_col());
        let scroll_view = Scroll::new(content).style(|s| s.size(200.0, 100.0));
        let view = stack((scroll_view,)).style(|s| s.size(200.0, 100.0));

        let mut harness = TestHarness::new_with_size(view, 200.0, 100.0);

        // Initially, button1 is visible (y=10 to y=40), button2 is not (y=150 to y=180)

        // Click on button1 - should work
        harness.click(50.0, 25.0);
        assert!(button1_clicked.get(), "Button1 should be clickable initially");
        button1_clicked.set(false);

        // Scroll down by 100px
        // Now button1 is at visual y=-90 to y=-60 (clipped, above viewport)
        // And button2 is at visual y=50 to y=80 (visible)
        harness.scroll_down(50.0, 50.0, 100.0);

        // Click at y=25 where button1 USED to be - should NOT hit button1 anymore
        harness.click(50.0, 25.0);
        assert!(
            !button1_clicked.get(),
            "Button1 should NOT be clickable after scrolling out of view"
        );

        // Click at y=65 where button2 now is - should work
        harness.click(50.0, 65.0);
        assert!(
            button2_clicked.get(),
            "Button2 should be clickable after scrolling into view"
        );
    }
}
