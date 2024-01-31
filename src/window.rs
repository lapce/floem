pub use floem_winit::window::Fullscreen;
pub use floem_winit::window::ResizeDirection;
pub use floem_winit::window::Theme;
pub use floem_winit::window::WindowButtons;
pub use floem_winit::window::WindowId;
pub use floem_winit::window::WindowLevel;
use kurbo::{Point, Size};

use crate::app::{add_app_update_event, AppUpdateEvent};
use crate::view::View;

#[derive(Default, Debug)]
pub struct WindowConfig {
    pub(crate) size: Option<Size>,
    pub(crate) position: Option<Point>,
    pub(crate) show_titlebar: Option<bool>,
    pub(crate) transparent: Option<bool>,
    pub(crate) fullscreen: Option<Fullscreen>,
    pub(crate) window_icon: Option<bool>,
    pub(crate) title: Option<String>,
    pub(crate) enabled_buttons: Option<WindowButtons>,
    pub(crate) resizable: Option<bool>,
    pub(crate) window_level: Option<WindowLevel>,
    pub(crate) apply_default_theme: Option<bool>,
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

    pub fn with_transparent(mut self, transparent: bool) -> Self {
        self.transparent = Some(transparent);
        self
    }

    pub fn fullscreen(mut self, fullscreen: Fullscreen) -> Self {
        self.fullscreen = Some(fullscreen);
        self
    }

    pub fn window_icon(mut self, window_icon: bool) -> Self {
        self.window_icon = Some(window_icon);
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn enabled_buttons(mut self, enabled_buttons: WindowButtons) -> Self {
        self.enabled_buttons = Some(enabled_buttons);
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = Some(resizable);
        self
    }

    pub fn window_level(mut self, window_level: WindowLevel) -> Self {
        self.window_level = Some(window_level);
        self
    }

    /// If set to true, the stylesheet for Floem's default theme will be
    /// injected into your window. You may want to disable this when using a
    /// completely custom theme.
    pub fn apply_default_theme(mut self, apply_default_theme: bool) -> Self {
        self.apply_default_theme = Some(apply_default_theme);
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
        view_fn: Box::new(|window_id| app_view(window_id).any()),
        config,
    });
}

/// request the window to be closed
pub fn close_window(window_id: WindowId) {
    add_app_update_event(AppUpdateEvent::CloseWindow { window_id });
}
