#![deny(missing_docs)]

//! Action functions that can be called anywhere in a Floem application
//!
//! This module includes a variety of functions that can interact with the window from which the function is being called.
//!
//! This includes, moving the window, resizing the window, adding context menus and overlays, and running a callback after a specified duration.

use std::sync::atomic::AtomicU64;

use floem_reactive::{SignalWith, UpdaterEffect};
use peniko::kurbo::{Point, Size, Vec2};
use winit::window::{ResizeDirection, Theme, WindowId};

use crate::IntoView;
use crate::platform::{Duration, Instant};

use crate::{
    app::{AppUpdateEvent, add_app_update_event},
    frame::{FrameCallbackRepeat, FrameRatePreference, FrameTime},
    message::{UPDATE_MESSAGES, UpdateMessage},
    platform::menu::Menu,
    view::View,
    view::ViewId,
    views::Decorators,
    window::handle::{get_current_view, set_current_view},
    window::tracking::with_window,
};

#[cfg(not(target_arch = "wasm32"))]
pub use crate::platform::file_action::*;

/// Add an update message
pub(crate) fn add_update_message(msg: UpdateMessage) {
    let current_view = get_current_view();
    let _ = UPDATE_MESSAGES.try_with(|msgs| {
        let mut msgs = msgs.borrow_mut();
        msgs.entry(current_view).or_default().push(msg);
    });
}

/// Toggle whether the window is maximized or not.
pub fn toggle_window_maximized() {
    add_update_message(UpdateMessage::ToggleWindowMaximized);
}

/// Set the maximized state of the window.
pub fn set_window_maximized(maximized: bool) {
    add_update_message(UpdateMessage::SetWindowMaximized(maximized));
}

/// Minimize the window.
pub fn minimize_window() {
    add_update_message(UpdateMessage::MinimizeWindow);
}

/// If and while the mouse is pressed, allow the window to be dragged.
pub fn drag_window() {
    add_update_message(UpdateMessage::DragWindow);
}

/// If and while the mouse is pressed, allow the window to be resized.
pub fn drag_resize_window(direction: ResizeDirection) {
    add_update_message(UpdateMessage::DragResizeWindow(direction));
}

/// Move the window by a specified delta.
pub fn set_window_delta(delta: Vec2) {
    add_update_message(UpdateMessage::SetWindowDelta(delta));
}

/// Set the window scale.
///
/// This will scale all view elements in the renderer.
pub fn set_window_scale(window_scale: f64) {
    add_update_message(UpdateMessage::WindowScale(window_scale));
}

/// Send a message to the application to open the Inspector for this Window.
pub fn inspect() {
    add_update_message(UpdateMessage::Inspect);
}

/// Capture the next Metal frame for this window.
///
/// This is only supported on macOS. On other platforms this action is a no-op.
pub fn capture_metal() {
    add_update_message(UpdateMessage::CaptureMetalFrame);
}

/// Toggle the compact performance HUD for the current window.
pub fn toggle_hud() {
    add_update_message(UpdateMessage::ToggleHud);
}

/// Set the **global** app theme in all windows.
///
/// Toggles both floem and window themes.
pub fn set_global_theme(theme: Theme) {
    add_app_update_event(AppUpdateEvent::ThemeChanged { theme });
}

/// Set the **window** theme.
///
/// Specify `None` to reset the theme to the system default.
pub fn set_theme(theme: Option<Theme>) {
    add_update_message(UpdateMessage::SetTheme(theme));
}

/// Toggle **global** app theme.
pub fn toggle_global_theme() {
    let theme = current_theme().unwrap_or(Theme::Dark);
    let theme = match theme {
        Theme::Light => Theme::Dark,
        Theme::Dark => Theme::Light,
    };
    add_app_update_event(AppUpdateEvent::ThemeChanged { theme });
}

/// Toggle **window** theme.
pub fn toggle_window_theme() {
    let theme = current_theme().unwrap_or(Theme::Dark);
    let theme = match theme {
        Theme::Light => Theme::Dark,
        Theme::Dark => Theme::Light,
    };
    // add_app_update_event(AppUpdateEvent::ThemeChanged { theme });
    add_update_message(UpdateMessage::SetTheme(Some(theme)));
}

