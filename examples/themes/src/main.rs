use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    peniko::Color,
    reactive::create_signal,
    style::Style,
    style_class,
    view::View,
    views::{label, stack, text, Decorators},
};

style_class!(pub Button);
style_class!(pub Label);
style_class!(pub Frame);

fn app_view() -> impl View {
    let blue_button = Style::new()
        .background(Color::rgb8(137, 145, 160))
        .color(Color::WHITE)
        .border(1.0)
        .border_color(Color::rgb8(109, 121, 135))
        .hover(|s| s.background(Color::rgb8(170, 175, 187)))
        .disabled(|s| {
            s.background(Color::DARK_GRAY.with_alpha_factor(0.1))
                .border_color(Color::BLACK.with_alpha_factor(0.2))
        })
        .active(|s| s.background(Color::BLACK.with_alpha_factor(0.4)))
        .padding(5.0)
        .margin(3.0)
        .border_radius(5.0);
    let blue_theme = Style::new()
        .background(Color::rgb8(95, 102, 118))
        .color(Color::WHITE)
        .class(Button, move |_| blue_button)
        .class(Label, |s| s.margin(4.0))
        .font_size(12.0);

    let green_button = Style::new()
        .background(Color::rgb8(180, 188, 175))
        .disabled(|s| {
            s.background(Color::rgb8(180, 188, 175).with_alpha_factor(0.3))
                .border_color(Color::rgb8(131, 145, 123).with_alpha_factor(0.3))
        })
        .active(|s| s.background(Color::rgb8(95, 105, 88)).color(Color::WHITE))
        .color(Color::BLACK.with_alpha_factor(0.7))
        .border(2.0)
        .border_color(Color::rgb8(131, 145, 123))
        .hover(|s| s.background(Color::rgb8(204, 209, 201)))
        .padding(8.0)
        .border_radius(8.0)
        .margin(6.0);
    let green_theme = Style::new()
        .background(Color::rgb8(227, 231, 226))
        .class(Button, move |_| green_button)
        .class(Label, |s| s.margin(4.0))
        .class(Frame, |s| {
            s.border(2.0)
                .border_color(Color::rgb8(131, 145, 123).with_alpha_factor(0.2))
                .border_radius(8.0)
                .background(Color::WHITE.with_alpha_factor(0.1))
                .padding(12.0)
        })
        .color(Color::BLACK.with_alpha_factor(0.5))
        .font_size(16.0);

    let (counter, set_counter) = create_signal(0);
    let (theme, set_theme) = create_signal(true);
    let view = stack((stack((
        text("Toggle Theme")
            .class(Button)
            .on_click({
                move |_| {
                    set_theme.update(|theme| *theme = !*theme);
                    true
                }
            })
            .keyboard_navigatable(),
        stack((
            label(move || format!("Value: {}", counter.get())).class(Label),
            text("Increment")
                .class(Button)
                .on_click({
                    move |_| {
                        set_counter.update(|value| *value += 1);
                        true
                    }
                })
                .keyboard_navigatable(),
            text("Decrement")
                .class(Button)
                .on_click({
                    move |_| {
                        set_counter.update(|value| *value -= 1);
                        true
                    }
                })
                .keyboard_navigatable(),
            text("Reset to 0")
                .class(Button)
                .on_click(move |_| {
                    println!("Reset counter pressed"); // will not fire if button is disabled
                    set_counter.update(|value| *value = 0);
                    true
                })
                .disabled(move || counter.get() == 0)
                .keyboard_navigatable(),
        ))
        .class(Frame)
        .style(|s| s.items_center()),
    ))
    .style(|s| s.items_center()),))
    .style(move |_| {
        if theme.get() {
            blue_theme.clone()
        } else {
            green_theme.clone()
        }
        .width_full()
        .height_full()
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
