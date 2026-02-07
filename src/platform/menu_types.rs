//! Platform-agnostic menu types.
//!
//! This module provides unified menu types that work across all platforms,
//! using muda on native platforms and wasm_stubs on wasm32.

#[cfg(not(target_arch = "wasm32"))]
pub use muda::{
    CheckMenuItem, Icon, IconMenuItem, IsMenuItem, Menu, MenuId, MenuItem, NativeIcon,
    PredefinedMenuItem, Submenu, accelerator::Accelerator,
};

// MenuItemKind is only used in context_menu.rs, which is only compiled on Linux/FreeBSD/wasm32
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub use muda::MenuItemKind;

#[cfg(target_arch = "wasm32")]
pub use crate::platform::wasm_stubs::{
    Accelerator, CheckMenuItem, Icon, IconMenuItem, IsMenuItem, Menu, MenuId, MenuItem,
    MenuItemKind, NativeIcon, PredefinedMenuItem, Submenu,
};
