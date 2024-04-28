use floem::{
    peniko::Color,
    reactive::create_signal,
    responsive::{range, ScreenSize},
    style::TextOverflow,
    unit::UnitExt,
    view::ViewBuilder,
    views::{h_stack, label, stack, text, Decorators},
};

fn app_view() -> impl ViewBuilder {
    let (is_text_overflown, set_is_text_overflown) = create_signal(false);

    stack({
        (
            label(|| "Resize the window to see the magic").style(|s| {
                s.border(1.0)
                    .border_radius(10.0)
                    .padding(10.0)
                    .margin_horiz(10.0)
                    .responsive(ScreenSize::XS, |s| s.background(Color::CYAN))
                    .responsive(ScreenSize::SM, |s| s.background(Color::PURPLE))
                    .responsive(ScreenSize::MD, |s| s.background(Color::ORANGE))
                    .responsive(ScreenSize::LG, |s| s.background(Color::GREEN))
                    .responsive(ScreenSize::XL, |s| s.background(Color::PINK))
                    .responsive(ScreenSize::XXL, |s| s.background(Color::RED))
                    .responsive(range(ScreenSize::XS..ScreenSize::LG), |s| {
                        s.width(90.0.pct()).max_width(500.0)
                    })
                    .responsive(
                        // equivalent to: range(ScreenSize::LG..)
                        ScreenSize::LG | ScreenSize::XL | ScreenSize::XXL,
                        |s| s.width(300.0),
                    )
            }),
            text(
                "Long text that will overflow on smaller screens since the available width is less",
            )
            .on_text_overflow(move |is_overflown| {
                set_is_text_overflown.update(|overflown| *overflown = is_overflown);
            })
            .style(move |s| {
                s.background(Color::DIM_GRAY)
                    .padding(10.0)
                    .color(Color::WHITE_SMOKE)
                    .margin_top(30.)
                    .width_pct(70.0)
                    .font_size(20.0)
                    .max_width(800.)
                    .text_overflow(TextOverflow::Ellipsis)
            }),
            h_stack((
                text("The text fits in the available width?:"),
                label(move || if is_text_overflown.get() { "No" } else { "Yes" }.to_string())
                    .style(move |s| {
                        s.color(if is_text_overflown.get() {
                            Color::RED
                        } else {
                            Color::GREEN
                        })
                        .font_bold()
                    }),
            )),
        )
    })
    .style(|s| {
        s.size(100.pct(), 100.pct())
            .flex_col()
            .justify_center()
            .items_center()
    })
}

fn main() {
    floem::launch(app_view);
}
