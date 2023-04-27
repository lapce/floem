#![allow(unused)]
use floem::{
    AppContext,
    glazier::Cursor,
    peniko::Color,
    reactive::{create_rw_signal, create_signal, RwSignal, SignalGet, SignalUpdate},
    style::{CursorStyle, Style},
    view::View,
    views::{click, label, stack, text_input, Decorators},
};

fn app_view(cx: AppContext) -> impl View {
    let (counter, set_counter) = create_signal(cx.scope, 0);
    let text = create_rw_signal(cx.scope, "Lorem Ipsum".to_string());

    stack(cx, |cx| {
        (
            label(cx, move || "Text input".to_owned()),
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
            label(cx, move || format!("You wrote: {}", text.get())),
        )
    })
    .style(cx, || {
        Style::BASE
            .background(Color::WHITE_SMOKE)
            .dimension_pct(1.0, 1.0)
            .flex_col()
            .items_center()
            // .justify_center()
    })
}

fn main() {
    floem::launch(app_view);
}
