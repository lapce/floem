use floem::{
    menu::{Menu, MenuItem},
    style::Style,
    view::View,
    views::{label, stack, Decorators},
};

pub fn menu_view() -> impl View {
    stack(move || {
        (
            label(|| "Click me (Popout menu)".to_string())
                .base_style(|| {
                    Style::BASE
                        .padding_px(10.0)
                        .margin_bottom_px(10.0)
                        .border(1.0)
                })
                .popout_menu(|| {
                    Menu::new("")
                        .entry(MenuItem::new("I am a menu item!"))
                        .separator()
                        .entry(MenuItem::new("I am another menu item"))
                }),
            label(|| "Right click me (Context menu)".to_string())
                .base_style(|| Style::BASE.padding_px(10.0).border(1.0))
                .context_menu(|| {
                    Menu::new("")
                        .entry(MenuItem::new("Menu item"))
                        .entry(MenuItem::new("Menu item with something on the\tright"))
                }),
        )
    })
    .base_style(|| Style::BASE.flex_col())
}
