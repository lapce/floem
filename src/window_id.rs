use crate::{
    screen_layout::screen_layout_for_window,
    window_tracking::{force_window_repaint, with_window},
    ScreenLayout, ViewId,
};
use std::{cell::RefCell, collections::HashMap};

use super::window_tracking::{
    monitor_bounds, root_view_id, window_inner_screen_bounds, window_inner_screen_position,
    window_outer_screen_bounds, window_outer_screen_position,
};
use floem_winit::{
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, Pixel},
    window::{UserAttentionType, Window, WindowId},
};
use peniko::kurbo::{Point, Rect, Size};

// Using thread_local for consistency with static vars in updates.rs, but I suspect these
// are thread_local not because thread-locality is desired, but only because static mutability is
// desired - but that's a patch for another day.
thread_local! {
    /// Holding pen for window state changes, processed as part of the event loop cycle
    pub(crate) static WINDOW_UPDATE_MESSAGES: RefCell<HashMap<WindowId, Vec<WindowUpdate>>> = Default::default();
}

/// Enum of state updates that can be requested on a window which are processed
/// asynchronously after event processing.
#[allow(dead_code)] // DocumentEdited is seen as unused on non-mac builds
enum WindowUpdate {
    Visibility(bool),
    InnerBounds(Rect),
    OuterBounds(Rect),
    // Since both inner bounds and outer bounds require some fudgery because winit
    // only supports setting outer location and *inner* bounds, it is a good idea
    // also to support setting the two things winit supports directly:
    OuterLocation(Point),
    InnerSize(Size),
    RequestAttention(Option<UserAttentionType>),
    Minimize(bool),
    Maximize(bool),
    // Mac OS only
    #[allow(unused_variables)] // seen as unused on linux, etc.
    DocumentEdited(bool),
}

/// Delegate enum for `winit`'s
/// [`UserAttentionType`](https://docs.rs/winit/latest/winit/window/enum.UserAttentionType.html),
/// used for making the window's icon bounce in the Mac OS dock or the equivalent of that on
/// other platforms.
#[derive(Default, Copy, Clone, Debug, Eq, PartialEq)]
pub enum Urgency {
    Critical,
    Informational,

    /// The default attention type (equivalent of passing `None` to `winit::Window::request_user_attention())`).
    /// On some platforms (X11), it is necessary to call `WindowId.request_attention(Urgency::Default)` to stop
    /// the attention-seeking behavior of the window.
    #[default]
    Default,
}

impl From<Urgency> for Option<UserAttentionType> {
    fn from(urgency: Urgency) -> Self {
        match urgency {
            Urgency::Critical => Some(UserAttentionType::Critical),
            Urgency::Informational => Some(UserAttentionType::Informational),
            Urgency::Default => None,
        }
    }
}

/// Ensures `WindowIdExt` cannot be implemented on arbitrary types.
trait WindowIdExtSealed: Sized + Copy {
    fn add_window_update(&self, msg: WindowUpdate);
}

impl WindowIdExtSealed for WindowId {
    fn add_window_update(&self, msg: WindowUpdate) {
        WINDOW_UPDATE_MESSAGES.with_borrow_mut(|map| match map.entry(*self) {
            std::collections::hash_map::Entry::Occupied(updates) => {
                updates.into_mut().push(msg);
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(vec![msg]);
            }
        });
    }
}

/// Extends `WindowId` to give instances methods to retrieve properties of the associated window,
/// much as `ViewId` does.  Methods may return None if the view is not realized on-screen, or
/// if information needed to compute the result is not available on the current platform or
/// available on the current platform but not from the calling thread.
///
/// **Platform support notes:**
///  * Mac OS: Many of the methods here, if called from a thread other than `main`, are
///    blocking because accessing most window properties may only be done from the main
///    thread on that OS.
///  * Android & Wayland: Getting the outer position of a window is not supported by `winit` and
///    methods whose return value have that as a prerequisite will return `None` or return a
///    reasonable default.
///  * X11: Some window managers (Openbox was one such which was tested) *appear* to support
///    retreiving separate window-with-frame and window-content positions and sizes, but in
///    fact report the same values for both.
#[allow(private_bounds)]
pub trait WindowIdExt: WindowIdExtSealed {
    /// Get the bounds of the content of this window, including
    /// titlebar and native window borders.
    fn bounds_on_screen_including_frame(&self) -> Option<Rect>;
    /// Get the bounds of the content of this window, excluding
    /// titlebar and native window borders.
    fn bounds_of_content_on_screen(&self) -> Option<Rect>;
    /// Get the location of the window including any OS titlebar.
    fn position_on_screen_including_frame(&self) -> Option<Point>;
    /// Get the location of the window's content on the monitor where
    /// it currently resides, **excluding** any OS titlebar.
    fn position_of_content_on_screen(&self) -> Option<Point>;
    /// Get the logical bounds of the monitor this window is on.
    fn monitor_bounds(&self) -> Option<Rect>;
    /// Determine if this window is currently visible.  Note that if a
    /// call to set a window visible which is invisible has happened within
    /// the current event loop cycle, the state returned will not reflect that.
    fn is_visible(&self) -> bool;
    /// Determine if this window is currently minimized. Note that if a
    /// call to minimize or unminimize this window, and it is currently in the
    /// opposite state, has happened the current event loop cycle, the state
    /// returned will not reflect that.
    fn is_minimized(&self) -> bool;