/// Get current window theme.
pub fn current_theme() -> Option<Theme> {
    let win_id = get_current_view().window_id()?;
    with_window(&win_id, |w| w.theme())?
}

pub(crate) struct Timer {
    pub(crate) token: TimerToken,
    pub(crate) kind: TimerKind,
    pub(crate) action: TimerAction,
    pub(crate) deadline: Instant,
    pub(crate) sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimerKind {
    Normal,
    CompositorCommitDeadline {
        window_id: WindowId,
        target_deadline: Instant,
        can_fire_early: bool,
    },
}

pub(crate) enum TimerAction {
    Main(Box<dyn FnOnce(TimerToken)>),
    Ui {
        window_id: WindowId,
        root: ViewId,
        action: Box<dyn FnOnce(TimerToken)>,
    },
}

/// A token associated with a timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub struct TimerToken(u64);

impl TimerToken {
    /// A token that does not correspond to any timer.
    pub const INVALID: TimerToken = TimerToken(0);

    /// Create a new token.
    pub fn next() -> TimerToken {
        static TIMER_COUNTER: AtomicU64 = AtomicU64::new(1);
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

    /// Cancel a timer.
    pub fn cancel(self) {
        add_app_update_event(AppUpdateEvent::CancelTimer { timer: self });
    }
}

/// A token for a registered animation-frame callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub struct AnimationFrameCallbackToken {
    id: u64,
    root: ViewId,
}

impl AnimationFrameCallbackToken {
    fn next(root: ViewId) -> Self {
        static CALLBACK_COUNTER: AtomicU64 = AtomicU64::new(1);
        Self {
            id: CALLBACK_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            root,
        }
    }

