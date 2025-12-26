use floem::{
    peniko::color::palette,
    reactive::{RwSignal, SignalGet, SignalUpdate},
    responsive::{range, ScreenSize},
    style::TextOverflow,
    unit::UnitExt,
    views::{Decorators, Label, Stack},
    IntoView,
};

fn app_view() -> impl IntoView {
    let is_text_overflown = RwSignal::new(false);

    Stack::new({
        (
            Label::derived(|| "Resize the window to see the magic").style(|s| {
                s.border(1.0)
                    .border_radius(10.0)
                    .padding(10.0)
                    .margin_horiz(10.0)
                    .responsive(ScreenSize::XS, |s| s.background(palette::css::CYAN))
                    .responsive(ScreenSize::SM, |s| s.background(palette::css::PURPLE))
                    .responsive(ScreenSize::MD, |s| s.background(palette::css::ORANGE))
                    .responsive(ScreenSize::LG, |s| s.background(palette::css::GREEN))
                    .responsive(ScreenSize::XL, |s| s.background(palette::css::PINK))
                    .responsive(ScreenSize::XXL, |s| s.background(palette::css::RED))
                    .responsive(range(ScreenSize::XS..ScreenSize::LG), |s| {
                        s.width(90.0.pct()).max_width(500.0)
                    })
                    .responsive(
                        // equivalent to: range(ScreenSize::LG..)
                        ScreenSize::LG | ScreenSize::XL | ScreenSize::XXL,
                        |s| s.width(300.0),
                    )
            }),
            Label::new(
                "Long text that will overflow on smaller screens since the available width is less",
            )
            .on_text_overflow(move |is_overflown| {
                is_text_overflown.set(is_overflown);
            })
            .style(move |s| {
                s.background(palette::css::DIM_GRAY)
                    .padding(10.0)
                    .color(palette::css::WHITE_SMOKE)
                    .margin_top(30.)
                    .width_pct(70.0)
                    .font_size(20.0)
                    .max_width(800.)
                    .text_overflow(TextOverflow::Ellipsis)
            }),
            Stack::horizontal((
                Label::new("The text fits in the available width?:"),
                Label::derived(move || {
                    if is_text_overflown.get() { "No" } else { "Yes" }.to_string()
                })
                .style(move |s| {
                    s.color(if is_text_overflown.get() {
                        palette::css::RED
                    } else {
                        palette::css::GREEN
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
