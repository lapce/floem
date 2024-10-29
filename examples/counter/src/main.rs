use floem::{
    keyboard::{Key, NamedKey},
    peniko::Color,
    reactive::{create_signal, SignalGet, SignalUpdate},
    unit::UnitExt,
    views::{dyn_view, Decorators, LabelClass, LabelCustomStyle},
    IntoView, View,
};

fn app_view() -> impl IntoView {
    let (counter, set_counter) = create_signal(0);
    let view = (
        dyn_view(move || format!("Value: {}", counter.get())),
        counter.style(|s| s.padding(10.0)),
        (
            "Increment"
                .style(|s| {
                    s.border_radius(10.0)
                        .padding(10.0)
                        .background(Color::WHITE)
                        .box_shadow_blur(5.0)
                        .focus_visible(|s| s.outline(2.).outline_color(Color::BLUE))
                        .hover(|s| s.background(Color::LIGHT_GREEN))
                        .active(|s| s.color(Color::WHITE).background(Color::DARK_GREEN))
                })
                .on_click_stop({
                    move |_| {
                        set_counter.update(|value| *value += 1);
                    }
                })
                .keyboard_navigable(),
            "Decrement"
                .on_click_stop({
                    move |_| {
                        set_counter.update(|value| *value -= 1);
                    }
                })
                .style(|s| {
                    s.box_shadow_blur(5.0)
                        .background(Color::WHITE)
                        .border_radius(10.0)
                        .padding(10.0)
                        .margin_left(10.0)
                        .focus_visible(|s| s.outline(2.).outline_color(Color::BLUE))
                        .hover(|s| s.background(Color::rgb8(244, 67, 54)))
                        .active(|s| s.color(Color::WHITE).background(Color::RED))
                })
                .keyboard_navigable(),
            "Reset to 0"
                .on_click_stop(move |_| {
                    println!("Reset counter pressed"); // will not fire if button is disabled
                    set_counter.update(|value| *value = 0);
                })
                .disabled(move || counter.get() == 0)
                .style(|s| {
                    s.box_shadow_blur(5.0)
                        .border_radius(10.0)
                        .padding(10.0)
                        .margin_left(10.0)
                        .background(Color::LIGHT_BLUE)
                        .focus_visible(|s| s.outline(2.).outline_color(Color::BLUE))
                        .disabled(|s| s.background(Color::LIGHT_GRAY))
                        .hover(|s| s.background(Color::LIGHT_YELLOW))
                        .active(|s| s.color(Color::WHITE).background(Color::YELLOW_GREEN))
                })
                .keyboard_navigable(),
        )
            .style(|s| {
                s.class(LabelClass, |s| {
                    s.apply(LabelCustomStyle::new().selectable(false).style())
                })
            }),
    )
        .style(|s| {
            s.size(100.pct(), 100.pct())
                .flex_col()
                .items_center()
                .justify_center()
        });

    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect(),
    )
}

fn main() {
    floem::launch(app_view);
}
