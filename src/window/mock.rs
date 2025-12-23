//! Mock Window implementation for headless testing.
//!
//! This module provides a mock implementation of `winit::window::Window` that can be used
//! for testing WindowHandle without creating a real window.

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, DisplayHandle, HandleError, HasDisplayHandle,
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle,
};
use winit::cursor::Cursor;
use winit::dpi::{PhysicalInsets, PhysicalPosition, PhysicalSize, Position, Size};
use winit::error::RequestError;
use winit::icon::Icon;
use winit::monitor::{Fullscreen, MonitorHandle};
use winit::window::{
    CursorGrabMode, ImeCapabilities, ImeRequest, ImeRequestError, ResizeDirection, Theme,
    UserAttentionType, Window, WindowButtons, WindowId, WindowLevel,
};

/// Counter for generating unique window IDs.
static WINDOW_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// A mock window implementation for headless testing.
///
/// This struct implements the `winit::window::Window` trait with stub implementations
/// that are suitable for testing without a real window system.
pub struct MockWindow {
    id: WindowId,
    scale_factor: f64,
    surface_size: PhysicalSize<u32>,
    title: String,
    visible: bool,
    maximized: bool,
    minimized: bool,
    resizable: bool,
    decorated: bool,
    theme: Option<Theme>,
    has_focus: bool,
}

impl MockWindow {
    /// Create a new mock window with default settings.
    pub fn new() -> Self {
        let id = WindowId::from_raw(WINDOW_ID_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            scale_factor: 1.0,
            surface_size: PhysicalSize::new(800, 600),
            title: String::new(),
            visible: true,
            maximized: false,
            minimized: false,
            resizable: true,
            decorated: true,
            theme: None,
            has_focus: true,
        }
    }

    /// Create a new mock window with the given size.
    pub fn with_size(width: u32, height: u32) -> Self {
        let mut window = Self::new();
        window.surface_size = PhysicalSize::new(width, height);
        window
    }

    /// Set the scale factor for this mock window.
    pub fn set_scale_factor(&mut self, scale: f64) {
        self.scale_factor = scale;
    }
}

impl Default for MockWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MockWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockWindow")
            .field("id", &self.id)
            .field("scale_factor", &self.scale_factor)
            .field("surface_size", &self.surface_size)
            .finish()
    }
}

/// Mock display handle for raw-window-handle.
struct MockDisplayHandle;

impl HasDisplayHandle for MockDisplayHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        // Return a no-op display handle
        // SAFETY: This is a mock that won't be used for real rendering
        Ok(unsafe {
            DisplayHandle::borrow_raw(RawDisplayHandle::AppKit(AppKitDisplayHandle::new()))
        })
    }
}

/// Mock window handle for raw-window-handle.
struct MockWindowHandle;

impl HasWindowHandle for MockWindowHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // Return a no-op window handle
        // SAFETY: This is a mock that won't be used for real rendering
        use std::ptr::NonNull;
        let ptr = NonNull::new(std::ptr::dangling_mut::<std::ffi::c_void>()).unwrap();
        let handle = AppKitWindowHandle::new(ptr);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::AppKit(handle)) })
    }
}

impl Window for MockWindow {
    fn id(&self) -> WindowId {
        self.id
    }

    fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    fn request_redraw(&self) {
        // No-op for mock
    }

    fn pre_present_notify(&self) {
        // No-op for mock
    }

    fn reset_dead_keys(&self) {
        // No-op for mock
    }

    fn surface_position(&self) -> PhysicalPosition<i32> {
        PhysicalPosition::new(0, 0)
    }

    fn outer_position(&self) -> Result<PhysicalPosition<i32>, RequestError> {
        Ok(PhysicalPosition::new(0, 0))
    }

    fn set_outer_position(&self, _position: Position) {
        // No-op for mock
    }

    fn surface_size(&self) -> PhysicalSize<u32> {
        self.surface_size
    }

    fn request_surface_size(&self, _size: Size) -> Option<PhysicalSize<u32>> {
        Some(self.surface_size)
    }

    fn outer_size(&self) -> PhysicalSize<u32> {
        self.surface_size
    }

    fn safe_area(&self) -> PhysicalInsets<u32> {
        PhysicalInsets::new(0, 0, 0, 0)
    }

    fn set_min_surface_size(&self, _min_size: Option<Size>) {
        // No-op for mock
    }

    fn set_max_surface_size(&self, _max_size: Option<Size>) {
        // No-op for mock
    }

    fn surface_resize_increments(&self) -> Option<PhysicalSize<u32>> {
        None
    }

    fn set_surface_resize_increments(&self, _increments: Option<Size>) {
        // No-op for mock
    }

