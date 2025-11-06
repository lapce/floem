pub mod animation;
pub mod buttons;
pub mod canvas;
pub mod checkbox;
pub mod clipboard;
pub mod context_menu;
pub mod draggable;
pub mod dropdown;
pub mod dropped_file;
pub mod form;
pub mod images;
pub mod inputs;
pub mod labels;
pub mod lists;
pub mod radio_buttons;
pub mod rich_text;
pub mod slider;
pub mod tabs;
pub mod texteditor;

use floem::{
    action::{add_overlay, set_window_menu, toggle_theme},
    event::{Event, EventListener},
    kurbo::Size,
    menu::*,
    muda::{AboutMetadataBuilder, PredefinedMenuItem},
    new_window,
    prelude::*,
    style::{Background, CursorStyle, TextColor, Transition},
    theme::StyleThemeExt,
    ui_events::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
    window::{WindowConfig, WindowId},
};

fn app_view(window_id: WindowId) -> impl IntoView {
    let tabs: Vec<&'static str> = vec![
        "Label",
        "Button",
        "Input",
        "Text Editor",
        "Lists",
        "Image",
        "Dropdown",
        "Checkbox",
        "Radio",
        "Tabs",
        "Slider",
        "Canvas",
        "Menu",
        "Rich Text",
        "Clipboard",
        "Animation",
        "Draggable",
        "Dropped File",
        "Files",
    ];

    let create_view = |it: &str| {
        match it {
            "Label" => labels::label_view().into_any(),
            "Button" => buttons::button_view().into_any(),
            "Checkbox" => checkbox::checkbox_view().into_any(),
            "Radio" => radio_buttons::radio_buttons_view().into_any(),
            "Input" => inputs::text_input_view().into_any(),
            "Canvas" => canvas::canvas_view().into_any(),
            "Lists" => lists::list_view().into_any(),
            "Tabs" => tabs::tab_view().into_any(),
            "Menu" => context_menu::menu_view().into_any(),
            "Rich Text" => rich_text::rich_text_view().into_any(),
            "Image" => images::img_view().into_any(),
            "Clipboard" => clipboard::clipboard_view().into_any(),
            "Slider" => slider::slider_view().into_any(),
            "Dropdown" => dropdown::dropdown_view().into_any(),
            "Animation" => animation::animation_view().into_any(),
            "Draggable" => draggable::draggable_view().into_any(),
            "Dropped File" => dropped_file::dropped_file_view().into_any(),
            #[cfg(feature = "full")]
            "Files" => files::files_view().into_any(),
            "Text Editor" => texteditor::editor_view().into_any(),
            _ => label(|| "Not implemented".to_owned()).into_any(),
        }
        .debug_name(it.to_string())
    };

    let tabs = RwSignal::new(tabs);

    let side_bar_list = tabs
        .get()
        .into_iter()
        .map(move |item| {
            item.debug_name(item).style(move |s| {
                s.flex_row()
                    .font_size(18.)
                    .height(36.0)
                    .transition(Background, Transition::ease_in_out(100.millis()))
                    .active(|s| {
                        s.with_theme(|s, t| {
                            s.background(t.primary())
                                .hover(|s| s.background(t.primary_muted()))
                                .border_radius(t.border_radius())
                        })
                    })
                    .hover(|s| s.cursor(CursorStyle::Pointer))
            })
        })
        .list()
        .style(|s| s.flex_col().width(140.0).flex_grow(1.));

    let active_tab = side_bar_list.selection();

    let side_tab_bar = side_bar_list
        .scroll()
        .debug_name("Side Tab Bar")
        .scroll_style(|s| s.shrink_to_fit().handle_thickness(8.))
        .style(|s| {
            s.border(1.)
                .flex_col()
                .padding(3.)
                .border_color(palette::css::GRAY)
                .class(LabelClass, |s| s.selectable(false))
        });

    let inspector = button("Open Inspector").action(floem::action::inspect);

    let new_window_button = button("Open In Window").action(move || {
        let name = tabs.with(|tabs| tabs.get(active_tab.get().unwrap_or(0)).copied());
        new_window(
            move |_| {
                create_view(name.unwrap_or_default())
                    .scroll()
                    .style(|s| s.size_full())
            },
            Some(
                WindowConfig::default()
                    .size(Size::new(700.0, 400.0))
                    .title(name.unwrap_or_default()),
            ),
        );
    });

    let left_side_bar = (side_tab_bar, new_window_button, inspector)
        .v_stack()
        .debug_name("Left Side Bar")
        .style(|s| s.height_full().row_gap(5.0));

    let tab = tab(
        move || Some(active_tab.get().unwrap_or(0)),
        move || tabs.get(),
        |it| *it,
        create_view,
    )
    .debug_name("Active Tab")
    .style(|s| s.flex_col().flex_grow(1.).items_start());

    let tab = tab.scroll().style(|s| s.size_full());

    let view = (left_side_bar, tab)
        .h_stack()
        .style(|s| s.padding(5.0).width_full().height_full().col_gap(5.0))
        .window_title(|| "Widget Gallery".to_owned());

    let file_submenu = |m: SubMenu| {
        m.item("New Window", |i| {
            i.action(move || {
                new_window(app_view, None);
            })
        })
        .separator()
        .item("Close Window", |i| {
            i.action(move || {
                floem::close_window(window_id);
            })
        })
        .item("Quit Widget Gallery", |i| {
            i.action(|| {
                floem::quit_app();
            })
        })
    };

    let widget_submenu = |m: SubMenu| {
        tabs.with(|tabs| {
            tabs.iter().enumerate().fold(m, |menu, (idx, &tab)| {
                menu.item(tab, move |i| i.action(move || active_tab.set(Some(idx))))
            })
        })
    };

    let view_submenu = |m: SubMenu| {
        m.item("Inspector", |i| {
            i.action(|| {
                floem::action::inspect();
            })
        })
        .separator()
        .submenu("Navigate to Widget", widget_submenu)
        .separator()
        .item("Next Tab", |i| {
            i.action(move || {
                let current = active_tab.get().unwrap_or(0);
                let tab_count = tabs.get().len();
                active_tab.set(Some((current + 1) % tab_count));
            })
        })
        .item("Previous Tab", |i| {
            i.action(move || {
                let current = active_tab.get().unwrap_or(0);
                let tab_count = tabs.get().len();
                active_tab.set(if current == 0 {
                    Some(tab_count - 1)
                } else {
                    Some(current - 1)
                });
            })
        })
    };

    let window_submenu = |m: SubMenu| {
        m.item("Open Current Tab in New Window", |i| {
            i.action(move || {
                let name = tabs.with(|tabs| tabs.get(active_tab.get().unwrap_or(0)).copied());
                new_window(
                    move |_| {
                        create_view(name.unwrap_or_default())
                            .scroll()
                            .style(|s| s.size_full())
                    },
                    Some(
                        WindowConfig::default()
                            .size(Size::new(700.0, 400.0))
                            .title(name.unwrap_or_default()),
                    ),
                );
            })
        })
        .separator()
        .item("Show Side Panel", |i| {
            i.checked(true).action(|| {
                println!("Toggle sidebar");
            })
        })
    };

    let help_submenu = |m: SubMenu| {
        m.item("About Widget Gallery", |i| {
            i.action(|| {
                println!("Floem Widget Gallery - A showcase of UI components built with Floem");
            })
        })
        .separator()
        .item("Floem Documentation", |i| {
            i.action(|| {
                println!("Opening Floem documentation...");
            })
        })
        .item("GitHub Repository", |i| {
            i.action(|| {
                println!("Opening GitHub repository...");
            })
        })
    };
    set_window_menu(
        Menu::new()
            .submenu("File", file_submenu)
            .submenu("View", view_submenu)
            .submenu("Window", window_submenu)
            .submenu("Help", help_submenu)
            .submenu("About", |s| {
                s.predefined(&PredefinedMenuItem::about(
                    Some("widget-gallery"),
                    Some(
                        AboutMetadataBuilder::new()
                            .name(Some("widget-gallery"))
                            .license(Some("MIT"))
                            .version(Some("0.1.0"))
                            .copyright(Some("Floem Authors"))
                            .build(),
                    ),
                ))
            }),
    );

    add_overlay(svg(include_str!("../assets/floem.svg")).style(|s| {
        s.set_style_value(TextColor, floem::style::StyleValue::Unset)
            .size(50, 50)
            .absolute()
            .inset_bottom(20.)
            .inset_right(15.)
    }));

    add_overlay(
        button("toggle theme")
            .action(toggle_theme)
            .style(|s| s.absolute().inset_top(10.).inset_right(22.)),
    );

    view.on_event_stop(EventListener::KeyUp, move |e| {
        if let Event::Key(KeyboardEvent {
            state: KeyState::Up,
            key,
            modifiers,
            ..
        }) = e
        {
            if *key == Key::Named(NamedKey::F11) {
                floem::action::inspect();
            } else if *key == Key::Character("q".into()) && modifiers.contains(Modifiers::META) {
                floem::quit_app();
            } else if *key == Key::Character("w".into()) && modifiers.contains(Modifiers::META) {
                floem::close_window(window_id);
            }
        }
    })
}

fn main() {
    floem::Application::new()
        .window(app_view, Some(WindowConfig::default().size((1200., 800.))))
        .on_event(|ae| match ae {
            floem::AppEvent::WillTerminate => {
                println!("terminating");
            }
            floem::AppEvent::Reopen {
                has_visible_windows,
            } => {
                if !has_visible_windows {
                    new_window(app_view, None);
                }
            }
        })
        .run();
}
