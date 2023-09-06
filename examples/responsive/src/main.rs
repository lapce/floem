use floem::{
    peniko::Color,
    responsive::{range, ScreenSize},
    view::View,
    views::{label, stack, Decorators},
};

fn app_view() -> impl View {
    stack({
        (label(|| "Resize the window to see the magic")
            .style(|s| {
                s.border(1.0)
                    .border_radius(10.0)
                    .padding_px(10.0)
                    .margin_horiz_px(10.0)
            })
            .responsive_style(ScreenSize::XS, |s| s.background(Color::CYAN))
            .responsive_style(ScreenSize::SM, |s| s.background(Color::PURPLE))
            .responsive_style(ScreenSize::MD, |s| s.background(Color::ORANGE))
            .responsive_style(ScreenSize::LG, |s| s.background(Color::GREEN))
            .responsive_style(ScreenSize::XL, |s| s.background(Color::PINK))
            .responsive_style(ScreenSize::XXL, |s| s.background(Color::RED))
            .responsive_style(range(ScreenSize::XS..ScreenSize::LG), |s| {
                s.width_pct(90.0).max_width_px(500.0)
            })
            .responsive_style(
                // equivalent to: range(ScreenSize::LG..)
                ScreenSize::LG | ScreenSize::XL | ScreenSize::XXL,
                |s| s.width_px(300.0),
            ),)
    })
    .style(|s| {
        s.size_pct(100.0, 100.0)
            .flex_col()
            .justify_center()
            .items_center()
    })
}

fn main() {
    floem::launch(app_view);
}
