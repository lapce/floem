use kurbo::{Point, Size};
pub use winit::window::ResizeDirection;
pub use winit::window::Theme;
pub use winit::window::WindowId;

use crate::{
    app::{add_app_update_event, AppUpdateEvent},
    view::View,
};

#[derive(Default, Debug)]
pub struct WindowConfig {
    pub(crate) size: Option<Size>,
    pub(crate) position: Option<Point>,
    pub(crate) show_titlebar: Option<bool>,
}

impl WindowConfig {
    pub fn size(mut self, size: impl Into<Size>) -> Self {
        self.size = Some(size.into());
        self
    }

    pub fn position(mut self, position: Point) -> Self {
        self.position = Some(position);
        self
    }

    pub fn show_titlebar(mut self, show_titlebar: bool) -> Self {
        self.show_titlebar = Some(show_titlebar);
        self
    }
}

/// create a new window. You'll need to create Application first, otherwise it
/// will panic
pub fn new_window<V: View + 'static>(
    app_view: impl FnOnce(WindowId) -> V + 'static,
    config: Option<WindowConfig>,
) {
    add_app_update_event(AppUpdateEvent::NewWindow {
        view_fn: Box::new(|window_id| Box::new(app_view(window_id))),
        config,
    });
}

/// request the window to be closed
pub fn close_window(window_id: WindowId) {
    add_app_update_event(AppUpdateEvent::CloseWindow { window_id });
}
