use peniko::Color;
use peniko::kurbo::{Point, Size};
pub use winit::icon::{Icon, RgbaIcon};
pub use winit::monitor::Fullscreen;
pub use winit::window::ResizeDirection;
pub use winit::window::Theme;
pub use winit::window::WindowButtons;
pub use winit::window::WindowId;
pub use winit::window::WindowLevel;

use crate::AnyView;
use crate::action::current_theme;
use crate::app::{AppUpdateEvent, add_app_update_event};
use crate::view::IntoView;

pub struct WindowCreation {
    pub(crate) view_fn: Box<dyn FnOnce(WindowId) -> AnyView>,
    pub(crate) config: Option<WindowConfig>,
}

/// Configures various attributes (e.g. size, position, transparency, etc.) of a window.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub(crate) size: Option<Size>,
    pub(crate) min_size: Option<Size>,
    pub(crate) max_size: Option<Size>,
    pub(crate) position: Option<Point>,
    pub(crate) show_titlebar: bool,
    pub(crate) transparent: bool,
    pub(crate) fullscreen: Option<Fullscreen>,
    pub(crate) window_icon: Option<Icon>,
    pub(crate) title: String,
    pub(crate) enabled_buttons: WindowButtons,
    pub(crate) resizable: bool,
    pub(crate) undecorated: bool,
    pub(crate) undecorated_shadow: bool,
    pub(crate) window_level: WindowLevel,
    /// Applies chosen theme or os theme, when `None` is provided.
    pub(crate) with_theme: Option<Theme>,
    pub(crate) font_embolden: f32,
    #[allow(dead_code)]
    pub(crate) mac_os_config: Option<MacOSWindowConfig>,
    pub(crate) win_os_config: Option<WinOSWindowConfig>,
    pub(crate) web_config: Option<WebWindowConfig>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            size: None,
            min_size: None,
            max_size: None,
            position: None,
            show_titlebar: true,
            transparent: false,
            fullscreen: None,
            window_icon: None,
            title: std::env::current_exe()
                .ok()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
                .unwrap_or("Floem Window".to_string()),
            enabled_buttons: WindowButtons::all(),
            resizable: true,
            undecorated: false,
            undecorated_shadow: false,
            window_level: WindowLevel::Normal,
            with_theme: None,
            font_embolden: if cfg!(target_os = "macos") { 0.2 } else { 0. },
            mac_os_config: None,
            win_os_config: None,
            web_config: None,
        }
    }
}

