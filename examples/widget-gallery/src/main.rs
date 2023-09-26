pub mod buttons;
pub mod checkbox;
pub mod context_menu;
pub mod form;
pub mod inputs;
pub mod labels;
pub mod lists;
pub mod rich_text;

use floem::{
    event::{Event, EventListener},
    keyboard::Key,
    peniko::Color,
    reactive::create_signal,
    style::CursorStyle,
    unit::UnitExt,
    view::View,
    views::{
        container, container_box, label, scroll, stack, tab, virtual_list, Decorators,
        VirtualListDirection, VirtualListItemSize,
    },
};

fn app_view() -> impl View {
    let tabs: im::Vector<&str> = vec![
        "Label", "Button", "Checkbox", "Input", "List", "Menu", "RichText",
    ]
    .into_iter()
    .collect();
    let (tabs, _set_tabs) = create_signal(tabs);

    let (active_tab, set_active_tab) = create_signal(0);
    stack({
        (
            container({
                scroll({
                    virtual_list(
                        VirtualListDirection::Vertical,
                        VirtualListItemSize::Fixed(Box::new(|| 32.0)),
                        move || tabs.get(),
                        move |item| *item,
                        move |item| {
                            let index = tabs
                                .get_untracked()
                                .iter()
                                .position(|it| *it == item)
                                .unwrap();
                            stack((label(move || item).style(|s| s.font_size(24.0)),))
                                .on_click(move |_| {
                                    set_active_tab.update(|v: &mut usize| {
                                        *v = tabs
                                            .get_untracked()
                                            .iter()
                                            .position(|it| *it == item)
                                            .unwrap();
                                    });
                                    true
                                })
                                .on_event(EventListener::KeyDown, move |e| {
                                    if let Event::KeyDown(key_event) = e {
                                        let active = active_tab.get();
                                        match key_event.key.logical_key {
                                            Key::ArrowUp => {
                                                if active > 0 {
                                                    set_active_tab.update(|v| *v -= 1)
                                                }
                                                true
                                            }
                                            Key::ArrowDown => {
                                                if active < tabs.get().len() - 1 {
                                                    set_active_tab.update(|v| *v += 1)
                                                }
                                                true
                                            }
                                            _ => false,
                                        }
                                    } else {
                                        false
                                    }
                                })
                                .keyboard_navigatable()
                                .draggable()
                                .focus_visible_style(|s| s.border(2.).border_color(Color::BLUE))
                                .style(move |s| {
                                    s.flex_row()
                                        .width(100.pct())
                                        .height(32.0)
                                        .border_bottom(1.0)
                                        .border_color(Color::LIGHT_GRAY)
                                        .apply_if(index == active_tab.get(), |s| {
                                            s.background(Color::GRAY)
                                        })
                                })
                                .hover_style(|s| {
                                    s.background(Color::LIGHT_GRAY).cursor(CursorStyle::Pointer)
                                })
                        },
                    )
                    .style(|s| s.flex_col().width(140.0))
                })
                .style(|s| {
                    s.flex_col()
                        .width(140.0)
                        .height(100.pct())
                        .border(1.0)
                        .border_color(Color::GRAY)
                })
            })
            .style(|s| {
                s.height(100.pct())
                    .width(150.0)
                    .padding_vert(5.0)
                    .padding_horiz(5.0)
                    .flex_col()
                    .items_center()
            }),
            container({
                tab(
                    move || active_tab.get(),
                    move || tabs.get(),
                    |it| *it,
                    |it| match it {
                        "Label" => container_box(labels::label_view()),
                        "Button" => container_box(buttons::button_view()),
                        "Checkbox" => container_box(checkbox::checkbox_view()),
                        "Input" => container_box(inputs::text_input_view()),
                        "List" => container_box(lists::virt_list_view()),
                        "Menu" => container_box(context_menu::menu_view()),
                        "RichText" => container_box(rich_text::rich_text_view()),
                        _ => container_box(label(|| "Not implemented".to_owned())),
                    },
                )
                .style(|s| s.size(100.pct(), 100.pct()))
            })
            .style(|s| {
                s.size(100.pct(), 100.pct())
                    .padding_vert(5.0)
                    .padding_horiz(5.0)
                    .flex_col()
                    .items_center()
            }),
        )
    })
    .style(|s| s.size(100.pct(), 100.pct()))
}

fn main() {
    floem::launch(app_view);
    println!("Hello, world!")
}
