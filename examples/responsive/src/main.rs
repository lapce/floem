use floem::{
    peniko::Color,
    responsive::{range, ScreenSize},
    style::Style,
    view::View,
    views::{label, stack, Decorators},
    AppContext,
};

fn app_view(cx: AppContext) -> impl View {
    stack(cx, |cx| {
        (
            label(cx, || "Resize the window to see the magic".to_string())
                .style(cx, || {
                    Style::BASE
                        .border(1.0)
                        .border_radius(10.0)
                        .padding_px(10.0)
                        .margin_horiz_px(10.0)
                })
                .responsive_style(cx, ScreenSize::XS, || Style::BASE.background(Color::CYAN))
                .responsive_style(cx, ScreenSize::SM, || Style::BASE.background(Color::PURPLE))
                .responsive_style(cx, ScreenSize::MD, || Style::BASE.background(Color::ORANGE))
                .responsive_style(cx, ScreenSize::LG, || Style::BASE.background(Color::GREEN))
                .responsive_style(cx, ScreenSize::XL, || Style::BASE.background(Color::PINK))
                .responsive_style(cx, ScreenSize::XXL, || Style::BASE.background(Color::RED))
                .responsive_style(cx, range(ScreenSize::XS..ScreenSize::LG), || {
                    Style::BASE.width_pct(90.0).max_width_px(500.0)
                })
                .responsive_style(
                    cx,
                    // equivalent to: range(ScreenSize::LG..)
                    ScreenSize::LG | ScreenSize::XL | ScreenSize::XXL,
                    || Style::BASE.width_px(300.0),
                ),
        )
    })
    .style(cx, || {
        Style::BASE
            .size_pct(100.0, 100.0)
            .flex_col()
            .justify_center()
            .items_center()
    })
}

fn main() {
    floem::launch(app_view);
}