impl WindowConfig {
    /// Requests the window to be of specific dimensions.
    ///
    /// If this is not set, some platform-specific dimensions will be used.
    #[inline]
    pub fn size(mut self, size: impl Into<Size>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Requests the window to be of specific min dimensions.
    #[inline]
    pub fn min_size(mut self, size: impl Into<Size>) -> Self {
        self.min_size = Some(size.into());
        self
    }

    /// Requests the window to be of specific max dimensions.
    #[inline]
    pub fn max_size(mut self, size: impl Into<Size>) -> Self {
        self.max_size = Some(size.into());
        self
    }

    /// Sets a desired initial position for the window.
    ///
    /// If this is not set, some platform-specific position will be chosen.
    #[inline]
    pub fn position(mut self, position: Point) -> Self {
        self.position = Some(position);
        self
    }

    /// Sets whether the window should have a title bar.
    ///
    /// The default is `true`.
    #[inline]
    pub fn show_titlebar(mut self, show_titlebar: bool) -> Self {
        self.show_titlebar = show_titlebar;
        self
    }

    /// Sets whether the window should have a border, a title bar, etc.
    ///
    /// The default is `false`.
    #[inline]
    pub fn undecorated(mut self, undecorated: bool) -> Self {
        self.undecorated = undecorated;
        self
    }

    /// Sets whether the window should have background drop shadow when undecorated.
    ///
    /// The default is `false`.
    #[inline]
    pub fn undecorated_shadow(mut self, undecorated_shadow: bool) -> Self {
        self.undecorated_shadow = undecorated_shadow;
        self
    }

    /// Sets whether the background of the window should be transparent.
    ///
    /// The default is `false`.
    #[inline]
    pub fn with_transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    /// Sets whether the window should be put into fullscreen upon creation.
    ///
    /// The default is `None`.
    #[inline]
    pub fn fullscreen(mut self, fullscreen: Fullscreen) -> Self {
        self.fullscreen = Some(fullscreen);
        self
    }

    /// Sets the window icon.
    ///
    /// The default is `None`.
    #[inline]
    pub fn window_icon(mut self, window_icon: Icon) -> Self {
        self.window_icon = Some(window_icon);
        self
    }

    /// Sets the initial title of the window in the title bar.
    ///
    /// The default is `"Floem window"`.
    #[inline]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets the enabled window buttons.
    ///
    /// The default is `WindowButtons::all()`.
    #[inline]
    pub fn enabled_buttons(mut self, enabled_buttons: WindowButtons) -> Self {
        self.enabled_buttons = enabled_buttons;
        self
    }

    /// Sets whether the window is resizable or not.
    ///
    /// The default is `true`.
    #[inline]
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Sets the window level.
    ///
    /// This is just a hint to the OS, and the system could ignore it.
    ///
    /// The default is `WindowLevel::Normal`.
    #[inline]
    pub fn window_level(mut self, window_level: WindowLevel) -> Self {
        self.window_level = window_level;
        self
    }

    /// Set theme for window.
    ///
    /// If not provided, window will follow OS theme.
    #[inline]
    pub fn apply_theme(mut self, theme: Theme) -> Self {
        self.with_theme = Some(theme);
        self
    }

    /// Sets the amount by which fonts are emboldened.
    ///
    /// The default is 0.0 except for on macOS where the default is 0.2
    #[inline]
    pub fn font_embolden(mut self, font_embolden: f32) -> Self {
        self.font_embolden = font_embolden;
        self
    }

    /// Set up Mac-OS specific configuration.  The passed closure will only be
    /// called on macOS.
    #[allow(unused_variables, unused_mut)] // build will complain on non-macOS's otherwise
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

    /// Set up web specific configuration.
    /// The passed closure will only be called on the web.
    #[allow(unused_variables, unused_mut)] // build will complain on non-web platforms otherwise
    pub fn with_web_config(mut self, f: impl FnOnce(WebWindowConfig) -> WebWindowConfig) -> Self {
        #[cfg(target_arch = "wasm32")]
        if let Some(existing_config) = self.web_config {
            self.web_config = Some(f(existing_config))
        } else {
            let new_config = f(WebWindowConfig {
                canvas_id: String::new(),
            });
            self.web_config = Some(new_config);
        }
        self
    }
}

/// Mac-OS specific window configuration properties, accessible via `WindowConfig::with_mac_os_config( FnMut( MacOsWindowConfig ) )`
///
/// See [the winit docs](https://docs.rs/winit/latest/winit/platform/macos/trait.WindowExtMacOS.html) for further
/// information.
#[derive(Default, Debug, Clone)]
pub struct MacOSWindowConfig {
    pub(crate) movable_by_window_background: Option<bool>,
    pub(crate) titlebar_transparent: Option<bool>,
    pub(crate) titlebar_hidden: Option<bool>,
    pub(crate) title_hidden: Option<bool>,
    pub(crate) titlebar_buttons_hidden: Option<bool>,
    pub(crate) full_size_content_view: Option<bool>,
    pub(crate) unified_titlebar: Option<bool>,
    pub(crate) movable: Option<bool>,
    pub(crate) traffic_lights_offset: Option<(f64, f64)>,
    pub(crate) accepts_first_mouse: Option<bool>,
    pub(crate) tabbing_identifier: Option<String>,
    pub(crate) option_as_alt: Option<MacOsOptionAsAlt>,
    pub(crate) has_shadow: Option<bool>,
    pub(crate) disallow_high_dpi: Option<bool>,
    pub(crate) panel: Option<bool>,
}

impl MacOSWindowConfig {
    /// Allow the window to be
    /// [moved by dragging its background](https://developer.apple.com/documentation/appkit/nswindow/1419072-movablebywindowbackground).
    pub fn movable_by_window_background(mut self, val: bool) -> Self {
        self.movable_by_window_background = Some(val);
        self
    }

    /// Make the titlebar's transparency (does nothing on some versions of macOS).
    pub fn transparent_title_bar(mut self, val: bool) -> Self {
        self.titlebar_transparent = Some(val);
        self
    }

    /// Hides the title bar.
    pub fn hide_titlebar(mut self, val: bool) -> Self {
        self.titlebar_hidden = Some(val);
        self
    }

    /// Hides the title.
    pub fn hide_title(mut self, val: bool) -> Self {
        self.title_hidden = Some(val);
        self
    }

    /// Hides the title bar buttons.
    pub fn hide_titlebar_buttons(mut self, val: bool) -> Self {
        self.titlebar_buttons_hidden = Some(val);
        self
    }

    /// Make the window content [use the full size of the window, including the title bar area](https://developer.apple.com/documentation/appkit/nswindow/stylemask/1644646-fullsizecontentview).
    pub fn full_size_content_view(mut self, val: bool) -> Self {
        self.full_size_content_view = Some(val);
        self
    }

    /// unify the titlebar
    pub fn unified_titlebar(mut self, val: bool) -> Self {
        self.unified_titlebar = Some(val);
        self
    }

    /// Allow the window to be moved or not.
    pub fn movable(mut self, val: bool) -> Self {
        self.movable = Some(val);
        self
    }

