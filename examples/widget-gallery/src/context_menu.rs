use floem::{
    menu::*,
    prelude::ViewTuple,
    views::{ButtonClass, Decorators},
    IntoView,
};

pub fn menu_view() -> impl IntoView {
    let export_submenu = |m: SubMenu| {
        m.item("PDF", |i| i.action(|| println!("Exporting as PDF...")))
            .item("PNG", |i| i.action(|| println!("Exporting as PNG...")))
            .item("SVG", |i| i.action(|| println!("Exporting as SVG...")))
            .separator()
            .item("HTML", |i| {
                i.enabled(false)
                    .action(|| println!("HTML export coming soon..."))
            })
    };
    let popout_menu = move || {
        Menu::new()
            .item("New Document", |i| {
                i.action(|| println!("Creating new document..."))
            })
            .item("Open Recent", |i| {
                i.action(|| println!("Opening recent files..."))
            })
            .separator()
            .submenu("Export As", export_submenu)
            .separator()
            .item("Auto Save", |i| {
                i.checked(true).action(|| println!("Toggled auto save"))
            })
            .item("Show Grid", |i| {
                i.checked(false)
                    .action(|| println!("Toggled grid visibility"))
            })
            .separator()
            .item("Preferences", |i| {
                i.action(|| println!("Opening preferences..."))
            })
    };

    let transform_submenu = |m: SubMenu| {
        m.item("Rotate 90°", |i| {
            i.action(|| println!("Rotating 90 degrees..."))
        })
        .item("Flip Horizontal", |i| {
            i.action(|| println!("Flipping horizontally..."))
        })
        .item("Flip Vertical", |i| {
            i.action(|| println!("Flipping vertically..."))
        })
        .separator()
        .item("Reset Transform", |i| {
            i.action(|| println!("Resetting transform..."))
        })
    };
    let context_menu = move || {
        Menu::new()
            .item("Cut", |i| i.action(|| println!("Cut to clipboard")))
            .item("Copy", |i| i.action(|| println!("Copied to clipboard")))
            .item("Paste", |i| {
                i.enabled(false) // Simulate empty clipboard
                    .action(|| println!("Pasted from clipboard"))
            })
            .separator()
            .submenu("Transform", transform_submenu)
            .separator()
            .item("Duplicate", |i| {
                i.action(|| println!("Creating duplicate..."))
            })
            .item("Delete", |i| i.action(|| println!("Deleting item...")))
            .separator()
            .item("Properties", |i| {
                i.action(|| println!("Opening properties panel..."))
            })
    };

    let popout_button = "Click me (Popout menu)"
        .class(ButtonClass)
        .style(|s| s.padding(10.0).margin_bottom(10.0))
        .popout_menu(popout_menu);

    let context_button = "Right click me (Context menu)"
        .class(ButtonClass)
        .style(|s| s.padding(10.0).border(1.0))
        .context_menu(context_menu);

    (popout_button, context_button)
        .v_stack()
        .style(|s| s.selectable(false))
}
