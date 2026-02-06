//! Platform abstractions for OS-level functionality.
//!
//! This module provides cross-platform abstractions for native operating system
//! features like clipboard access, native menus, and file dialogs.

pub(crate) mod clipboard;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
pub(crate) mod context_menu;
pub mod file;
#[cfg(not(target_arch = "wasm32"))]
pub mod file_action;
pub mod menu;
#[cfg(target_arch = "wasm32")]
pub mod wasm_stubs;

pub use clipboard::{Clipboard, ClipboardError};
pub use file::{FileDialogOptions, FileInfo, FileSpec};
#[cfg(not(target_arch = "wasm32"))]
pub use file_action::{open_file, save_as};
pub use menu::{Menu, SubMenu};