    /// Cancel this animation-frame callback.
    pub fn cancel(self) {
        let _ = UPDATE_MESSAGES.try_with(|msgs| {
            msgs.borrow_mut()
                .entry(self.root)
                .or_default()
                .push(UpdateMessage::CancelAnimationFrameCallback { token: self.id });
        });
        if let Some(window_id) = self.root.window_id() {
            crate::window::tracking::force_window_repaint(&window_id);
        }
    }
}

/// Execute a callback after a specified duration.
///
/// This must be called on Floem's main UI thread.
pub fn exec_after(duration: Duration, action: impl FnOnce(TimerToken) + 'static) -> TimerToken {
    let view = get_current_view();
    let root = view.root();
    let window_id = view
        .window_id()
        .expect("timers must be scheduled from a view attached to a window");

    let token = TimerToken::next();
    add_app_update_event(AppUpdateEvent::RequestTimer {
        timer: Timer {
            token,
            kind: TimerKind::Normal,
            action: TimerAction::Ui {
                window_id,
                root,
                action: Box::new(action),
            },
            deadline: Instant::now() + duration,
            sequence: token.into_raw(),
        },
    });
    token
}

/// Register an animation-frame callback for the current window.
///
/// `frame_rate` is a presentation-cadence preference, not a wall-clock timer
/// and not a guarantee that every callback will produce a presented frame.
/// Floem aligns callbacks to the window's frame source and may delay, coalesce,
/// or skip opportunities when the window, compositor, or dependent layers
/// cannot present at the requested cadence.
pub fn schedule_animation_frame_callback(
    frame_rate: FrameRatePreference,
    repeat: FrameCallbackRepeat,
    mut action: impl FnMut(FrameTime) + 'static,
) -> AnimationFrameCallbackToken {
    let view = get_current_view();
    let root = view.root();
    let token = AnimationFrameCallbackToken::next(root);

    let callback = move |frame_time| {
        set_current_view(root);
        action(frame_time);
    };

    add_update_message(UpdateMessage::SetAnimationFrameCallback {
        token: token.id,
        frame_rate,
        repeat,
        callback: Box::new(callback),
    });
    if let Some(window_id) = view.window_id() {
        crate::window::tracking::force_window_repaint(&window_id);
    }
    token
}

/// Register a repeating animation-frame callback for the current window.
///
/// The callback runs at each eligible begin-frame opportunity until the
/// returned [`AnimationFrameCallbackToken`] is cancelled.
pub fn set_animation_frame_callback(
    frame_rate: FrameRatePreference,
    action: impl FnMut(FrameTime) + 'static,
) -> AnimationFrameCallbackToken {
    schedule_animation_frame_callback(frame_rate, FrameCallbackRepeat::every_frame(), action)
}

/// Execute a callback once at the next eligible begin-frame opportunity.
///
/// The returned token may be used to cancel the callback before it runs.
pub fn request_animation_frame(
    action: impl FnOnce(FrameTime) + 'static,
) -> AnimationFrameCallbackToken {
    request_animation_frame_with_preference(FrameRatePreference::full(), action)
}

/// Execute a callback once at the next eligible begin-frame opportunity.
pub fn request_animation_frame_with_preference(
    frame_rate: FrameRatePreference,
    action: impl FnOnce(FrameTime) + 'static,
) -> AnimationFrameCallbackToken {
    let mut action = Some(action);

    schedule_animation_frame_callback(frame_rate, FrameCallbackRepeat::none(), move |frame_time| {
        if let Some(action) = action.take() {
            action(frame_time);
        }
    })
}

/// Debounce an action.
///
/// This tracks a signal and checks if the inner value has changed by checking it's hash and will
/// run the action only once an **uninterrupted** duration has passed.
pub fn debounce_action<T, F>(signal: impl SignalWith<T> + 'static, duration: Duration, action: F)
where
    T: std::hash::Hash + 'static,
    F: Fn() + Clone + 'static,
{
    UpdaterEffect::new_stateful(
        move |prev_opt: Option<(u64, Option<TimerToken>)>| {
            use std::hash::Hasher;
            let mut hasher = std::hash::DefaultHasher::new();
            signal.with(|v| v.hash(&mut hasher));
            let hash = hasher.finish();
            let execute = prev_opt
                .map(|(prev_hash, _)| prev_hash != hash)
                .unwrap_or(true);
            (execute, (hash, prev_opt.and_then(|(_, timer)| timer)))
        },
        move |execute, (hash, prev_timer): (u64, Option<TimerToken>)| {
            // Cancel the previous timer if it exists
            if let Some(timer) = prev_timer {
                timer.cancel();
            }
            let timer_token = if execute {
                let action = action.clone();
                Some(exec_after(duration, move |_| {
                    action();
                }))
            } else {
                None
            };
            (hash, timer_token)
        },
    );
}

/// Show a system context menu at the specified position.
///
/// Platform support:
/// - Windows: Yes
/// - macOS: Yes
/// - Linux: Uses a custom Floem View
pub fn show_context_menu(menu: Menu, pos: Option<Point>) {
    add_update_message(UpdateMessage::ShowContextMenu { menu, pos });
}

/// Set the system window menu.
///
/// Platform support:
/// - Windows: Yes
/// - macOS: Yes
/// - Linux: No
/// - wasm32: No
#[cfg(not(target_arch = "wasm32"))]
pub fn set_window_menu(menu: Menu) {
    add_update_message(UpdateMessage::WindowMenu { menu });
}

/// Set the title of the window.
pub fn set_window_title(title: String) {
    add_update_message(UpdateMessage::SetWindowTitle { title });
}

/// Clear the focus from this window
pub fn clear_focus() {
    add_update_message(UpdateMessage::ClearFocus);
}

/// Focus the window.
pub fn focus_window() {
    add_update_message(UpdateMessage::FocusWindow);
}

/// Set whether ime input is shown.
pub fn set_ime_allowed(allowed: bool) {
    add_update_message(UpdateMessage::SetImeAllowed { allowed });
}

/// Set the ime cursor area.
pub fn set_ime_cursor_area(position: Point, size: Size) {
    add_update_message(UpdateMessage::SetImeCursorArea { position, size });
}

/// Creates a new overlay on the current window.
pub fn add_overlay<V: View + 'static>(view: V) -> ViewId {
    let view = view.style(move |s| s.absolute()).into_any();
    let id = view.id();

    add_update_message(UpdateMessage::AddOverlay { view });
    id
}

/// Removes an overlay from the current window.
pub fn remove_overlay(id: ViewId) {
    add_update_message(UpdateMessage::RemoveOverlay { id });
}
