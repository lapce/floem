use floem::{
    menu::{Menu, MenuItem},
    views::{label, stack, Decorators},
    IntoView,
};

pub fn menu_view() -> impl IntoView {
    stack({
        (
            label(|| "Click me (Popout menu)")
                .style(|s| s.padding(10.0).margin_bottom(10.0).border(1.0))
                .popout_menu(|| {
                    Menu::new("")
                        .entry(MenuItem::new("I am a menu item!"))
                        .separator()
                        .entry(MenuItem::new("I am another menu item"))
                }),
            label(|| "Right click me (Context menu)")
                .style(|s| s.padding(10.0).border(1.0))
                .context_menu(|| {
                    Menu::new("")
                        .entry(MenuItem::new("Menu item"))
                        .entry(MenuItem::new("Menu item with something on the\tright"))
                }),
        )
    })
    .style(|s| s.flex_col())
}
