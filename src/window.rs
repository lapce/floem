pub use floem_winit::window::Fullscreen;
pub use floem_winit::window::Icon;
pub use floem_winit::window::ResizeDirection;
pub use floem_winit::window::Theme;
pub use floem_winit::window::WindowButtons;
pub use floem_winit::window::WindowId;
pub use floem_winit::window::WindowLevel;
use peniko::kurbo::{Point, Size};

use crate::app::{add_app_update_event, AppUpdateEvent};
use crate::view::IntoView;

#[derive(Default, Debug)]
pub struct WindowConfig {
    pub(crate) size: Option<Size>,
    pub(crate) position: Option<Point>,
    pub(crate) show_titlebar: Option<bool>,
    pub(crate) transparent: Option<bool>,
    pub(crate) fullscreen: Option<Fullscreen>,
    pub(crate) window_icon: Option<Icon>,
    pub(crate) title: Option<String>,
    pub(crate) enabled_buttons: Option<WindowButtons>,
    pub(crate) resizable: Option<bool>,
    pub(crate) undecorated: Option<bool>,
    pub(crate) window_level: Option<WindowLevel>,
    pub(crate) apply_default_theme: Option<bool>,
    #[allow(dead_code)]
    pub(crate) mac_os_config: Option<MacOSWindowConfig>,
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

