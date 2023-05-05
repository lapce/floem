use floem::{
    peniko::Color,
    reactive::{create_rw_signal, create_signal, SignalGet, SignalUpdate},
    style::{JustifyContent, Style, CursorStyle},
    view::View,
    views::{
        label, scroll, stack, text_input, virtual_list, Decorators, VirtualListDirection,
        VirtualListItemSize, tab, list, container, container_box,
    },
    AppContext, cosmic_text::{Style as FontStyle, Weight},
};

fn widg_cont_style() -> Style {
    Style::BASE
        .items_start()
        .margin_bottom(25.0)
        .dimension_pct(1.0, 1.0)
        .justify_content(Some(JustifyContent::SpaceBetween))
}

fn label_view(cx: AppContext) -> impl View {
    stack(cx, |cx| {
        (
            label(cx, move || "Label:".to_owned()).style(cx, || Style::BASE.margin_right(50.0)),
            label(cx, move || "This is a label".to_owned()),
        )
    })
    .style(cx, || widg_cont_style())
}

fn button_view(cx: AppContext) -> impl View {
    stack(cx, |cx| {
        (
            label(cx, move || "Button:".to_owned()).style(cx, || Style::BASE.margin_right(50.0)),
            label(cx, || "Click me".to_string())
                .on_click(|_| {
                    println!("Button clicked");
                    true
                })
                .style(cx, || {
                    Style::BASE.border(1.0).border_radius(10.0).padding(10.0)
                }),
        )
    })
    .style(cx, || widg_cont_style())
}

fn text_input_view(cx: AppContext) -> impl View {
    let text = create_rw_signal(cx.scope, "".to_string());

    stack(cx, |cx| {
        (
            label(cx, move || "Text input:".to_owned())
                .style(cx, || Style::BASE.margin_right(50.0)),
            text_input(cx, text)
                .style(cx, || {
                    Style::BASE
                        .border(1.5)
                        .background(Color::rgb8(224, 224, 224))
                        .border_radius(15.0)
                        .border_color(Color::rgb8(189, 189, 189))
                        .padding(10.0)
                })
                .hover_style(cx, || Style::BASE.border_color(Color::rgb8(66, 66, 66)))
                .focus_style(cx, || Style::BASE.border_color(Color::LIGHT_SKY_BLUE)),
        )
    })
    .style(cx, || widg_cont_style())
}

fn virt_list_view(cx: AppContext) -> impl View {
    let long_list: im::Vector<i32> = (0..1000000).into_iter().collect();
    let (long_list, _set_long_list) = create_signal(cx.scope, long_list);

    stack(cx, move |cx| {
        (
            label(cx, move || "List:".to_owned()).style(cx, || Style::BASE.margin_right(50.0)),
            scroll(cx, move |cx| {
                virtual_list(
                    cx,
                    VirtualListDirection::Vertical,
                    move || long_list.get(),
                    move |item| *item,
                    move |cx, item| {
                        label(cx, move || item.to_string())
                            .style(cx, || Style::BASE.height_px(20.0))
                    },
                    VirtualListItemSize::Fixed(20.0),
                )
                .style(cx, || Style::BASE.flex_col())
            })
            .style(cx, || {
                Style::BASE.width_px(100.0).height_px(100.0).border(1.0)
            }),
        )
    })
    .style(cx, || widg_cont_style())
}

fn app_view(cx: AppContext) -> impl View {
    let tabs: im::Vector<&str> = vec!["Label", "Button", "Input", "List"].into_iter().collect();
    let (tabs, _set_tabs) = create_signal(cx.scope, tabs);

    let (active_tab, set_active_tab) = create_signal(cx.scope, 0);
    stack(cx, |cx| {
        (
            container(cx, move |cx| {
                scroll(cx, move |cx| {
                    virtual_list(
                        cx,
                        VirtualListDirection::Vertical,
                        move || tabs.get(),
                        move |item| *item,
                        move |cx, item| {
                            let index = tabs.get().iter().position(|it| *it == item).unwrap();
                            container(cx, move |cx| {
                                label(cx, move || item.to_string())
                                .style(cx, || Style::BASE.font_size(24.0))
                            }) 
                            .on_click(move |_| {
                                set_active_tab.update( |v| {
                                    *v = tabs.get().iter().position(|it| *it == item).unwrap();
                                });
                                true
                            }).style(cx, move || 
                                Style::BASE.width_pct(1.0).height_px(32.0).padding(2.0).flex_row().justify_center()
                                .apply_if(index == active_tab.get(), |s| {s.background(Color::GRAY)} )
                            )
                            .hover_style(cx, || Style::BASE.background(Color::LIGHT_GRAY).cursor(CursorStyle::Pointer))
                        },
                        VirtualListItemSize::Fixed(20.0),
                    )
                    .style(cx, || Style::BASE.flex_col())
                })
                .style(cx, || {
                    Style::BASE.dimension_pct(1.0, 1.0).border(1.0).border_color(Color::GRAY)
                })
            })
            .style(cx, || {
                Style::BASE
                    .height_pct(1.0)
                    .width_px(150.0)
                    .padding_vert(5.0)
                    .padding_horiz(5.0)
                    .flex_col()
                    .items_center()
            }),
            container(cx, move |cx| {
                scroll(cx, move |cx| {
                    tab(cx, 
                        move || { active_tab.get() }, 
                        move || { tabs.get() }, 
                    |it| { *it }, 
                    |cx, it|  {
                        match it {
                            "Label" => container_box(cx, |cx| {
                                Box::new(label_view(cx))
                            }),
                            "Button" => container_box(cx, |cx| {
                                Box::new(button_view(cx))
                            }),
                            "Input" => container_box(cx, |cx| {
                                Box::new(text_input_view(cx))
                            }),
                            "List" => container_box(cx, |cx| {
                                Box::new(virt_list_view(cx))
                            }),
                            _ => container_box(cx, |cx| {
                                Box::new(label(cx, || "Not implemented".to_owned()))
                            }),
                        }
                    }).style(cx, || Style::BASE.dimension_pct(1.0, 1.0))
                })
                .style(cx, || {
                    Style::BASE.dimension_pct(1.0, 1.0).border(1.0).border_color(Color::GRAY)
                })
            })
            .style(cx, || {
                Style::BASE
                    .dimension_pct(1.0, 1.0)
                    .padding_vert(5.0)
                    .padding_horiz(5.0)
                    .flex_col()
                    .items_center()
            }),
        )
    }).style(cx, || Style::BASE.dimension_pct(1.0, 1.0))

}

fn main() {
    floem::launch(app_view);
}
