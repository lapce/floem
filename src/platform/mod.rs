//! Platform abstractions for OS-level functionality.
//!
//! This module provides cross-platform abstractions for native operating system
//! features like clipboard access, native menus, and file dialogs.

pub(crate) mod clipboard;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(crate) mod context_menu;
pub mod file;
#[cfg(any(feature = "rfd-async-std", feature = "rfd-tokio"))]
pub mod file_action;
pub mod menu;

pub use clipboard::{Clipboard, ClipboardError};
pub use file::{FileDialogOptions, FileInfo, FileSpec};
#[cfg(any(feature = "rfd-async-std", feature = "rfd-tokio"))]
pub use file_action::{open_file, save_as};
pub use menu::{Menu, SubMenu};
