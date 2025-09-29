use muda::IsMenuItem;
use muda::{
    accelerator::Accelerator, CheckMenuItem, IconMenuItem, MenuId,
    MenuItem as MudaMenuItem, NativeIcon, PredefinedMenuItem, Submenu as MudaSubmenu,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

pub use muda::Menu as MudaMenu;

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

// Type state markers for submenus
pub struct NoSubmenuIcon;
pub struct HasSubmenuIcon;
pub struct HasSubmenuNativeIcon;

pub struct SubmenuBuilder<State = NoSubmenuIcon> {
    text: String,
    enabled: bool,
    state: std::marker::PhantomData<State>,
    icon: Option<muda::Icon>,
    native_icon: Option<NativeIcon>,
}

impl SubmenuBuilder<NoSubmenuIcon> {
    fn new(text: impl AsRef<str>, enabled: bool) -> Self {
        Self {
            text: text.as_ref().to_string(),
            enabled,
            state: std::marker::PhantomData,
            icon: None,
            native_icon: None,
        }
    }

    pub fn icon(self, icon: muda::Icon) -> SubmenuBuilder<HasSubmenuIcon> {
        SubmenuBuilder {
            text: self.text,
            enabled: self.enabled,
            state: std::marker::PhantomData,
            icon: Some(icon),
            native_icon: None,
        }
    }

    pub fn native_icon(self, native_icon: NativeIcon) -> SubmenuBuilder<HasSubmenuNativeIcon> {
        SubmenuBuilder {
            text: self.text,
            enabled: self.enabled,
            state: std::marker::PhantomData,
            icon: None,
            native_icon: Some(native_icon),
        }
    }
}

// Common methods available on all submenu states
impl<State> SubmenuBuilder<State> {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

trait BuildSubmenu {
    fn build_submenu(self) -> MudaSubmenu;
}

impl BuildSubmenu for SubmenuBuilder<NoSubmenuIcon> {
    fn build_submenu(self) -> MudaSubmenu {
        MudaSubmenu::new(&self.text, self.enabled)
    }
}

impl BuildSubmenu for SubmenuBuilder<HasSubmenuIcon> {
    fn build_submenu(self) -> MudaSubmenu {
        let sub_menu = MudaSubmenu::new(&self.text, self.enabled);
        sub_menu.set_icon(self.icon);
        sub_menu
    }
}

impl BuildSubmenu for SubmenuBuilder<HasSubmenuNativeIcon> {
    fn build_submenu(self) -> MudaSubmenu {
        let sub_menu = MudaSubmenu::new(&self.text, self.enabled);
        sub_menu.set_native_icon(self.native_icon);
        sub_menu
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

pub struct MenuBuilder {
    submenu: Option<MudaSubmenu>,
    menu: MudaMenu,
    pub(crate) actions: HashMap<MenuId, MenuAction>,
}

impl MenuBuilder {
    fn new_menu() -> Self {
        Self {
            submenu: None,
            menu: MudaMenu::new(),
            actions: HashMap::new(),
        }
    }

    fn new_submenu(submenu: MudaSubmenu) -> Self {
        Self {
            submenu: Some(submenu),
            menu: MudaMenu::new(), // Dummy menu, won't be used
            actions: HashMap::new(),
        }
    }

    #[allow(private_bounds)]
    pub fn item<P, T>(self, text: impl AsRef<str>, props: P) -> Self
    where
        P: FnOnce(ItemBuilder<NoCheckNoIcon>) -> T,
        T: BuildMenuItem,
    {
        let mut builder = props(ItemBuilder::new(text));
        let id = builder.get_id().clone();
        let action = builder.take_action();
        let menu_item = builder.build_menu_item();

        if let Some(ref submenu) = self.submenu {
            let _ = submenu.append(menu_item.as_ref());
        } else {
            let _ = self.menu.append(menu_item.as_ref());
        }

        let mut actions = self.actions;
        if let Some(action) = action {
            actions.insert(id, action);
        }

        Self {
            submenu: self.submenu,
            menu: self.menu,
            actions,
        }
    }

    pub fn separator(self) -> Self {
        let separator = PredefinedMenuItem::separator();

        if let Some(ref submenu) = self.submenu {
            let _ = submenu.append(&separator);
        } else {
            let _ = self.menu.append(&separator);
        }

        self
    }

    pub fn predefined(self, predefined: &PredefinedMenuItem) -> Self {
        if let Some(ref submenu) = self.submenu {
            let _ = submenu.append(predefined);
        } else {
            let _ = self.menu.append(predefined);
        }

        self
    }

    pub fn muda(self, item: &dyn IsMenuItem) -> Self {
        if let Some(ref submenu) = self.submenu {
            let _ = submenu.append(item);
        } else {
            let _ = self.menu.append(item);
        }

        self
    }

    #[allow(private_bounds)]
    pub fn submenu<P, T>(
        self,
        text: impl AsRef<str>,
        props: P,
        menu_builder: impl FnOnce(MenuBuilder) -> MenuBuilder,
    ) -> Self
    where
        P: FnOnce(SubmenuBuilder<NoSubmenuIcon>) -> T,
        T: BuildSubmenu,
    {
        let submenu_config = props(SubmenuBuilder::new(text.as_ref(), true));
        let submenu = submenu_config.build_submenu();
        let submenu_builder = MenuBuilder::new_submenu(submenu.clone());
        let built_submenu = menu_builder(submenu_builder);

        if let Some(ref parent_submenu) = self.submenu {
            let _ = parent_submenu.append(&submenu);
        } else {
            let _ = self.menu.append(&submenu);
        }

        // Merge submenu actions into our actions
        let mut actions = self.actions;
        for (id, action) in built_submenu.actions {
            actions.insert(id, action);
        }

        Self {
            submenu: self.submenu,
            menu: self.menu,
            actions,
        }
    }

    pub(crate) fn build(self) -> (MudaMenu, HashMap<MenuId, MenuAction>) {
        (self.menu, self.actions)
    }
}

// Convenience function
pub fn menu() -> MenuBuilder {
    MenuBuilder::new_menu()
}
