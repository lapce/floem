use once_cell::sync::Lazy;
use parking_lot::Mutex;
use raw_window_handle::RawDisplayHandle;

#[cfg(not(any(target_os = "macos", windows)))]
use copypasta::{
    wayland_clipboard,
    x11_clipboard::{Primary as X11SelectionClipboard, X11ClipboardContext},
};

use copypasta::{ClipboardContext, ClipboardProvider};

static CLIPBOARD: Lazy<Mutex<Option<Clipboard>>> = Lazy::new(|| Mutex::new(None));

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    #[allow(dead_code)]
    selection: Option<Box<dyn ClipboardProvider>>,
}

#[derive(Clone, Debug)]
pub enum ClipboardError {
    NotAvailable,
    ProviderError(String),
}

impl Clipboard {
    pub fn get_contents() -> Result<String, ClipboardError> {
        CLIPBOARD
            .lock()
            .as_mut()
            .ok_or(ClipboardError::NotAvailable)?
            .clipboard
            .get_contents()
            .map_err(|e| ClipboardError::ProviderError(e.to_string()))
    }

    pub fn set_contents(s: String) -> Result<(), ClipboardError> {
        CLIPBOARD
            .lock()
            .as_mut()
            .ok_or(ClipboardError::NotAvailable)?
            .clipboard
            .set_contents(s)
            .map_err(|e| ClipboardError::ProviderError(e.to_string()))
    }

    pub(crate) unsafe fn init(display: RawDisplayHandle) {
        *CLIPBOARD.lock() = Some(Self::new(display));
    }

    /// # Safety
    /// The `display` must be valid as long as the returned Clipboard exists.
    unsafe fn new(
        #[allow(unused_variables)] /* on some platforms */ display: RawDisplayHandle,
    ) -> Self {
        #[cfg(not(any(target_os = "macos", windows)))]
        if let RawDisplayHandle::Wayland(display) = display {
            let (selection, clipboard) =
                wayland_clipboard::create_clipboards_from_external(display.display);
            return Self {
                clipboard: Box::new(clipboard),
                selection: Some(Box::new(selection)),
            };
        }

        #[cfg(not(any(target_os = "macos", windows)))]
        return Self {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: Some(Box::new(
                X11ClipboardContext::<X11SelectionClipboard>::new().unwrap(),
            )),
        };

        #[cfg(any(target_os = "macos", windows))]
        return Self {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: None,
        };
    }
}