    /// Determine if this window is currently maximize. Note that if a
    /// call to maximize or unmaximize this window, and it is currently in the
    /// opposite state, has happened the current event loop cycle, the state
    /// returned will not reflect that.
    fn is_maximized(&self) -> bool;

    /// Determine if the window decorations should indicate an edited, unsaved
    /// document.  Platform-dependent: Will only ever return `true` on Mac OS.
    fn is_document_edited(&self) -> bool;

    /// Instruct the window manager to indicate in the window's decorations
    /// the the window contains an unsaved, edited document.  Only has an
    /// effect on Mac OS.
    #[allow(unused_variables)] // edited unused on non-mac builds
    fn set_document_edited(&self, edited: bool) {
        #[cfg(target_os = "macos")]
        self.add_window_update(WindowUpdate::DocumentEdited(edited))
    }

    /// Set this window's visible state, hiding or showing it if it has been
    /// hidden
    fn set_visible(&self, visible: bool) {
        self.add_window_update(WindowUpdate::Visibility(visible))
    }

    /// Update the bounds of this window.
    fn set_window_inner_bounds(&self, bounds: Rect) {
        self.add_window_update(WindowUpdate::InnerBounds(bounds))
    }

    /// Update the bounds of this window.
    fn set_window_outer_bounds(&self, bounds: Rect) {
        self.add_window_update(WindowUpdate::OuterBounds(bounds))
    }

    /// Change this window's maximized state.
    fn maximized(&self, maximized: bool) {
        self.add_window_update(WindowUpdate::Maximize(maximized))
    }

    /// Change this window's minimized state.
    fn minimized(&self, minimized: bool) {
        self.add_window_update(WindowUpdate::Minimize(minimized))
    }

    /// Change this window's minimized state.
    fn set_outer_location(&self, location: Point) {
        self.add_window_update(WindowUpdate::OuterLocation(location))
    }

    /// Ask the OS's windowing framework to update the size of the window
    /// based on the passed size for its *content* (excluding titlebar, frame
    /// or other decorations).
    fn set_content_size(&self, size: Size) {
        self.add_window_update(WindowUpdate::InnerSize(size))
    }

    /// Cause the desktop to perform some attention-drawing behavior that draws
    /// the user's attention specifically to this window - e.g. bouncing in
    /// the dock on Mac OS.  On X11, after calling this method with some urgency
    /// other than `None`, it is necessary to *clear* the attention-seeking state
    /// by calling this method again with `Urgency::None`.
    fn request_attention(&self, urgency: Urgency) {
        self.add_window_update(WindowUpdate::RequestAttention(urgency.into()))
    }

    /// Force a repaint of this window through the native window's repaint mechanism,
    /// bypassing floem's normal repaint mechanism.
    ///
    /// This method may be removed or deprecated in the future, but has been needed
    /// in [some situations](https://github.com/lapce/floem/issues/463), and to
    /// address a few ongoing issues in `winit` (window unmaximize is delayed until
    /// an external event triggers a repaint of the requesting window), and may
    /// be needed as a workaround if other such issues are discovered until they
    /// can be addressed.
    ///
    /// Returns true if the repaint request was issued successfully (i.e. there is
    /// an actual system-level window corresponding to this `WindowId`).
    fn force_repaint(&self) -> bool;

    /// Get the root view of this window.
    fn root_view(&self) -> Option<ViewId>;

    /// Get a layout of this window in relation to the monitor on which it currently
    /// resides, if any.
    fn screen_layout(&self) -> Option<ScreenLayout>;

    /// Get the dots-per-inch scaling of this window or 1.0 if the platform does not
    /// support it (Android).
    fn scale(&self) -> f64;
}

impl WindowIdExt for WindowId {
    fn bounds_on_screen_including_frame(&self) -> Option<Rect> {
        window_outer_screen_bounds(self)
    }

