use floem::{
    app::AppContext,
    cosmic_text::{Attrs, AttrsList, FamilyOwned, TextLayout},
    peniko::Color,
    reactive::{create_signal, SignalGet, SignalUpdate},
    stack::stack,
    style::{AlignItems, Dimension, FlexDirection, Style},
    view::View,
    views::Decorators,
    views::{click, label, rich_text, scroll, VirtualListDirection, VirtualListItemSize},
    views::{list, virtual_list},
};

fn app_logic(cx: AppContext) -> impl View {
    let (couter, set_counter) = create_signal(cx.scope, 0);
    let (a, set_a) = create_signal(cx.scope, "a".to_string());
    let (b, set_b) = create_signal(cx.scope, "b".to_string());
    let (c, set_c) = create_signal(cx.scope, "b".to_string());
    let (labels, set_labels) = create_signal(cx.scope, vec![a, b, c]);

    let mut virtual_list_strings = Vec::new();
    for i in 0..10 {
        virtual_list_strings.push(i.to_string());
    }
    let (value, set_value) = create_signal(cx.scope, im::Vector::from(virtual_list_strings));

    let family = &[FamilyOwned::Name("DejaVu Sans Mono".to_string())];
    let attrs = Attrs::new().family(family).line_height(1.5);
    let mut attrs_list = AttrsList::new(attrs);
    // attrs_list.add_span(0..5, attrs.font_size(40.0));
    let mut text_layout = TextLayout::new();
    text_layout.set_text("SHget测试counter😀😃", attrs_list);

    stack(cx, move |cx| {
        (
            // label(cx, || "".to_string()).style(cx, || Style {
            //     position: Position::Absolute,
            //     margin_top: Some(50.0),
            //     height: Dimension::Points(23.242188),
            //     width: Dimension::Points(600.0),
            //     background: Some(Color::GREEN),
            //     ..Default::default()
            // }),
            // label(cx, || "".to_string()).style(cx, || Style {
            //     position: Position::Absolute,
            //     margin_top: Some(50.0 + 3.75),
            //     height: Dimension::Points(11.09375),
            //     width: Dimension::Points(600.0),
            //     background: Some(Color::GREEN),
            //     ..Default::default()
            // }),
            rich_text(cx, move || text_layout.clone()).style(cx, || {
                Style::default().margin_vert(50.0).background(Color::GRAY)
            }),
            label(cx, move || "Hi Test Test".to_string()).style(cx, || {
                Style::default().margin_vert(50.0).background(Color::GRAY)
            }),
            // label(cx, || "".to_string()).style(cx, || Style {
            //     position: Position::Absolute,
            //     margin_top: Some(50.0 + 11.71875 - 1.0),
            //     height: Dimension::Points(1.0),
            //     width: Dimension::Points(600.0),
            //     background: Some(Color::WHITE),
            //     ..Default::default()
            // }),
            // label(cx, || "".to_string()).style(cx, || Style {
            //     position: Position::Absolute,
            //     margin_top: Some(50.0 + 58.10547 - 11.71875 - 1.0),
            //     height: Dimension::Points(1.0),
            //     width: Dimension::Points(600.0),
            //     background: Some(Color::WHITE),
            //     ..Default::default()
            // }),
            // label(cx, || "".to_string()).style(cx, || Style {
            //     position: Position::Absolute,
            //     margin_top: Some(50.0 + 58.10547 - 1.0),
            //     height: Dimension::Points(1.0),
            //     width: Dimension::Points(600.0),
            //     background: Some(Color::WHITE),
            //     ..Default::default()
            // }),
            // scroll(cx, move |cx| {
            //     virtual_list(
            //         cx,
            //         VirtualListDirection::Vertical,
            //         move || value.get(),
            //         move |item| item.clone(),
            //         move |cx, item| {
            //             label(cx, move || format!("{item} {}", couter.get()))
            //                 .style(cx, || Style::default().height(Dimension::Points(20.0)))
            //         },
            //         VirtualListItemSize::Fixed(20.0),
            //     )
            //     .style(cx, || {
            //         Style::default().flex_direction(FlexDirection::Column)
            //     })
            // })
            // .style(cx, || {
            //     Style::default()
            //         .width(Dimension::Points(100.0))
            //         .flex_grow(1.0)
            //         .border(1.0)
            // }),
            scroll(cx, |cx| {
                list(
                    cx,
                    move || labels.get(),
                    move |item| item.get(),
                    move |cx, item| {
                        label(cx, move || item.get()).style(cx, || {
                            Style::default().width_pt(50.0).height_pt(30.0).border(1.0)
                        })
                    },
                )
                .style(cx, || {
                    Style::default().flex_direction(FlexDirection::Column)
                })
            })
            .style(cx, || {
                Style::default().height(Dimension::Points(30.0)).border(1.0)
            }),
            stack(cx, move |cx| {
                (
                    label(cx, move || couter.get().to_string()),
                    click(
                        cx,
                        |cx| label(cx, move || "button".to_string()),
                        move || {
                            set_counter.update(|counter| *counter += 1);
                        },
                    )
                    .style(cx, || {
                        Style::default().width_pt(50.0).height_pt(20.0).border(1.0)
                    }),
                )
            }),
            label(cx, move || couter.get().to_string()),
            click(
                cx,
                |cx| label(cx, move || "button".to_string()),
                move || {
                    set_counter.update(|counter| *counter += 1);
                },
            )
            .style(cx, || {
                Style::default()
                    .width(Dimension::Auto)
                    .height(Dimension::Auto)
                    .border(1.0)
            }),
            label(cx, move || "seprate\nseprate\nseprate\n".to_string()).style(cx, || {
                Style::default()
                    .background(Color::rgb8(180, 0, 0))
                    .border(2.0)
                    .border_radius(10.0)
            }),
            click(
                cx,
                |cx| label(cx, move || "button".to_string()),
                move || {
                    set_counter.update(|counter| *counter += 1);
                },
            )
            .style(cx, || {
                Style::default()
                    .width(Dimension::Auto)
                    .height(Dimension::Auto)
                    .border(1.0)
                    .flex_grow(2.0)
            }),
            label(cx, move || couter.get().to_string()),
            stack(cx, move |cx| {
                (
                    label(cx, move || couter.get().to_string()),
                    label(cx, move || couter.get().to_string()),
                    label(cx, move || couter.get().to_string()),
                    click(
                        cx,
                        |cx| label(cx, move || "button a".to_string()),
                        move || {
                            set_counter.update(|counter| *counter += 1);
                        },
                    )
                    .style(cx, || Style::default().border(1.0)),
                    label(cx, move || couter.get().to_string()),
                )
            }),
        )
    })
    .style(cx, || {
        Style::default()
            .width_perc(1.0)
            .height_perc(1.0)
            .flex_direction(FlexDirection::Column)
            .align_items(Some(AlignItems::Center))
            .font_family("DejaVu Sans Mono".to_string())
    })
}

fn main() {
    floem::launch(app_logic);
}
