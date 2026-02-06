//! Muda-compatible type implementations for wasm32.
//!
//! Since muda (native menu library) doesn't support wasm32, these types provide
//! a compatible API that works with the custom UI-based context menu renderer
//! (same as Linux/FreeBSD).

use std::cell::RefCell;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MenuId(String);

impl MenuId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

impl From<String> for MenuId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for MenuId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MenuId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone)]
pub struct Menu {
    items: RefCell<Vec<MenuItemKind>>,
}

impl Menu {
    pub fn new() -> Self {
        Self {
            items: RefCell::new(Vec::new()),
        }
    }

    pub fn items(&self) -> Vec<MenuItemKind> {
        self.items.borrow().clone()
    }

    pub fn append(&self, item: &dyn IsMenuItem) -> Result<(), MenuError> {
        self.items.borrow_mut().push(item.as_menu_item_kind());
        Ok(())
    }
}

#[derive(Clone)]
pub struct Submenu {
    text: String,
    enabled: RefCell<bool>,
    items: RefCell<Vec<MenuItemKind>>,
}

impl Submenu {
    pub fn new(text: impl AsRef<str>, enabled: bool) -> Self {
        Self {
            text: text.as_ref().to_string(),
            enabled: RefCell::new(enabled),
            items: RefCell::new(Vec::new()),
        }
    }

    pub fn set_icon(&self, _icon: Option<Icon>) {
        // No-op on wasm32
    }

    pub fn set_native_icon(&self, _icon: Option<NativeIcon>) {
        // No-op on wasm32
    }

    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.borrow_mut() = enabled;
    }

    pub fn append(&self, item: &dyn IsMenuItem) -> Result<(), MenuError> {
        self.items.borrow_mut().push(item.as_menu_item_kind());
        Ok(())
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.borrow()
    }

    pub fn items(&self) -> Vec<MenuItemKind> {
        self.items.borrow().clone()
    }
}

#[derive(Clone)]
pub enum MenuItemKind {
    MenuItem(MenuItem),
    Submenu(Submenu),
    Predefined(PredefinedMenuItem),
    Check(CheckMenuItem),
    Icon(IconMenuItem),
}

#[derive(Clone)]
pub struct MenuItem {
    id: MenuId,
    text: String,
    enabled: bool,
}

impl MenuItem {
    pub fn new(text: impl AsRef<str>, enabled: bool, _accelerator: Option<Accelerator>) -> Self {
        Self {
            id: MenuId::new(format!("menu_{}", text.as_ref())),
            text: text.as_ref().to_string(),
            enabled,
        }
    }

    pub fn with_id(
        id: MenuId,
        text: impl AsRef<str>,
        enabled: bool,
        _accelerator: Option<Accelerator>,
    ) -> Self {
        Self {
            id,
            text: text.as_ref().to_string(),
            enabled,
        }
    }

    pub fn id(&self) -> &MenuId {
        &self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Clone)]
pub struct CheckMenuItem {
    id: MenuId,
    text: String,
    enabled: bool,
    checked: bool,
}

impl CheckMenuItem {
    pub fn with_id(
        id: MenuId,
        text: impl AsRef<str>,
        enabled: bool,
        checked: bool,
        _accelerator: Option<Accelerator>,
    ) -> Self {
        Self {
            id,
            text: text.as_ref().to_string(),
            enabled,
            checked,
        }
    }

    pub fn id(&self) -> &MenuId {
        &self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }
}

#[derive(Clone)]
pub struct IconMenuItem {
    id: MenuId,
    text: String,
    enabled: bool,
}

impl IconMenuItem {
    pub fn with_id(
        id: MenuId,
        text: impl AsRef<str>,
        enabled: bool,
        _icon: Option<Icon>,
        _accelerator: Option<Accelerator>,
    ) -> Self {
        Self {
            id,
            text: text.as_ref().to_string(),
            enabled,
        }
    }

    pub fn set_icon(&self, _icon: Icon) {
        // Stub - does nothing on wasm32
    }

    pub fn set_native_icon(&self, _icon: Option<NativeIcon>) {
        // Stub - does nothing on wasm32
    }

    pub fn id(&self) -> &MenuId {
        &self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Clone)]
pub struct PredefinedMenuItem;

impl PredefinedMenuItem {
    pub fn separator() -> Self {
        Self
    }
}

#[derive(Clone)]
pub struct Accelerator;

#[derive(Clone)]
pub struct Icon;

#[derive(Clone)]
pub enum NativeIcon {}

pub struct MenuError;

pub enum MenuTheme {
    Light,
    Dark,
}

pub trait IsMenuItem {
    fn as_menu_item_kind(&self) -> MenuItemKind;
}

impl IsMenuItem for MenuItem {
    fn as_menu_item_kind(&self) -> MenuItemKind {
        MenuItemKind::MenuItem(self.clone())
    }
}

impl IsMenuItem for CheckMenuItem {
    fn as_menu_item_kind(&self) -> MenuItemKind {
        MenuItemKind::Check(self.clone())
    }
}

impl IsMenuItem for IconMenuItem {
    fn as_menu_item_kind(&self) -> MenuItemKind {
        MenuItemKind::Icon(self.clone())
    }
}

impl IsMenuItem for PredefinedMenuItem {
    fn as_menu_item_kind(&self) -> MenuItemKind {
        MenuItemKind::Predefined(self.clone())
    }
}

impl IsMenuItem for Submenu {
    fn as_menu_item_kind(&self) -> MenuItemKind {
        MenuItemKind::Submenu(self.clone())
    }
}
