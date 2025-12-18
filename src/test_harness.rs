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

use crate::context::{ComputeLayoutCx, EventCx, LayoutCx, StyleCx};
use crate::event::Event;
use crate::id::ViewId;
use crate::view::{IntoView, View};
use crate::window_state::WindowState;

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
pub struct TestHarness {
    root_id: ViewId,
    window_state: WindowState,
    size: Size,
}

impl TestHarness {
    /// Create a new test harness with the given root view.
    ///
    /// The view will be set up with default size (800x600) and scale (1.0).
    pub fn new(view: impl IntoView) -> Self {
        let view = view.into_view();
        let root_id = view.id();

        // Set the view in storage
        root_id.set_view(view.into_any());

        let window_state = WindowState::new(root_id, None);

        let mut harness = Self {
            root_id,
            window_state,
            size: Size::new(800.0, 600.0),
        };

        // Run initial passes to set up the view tree
        harness.rebuild();

        harness
    }

    /// Set the window size and rebuild layout.
    pub fn set_size(&mut self, width: f64, height: f64) -> &mut Self {
        self.size = Size::new(width, height);
        self.window_state.root_size = self.size;
        self.rebuild();
        self
    }

    /// Set the window scale factor.
    pub fn set_scale(&mut self, scale: f64) -> &mut Self {
        self.window_state.scale = scale;
        self.rebuild();
        self
    }

    /// Run all passes: style → layout → compute_layout.
    ///
    /// This must be called after any changes to view styles or structure
    /// for events to be dispatched correctly.
    pub fn rebuild(&mut self) {
        self.window_state.root_size = self.size;

        // Style pass
        self.style();

        // Layout pass
        self.layout();

        // Compute layout pass (sets window_origin, layout_rect, viewport)
        self.compute_layout();
    }

    /// Run the style pass.
    fn style(&mut self) {
        let mut cx = StyleCx::new(&mut self.window_state, self.root_id);
        cx.style_view(self.root_id);
    }

    /// Run the layout pass.
    fn layout(&mut self) {
        let mut cx = LayoutCx::new(&mut self.window_state);
        let view = self.root_id.view();
        let mut view = view.borrow_mut();
        self.window_state.root = Some(cx.layout_view(view.as_mut()));
        self.window_state.compute_layout();
    }

    /// Run the compute layout pass.
    fn compute_layout(&mut self) {
        let viewport = (self.size / self.window_state.scale).to_rect();
        let mut cx = ComputeLayoutCx::new(&mut self.window_state, viewport);
        cx.compute_view_layout(self.root_id);
    }

    /// Get the root view ID.
    pub fn root_id(&self) -> ViewId {
        self.root_id
    }

    /// Get a reference to the window state.
    pub fn window_state(&self) -> &WindowState {
        &self.window_state
    }

    /// Dispatch an event to the view tree.
    pub fn dispatch_event(&mut self, event: Event) -> EventResult {
        let mut cx = EventCx {
            window_state: &mut self.window_state,
        };

        let (propagation, consumed) = cx.unconditional_view_event(self.root_id, event, false);

        EventResult {
            handled: propagation.is_processed(),
            consumed: consumed == crate::context::PointerEventConsumed::Yes,
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

    /// Find the view at the given position (hit test).
    pub fn view_at(&self, x: f64, y: f64) -> Option<ViewId> {
        hit_test(self.root_id, Point::new(x, y))
    }
}

/// Create a pointer down event at the given position.
fn create_pointer_down(x: f64, y: f64) -> Event {
    use ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
    };

    Event::Pointer(PointerEvent::Down(PointerButtonEvent {
        state: ui_events::pointer::PointerState {
            position: dpi::PhysicalPosition::new(x, y),
            count: 1,
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
    use ui_events::pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerType,
    };

    Event::Pointer(PointerEvent::Up(PointerButtonEvent {
        state: ui_events::pointer::PointerState {
            position: dpi::PhysicalPosition::new(x, y),
            count: 1,
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

/// Perform a hit test to find the view at the given point.
fn hit_test(view_id: ViewId, point: Point) -> Option<ViewId> {
    if view_id.is_hidden() {
        return None;
    }

    let state = view_id.state();
    let layout_rect = state.borrow().layout_rect;

    if !layout_rect.contains(point) {
        return None;
    }

    // Check children in reverse paint order (top-first)
    let children = crate::context::children_in_paint_order(view_id);
    for child in children.into_iter().rev() {
        if let Some(hit) = hit_test(child, point) {
            return Some(hit);
        }
    }

    Some(view_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::{stack, Decorators, Empty};
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

        let mut harness = TestHarness::new(view);
        harness.set_size(100.0, 100.0);

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

        let mut harness = TestHarness::new(view);
        harness.set_size(100.0, 100.0);

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
}
