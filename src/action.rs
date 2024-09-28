#![deny(missing_docs)]

//! Action functions that can be called anywhere in a Floem application
//!
//! This module includes a variety of functions that can interact with the window from which the function is being called.
//!
//! This includes, moving the window, resizing the window, adding context menus and overlays, and running a callback after a specified duration.

use std::sync::atomic::AtomicU64;

use floem_winit::window::ResizeDirection;
use peniko::kurbo::{Point, Size, Vec2};

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use crate::{
    app::{add_app_update_event, AppUpdateEvent},
    id::ViewId,
    menu::Menu,
    update::{UpdateMessage, UPDATE_MESSAGES},
    view::View,
    window_handle::{get_current_view, set_current_view},
};

#[cfg(any(feature = "rfd-async-std", feature = "rfd-tokio"))]
pub use crate::file_action::*;

pub(crate) fn add_update_message(msg: UpdateMessage) {
    let current_view = get_current_view();
    UPDATE_MESSAGES.with_borrow_mut(|msgs| {
        msgs.entry(current_view).or_default().push(msg);
    });
}

/// Toggle whether the window is maximized or not
pub fn toggle_window_maximized() {
    add_update_message(UpdateMessage::ToggleWindowMaximized);
}

/// Set the maximized state of the window
pub fn set_window_maximized(maximized: bool) {
    add_update_message(UpdateMessage::SetWindowMaximized(maximized));
}

/// Minimize the window
pub fn minimize_window() {
    add_update_message(UpdateMessage::MinimizeWindow);
}

/// If and while the mouse is pressed, allow the window to be dragged
pub fn drag_window() {
    add_update_message(UpdateMessage::DragWindow);
}

/// If and while the mouse is pressed, allow the window to be resized
pub fn drag_resize_window(direction: ResizeDirection) {
    add_update_message(UpdateMessage::DragResizeWindow(direction));
}

/// Move the window by a specified delta
pub fn set_window_delta(delta: Vec2) {
    add_update_message(UpdateMessage::SetWindowDelta(delta));
}

/// Set the window scale
///
/// This will scale all view elements in the renderer
pub fn update_window_scale(window_scale: f64) {
    add_update_message(UpdateMessage::WindowScale(window_scale));
}

pub(crate) struct Timer {
    pub(crate) token: TimerToken,
    pub(crate) action: Box<dyn FnOnce(TimerToken)>,
    pub(crate) deadline: Instant,
}

/// A token associated with a timer
// TODO: what is this for?
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub struct TimerToken(u64);

impl TimerToken {
    /// A token that does not correspond to any timer.
    pub const INVALID: TimerToken = TimerToken(0);

    /// Create a new token.
    pub fn next() -> TimerToken {
        static TIMER_COUNTER: AtomicU64 = AtomicU64::new(0);
        TimerToken(TIMER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    /// Create a new token from a raw value.
    pub const fn from_raw(id: u64) -> TimerToken {
        TimerToken(id)
    }

    /// Get the raw value for a token.
    pub const fn into_raw(self) -> u64 {
        self.0
    }
}

/// Execute a callback after a specified duration
pub fn exec_after(duration: Duration, action: impl FnOnce(TimerToken) + 'static) -> TimerToken {
    let view = get_current_view();
    let action = move |token| {
        let current_view = get_current_view();
        set_current_view(view);
        action(token);
        set_current_view(current_view);
    };

    let token = TimerToken::next();
    let deadline = Instant::now() + duration;
    add_app_update_event(AppUpdateEvent::RequestTimer {
        timer: Timer {
            token,
            action: Box::new(action),
            deadline,
        },
    });
    token
}

/// Show a system context menu at the specified position
///
/// Platform support:
/// - Windows: Yes
/// - macOS: Yes
/// - Linux: Uses a custom Floem View
pub fn show_context_menu(menu: Menu, pos: Option<Point>) {
    add_update_message(UpdateMessage::ShowContextMenu { menu, pos });
}

/// Set the system window menu
///
/// Platform support:
/// - Windows: No
/// - macOS: Yes (not currently implemented)
/// - Linux: No
pub fn set_window_menu(menu: Menu) {
    add_update_message(UpdateMessage::WindowMenu { menu });
}

/// Set the title of the window
pub fn set_window_title(title: String) {
    add_update_message(UpdateMessage::SetWindowTitle { title });
}

/// Focus the window
pub fn focus_window() {
    add_update_message(UpdateMessage::FocusWindow);
}

/// Set whether ime input is shown
pub fn set_ime_allowed(allowed: bool) {
    add_update_message(UpdateMessage::SetImeAllowed { allowed });
}

/// Set the ime cursor area
pub fn set_ime_cursor_area(position: Point, size: Size) {
    add_update_message(UpdateMessage::SetImeCursorArea { position, size });
}

/// Creates a new overlay on the current window.
pub fn add_overlay<V: View + 'static>(
    position: Point,
    view: impl FnOnce(ViewId) -> V + 'static,
) -> ViewId {
    let id = ViewId::new();
    add_update_message(UpdateMessage::AddOverlay {
        id,
        position,
        view: Box::new(move || Box::new(view(id))),
    });
    id
}

/// Removes an overlay from the current window.
pub fn remove_overlay(id: ViewId) {
    add_update_message(UpdateMessage::RemoveOverlay { id });
}