    pub fn undecorated(mut self, undecorated: bool) -> Self {
        self.undecorated = Some(undecorated);
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

    pub fn window_icon(mut self, window_icon: Icon) -> Self {
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

    /// Set up Mac-OS specific configuration.  The passed closure will only be
    /// called on Mac OS.
    #[allow(unused_variables, unused_mut)] // build will complain on non-mac os's otherwise
    pub fn with_mac_os_config(
        mut self,
        mut f: impl FnMut(MacOSWindowConfig) -> MacOSWindowConfig,
    ) -> Self {
        #[cfg(target_os = "macos")]
        if let Some(existing_config) = self.mac_os_config {
            self.mac_os_config = Some(f(existing_config))
        } else {
            let new_config = f(MacOSWindowConfig::default());
            self.mac_os_config = Some(new_config);
        }
        self
    }
}

/// Mac-OS specific window configuration properties, accessible via `WindowConfig::with_mac_os_config( FnMut( MacOsWindowConfig ) )`.
/// See [the winit docs](https://docs.rs/winit/latest/winit/platform/macos/trait.WindowExtMacOS.html) for further
/// information.
#[derive(Default, Debug, Clone)]
pub struct MacOSWindowConfig {
    pub(crate) movable_by_window_background: Option<bool>,
    pub(crate) titlebar_transparent: Option<bool>,
    pub(crate) titlebar_hidden: Option<bool>,
    pub(crate) titlebar_buttons_hidden: Option<bool>,
    pub(crate) full_size_content_view: Option<bool>,
    pub(crate) movable: Option<bool>,
    pub(crate) traffic_lights_offset: Option<(f64, f64)>,
    pub(crate) accepts_first_mouse: Option<bool>,
    pub(crate) tabbing_identifier: Option<String>,
    pub(crate) option_as_alt: Option<MacOsOptionAsAlt>,
    pub(crate) has_shadow: Option<bool>,
    pub(crate) disallow_high_dpi: Option<bool>,
}

impl MacOSWindowConfig {
    /// Allow the window to be
    /// [moved by dragging its background](https://developer.apple.com/documentation/appkit/nswindow/1419072-movablebywindowbackground).
    pub fn movable_by_window_background(mut self, val: bool) -> Self {
        self.movable_by_window_background = Some(val);
        self
    }

    /// Make the titlebar's transparency (does nothing on some versions of Mac OS).
    pub fn transparent_title_bar(mut self, val: bool) -> Self {
        self.titlebar_transparent = Some(val);
        self
    }

    /// Hides the title bar.
    pub fn hide_titlebar(mut self, val: bool) -> Self {
        self.titlebar_hidden = Some(val);
        self
    }

    /// Hides the title bar buttons.
    pub fn hide_titlebar_buttons(mut self, val: bool) -> Self {
        self.titlebar_buttons_hidden = Some(val);
        self
    }

    /// Make the window content [use the full size of the window, including the title bar area]
    /// (https://developer.apple.com/documentation/appkit/nswindow/stylemask/1644646-fullsizecontentview).
    pub fn full_size_content_view(mut self, val: bool) -> Self {
        self.full_size_content_view = Some(val);
        self
    }

    /// Allow the window to be moved or not.
    pub fn movable(mut self, val: bool) -> Self {
        self.movable = Some(val);
        self
    }

    /// Specify the position of the close / minimize / full screen buttons
    /// on Mac OS
    pub fn traffic_lights_offset(mut self, x_y_offset: (f64, f64)) -> Self {
        self.traffic_lights_offset = Some(x_y_offset);
        self
    }

    /// Specify that this window should be sent an event for the initial
    /// click in it when it was previously inactive, rather than treating
    /// that click is only activating the window and not forwarding it to
    /// application code.
    pub fn accept_first_mouse(mut self, val: bool) -> Self {
        self.accepts_first_mouse = Some(val);
        self
    }

    /// Give this window an identifier when tabbing between windows.
    pub fn tabbing_identifier(mut self, val: impl Into<String>) -> Self {
        self.tabbing_identifier = Some(val.into());
        self
    }

    /// Specify how the window will treat `Option` keys on the Mac keyboard -
    /// as a compose key for additional characters, or as a modifier key.
    pub fn interpret_option_as_alt(mut self, val: MacOsOptionAsAlt) -> Self {
        self.option_as_alt = Some(val);
        self
    }

    /// Set whether the window should have a shadow.
    pub fn enable_shadow(mut self, val: bool) -> Self {
        self.has_shadow = Some(val);
        self
    }

    /// Set whether the window's coordinate space and painting should
    /// be scaled for the display or pixel-accurate.
    pub fn disallow_high_dpi(mut self, val: bool) -> Self {
        self.disallow_high_dpi = Some(val);
        self
    }
}

/// Mac OS allows altering the way Option and Alt keys so Alt is treated
/// as a modifier key rather than in character compose key.  This is a proxy
/// for winit's [OptionAsAlt](https://docs.rs/winit/latest/winit/platform/macos/enum.OptionAsAlt.html).
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacOsOptionAsAlt {
    OnlyLeft,
    OnlyRight,
    Both,
    #[default]
    None,
}

#[cfg(target_os = "macos")]
impl From<MacOsOptionAsAlt> for floem_winit::platform::macos::OptionAsAlt {
    fn from(opts: MacOsOptionAsAlt) -> floem_winit::platform::macos::OptionAsAlt {
        match opts {
            MacOsOptionAsAlt::OnlyLeft => floem_winit::platform::macos::OptionAsAlt::OnlyLeft,
            MacOsOptionAsAlt::OnlyRight => floem_winit::platform::macos::OptionAsAlt::OnlyRight,
            MacOsOptionAsAlt::Both => floem_winit::platform::macos::OptionAsAlt::Both,
            MacOsOptionAsAlt::None => floem_winit::platform::macos::OptionAsAlt::None,
        }
    }
}

/// create a new window. You'll need to create Application first, otherwise it
/// will panic
pub fn new_window<V: IntoView + 'static>(
    app_view: impl FnOnce(WindowId) -> V + 'static,
    config: Option<WindowConfig>,
) {
    add_app_update_event(AppUpdateEvent::NewWindow {
        view_fn: Box::new(|window_id| app_view(window_id).into_any()),
        config,
    });
}

/// request the window to be closed
pub fn close_window(window_id: WindowId) {
    add_app_update_event(AppUpdateEvent::CloseWindow { window_id });
}
