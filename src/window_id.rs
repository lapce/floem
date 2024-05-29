use super::window_tracking::{
    monitor_bounds, window_inner_screen_bounds, window_inner_screen_position,
    window_outer_screen_bounds, window_outer_screen_position,
};
use floem_winit::window::WindowId;
use peniko::kurbo::{Point, Rect};

/// Ensures `WindowIdExt` cannot be implemented on arbitrary types.
trait WindowIdExtSealed {}
impl WindowIdExtSealed for WindowId {}

/// Extends WindowId to give instances methods to retrieve properties of the associated window,
/// much as ViewId does.  Methods may return None if the view is not realized on-screen, or
/// if information needed to compute the result is not available on the current platform or
/// available on the current platform but not from the calling thread.
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
    /// Get the location of the window **excluding** any OS titlebar.
    fn position_of_content_on_screen(&self) -> Option<Point>;
    /// Get the logical bounds of the monitor this window is on
    fn monitor_bounds(&self) -> Option<Rect>;
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
}