    /// Specify the position of the close / minimize / full screen buttons
    /// on macOS
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

    /// Set whether the window is a panel
    pub fn panel(mut self, val: bool) -> Self {
        self.panel = Some(val);
        self
    }
}

/// macOS specific configuration for how the Option key is treated
///
/// macOS allows altering the way Option and Alt keys so Alt is treated
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
impl From<MacOsOptionAsAlt> for winit::platform::macos::OptionAsAlt {
    fn from(opts: MacOsOptionAsAlt) -> winit::platform::macos::OptionAsAlt {
        match opts {
            MacOsOptionAsAlt::OnlyLeft => winit::platform::macos::OptionAsAlt::OnlyLeft,
            MacOsOptionAsAlt::OnlyRight => winit::platform::macos::OptionAsAlt::OnlyRight,
            MacOsOptionAsAlt::Both => winit::platform::macos::OptionAsAlt::Both,
            MacOsOptionAsAlt::None => winit::platform::macos::OptionAsAlt::None,
        }
    }
}

// Windows specific window configuration properties.
#[derive(Debug, Clone)]
pub struct WinOSWindowConfig {
    // pub(crate) top_resize_border: bool,
    pub(crate) corner_preference: WinOsCornerPreference,
    pub(crate) set_enable: bool,
    pub(crate) set_skip_taskbar: bool,
    // /// Shows or hides the background drop shadow for undecorated windows.
    // ///
    // /// Enabling the shadow causes a thin 1px line to appear on the top of the window.
    // pub(crate) set_undecorated_shadow: bool,
    pub(crate) set_system_backdrop: WinOsBackdropType,
    pub(crate) set_border_color: Option<Color>,
    pub(crate) set_title_background_color: Option<Color>,
    pub(crate) set_title_text_color: Option<Color>,
}

impl Default for WinOSWindowConfig {
    fn default() -> Self {
        Self {
            // top_resize_border: true,
            corner_preference: WinOsCornerPreference::Default,
            set_enable: true,
            set_skip_taskbar: false,
            set_system_backdrop: WinOsBackdropType::Auto,
            set_border_color: None,
            set_title_background_color: None,
            set_title_text_color: None,
        }
    }
}

impl WinOSWindowConfig {
    // TODO: need to patch winit first
    // /// Turn window top resize border on or off (for windows without a title bar).
    // /// By default this is enabled.
    // pub fn top_resize_border(mut self, top_resize_border: bool) -> Self {
    //     self.top_resize_border = top_resize_border;
    //     self
    // }

    /// Sets the preferred style of the window corners.
    ///
    /// Supported starting with Windows 11 Build 22000.
    pub fn corner_preference(mut self, corner_preference: WinOsCornerPreference) -> Self {
        self.corner_preference = corner_preference;
        self
    }

    /// Enables or disables mouse and keyboard input to the specified window.
    ///
    /// A window must be enabled before it can be activated.
    /// If an application has create a modal dialog box by disabling its owner window
    /// (as described in [`WindowAttributesExtWindows::with_owner_window`]), the application must
    /// enable the owner window before destroying the dialog box.
    /// Otherwise, another window will receive the keyboard focus and be activated.
    ///
    /// If a child window is disabled, it is ignored when the system tries to determine which
    /// window should receive mouse messages.
    ///
    /// For more information, see <https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enablewindow#remarks>
    /// and <https://docs.microsoft.com/en-us/windows/win32/winmsg/window-features#disabled-windows>.
    pub fn set_enable(mut self, set_enable: bool) -> Self {
        self.set_enable = set_enable;
        self
    }

    /// Whether to show or hide the window icon in the taskbar.
    pub fn set_skip_taskbar(mut self, set_skip_taskbar: bool) -> Self {
        self.set_skip_taskbar = set_skip_taskbar;
        self
    }

    /// Sets system-drawn backdrop type.
    ///
    /// Requires Windows 11 build 22523+.
    pub fn set_system_backdrop(mut self, set_system_backdrop: WinOsBackdropType) -> Self {
        self.set_system_backdrop = set_system_backdrop;
        self
    }

    /// Sets the color of the window border.
    ///
    /// Supported starting with Windows 11 Build 22000.
    pub fn set_border_color(mut self, set_border_color: Color) -> Self {
        self.set_border_color = Some(set_border_color);
        self
    }

    /// Sets the background color of the title bar.
    ///
    /// Supported starting with Windows 11 Build 22000.
    pub fn set_title_background_color(mut self, set_title_background_color: Color) -> Self {
        self.set_title_background_color = Some(set_title_background_color);
        self
    }

