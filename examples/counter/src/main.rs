use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    peniko::Color,
    reactive::create_signal,
    unit::UnitExt,
    view::View,
    views::{label, stack, text, Decorators},
};

fn app_view() -> impl View {
    let (counter, set_counter) = create_signal(0);
    let view = stack((
        label(move || format!("Value: {}", counter.get())).style(|s| s.padding(10.0)),
        stack((
            text("Increment")
                .style(|s| {
                    s.border_radius(10.0)
                        .padding(10.0)
                        .background(Color::WHITE)
                        .box_shadow_blur(5.0)
                        .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                        .hover(|s| s.background(Color::LIGHT_GREEN))
                        .active(|s| s.color(Color::WHITE).background(Color::DARK_GREEN))
                })
                .on_click({
                    move |_| {
                        set_counter.update(|value| *value += 1);
                        true
                    }
                })
                .keyboard_navigatable(),
            text("Decrement")
                .on_click({
                    move |_| {
                        set_counter.update(|value| *value -= 1);
                        true
                    }
                })
                .style(|s| {
                    s.box_shadow_blur(5.0)
                        .background(Color::WHITE)
                        .border_radius(10.0)
                        .padding(10.0)
                        .margin_left(10.0)
                        .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                        .hover(|s| s.background(Color::rgb8(244, 67, 54)))
                        .active(|s| s.color(Color::WHITE).background(Color::RED))
                })
                .keyboard_navigatable(),
            text("Reset to 0")
                .on_click(move |_| {
                    println!("Reset counter pressed"); // will not fire if button is disabled
                    set_counter.update(|value| *value = 0);
                    true
                })
                .disabled(move || counter.get() == 0)
                .style(|s| {
                    s.box_shadow_blur(5.0)
                        .border_radius(10.0)
                        .padding(10.0)
                        .margin_left(10.0)
                        .background(Color::LIGHT_BLUE)
                        .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                        .disabled(|s| s.background(Color::LIGHT_GRAY))
                        .hover(|s| s.background(Color::LIGHT_YELLOW))
                        .active(|s| s.color(Color::WHITE).background(Color::YELLOW_GREEN))
                })
                .keyboard_navigatable(),
        )),
    ))
    .style(|s| {
        s.size(100.pct(), 100.pct())
            .flex_col()
            .items_center()
            .justify_center()
    });

    let id = view.id();
    view.on_event(EventListener::KeyUp, move |e| {
        if let Event::KeyUp(e) = e {
            if e.key.logical_key == Key::Named(NamedKey::F11) {
                id.inspect();
            }
        }
        true
    })
}

fn main() {
    floem::launch(app_view);
}