    fn set_title(&self, _title: &str) {
        // No-op for mock (would need interior mutability to store)
    }

    fn set_transparent(&self, _transparent: bool) {
        // No-op for mock
    }

    fn set_blur(&self, _blur: bool) {
        // No-op for mock
    }

    fn set_visible(&self, _visible: bool) {
        // No-op for mock (would need interior mutability)
    }

    fn is_visible(&self) -> Option<bool> {
        Some(self.visible)
    }

    fn set_resizable(&self, _resizable: bool) {
        // No-op for mock
    }

    fn is_resizable(&self) -> bool {
        self.resizable
    }

    fn set_enabled_buttons(&self, _buttons: WindowButtons) {
        // No-op for mock
    }

    fn enabled_buttons(&self) -> WindowButtons {
        WindowButtons::all()
    }

    fn set_minimized(&self, _minimized: bool) {
        // No-op for mock
    }

    fn is_minimized(&self) -> Option<bool> {
        Some(self.minimized)
    }

    fn set_maximized(&self, _maximized: bool) {
        // No-op for mock
    }

    fn is_maximized(&self) -> bool {
        self.maximized
    }

    fn set_fullscreen(&self, _fullscreen: Option<Fullscreen>) {
        // No-op for mock
    }

    fn fullscreen(&self) -> Option<Fullscreen> {
        None
    }

    fn set_decorations(&self, _decorations: bool) {
        // No-op for mock
    }

    fn is_decorated(&self) -> bool {
        self.decorated
    }

    fn set_window_level(&self, _level: WindowLevel) {
        // No-op for mock
    }

    fn set_window_icon(&self, _window_icon: Option<Icon>) {
        // No-op for mock
    }

    fn request_ime_update(&self, _request: ImeRequest) -> Result<(), ImeRequestError> {
        // Return error indicating IME is not supported in mock
        Err(ImeRequestError::NotSupported)
    }

    fn ime_capabilities(&self) -> Option<ImeCapabilities> {
        None
    }

    fn focus_window(&self) {
        // No-op for mock
    }

    fn has_focus(&self) -> bool {
        self.has_focus
    }

    fn request_user_attention(&self, _request_type: Option<UserAttentionType>) {
        // No-op for mock
    }

    fn set_theme(&self, _theme: Option<Theme>) {
        // No-op for mock
    }

    fn theme(&self) -> Option<Theme> {
        self.theme
    }

    fn set_content_protected(&self, _protected: bool) {
        // No-op for mock
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn set_cursor(&self, _cursor: Cursor) {
        // No-op for mock
    }

    fn set_cursor_position(&self, _position: Position) -> Result<(), RequestError> {
        Ok(())
    }

    fn set_cursor_grab(&self, _mode: CursorGrabMode) -> Result<(), RequestError> {
        Ok(())
    }

    fn set_cursor_visible(&self, _visible: bool) {
        // No-op for mock
    }

    fn drag_window(&self) -> Result<(), RequestError> {
        Ok(())
    }

    fn drag_resize_window(&self, _direction: ResizeDirection) -> Result<(), RequestError> {
        Ok(())
    }

    fn show_window_menu(&self, _position: Position) {
        // No-op for mock
    }

    fn set_cursor_hittest(&self, _hittest: bool) -> Result<(), RequestError> {
        Ok(())
    }

    fn current_monitor(&self) -> Option<MonitorHandle> {
        None
    }

    fn available_monitors(&self) -> Box<dyn Iterator<Item = MonitorHandle>> {
        Box::new(std::iter::empty())
    }

    fn primary_monitor(&self) -> Option<MonitorHandle> {
        None
    }

    fn rwh_06_display_handle(&self) -> &dyn HasDisplayHandle {
        // Return a static reference to our mock display handle
        static MOCK_DISPLAY: MockDisplayHandle = MockDisplayHandle;
        &MOCK_DISPLAY
    }

    fn rwh_06_window_handle(&self) -> &dyn HasWindowHandle {
        // Return a static reference to our mock window handle
        static MOCK_WINDOW: MockWindowHandle = MockWindowHandle;
        &MOCK_WINDOW
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_window_creation() {
        let window = MockWindow::new();
        assert_eq!(window.scale_factor(), 1.0);
        assert_eq!(window.surface_size(), PhysicalSize::new(800, 600));
    }

    #[test]
    fn test_mock_window_with_size() {
        let window = MockWindow::with_size(1024, 768);
        assert_eq!(window.surface_size(), PhysicalSize::new(1024, 768));
    }

    #[test]
    fn test_unique_window_ids() {
        let window1 = MockWindow::new();
        let window2 = MockWindow::new();
        assert_ne!(window1.id(), window2.id());
    }
}
