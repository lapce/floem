use floem::{
    animate::{anim, EasingFn},
    peniko::Color,
    view::View,
    views::{label, stack, Decorators},
};

fn app_view() -> impl View {
    stack({
        (label(|| "Hover me!")
            .style(|s| {
                s.border(1.0)
                    .background(Color::RED)
                    .color(Color::BLACK)
                    .padding(10.0)
                    .margin(20.0)
                    .size(120.0, 120.0)
            })
            .active_style(|s| s.color(Color::BLACK))
            .hover_style_anim(anim(0.5), |s| {
                s.blend()
                    .ease(floem::animate::EasingMode::In, EasingFn::Quadratic)
                    .background(Color::YELLOW)
            }),)
    })
    .style(|s| {
        s.border(5.0)
            .background(Color::BLUE)
            .padding(10.0)
            .size(400.0, 400.0)
            .color(Color::BLACK)
    })
}

fn main() {
    floem::launch(app_view);
}
