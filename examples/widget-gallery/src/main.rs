pub mod buttons;
pub mod checkbox;
pub mod form;
pub mod inputs;
pub mod labels;
pub mod lists;

use floem::{
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

fn app_view(cx: AppContext) -> impl View {
    let tabs: im::Vector<&str> = vec!["Label", "Button", "Checkbox", "Input", "List"]
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
                        VirtualListItemSize::Fixed(20.0),
                        move || tabs.get(),
                        move |item| *item,
                        move |item| {
                            let index = tabs.get().iter().position(|it| *it == item).unwrap();
                            container(move || {
                                label(move || item.to_string())
                                    .style(|| Style::BASE.font_size(24.0))
                            })
                            .on_click(move |_| {
                                set_active_tab.update(|v| {
                                    *v = tabs.get().iter().position(|it| *it == item).unwrap();
                                });
                                true
                            })
                            .keyboard_navigatable()
                            .focus_visible_style(|| {
                                Style::BASE.border(2.).border_color(Color::BLUE)
                            })
                            .style(move || {
                                Style::BASE
                                    .width_pct(100.0)
                                    .height_px(32.0)
                                    .padding_px(2.0)
                                    .flex_row()
                                    .justify_center()
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
                    .style(|| Style::BASE.flex_col())
                })
                .style(|| {
                    Style::BASE
                        .size_pct(100.0, 100.0)
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
                    |cx, it| match it {
                        "Label" => container_box(cx, |cx| Box::new(labels::label_view(cx))),
                        "Button" => container_box(cx, |cx| Box::new(buttons::button_view(cx))),
                        "Checkbox" => container_box(cx, |cx| Box::new(checkbox::checkbox_view(cx))),
                        "Input" => container_box(cx, |cx| Box::new(inputs::text_input_view(cx))),
                        "List" => container_box(cx, |cx| Box::new(lists::virt_list_view(cx))),
                        _ => container_box(cx, |cx| {
                            Box::new(label(cx, || "Not implemented".to_owned()))
                        }),
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
}
