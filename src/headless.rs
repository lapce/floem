//! Headless harness for UI testing and benchmarking.
//!
//! This module provides utilities for running Floem views without creating
//! an actual window. It allows simulating user interactions and verifying
//! view behavior in tests and benchmarks.
//!
//! # Example
//!
//! ```rust,ignore
//! use floem::headless::HeadlessHarness;
//! use floem::views::*;
//!
//! let harness = HeadlessHarness::new(
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
use crate::event::path::hit_test;
use crate::style::{Style, StyleSelector};
use crate::view::IntoView;
use crate::view::ViewId;
use crate::window::handle::WindowHandle;

/// Result of an event dispatch operation.
#[derive(Debug, Clone)]
pub struct EventResult {
    /// Whether the event was handled (propagation stopped).
    pub handled: bool,
    /// Whether a pointer event was consumed.
    pub consumed: bool,
}

/// A headless harness for UI testing and benchmarking.
///
/// HeadlessHarness manages a view tree and provides methods to simulate
/// user interactions without creating an actual window.
///
/// Internally, it uses a headless `WindowHandle` to ensure behavior
/// matches real window behavior, including the full `process_update()` cycle
/// for style recalculation and layout.
pub struct HeadlessHarness {
    window_handle: WindowHandle,
}

impl HeadlessHarness {
    /// Create a new headless harness with the given root view.
    ///
    /// The view will be set up with default size (800x600) and scale (1.0).
    pub fn new(view: impl IntoView) -> Self {
        Self::new_with_size(view, 800.0, 600.0)
    }

    /// Create a new headless harness with the given root view and window size.
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

    /// Simulate a touch down event at the given position.
    ///
    /// Touch pointers automatically get implicit capture per W3C Pointer Events spec.
    pub fn touch_down(&mut self, x: f64, y: f64) -> EventResult {
        self.dispatch_event(create_touch_down(x, y))
    }

    /// Simulate a touch up event at the given position.
    pub fn touch_up(&mut self, x: f64, y: f64) -> EventResult {
        self.dispatch_event(create_touch_up(x, y))
    }

    /// Simulate a touch move event to the given position.
    pub fn touch_move(&mut self, x: f64, y: f64) -> EventResult {
        self.dispatch_event(create_touch_move(x, y))
    }

    /// Simulate a tap (touch down + touch up) at the given position.
    pub fn tap(&mut self, x: f64, y: f64) -> EventResult {
        self.touch_down(x, y);
        self.touch_up(x, y)
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

/// Create a touch pointer down event at the given position.
fn create_touch_down(x: f64, y: f64) -> Event {
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
            pointer_type: PointerType::Touch,
        },
    }))
}

/// Create a touch pointer up event at the given position.
fn create_touch_up(x: f64, y: f64) -> Event {
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
            pointer_type: PointerType::Touch,
        },
    }))
}

/// Create a touch pointer move event to the given position.
fn create_touch_move(x: f64, y: f64) -> Event {
    use ui_events::pointer::{PointerEvent, PointerId, PointerInfo, PointerType, PointerUpdate};

    Event::Pointer(PointerEvent::Move(PointerUpdate {
        pointer: PointerInfo {
            pointer_id: Some(PointerId::PRIMARY),
            persistent_device_id: None,
            pointer_type: PointerType::Touch,
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

// Hit test functionality has been moved to crate::event::path module.
// Use crate::event::clear_hit_test_cache to clear the cache when needed.
