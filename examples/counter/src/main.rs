use std::sync::Arc;

use floem::{
    app::AppContext,
    peniko::Color,
    reactive::create_signal,
    stack::stack,
    style::{AlignItems, Dimension, FlexDirection, Style},
    view::View,
    views::Decorators,
    views::{click, label, scroll, VirtualListDirection, VirtualListItemSize},
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

    stack(cx, move |cx| {
        (
            label(cx, move || couter.get().to_string()),
            scroll(cx, move |cx| {
                virtual_list(
                    cx,
                    VirtualListDirection::Vertical,
                    move || value.get(),
                    move |item| item.clone(),
                    move |cx, item| {
                        label(cx, move || format!("{item} {}", couter.get())).style(cx, || Style {
                            height: Dimension::Points(20.0),
                            ..Default::default()
                        })
                    },
                    VirtualListItemSize::Fixed(20.0),
                )
                .style(cx, || Style {
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                })
            })
            .style(cx, || Style {
                width: Dimension::Points(100.0),
                flex_grow: 1.0,
                border: 1.0,
                ..Default::default()
            }),
            scroll(cx, |cx| {
                list(
                    cx,
                    move || labels.get(),
                    move |item| item.get(),
                    move |cx, item| {
                        label(cx, move || item.get()).style(cx, || Style {
                            width: Dimension::Points(50.0),
                            height: Dimension::Points(30.0),
                            border: 1.0,
                            ..Default::default()
                        })
                    },
                )
                .style(cx, || Style {
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                })
            })
            .style(cx, || Style {
                height: Dimension::Points(30.0),
                ..Default::default()
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
                    .style(cx, || Style {
                        width: Dimension::Points(50.0),
                        height: Dimension::Points(20.0),
                        border: 1.0,
                        ..Default::default()
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
            .style(cx, || Style {
                width: Dimension::Auto,
                height: Dimension::Auto,
                border: 1.0,
                flex_grow: 1.0,
                ..Default::default()
            }),
            label(cx, move || "seprate\nseprate\nseprate\n".to_string()).style(cx, || Style {
                background: Some(Color::rgb8(180, 0, 0)),
                border: 2.0,
                border_radius: 10.0,
                ..Default::default()
            }),
            click(
                cx,
                |cx| label(cx, move || "button".to_string()),
                move || {
                    set_counter.update(|counter| *counter += 1);
                },
            )
            .style(cx, || Style {
                width: Dimension::Auto,
                height: Dimension::Auto,
                border: 1.0,
                flex_grow: 2.0,
                ..Default::default()
            }),
            label(cx, move || couter.get().to_string()),
            stack(cx, move |cx| {
                (
                    label(cx, move || couter.get().to_string()),
                    label(cx, move || couter.get().to_string()),
                    label(cx, move || couter.get().to_string()),
                    click(
                        cx,
                        |cx| label(cx, move || "button".to_string()),
                        move || {
                            set_counter.update(|counter| *counter += 1);
                        },
                    )
                    .style(cx, || Style {
                        width: Dimension::Points(50.0),
                        height: Dimension::Points(20.0),
                        border: 1.0,
                        ..Default::default()
                    }),
                    label(cx, move || couter.get().to_string()),
                )
            }),
        )
    })
    .style(cx, || Style {
        width: Dimension::Percent(1.0),
        height: Dimension::Percent(1.0),
        flex_direction: FlexDirection::Column,
        align_items: Some(AlignItems::Center),
        ..Default::default()
    })
}

fn main() {
    floem::launch(app_logic);
}
