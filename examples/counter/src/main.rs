use floem::{prelude::*, unit::UnitExt};

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
                        .background(palette::css::WHITE)
                        .box_shadow_blur(5.0)
                        .focusable(true)
                        .focus_visible(|s| s.outline(2.).outline_color(palette::css::BLUE))
                        .hover(|s| s.background(palette::css::LIGHT_GREEN))
                        .active(|s| {
                            s.color(palette::css::WHITE)
                                .background(palette::css::DARK_GREEN)
                        })
                })
                .on_click_stop({
                    move |_| {
                        set_counter.update(|value| *value += 1);
                    }
                }),
            "Decrement"
                .on_click_stop({
                    move |_| {
                        set_counter.update(|value| *value -= 1);
                    }
                })
                .style(|s| {
                    s.box_shadow_blur(5.0)
                        .background(palette::css::WHITE)
                        .border_radius(10.0)
                        .padding(10.0)
                        .margin_left(10.0)
                        .focusable(true)
                        .focus_visible(|s| s.outline(2.).outline_color(palette::css::BLUE))
                        .hover(|s| s.background(Color::from_rgb8(244, 67, 54)))
                        .active(|s| s.color(palette::css::WHITE).background(palette::css::RED))
                }),
            "Reset to 0"
                .on_click_stop(move |_| {
                    println!("Reset counter pressed"); // will not fire if button is disabled
                    set_counter.update(|value| *value = 0);
                })
                .style(move |s| {
                    s.box_shadow_blur(5.0)
                        .border_radius(10.0)
                        .padding(10.0)
                        .margin_left(10.0)
                        .background(palette::css::LIGHT_BLUE)
                        .focusable(true)
                        .focus_visible(|s| s.outline(2.).outline_color(palette::css::BLUE))
                        .set_disabled(counter.get() == 0)
                        .disabled(|s| s.background(palette::css::LIGHT_GRAY))
                        .hover(|s| s.background(palette::css::LIGHT_YELLOW))
                        .active(|s| {
                            s.color(palette::css::WHITE)
                                .background(palette::css::YELLOW_GREEN)
                        })
                }),
        )
            .style(|s| s.custom_style_class(|s: LabelCustomStyle| s.selectable(false))),
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