    /// Sets the color of the window title.
    ///
    /// Supported starting with Windows 11 Build 22000.
    pub fn set_title_text_color(mut self, set_title_text_color: Color) -> Self {
        self.set_title_text_color = Some(set_title_text_color);
        self
    }
}

/// Describes how the corners of a window should look like.
///
/// For a detailed explanation, see [`DWM_WINDOW_CORNER_PREFERENCE docs`].
///
/// [`DWM_WINDOW_CORNER_PREFERENCE docs`]: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_window_corner_preference
#[derive(Debug, Clone)]
pub enum WinOsCornerPreference {
    /// Corresponds to `DWMWCP_DEFAULT`.
    ///
    /// Let the system decide when to round window corners.
    Default,

    /// Corresponds to `DWMWCP_DONOTROUND`.
    ///
    /// Never round window corners.
    DoNotRound,

    /// Corresponds to `DWMWCP_ROUND`.
    ///
    /// Round the corners, if appropriate.
    Round,

    /// Corresponds to `DWMWCP_ROUNDSMALL`.
    ///
    /// Round the corners if appropriate, with a small radius.
    RoundSmall,
}

#[cfg(target_os = "windows")]
impl From<WinOsCornerPreference> for winit::platform::windows::CornerPreference {
    fn from(value: WinOsCornerPreference) -> Self {
        use winit::platform::windows::CornerPreference;
        match value {
            WinOsCornerPreference::Default => CornerPreference::Default,
            WinOsCornerPreference::DoNotRound => CornerPreference::DoNotRound,
            WinOsCornerPreference::Round => CornerPreference::Round,
            WinOsCornerPreference::RoundSmall => CornerPreference::RoundSmall,
        }
    }
}

/// Describes a system-drawn backdrop material of a window.
///
/// For a detailed explanation, see [`DWM_SYSTEMBACKDROP_TYPE docs`].
///
/// [`DWM_SYSTEMBACKDROP_TYPE docs`]: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type
#[derive(Debug, Clone)]
pub enum WinOsBackdropType {
    /// Corresponds to `DWMSBT_AUTO`.
    ///
    /// Usually draws a default backdrop effect on the title bar.
    Auto,

    /// Corresponds to `DWMSBT_NONE`.
    None,

    /// Corresponds to `DWMSBT_MAINWINDOW`.
    ///
    /// Draws the Mica backdrop material.
    MainWindow,

    /// Corresponds to `DWMSBT_TRANSIENTWINDOW`.
    ///
    /// Draws the Background Acrylic backdrop material.
    TransientWindow,

    /// Corresponds to `DWMSBT_TABBEDWINDOW`.
    ///
    /// Draws the Alt Mica backdrop material.
    TabbedWindow,
}

#[cfg(target_os = "windows")]
impl From<WinOsBackdropType> for winit::platform::windows::BackdropType {
    fn from(value: WinOsBackdropType) -> Self {
        use winit::platform::windows::BackdropType;
        match value {
            WinOsBackdropType::Auto => BackdropType::Auto,
            WinOsBackdropType::None => BackdropType::None,
            WinOsBackdropType::MainWindow => BackdropType::MainWindow,
            WinOsBackdropType::TransientWindow => BackdropType::TransientWindow,
            WinOsBackdropType::TabbedWindow => BackdropType::TabbedWindow,
        }
    }
}

#[cfg(target_os = "windows")]
pub(super) fn convert_to_win(c: Option<peniko::Color>) -> Option<winit::platform::windows::Color> {
    c.map(|c| {
        let c = c.to_rgba8();
        winit::platform::windows::Color::from_rgb(c.r, c.g, c.b)
    })
}

/// Web specific window (canvas) configuration properties, accessible via
/// `WindowConfig::with_web_config( WebWindowConfig )`.
#[derive(Default, Debug, Clone)]
pub struct WebWindowConfig {
    /// The id of the HTML canvas element that floem should render to.
    pub(crate) canvas_id: String,
}

impl WebWindowConfig {
    /// Specify the id of the HTML canvas element that floem should render to.
    pub fn canvas_id(mut self, val: impl Into<String>) -> Self {
        self.canvas_id = val.into();
        self
    }
}

/// Create a new window. You'll need to create Application first, otherwise it
/// will panic.
pub fn new_window<V: IntoView + 'static>(
    app_view: impl FnOnce(WindowId) -> V + 'static,
    config: Option<WindowConfig>,
) {
    add_app_update_event(AppUpdateEvent::NewWindow {
        window_creation: WindowCreation {
            view_fn: Box::new(|window_id| app_view(window_id).into_any()),
            config: match current_theme() {
                Some(theme) => Some(config.unwrap_or_default().apply_theme(theme)),
                None => config,
            },
        },
    });
}

/// request the window to be closed
pub fn close_window(window_id: WindowId) {
    add_app_update_event(AppUpdateEvent::CloseWindow { window_id });
}
