use floem::{
    peniko::Color,
    responsive::{range, ScreenSize},
    unit::UnitExt,
    view::View,
    views::{label, stack, Decorators},
};

fn app_view() -> impl View {
    stack({
        (label(|| "Resize the window to see the magic")
            .style(|s| {
                s.border(1.0)
                    .border_radius(10.0)
                    .padding(10.0)
                    .margin_horiz(10.0)
            })
            .responsive_style(ScreenSize::XS, |s| s.background(Color::CYAN))
            .responsive_style(ScreenSize::SM, |s| s.background(Color::PURPLE))
            .responsive_style(ScreenSize::MD, |s| s.background(Color::ORANGE))
            .responsive_style(ScreenSize::LG, |s| s.background(Color::GREEN))
            .responsive_style(ScreenSize::XL, |s| s.background(Color::PINK))
            .responsive_style(ScreenSize::XXL, |s| s.background(Color::RED))
            .responsive_style(range(ScreenSize::XS..ScreenSize::LG), |s| {
                s.width(90.0.pct()).max_width(500.0)
            })
            .responsive_style(
                // equivalent to: range(ScreenSize::LG..)
                ScreenSize::LG | ScreenSize::XL | ScreenSize::XXL,
                |s| s.width(300.0),
            ),)
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
