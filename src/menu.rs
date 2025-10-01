use muda::IsMenuItem;
use muda::{
    accelerator::Accelerator, CheckMenuItem, IconMenuItem, MenuId, MenuItem as MudaMenuItem,
    NativeIcon, PredefinedMenuItem,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

pub use muda::Menu as MudaMenu;
pub use muda::Submenu as MudaSubmenu;

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

pub struct ItemBuilder<State = NoCheckNoIcon> {
    id: MenuId,
    text: String,
    enabled: bool,
    accelerator: Option<Accelerator>,
    action: Option<MenuAction>,
    state: std::marker::PhantomData<State>,
    // State-specific data
    checked: Option<bool>,
    icon: Option<muda::Icon>,
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

    pub fn icon(self, icon: muda::Icon) -> ItemBuilder<HasIcon> {
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

// Common methods available on all item states
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
    fn build_menu_item(self) -> Box<dyn muda::IsMenuItem>;
    fn get_id(&self) -> &MenuId;
    fn take_action(&mut self) -> Option<MenuAction>;
}

impl BuildMenuItem for ItemBuilder<NoCheckNoIcon> {
    fn build_menu_item(self) -> Box<dyn muda::IsMenuItem> {
        Box::new(MudaMenuItem::with_id(
            self.id.clone(),
            &self.text,
            self.enabled,
            self.accelerator,
        ))
    }

    fn get_id(&self) -> &MenuId {
        &self.id
    }

    fn take_action(&mut self) -> Option<MenuAction> {
        self.action.take()
    }
}

impl BuildMenuItem for ItemBuilder<HasCheck> {
    fn build_menu_item(self) -> Box<dyn muda::IsMenuItem> {
        Box::new(CheckMenuItem::with_id(
            self.id.clone(),
            &self.text,
            self.enabled,
            self.checked.unwrap_or(false),
            self.accelerator,
        ))
    }

    fn get_id(&self) -> &MenuId {
        &self.id
    }

    fn take_action(&mut self) -> Option<MenuAction> {
        self.action.take()
    }
}

impl BuildMenuItem for ItemBuilder<HasIcon> {
    fn build_menu_item(self) -> Box<dyn muda::IsMenuItem> {
        Box::new(IconMenuItem::with_id(
            self.id.clone(),
            &self.text,
            self.enabled,
            self.icon,
            self.accelerator,
        ))
    }

    fn get_id(&self) -> &MenuId {
        &self.id
    }

    fn take_action(&mut self) -> Option<MenuAction> {
        self.action.take()
    }
}

impl BuildMenuItem for ItemBuilder<HasNativeIcon> {
    fn build_menu_item(self) -> Box<dyn muda::IsMenuItem> {
        let item = IconMenuItem::with_id(
            self.id.clone(),
            &self.text,
            self.enabled,
            self.icon,
            self.accelerator,
        );
        item.set_native_icon(self.native_icon);
        Box::new(item)
    }

    fn get_id(&self) -> &MenuId {
        &self.id
    }

    fn take_action(&mut self) -> Option<MenuAction> {
        self.action.take()
    }
}

pub type Menu = MenuBuilder<MudaMenu, NoIcon>;
pub type SubMenu = MenuBuilder<MudaSubmenu, NoIcon>;

pub struct MenuBuilder<MenuType, Icon = NoIcon> {
    menu: MenuType,
    actions: HashMap<MenuId, MenuAction>,
    icon: std::marker::PhantomData<Icon>,
}

impl MenuBuilder<MudaMenu, NoIcon> {
    pub fn new() -> Self {
        Self {
            menu: MudaMenu::new(),
            actions: HashMap::new(),
            icon: std::marker::PhantomData,
        }
    }

    pub(crate) fn build(self) -> (MudaMenu, HashMap<MenuId, MenuAction>) {
        (self.menu, self.actions)
    }
}

impl Default for MenuBuilder<MudaMenu, NoIcon> {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuBuilder<MudaSubmenu, NoIcon> {
    fn new_submenu(text: impl AsRef<str>, enabled: bool) -> Self {
        Self {
            menu: MudaSubmenu::new(text.as_ref(), enabled),
            actions: HashMap::new(),
            icon: std::marker::PhantomData,
        }
    }

    pub fn icon(self, icon: muda::Icon) -> MenuBuilder<MudaSubmenu, self::HasIcon> {
        self.menu.set_icon(Some(icon));
        MenuBuilder {
            menu: self.menu,
            actions: self.actions,
            icon: std::marker::PhantomData,
        }
    }

    pub fn native_icon(
        self,
        native_icon: NativeIcon,
    ) -> MenuBuilder<MudaSubmenu, self::HasNativeIcon> {
        self.menu.set_native_icon(Some(native_icon));
        MenuBuilder {
            menu: self.menu,
            actions: self.actions,
            icon: std::marker::PhantomData,
        }
    }
}

impl<IconState> MenuBuilder<MudaSubmenu, IconState> {
    pub fn enabled(self, enabled: bool) -> Self {
        self.menu.set_enabled(enabled);
        self
    }
}

trait AppendItem {
    fn append_item(&self, item: &dyn IsMenuItem);
}

impl AppendItem for MudaMenu {
    fn append_item(&self, item: &dyn IsMenuItem) {
        let _ = self.append(item);
    }
}

impl AppendItem for MudaSubmenu {
    fn append_item(&self, item: &dyn IsMenuItem) {
        let _ = self.append(item);
    }
}

// Common methods available on all states
#[allow(private_bounds)]
impl<MenuType, IconState> MenuBuilder<MenuType, IconState>
where
    MenuType: AppendItem,
{
    pub fn item<P, T>(mut self, text: impl AsRef<str>, props: P) -> Self
    where
        P: FnOnce(ItemBuilder<NoCheckNoIcon>) -> T,
        T: BuildMenuItem,
    {
        let mut builder = props(ItemBuilder::new(text));
        let id = builder.get_id().clone();
        let action = builder.take_action();
        let menu_item = builder.build_menu_item();

        self.menu.append_item(menu_item.as_ref());

        if let Some(action) = action {
            self.actions.insert(id, action);
        }

        self
    }

    pub fn separator(self) -> Self {
        let separator = PredefinedMenuItem::separator();
        self.menu.append_item(&separator);
        self
    }

    pub fn predefined(self, predefined: &PredefinedMenuItem) -> Self {
        self.menu.append_item(predefined);
        self
    }

    pub fn muda(self, item: &dyn IsMenuItem) -> Self {
        self.menu.append_item(item);
        self
    }

    pub fn submenu<I: Sized>(
        mut self,
        text: impl AsRef<str>,
        menu_builder: impl FnOnce(SubMenu) -> MenuBuilder<MudaSubmenu, I>,
    ) -> Self {
        let submenu_builder = MenuBuilder::new_submenu(text, true);
        let built_submenu = menu_builder(submenu_builder);

        self.menu.append_item(&built_submenu.menu);
        self.actions.extend(built_submenu.actions);

        self
    }
}
