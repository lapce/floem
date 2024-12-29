use std::sync::atomic::AtomicU64;

/// An entry in a menu.
///
/// An entry is either a [`MenuItem`], a submenu (i.e. [`Menu`]).
pub enum MenuEntry {
    Separator,
    Item(MenuItem),
    SubMenu(Menu),
}

pub struct Menu {
    pub(crate) popup: bool,
    pub(crate) item: MenuItem,
    pub(crate) children: Vec<MenuEntry>,
}

impl From<Menu> for MenuEntry {
    fn from(m: Menu) -> MenuEntry {
        MenuEntry::SubMenu(m)
    }
}

impl Menu {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            popup: false,
            item: MenuItem::new(title),
            children: Vec::new(),
        }
    }

    pub(crate) fn popup(mut self) -> Self {
        self.popup = true;
        self
    }

    /// Append a menu entry to this menu, returning the modified menu.
    pub fn entry(mut self, entry: impl Into<MenuEntry>) -> Self {
        self.children.push(entry.into());
        self
    }

    /// Append a separator to this menu, returning the modified menu.
    pub fn separator(self) -> Self {
        self.entry(MenuEntry::Separator)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn platform_menu(&self) -> muda::Menu {
        let menu = muda::Menu::new();
        for entry in &self.children {
            match entry {
                MenuEntry::Separator => {
                    menu.append(&muda::PredefinedMenuItem::separator());
                }
                MenuEntry::Item(item) => {
                    menu.append(&muda::MenuItem::with_id(
                        item.id,
                        item.title.clone(),
                        item.enabled,
                        None,
                    ));
                }
                MenuEntry::SubMenu(floem_menu) => {
                    menu.append(&floem_menu.platform_submenu());
                }
            }
        }
        menu
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn platform_submenu(&self) -> muda::Submenu {
        let menu = muda::Submenu::new(self.item.title.clone(), self.item.enabled);
        for entry in &self.children {
            match entry {
                MenuEntry::Separator => {
                    menu.append(&muda::PredefinedMenuItem::separator());
                }
                MenuEntry::Item(item) => {
                    menu.append(&muda::MenuItem::with_id(
                        item.id,
                        item.title.clone(),
                        item.enabled,
                        None,
                    ));
                }
                MenuEntry::SubMenu(floem_menu) => {
                    menu.append(&floem_menu.platform_submenu());
                }
            }
        }
        menu
    }
}

pub struct MenuItem {
    pub(crate) id: u64,
    pub(crate) title: String,
    // key: Option<HotKey>,
    selected: Option<bool>,
    pub(crate) enabled: bool,
    pub(crate) action: Option<Box<dyn Fn()>>,
}

impl From<MenuItem> for MenuEntry {
    fn from(i: MenuItem) -> MenuEntry {
        MenuEntry::Item(i)
    }
}

impl MenuItem {
    pub fn new(title: impl Into<String>) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            id,
            title: title.into(),
            // key: None,
            selected: None,
            enabled: true,
            action: None,
        }
    }

    pub fn action(mut self, action: impl Fn() + 'static) -> Self {
        self.action = Some(Box::new(action));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}
