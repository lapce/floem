pub mod buttons;
pub mod checkbox;
pub mod form;
pub mod inputs;
pub mod labels;
pub mod lists;
pub mod rich_text;

use floem::{
    event::{Event, EventListner},
    glazier::keyboard_types::Key,
    peniko::Color,
    reactive::{create_signal, SignalGet, SignalUpdate},
    style::{CursorStyle, Style},
    view::View,
    views::{
        container, container_box, label, scroll, stack, tab, virtual_list, Decorators,
        VirtualListDirection, VirtualListItemSize,
    },
    AppContext,
};

fn app_view() -> impl View {
    let cx = AppContext::get_current();
    let tabs: im::Vector<&str> = vec!["Label", "Button", "Checkbox", "Input", "List", "RichText"]
        .into_iter()
        .collect();
    let (tabs, _set_tabs) = create_signal(cx.scope, tabs);

    let (active_tab, set_active_tab) = create_signal(cx.scope, 0);
    stack(|| {
        (
            container(move || {
                scroll(move || {
                    virtual_list(
                        VirtualListDirection::Vertical,
                        VirtualListItemSize::Fixed(Box::new(|| 32.0)),
                        move || tabs.get(),
                        move |item| *item,
                        move |item| {
                            let index = tabs.get().iter().position(|it| *it == item).unwrap();
                            stack(|| {
                                (label(move || item.to_string())
                                    .style(|| Style::BASE.font_size(24.0)),)
                            })
                            .on_click(move |_| {
                                set_active_tab.update(|v: &mut usize| {
                                    *v = tabs.get().iter().position(|it| *it == item).unwrap();
                                });
                                true
                            })
                            .on_event(EventListner::KeyDown, move |e| {
                                if let Event::KeyDown(key_event) = e {
                                    let active = active_tab.get();
                                    match key_event.key {
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
                            .focus_visible_style(|| {
                                Style::BASE.border(2.).border_color(Color::BLUE)
                            })
                            .style(move || {
                                Style::BASE
                                    .flex_row()
                                    .width_pct(100.0)
                                    .height_px(32.0)
                                    .border_bottom(1.0)
                                    .border_color(Color::LIGHT_GRAY)
                                    .apply_if(index == active_tab.get(), |s| {
                                        s.background(Color::GRAY)
                                    })
                            })
                            .hover_style(|| {
                                Style::BASE
                                    .background(Color::LIGHT_GRAY)
                                    .cursor(CursorStyle::Pointer)
                            })
                        },
                    )
                    .style(|| Style::BASE.flex_col().width_px(140.0))
                })
                .style(|| {
                    Style::BASE
                        .flex_col()
                        .width_px(140.0)
                        .height_pct(100.0)
                        .border(1.0)
                        .border_color(Color::GRAY)
                })
            })
            .style(|| {
                Style::BASE
                    .height_pct(100.0)
                    .width_px(150.0)
                    .padding_vert_px(5.0)
                    .padding_horiz_px(5.0)
                    .flex_col()
                    .items_center()
            }),
            container(move || {
                tab(
                    move || active_tab.get(),
                    move || tabs.get(),
                    |it| *it,
                    |it| match it {
                        "Label" => container_box(|| Box::new(labels::label_view())),
                        "Button" => container_box(|| Box::new(buttons::button_view())),
                        "Checkbox" => container_box(|| Box::new(checkbox::checkbox_view())),
                        "Input" => container_box(|| Box::new(inputs::text_input_view())),
                        "List" => container_box(|| Box::new(lists::virt_list_view())),
                        "RichText" => container_box(|| Box::new(rich_text::rich_text_view())),
                        _ => container_box(|| Box::new(label(|| "Not implemented".to_owned()))),
                    },
                )
                .style(|| Style::BASE.size_pct(100.0, 100.0))
            })
            .style(|| {
                Style::BASE
                    .size_pct(100.0, 100.0)
                    .padding_vert_px(5.0)
                    .padding_horiz_px(5.0)
                    .flex_col()
                    .items_center()
            }),
        )
    })
    .style(|| Style::BASE.size_pct(100.0, 100.0))
}

fn main() {
    floem::launch(app_view);
    println!("Hello, world!")
}
