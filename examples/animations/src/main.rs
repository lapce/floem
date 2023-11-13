use std::time::Duration;

use floem::{
    animate::{animation, EasingFn},
    event::EventListener,
    peniko::Color,
    reactive::create_signal,
    view::View,
    views::{label, stack, Decorators},
};

fn app_view() -> impl View {
    let (counter, set_counter) = create_signal(0.0);
    let (is_hovered, set_is_hovered) = create_signal(false);

    stack({
        (label(|| "Hover or click me!")
            .on_click_stop(move |_| {
                set_counter.update(|value| *value += 1.0);
            })
            .on_event_stop(EventListener::PointerEnter, move |_| {
                set_is_hovered.update(|val| *val = true);
            })
            .on_event_stop(EventListener::PointerLeave, move |_| {
                set_is_hovered.update(|val| *val = false);
            })
            .style(|s| {
                s.border(1.0)
                    .background(Color::RED)
                    .color(Color::BLACK)
                    .padding(10.0)
                    .margin(20.0)
                    .size(120.0, 120.0)
                    .active(|s| s.color(Color::BLACK))
            })
            .animation(
                animation()
                    .border_radius(move || if is_hovered.get() { 1.0 } else { 40.0 })
                    .border_color(|| Color::CYAN)
                    .color(|| Color::CYAN)
                    .background(move || {
                        if is_hovered.get() {
                            Color::DEEP_PINK
                        } else {
                            Color::DARK_ORANGE
                        }
                    })
                    .easing_fn(EasingFn::Quartic)
                    .ease_in_out()
                    .duration(Duration::from_secs(1)),
            ),)
    })
    .style(|s| {
        s.border(5.0)
            .background(Color::BLUE)
            .padding(10.0)
            .size(400.0, 400.0)
            .color(Color::BLACK)
    })
    .animation(
        animation()
            .width(move || {
                if counter.get() % 2.0 == 0.0 {
                    400.0
                } else {
                    600.0
                }
            })
            .height(move || {
                if counter.get() % 2.0 == 0.0 {
                    200.0
                } else {
                    500.0
                }
            })
            .border_color(|| Color::CYAN)
            .color(|| Color::CYAN)
            .background(|| Color::LAVENDER)
            .easing_fn(EasingFn::Cubic)
            .ease_in_out()
            .auto_reverse(true)
            .duration(Duration::from_secs(2)),
    )
}

fn main() {
    floem::launch(app_view);
}
