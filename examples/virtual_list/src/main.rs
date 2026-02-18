use floem::prelude::{palette::css, *};

fn app_view() -> impl IntoView {
    VirtualStack::new(move || 1..=1000000)
        .style(|s| {
            s.flex_col().items_center().class(LabelClass, |s| {
                s.padding_vert(2.5).width_full().justify_center()
            })
        })
        .style(|s| s.border(1.0).border_color(css::BLUE))
        .scroll()
        .style(|s| s.size_pct(50., 75.).border(1.0).border_color(css::RED))
        .container()
        .style(|s| {
            s.size(100.pct(), 100.pct())
                .padding_vert(20.0)
                .flex_col()
                .items_center()
                .justify_center()
        })
        .style(|s| s.border(1.0).border_color(css::PINK))
}

fn main() {
    floem::launch(app_view);
}
