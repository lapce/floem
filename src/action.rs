use std::{
    sync::atomic::AtomicU64,
    time::{Duration, Instant},
};

use floem_winit::window::ResizeDirection;
use peniko::kurbo::{Point, Size, Vec2};

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

pub fn toggle_window_maximized() {
    add_update_message(UpdateMessage::ToggleWindowMaximized);
}

pub fn set_window_maximized(maximized: bool) {
    add_update_message(UpdateMessage::SetWindowMaximized(maximized));
}

pub fn minimize_window() {
    add_update_message(UpdateMessage::MinimizeWindow);
}

pub fn drag_window() {
    add_update_message(UpdateMessage::DragWindow);
}

pub fn drag_resize_window(direction: ResizeDirection) {
    add_update_message(UpdateMessage::DragResizeWindow(direction));
}

pub fn set_window_delta(delta: Vec2) {
    add_update_message(UpdateMessage::SetWindowDelta(delta));
}

pub fn update_window_scale(window_scale: f64) {
    add_update_message(UpdateMessage::WindowScale(window_scale));
}

pub(crate) struct Timer {
    pub(crate) token: TimerToken,
    pub(crate) action: Box<dyn FnOnce(TimerToken)>,
    pub(crate) deadline: Instant,
}

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

pub fn show_context_menu(menu: Menu, pos: Option<Point>) {
    add_update_message(UpdateMessage::ShowContextMenu { menu, pos });
}

pub fn set_window_menu(menu: Menu) {
    add_update_message(UpdateMessage::WindowMenu { menu });
}

pub fn set_window_title(title: String) {
    add_update_message(UpdateMessage::SetWindowTitle { title });
}

pub fn focus_window() {
    add_update_message(UpdateMessage::FocusWindow);
}

pub fn set_ime_allowed(allowed: bool) {
    add_update_message(UpdateMessage::SetImeAllowed { allowed });
}

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
