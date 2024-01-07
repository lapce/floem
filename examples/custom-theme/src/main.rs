use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    peniko::Color,
    reactive::create_signal,
    style::{Background, BorderColor, Outline, OutlineColor, Style, TextColor, Transition},
    style_class,
    view::View,
    views::{label, stack, Decorators, LabelClass},
    widgets::{button, ButtonClass, ButtonLabelClass, WindowTheme},
    window::WindowConfig,
    Application,
};

style_class!(pub Frame);

fn compile_custom_theme() -> WindowTheme {
    let green_button = Style::new()
        .background(Color::rgb8(180, 188, 175))
        .disabled(|s| {
            s.background(Color::rgb8(180, 188, 175).with_alpha_factor(0.3))
                .border_color(Color::rgb8(131, 145, 123).with_alpha_factor(0.3))
                .color(Color::GRAY)
        })
        .active(|s| s.background(Color::rgb8(95, 105, 88)).color(Color::WHITE))
        .color(Color::BLACK.with_alpha_factor(0.7))
        .border(2.0)
        .transition(TextColor, Transition::linear(0.3))
        .transition(BorderColor, Transition::linear(0.3))
        .transition(Background, Transition::linear(0.3))
        .transition(Outline, Transition::linear(0.2))
        .transition(OutlineColor, Transition::linear(0.2))
        .outline_color(Color::rgba8(131, 145, 123, 0))
        .focus_visible(|s| {
            s.outline(10.0)
                .outline_color(Color::rgb8(131, 145, 123).with_alpha_factor(0.3))
        })
        .border_color(Color::rgb8(131, 145, 123))
        .hover(|s| s.background(Color::rgb8(204, 209, 201)))
        .padding(8.0)
        .border_radius(8.0)
        .margin(6.0);

    let green_theme = Style::new()
        .background(Color::rgb8(227, 231, 226))
        .transition(Background, Transition::linear(0.5))
        .class(ButtonClass, move |_| green_button)
        .class(ButtonLabelClass, |s| s.background(Color::TRANSPARENT))
        .class(LabelClass, |s| {
            s.margin(4.0).transition(TextColor, Transition::linear(0.5))
        })
        .class(Frame, |s| {
            s.border(2.0)
                .border_color(Color::rgb8(131, 145, 123).with_alpha_factor(0.2))
                .border_radius(8.0)
                .background(Color::WHITE.with_alpha_factor(0.1))
                .padding(12.0)
        })
        .color(Color::BLACK.with_alpha_factor(0.5))
        .font_size(16.0);

    WindowTheme::default().style(green_theme)
}

fn app_view() -> impl View {
    let (counter, set_counter) = create_signal(0);
    let view = stack((stack((stack((
        label(move || format!("Value: {}", counter.get())).class(LabelClass),
        button(|| "Increment")
            .on_click_stop({
                move |_| {
                    set_counter.update(|value| *value += 1);
                }
            })
            .keyboard_navigatable(),
        button(|| "Decrement")
            .on_click_stop({
                move |_| {
                    set_counter.update(|value| *value -= 1);
                }
            })
            .keyboard_navigatable(),
        button(|| "Reset to 0")
            .on_click_stop(move |_| {
                println!("Reset counter pressed"); // will not fire if button is disabled
                set_counter.update(|value| *value = 0);
            })
            .disabled(move || counter.get() == 0)
            .keyboard_navigatable(),
    ))
    .class(Frame)
    .style(|s| s.items_center()),))
    .style(|s| s.items_center()),))
    .style(move |s| {
        s.width_full()
            .height_full()
            .flex_col()
            .items_center()
            .justify_center()
    });

    let id = view.id();
    view.on_event_stop(EventListener::KeyUp, move |e| {
        if let Event::KeyUp(e) = e {
            if e.key.logical_key == Key::Named(NamedKey::F11) {
                id.inspect();
            }
        }
    })
}

fn main() {
    Application::new()
        .window(
            |_| app_view(),
            Some(
                WindowConfig::default()
                    .title("Custom Theme Example")
                    .theme(compile_custom_theme()),
            ),
        )
        .run();
}
