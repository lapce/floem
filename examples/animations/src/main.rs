use std::time::Duration;

use floem::{
    animate::{Animate, Animation, EasingFn, EasingMode, KeyFrame},
    kurbo::CubicBez,
    peniko::Color,
    style::Style,
    unit::DurationUnitExt,
    views::{container, empty, h_stack, Decorators},
    IntoView,
};

fn app_view() -> impl IntoView {
    let animation = Animation::new()
        .duration(5.seconds())
        .keyframe(50, |kf| {
            kf.style(|s| s.background(Color::BLACK).size(30, 30))
        })
        .keyframe(100, |kf| {
            kf.style(|s| s.background(Color::AQUAMARINE).size(10, 300))
                .ease_fn_sine()
        })
        .repeat(true)
        .auto_reverse(true);

    h_stack((
        empty()
            .style(|s| s.background(Color::RED).size(500, 100))
            .animation(animation.clone()),
        empty()
            .style(|s| s.background(Color::BLUE).size(50, 100))
            .animation(animation.clone()),
        empty()
            .style(|s| s.background(Color::GREEN).size(100, 300))
            .animation(animation),
    ))
    .style(|s| s.size_full().gap(10).items_center().justify_center())
}

fn main() {
    floem::launch(app_view);
}
