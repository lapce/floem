use std::{cell::RefCell, collections::HashMap};

use glazier::{
    kurbo::{Point, Size},
    WindowBuilder,
};

use crate::{app_handle::AppHandle, id::WindowId, view::View};

thread_local! {
    pub(crate) static WINDOWS:RefCell<HashMap<WindowId, glazier::WindowHandle>> = Default::default();
}

#[derive(Default)]
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
    window_id: WindowId,
    app_view: impl FnOnce() -> V + 'static,
    config: Option<WindowConfig>,
) {
    let application = glazier::Application::global();
    let app = AppHandle::new(window_id, app_view);
    let mut builder = WindowBuilder::new(application).size(
        config
            .as_ref()
            .and_then(|c| c.size)
            .unwrap_or_else(|| Size::new(800.0, 600.0)),
    );
    if let Some(position) = config.as_ref().and_then(|c| c.position) {
        builder = builder.position(position);
    }
    if let Some(show_titlebar) = config.as_ref().and_then(|c| c.show_titlebar) {
        builder = builder.show_titlebar(show_titlebar);
    }

    builder = builder.handler(Box::new(app));
    let window = builder.build().unwrap();
    window.show();
}

/// request the window to be closed
pub fn close_window(window_id: WindowId) {
    WINDOWS.with(|windows| {
        if let Some(window) = windows.borrow().get(&window_id) {
            window.close();
        }
    })
}