    fn bounds_of_content_on_screen(&self) -> Option<Rect> {
        window_inner_screen_bounds(self)
    }

    fn position_on_screen_including_frame(&self) -> Option<Point> {
        window_outer_screen_position(self)
    }

    fn position_of_content_on_screen(&self) -> Option<Point> {
        window_inner_screen_position(self)
    }

    fn monitor_bounds(&self) -> Option<Rect> {
        monitor_bounds(self)
    }

    fn is_visible(&self) -> bool {
        with_window(self, |window| window.is_visible().unwrap_or(false)).unwrap_or(false)
    }

    fn is_minimized(&self) -> bool {
        with_window(self, |window| window.is_minimized().unwrap_or(false)).unwrap_or(false)
    }

    fn is_maximized(&self) -> bool {
        with_window(self, Window::is_maximized).unwrap_or(false)
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    fn is_document_edited(&self) -> bool {
        with_window(
            self,
            floem_winit::platform::macos::WindowExtMacOS::is_document_edited,
        )
        .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    #[allow(dead_code)]
    fn is_document_edited(&self) -> bool {
        false
    }

    fn force_repaint(&self) -> bool {
        force_window_repaint(self)
    }

    fn root_view(&self) -> Option<ViewId> {
        root_view_id(self)
    }

    fn screen_layout(&self) -> Option<ScreenLayout> {
        with_window(self, move |window| screen_layout_for_window(*self, window)).unwrap_or(None)
    }

    fn scale(&self) -> f64 {
        with_window(self, Window::scale_factor).unwrap_or(1.0)
    }
}

/// Called by `ApplicationHandle` at the end of the event loop callback.
pub(crate) fn process_window_updates(id: &WindowId) -> bool {
    let mut result = false;
    if let Some(items) = WINDOW_UPDATE_MESSAGES.with_borrow_mut(|map| map.remove(id)) {
        result = !items.is_empty();
        for update in items {
            match update {
                WindowUpdate::Visibility(visible) => {
                    with_window(id, |window| {
                        window.set_visible(visible);
                    });
                }
                #[allow(unused_variables)] // non mac - edited is unused
                WindowUpdate::DocumentEdited(edited) => {
                    #[cfg(target_os = "macos")]
                    with_window(id, |window| {
                        use floem_winit::platform::macos::WindowExtMacOS;
                        window.set_document_edited(edited);
                    });
                }
                WindowUpdate::OuterBounds(bds) => {
                    with_window(id, |window| {
                        let params =
                            bounds_to_logical_outer_position_and_inner_size(window, bds, true);
                        window.set_outer_position(params.0);
                        // XXX log any returned error?
                        let _ = window.request_inner_size(params.1);
                    });
                }
                WindowUpdate::InnerBounds(bds) => {
                    with_window(id, |window| {
                        let params =
                            bounds_to_logical_outer_position_and_inner_size(window, bds, false);
                        window.set_outer_position(params.0);
                        // XXX log any returned error?
                        let _ = window.request_inner_size(params.1);
                    });
                }
                WindowUpdate::RequestAttention(att) => {
                    with_window(id, |window| {
                        window.request_user_attention(att);
                    });
                }
                WindowUpdate::Minimize(minimize) => {
                    with_window(id, |window| {
                        window.set_minimized(minimize);
                        if !minimize {
                            // If we don't trigger a repaint on Mac OS,
                            // unminimize doesn't happen until an input
                            // event arrives. Unrelated to
                            // https://github.com/lapce/floem/issues/463 -
                            // this is in winit or below.
                            maybe_yield_with_repaint(window);
                        }
                    });
                }
                WindowUpdate::Maximize(maximize) => {
                    with_window(id, |window| window.set_maximized(maximize));
                }
                WindowUpdate::OuterLocation(outer) => {
                    with_window(id, |window| {
                        window.set_outer_position(LogicalPosition::new(outer.x, outer.y));
                    });
                }
                WindowUpdate::InnerSize(size) => {
                    with_window(id, |window| {
                        window.request_inner_size(LogicalSize::new(size.width, size.height))
                    });
                }
            }
        }
    }
    result
}

/// Compute a new logical position and size, given a window, a rectangle and whether the
/// rectangle represents the desired inner or outer bounds of the window.
///
/// This is complex because winit offers us two somewhat contradictory ways of setting
/// the bounds:
///
///  * You can set the **outer** position with `window.set_outer_position(position)`
///  * You can set the **inner** size with `window.request_inner_size(size)`
///  * You can obtain inner and outer sizes and positions, but you can only set outer
///    position and *inner* size
///
/// So we must take the delta of the inner and outer size and/or positions (position
/// availability is more limited by platform), and from that, create an appropriate
/// inner size and outer position based on a `Rect` that represents either inner or
/// outer.
fn bounds_to_logical_outer_position_and_inner_size(
    window: &Window,
    target_bounds: Rect,
    target_is_outer: bool,
) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    if !window.is_decorated() {
        // For undecorated windows, the inner and outer location and size are always identical
        // so no further work is needed
        return (
            LogicalPosition::new(target_bounds.x0, target_bounds.y0),
            LogicalSize::new(target_bounds.width(), target_bounds.height()),
        );
    }

    let scale = window.scale_factor();
    if target_is_outer {
        // We need to reduce the size we are requesting by the width and height of the
        // OS-added decorations to get the right target INNER size:
        let inner_to_outer_size_delta = delta_size(window.inner_size(), window.outer_size(), scale);

        (
            LogicalPosition::new(target_bounds.x0, target_bounds.y0),
            LogicalSize::new(
                (target_bounds.width() + inner_to_outer_size_delta.0).max(0.),
                (target_bounds.height() + inner_to_outer_size_delta.1).max(0.),
            ),
        )
    } else {
        // We need to shift the x/y position we are requesting up and left (negatively)
        // to come up with an *outer* location that makes sense with the passed rectangle's
        // size as an *inner* size
        let size_delta = delta_size(window.inner_size(), window.outer_size(), scale);
        let inner_to_outer_delta: (f64, f64) = if let Some(delta) =
            delta_position(window.inner_position(), window.outer_position(), scale)
        {
            // This is the more accurate way, but may be unavailable on some platforms
            delta
        } else {
            // We have to make a few assumptions here, one of which is that window
            // decorations are horizontally symmetric - the delta-x / 2 equals a position
            // on the perimeter of the window's frame.  A few ancient XWindows window
            // managers (Enlightenment) might violate that assumption, but it is a rarity.
            (
                size_delta.0 / 2.0,
                size_delta.1, // assume vertical is titlebar and give it full weight
            )
        };
        (
            LogicalPosition::new(
                target_bounds.x0 - inner_to_outer_delta.0,
                target_bounds.y0 - inner_to_outer_delta.1,
            ),
            LogicalSize::new(target_bounds.width(), target_bounds.height()),
        )
    }
}

/// Some operations - notably minimize and restoring visibility - don't take
/// effect on Mac OS until something triggers a repaint in the target window - the
/// issue is below the level of floem's event loops and seems to be in winit or
/// deeper.  Workaround is to force the window to repaint.
#[allow(unused_variables)] // non mac builds see `window` as unused
fn maybe_yield_with_repaint(window: &Window) {
    #[cfg(target_os = "macos")]
    {
        window.request_redraw();
        let main = Some("main") != std::thread::current().name();
        if !main {
            // attempt to get out of the way of the main thread
            std::thread::yield_now();
        }
    }
}

fn delta_size(inner: PhysicalSize<u32>, outer: PhysicalSize<u32>, window_scale: f64) -> (f64, f64) {
    let inner = winit_phys_size_to_size(inner, window_scale);
    let outer = winit_phys_size_to_size(outer, window_scale);
    (outer.width - inner.width, outer.height - inner.height)
}

type PositionResult =
    Result<floem_winit::dpi::PhysicalPosition<i32>, floem_winit::error::NotSupportedError>;

fn delta_position(
    inner: PositionResult,
    outer: PositionResult,
    window_scale: f64,
) -> Option<(f64, f64)> {
    if let Ok(inner) = inner {
        if let Ok(outer) = outer {
            let outer = winit_phys_position_to_point(outer, window_scale);
            let inner = winit_phys_position_to_point(inner, window_scale);

            return Some((inner.x - outer.x, inner.y - outer.y));
        }
    }
    None
}

// Conversion functions for winit's size and point types:

fn winit_position_to_point<I: Into<f64> + Pixel>(pos: LogicalPosition<I>) -> Point {
    Point::new(pos.x.into(), pos.y.into())
}

fn winit_size_to_size<I: Into<f64> + Pixel>(size: LogicalSize<I>) -> Size {
    Size::new(size.width.into(), size.height.into())
}

fn winit_phys_position_to_point<I: Into<f64> + Pixel>(
    pos: PhysicalPosition<I>,
    window_scale: f64,
) -> Point {
    winit_position_to_point::<I>(pos.to_logical(window_scale))
}

fn winit_phys_size_to_size<I: Into<f64> + Pixel>(size: PhysicalSize<I>, window_scale: f64) -> Size {
    winit_size_to_size::<I>(size.to_logical(window_scale))
}
