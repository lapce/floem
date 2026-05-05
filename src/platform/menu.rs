pub use super::menu_types::{Accelerator, Icon, MenuId, NativeIcon};

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(not(target_arch = "wasm32"))]
pub use super::menu_types::{Menu as MudaMenu, Submenu as MudaSubmenu};
#[cfg(target_arch = "wasm32")]
pub use super::menu_types::{Menu as MudaMenu, Submenu as MudaSubmenu};

#[cfg(not(target_arch = "wasm32"))]
pub use muda::{AboutMetadata, AboutMetadataBuilder};

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Default)]
pub struct AboutMetadata;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Default)]
pub struct AboutMetadataBuilder(AboutMetadata);

#[cfg(target_arch = "wasm32")]
impl AboutMetadataBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(self, _name: Option<&str>) -> Self {
        self
    }

    pub fn license(self, _license: Option<&str>) -> Self {
        self
    }

    pub fn version(self, _version: Option<&str>) -> Self {
        self
    }

    pub fn copyright(self, _copyright: Option<&str>) -> Self {
        self
    }

    pub fn build(self) -> AboutMetadata {
        self.0
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_id() -> MenuId {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    MenuId::new(id.to_string())
}

type MenuAction = Box<dyn Fn()>;

// Type state markers for items
pub struct NoCheckNoIcon;
pub struct HasCheck;
pub struct HasIcon;
pub struct HasNativeIcon;

// Type state markers for icon configuration
pub struct NoIcon;

#[derive(Clone)]
pub(crate) struct MenuSpec {
    items: Vec<MenuEntry>,
}

impl MenuSpec {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn items(&self) -> &[MenuEntry] {
        &self.items
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn into_muda_menu(self) -> MudaMenu {
        let menu = MudaMenu::new();
        for item in self.items {
            append_muda_entry(&menu, item);
        }
        menu
    }
}

#[derive(Clone)]
pub(crate) enum MenuEntry {
    Item(MenuItemSpec),
    Submenu(SubmenuSpec),
    Predefined(PredefinedMenuItem),
}

#[derive(Clone)]
pub(crate) struct MenuItemSpec {
    id: MenuId,
    text: String,
    enabled: bool,
    accelerator: Option<Accelerator>,
    kind: MenuItemKind,
}

impl MenuItemSpec {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn id(&self) -> &MenuId {
        &self.id
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn is_checked(&self) -> Option<bool> {
        match self.kind {
            MenuItemKind::Plain | MenuItemKind::Icon { .. } | MenuItemKind::NativeIcon { .. } => {
                None
            }
            MenuItemKind::Check { checked } => Some(checked),
        }
    }
}

#[derive(Clone)]
enum MenuItemKind {
    Plain,
    Check { checked: bool },
    Icon { icon: Icon },
    NativeIcon { native_icon: NativeIcon },
}

#[derive(Clone)]
pub(crate) struct SubmenuSpec {
    text: String,
    enabled: bool,
    icon: Option<Icon>,
    native_icon: Option<NativeIcon>,
    items: Vec<MenuEntry>,
}

impl SubmenuSpec {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn items(&self) -> &[MenuEntry] {
        &self.items
    }
}

/// A data-only predefined menu item.
///
/// Floem stores predefined items as descriptors so menus can be created on the
/// UI thread and converted to native `muda` menu items later on the main thread.
#[derive(Clone)]
pub enum PredefinedMenuItem {
    Separator,
    Copy(Option<String>),
    Cut(Option<String>),
    Paste(Option<String>),
    SelectAll(Option<String>),
    Undo(Option<String>),
    Redo(Option<String>),
    Minimize(Option<String>),
    Maximize(Option<String>),
    Fullscreen(Option<String>),
    Hide(Option<String>),
    HideOthers(Option<String>),
    ShowAll(Option<String>),
    CloseWindow(Option<String>),
    Quit(Option<String>),
    About {
        text: Option<String>,
        metadata: Option<AboutMetadata>,
    },
    Services(Option<String>),
    BringAllToFront(Option<String>),
}

impl PredefinedMenuItem {
    pub fn separator() -> Self {
        Self::Separator
    }

    pub fn copy(text: Option<&str>) -> Self {
        Self::Copy(text.map(ToOwned::to_owned))
    }

    pub fn cut(text: Option<&str>) -> Self {
        Self::Cut(text.map(ToOwned::to_owned))
    }

    pub fn paste(text: Option<&str>) -> Self {
        Self::Paste(text.map(ToOwned::to_owned))
    }

    pub fn select_all(text: Option<&str>) -> Self {
        Self::SelectAll(text.map(ToOwned::to_owned))
    }

    pub fn undo(text: Option<&str>) -> Self {
        Self::Undo(text.map(ToOwned::to_owned))
    }

    pub fn redo(text: Option<&str>) -> Self {
        Self::Redo(text.map(ToOwned::to_owned))
    }

    pub fn minimize(text: Option<&str>) -> Self {
        Self::Minimize(text.map(ToOwned::to_owned))
    }

    pub fn maximize(text: Option<&str>) -> Self {
        Self::Maximize(text.map(ToOwned::to_owned))
    }

    pub fn fullscreen(text: Option<&str>) -> Self {
        Self::Fullscreen(text.map(ToOwned::to_owned))
    }

    pub fn hide(text: Option<&str>) -> Self {
        Self::Hide(text.map(ToOwned::to_owned))
    }

    pub fn hide_others(text: Option<&str>) -> Self {
        Self::HideOthers(text.map(ToOwned::to_owned))
    }

    pub fn show_all(text: Option<&str>) -> Self {
        Self::ShowAll(text.map(ToOwned::to_owned))
    }

    pub fn close_window(text: Option<&str>) -> Self {
        Self::CloseWindow(text.map(ToOwned::to_owned))
    }

    pub fn quit(text: Option<&str>) -> Self {
        Self::Quit(text.map(ToOwned::to_owned))
    }

    pub fn about(text: Option<&str>, metadata: Option<AboutMetadata>) -> Self {
        Self::About {
            text: text.map(ToOwned::to_owned),
            metadata,
        }
    }

    pub fn services(text: Option<&str>) -> Self {
        Self::Services(text.map(ToOwned::to_owned))
    }

    pub fn bring_all_to_front(text: Option<&str>) -> Self {
        Self::BringAllToFront(text.map(ToOwned::to_owned))
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_arch = "wasm32"))]
    pub(crate) fn is_separator(&self) -> bool {
        matches!(self, Self::Separator)
    }
}

pub struct ItemBuilder<State = NoCheckNoIcon> {
    id: MenuId,
    text: String,
    enabled: bool,
    accelerator: Option<Accelerator>,
    action: Option<MenuAction>,
    state: std::marker::PhantomData<State>,
    checked: Option<bool>,
    icon: Option<Icon>,
    native_icon: Option<NativeIcon>,
}

impl ItemBuilder<NoCheckNoIcon> {
    fn new(text: impl AsRef<str>) -> Self {
        Self {
            id: next_id(),
            text: text.as_ref().to_string(),
            enabled: true,
            accelerator: None,
            action: None,
            state: std::marker::PhantomData,
            checked: None,
            icon: None,
            native_icon: None,
        }
    }

    pub fn checked(self, checked: bool) -> ItemBuilder<HasCheck> {
        ItemBuilder {
            id: self.id,
            text: self.text,
            enabled: self.enabled,
            accelerator: self.accelerator,
            action: self.action,
            state: std::marker::PhantomData,
            checked: Some(checked),
            icon: None,
            native_icon: None,
        }
    }

    pub fn icon(self, icon: Icon) -> ItemBuilder<HasIcon> {
        ItemBuilder {
            id: self.id,
            text: self.text,
            enabled: self.enabled,
            accelerator: self.accelerator,
            action: self.action,
            state: std::marker::PhantomData,
            checked: None,
            icon: Some(icon),
            native_icon: None,
        }
    }

    pub fn native_icon(self, native_icon: NativeIcon) -> ItemBuilder<HasNativeIcon> {
        ItemBuilder {
            id: self.id,
            text: self.text,
            enabled: self.enabled,
            accelerator: self.accelerator,
            action: self.action,
            state: std::marker::PhantomData,
            checked: None,
            icon: None,
            native_icon: Some(native_icon),
        }
    }
}

impl<State> ItemBuilder<State> {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn accelerator(mut self, accelerator: Accelerator) -> Self {
        self.accelerator = Some(accelerator);
        self
    }

    pub fn action(mut self, action: impl Fn() + 'static) -> Self {
        self.action = Some(Box::new(action));
        self
    }
}

trait BuildMenuItem {
    fn build_menu_item(&self) -> MenuItemSpec;
    fn get_id(&self) -> &MenuId;
    fn take_action(&mut self) -> Option<MenuAction>;
}

impl BuildMenuItem for ItemBuilder<NoCheckNoIcon> {
    fn build_menu_item(&self) -> MenuItemSpec {
        MenuItemSpec {
            id: self.id.clone(),
            text: self.text.clone(),
            enabled: self.enabled,
            accelerator: self.accelerator.clone(),
            kind: MenuItemKind::Plain,
        }
    }

    fn get_id(&self) -> &MenuId {
        &self.id
    }

    fn take_action(&mut self) -> Option<MenuAction> {
        self.action.take()
    }
}

impl BuildMenuItem for ItemBuilder<HasCheck> {
    fn build_menu_item(&self) -> MenuItemSpec {
        MenuItemSpec {
            id: self.id.clone(),
            text: self.text.clone(),
            enabled: self.enabled,
            accelerator: self.accelerator.clone(),
            kind: MenuItemKind::Check {
                checked: self.checked.unwrap_or(false),
            },
        }
    }

    fn get_id(&self) -> &MenuId {
        &self.id
    }

    fn take_action(&mut self) -> Option<MenuAction> {
        self.action.take()
    }
}

impl BuildMenuItem for ItemBuilder<HasIcon> {
    fn build_menu_item(&self) -> MenuItemSpec {
        MenuItemSpec {
            id: self.id.clone(),
            text: self.text.clone(),
            enabled: self.enabled,
            accelerator: self.accelerator.clone(),
            kind: MenuItemKind::Icon {
                icon: self.icon.clone().expect("icon item must contain an icon"),
            },
        }
    }

    fn get_id(&self) -> &MenuId {
        &self.id
    }

    fn take_action(&mut self) -> Option<MenuAction> {
        self.action.take()
    }
}

impl BuildMenuItem for ItemBuilder<HasNativeIcon> {
    fn build_menu_item(&self) -> MenuItemSpec {
        MenuItemSpec {
            id: self.id.clone(),
            text: self.text.clone(),
            enabled: self.enabled,
            accelerator: self.accelerator.clone(),
            kind: MenuItemKind::NativeIcon {
                native_icon: self
                    .native_icon
                    .clone()
                    .expect("native icon item must contain an icon"),
            },
        }
    }

    fn get_id(&self) -> &MenuId {
        &self.id
    }

    fn take_action(&mut self) -> Option<MenuAction> {
        self.action.take()
    }
}

#[derive(Clone, Default)]
pub struct MenuRoot;

#[derive(Clone)]
pub struct MenuSubmenu {
    text: String,
    enabled: bool,
    icon: Option<Icon>,
    native_icon: Option<NativeIcon>,
}

pub type Menu = MenuBuilder<MenuRoot, NoIcon>;
pub type SubMenu = MenuBuilder<MenuSubmenu, NoIcon>;

pub struct MenuBuilder<MenuType, IconState = NoIcon> {
    menu: MenuType,
    items: Vec<MenuEntry>,
    actions: HashMap<MenuId, MenuAction>,
    icon: std::marker::PhantomData<IconState>,
}

impl MenuBuilder<MenuRoot, NoIcon> {
    pub fn new() -> Self {
        Self {
            menu: MenuRoot,
            items: Vec::new(),
            actions: HashMap::new(),
            icon: std::marker::PhantomData,
        }
    }

    pub(crate) fn build(self) -> (MenuSpec, HashMap<MenuId, MenuAction>) {
        (MenuSpec { items: self.items }, self.actions)
    }
}

impl Default for MenuBuilder<MenuRoot, NoIcon> {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuBuilder<MenuSubmenu, NoIcon> {
    fn new_submenu(text: impl AsRef<str>, enabled: bool) -> Self {
        Self {
            menu: MenuSubmenu {
                text: text.as_ref().to_string(),
                enabled,
                icon: None,
                native_icon: None,
            },
            items: Vec::new(),
            actions: HashMap::new(),
            icon: std::marker::PhantomData,
        }
    }

    pub fn icon(mut self, icon: Icon) -> MenuBuilder<MenuSubmenu, self::HasIcon> {
        self.menu.icon = Some(icon);
        MenuBuilder {
            menu: self.menu,
            items: self.items,
            actions: self.actions,
            icon: std::marker::PhantomData,
        }
    }

    pub fn native_icon(
        mut self,
        native_icon: NativeIcon,
    ) -> MenuBuilder<MenuSubmenu, self::HasNativeIcon> {
        self.menu.native_icon = Some(native_icon);
        MenuBuilder {
            menu: self.menu,
            items: self.items,
            actions: self.actions,
            icon: std::marker::PhantomData,
        }
    }
}

impl<IconState> MenuBuilder<MenuSubmenu, IconState> {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.menu.enabled = enabled;
        self
    }
}

// Common methods available on all states.
#[allow(private_bounds)]
impl<MenuType, IconState> MenuBuilder<MenuType, IconState> {
    pub fn item<P, T>(mut self, text: impl AsRef<str>, props: P) -> Self
    where
        P: FnOnce(ItemBuilder<NoCheckNoIcon>) -> T,
        T: BuildMenuItem,
    {
        let mut builder = props(ItemBuilder::new(text));
        let id = builder.get_id().clone();
        let action = builder.take_action();
        let menu_item = builder.build_menu_item();

        self.items.push(MenuEntry::Item(menu_item));

        if let Some(action) = action {
            self.actions.insert(id, action);
        }

        self
    }

    pub fn separator(mut self) -> Self {
        self.items
            .push(MenuEntry::Predefined(PredefinedMenuItem::separator()));
        self
    }

    pub fn predefined(mut self, predefined: &PredefinedMenuItem) -> Self {
        self.items.push(MenuEntry::Predefined(predefined.clone()));
        self
    }

    pub fn submenu<I: Sized>(
        mut self,
        text: impl AsRef<str>,
        menu_builder: impl FnOnce(SubMenu) -> MenuBuilder<MenuSubmenu, I>,
    ) -> Self {
        let submenu_builder = MenuBuilder::new_submenu(text, true);
        let built_submenu = menu_builder(submenu_builder);
        let spec = SubmenuSpec {
            text: built_submenu.menu.text,
            enabled: built_submenu.menu.enabled,
            icon: built_submenu.menu.icon,
            native_icon: built_submenu.menu.native_icon,
            items: built_submenu.items,
        };

        self.items.push(MenuEntry::Submenu(spec));
        self.actions.extend(built_submenu.actions);

        self
    }
}

#[cfg(not(target_arch = "wasm32"))]
trait MudaAppend {
    fn append_menu_item(&self, item: &dyn muda::IsMenuItem);
}

#[cfg(not(target_arch = "wasm32"))]
impl MudaAppend for muda::Menu {
    fn append_menu_item(&self, item: &dyn muda::IsMenuItem) {
        let _ = self.append(item);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl MudaAppend for muda::Submenu {
    fn append_menu_item(&self, item: &dyn muda::IsMenuItem) {
        let _ = self.append(item);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn append_muda_entry(parent: &impl MudaAppend, entry: MenuEntry) {
    match entry {
        MenuEntry::Item(item) => {
            let platform_item: Box<dyn muda::IsMenuItem> = match item.kind {
                MenuItemKind::Plain => Box::new(muda::MenuItem::with_id(
                    item.id,
                    &item.text,
                    item.enabled,
                    item.accelerator,
                )),
                MenuItemKind::Check { checked } => Box::new(muda::CheckMenuItem::with_id(
                    item.id,
                    &item.text,
                    item.enabled,
                    checked,
                    item.accelerator,
                )),
                MenuItemKind::Icon { icon } => Box::new(muda::IconMenuItem::with_id(
                    item.id,
                    &item.text,
                    item.enabled,
                    Some(icon),
                    item.accelerator,
                )),
                MenuItemKind::NativeIcon { native_icon } => {
                    let item = muda::IconMenuItem::with_id(
                        item.id,
                        &item.text,
                        item.enabled,
                        None,
                        item.accelerator,
                    );
                    item.set_native_icon(Some(native_icon));
                    Box::new(item)
                }
            };
            parent.append_menu_item(platform_item.as_ref());
        }
        MenuEntry::Submenu(submenu) => {
            let platform_submenu = muda::Submenu::new(&submenu.text, submenu.enabled);
            if let Some(icon) = submenu.icon {
                platform_submenu.set_icon(Some(icon));
            }
            if let Some(native_icon) = submenu.native_icon {
                platform_submenu.set_native_icon(Some(native_icon));
            }
            for item in submenu.items {
                append_muda_entry(&platform_submenu, item);
            }
            parent.append_menu_item(&platform_submenu);
        }
        MenuEntry::Predefined(predefined) => {
            let predefined = predefined.into_muda();
            parent.append_menu_item(&predefined);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PredefinedMenuItem {
    fn into_muda(self) -> muda::PredefinedMenuItem {
        match self {
            Self::Separator => muda::PredefinedMenuItem::separator(),
            Self::Copy(text) => muda::PredefinedMenuItem::copy(text.as_deref()),
            Self::Cut(text) => muda::PredefinedMenuItem::cut(text.as_deref()),
            Self::Paste(text) => muda::PredefinedMenuItem::paste(text.as_deref()),
            Self::SelectAll(text) => muda::PredefinedMenuItem::select_all(text.as_deref()),
            Self::Undo(text) => muda::PredefinedMenuItem::undo(text.as_deref()),
            Self::Redo(text) => muda::PredefinedMenuItem::redo(text.as_deref()),
            Self::Minimize(text) => muda::PredefinedMenuItem::minimize(text.as_deref()),
            Self::Maximize(text) => muda::PredefinedMenuItem::maximize(text.as_deref()),
            Self::Fullscreen(text) => muda::PredefinedMenuItem::fullscreen(text.as_deref()),
            Self::Hide(text) => muda::PredefinedMenuItem::hide(text.as_deref()),
            Self::HideOthers(text) => muda::PredefinedMenuItem::hide_others(text.as_deref()),
            Self::ShowAll(text) => muda::PredefinedMenuItem::show_all(text.as_deref()),
            Self::CloseWindow(text) => muda::PredefinedMenuItem::close_window(text.as_deref()),
            Self::Quit(text) => muda::PredefinedMenuItem::quit(text.as_deref()),
            Self::About { text, metadata } => {
                muda::PredefinedMenuItem::about(text.as_deref(), metadata)
            }
            Self::Services(text) => muda::PredefinedMenuItem::services(text.as_deref()),
            Self::BringAllToFront(text) => {
                muda::PredefinedMenuItem::bring_all_to_front(text.as_deref())
            }
        }
    }
}
