use parking_lot::Mutex;
use raw_window_handle::RawDisplayHandle;

use copypasta::{ClipboardContext, ClipboardProvider};

static CLIPBOARD: Mutex<Option<Clipboard>> = Mutex::new(None);

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    #[allow(dead_code)]
    selection: Option<Box<dyn ClipboardProvider>>,
}

#[derive(Clone, Debug)]
pub enum ClipboardError {
    NotAvailable,
    ProviderError(String),
    PathError(String),
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
        if s.is_empty() {
            return Err(ClipboardError::ProviderError(
                "content is empty".to_string(),
            ));
        }
        CLIPBOARD
            .lock()
            .as_mut()
            .ok_or(ClipboardError::NotAvailable)?
            .clipboard
            .set_contents(s)
            .map_err(|e| ClipboardError::ProviderError(e.to_string()))
    }

    #[cfg(windows)]
    pub fn get_file_list() -> Result<Vec<std::path::PathBuf>, ClipboardError> {
        use std::{path::PathBuf, str::FromStr};

        let mut out: Vec<String> = Vec::new();
        clipboard_win::raw::get_file_list(&mut out)
            .map_err(|e| ClipboardError::ProviderError(e.to_string()))?;
        let out = out
            .iter()
            .map(|s| PathBuf::from_str(s).map_err(|e| ClipboardError::PathError(e.to_string())))
            .collect();
        out
    }

    pub(crate) unsafe fn init(display: RawDisplayHandle) {
        unsafe {
            *CLIPBOARD.lock() = Some(Self::new(display));
        }
    }

    /// # Safety
    /// The `display` must be valid as long as the returned Clipboard exists.
    unsafe fn new(
        #[allow(unused_variables)] /* on some platforms */ display: RawDisplayHandle,
    ) -> Self {
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "ios",
            target_os = "android",
            target_arch = "wasm32"
        )))]
        {
            if let RawDisplayHandle::Wayland(display) = display {
                use copypasta::wayland_clipboard;
                let (selection, clipboard) =
                    wayland_clipboard::create_clipboards_from_external(display.display.as_ptr());
                return Self {
                    clipboard: Box::new(clipboard),
                    selection: Some(Box::new(selection)),
                };
            }

            use copypasta::x11_clipboard::{Primary, X11ClipboardContext};
            Self {
                clipboard: Box::new(ClipboardContext::new().unwrap()),
                selection: Some(Box::new(X11ClipboardContext::<Primary>::new().unwrap())),
            }
        }

        // TODO: Implement clipboard support for the web, ios, and android
        #[cfg(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "ios",
            target_os = "android",
            target_arch = "wasm32"
        ))]
        return Self {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: None,
        };
    }
}
