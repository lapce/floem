use floem::{
    peniko::Color,
    reactive::{create_effect, create_rw_signal},
    style::Foreground,
    unit::UnitExt,
    view::View,
    views::{label, stack, text_input, Decorators},
    widgets::slider,
};

use crate::form::{self, form_item};

pub fn slider_view() -> impl View {
    let set_slider = create_rw_signal(50.);
    let input = create_rw_signal(String::from("50"));
    create_effect(move |_| {
        set_slider.set(input.get().parse::<f32>().unwrap_or_default());
    });
    form::form({
        (
            form_item("Input Control:".to_string(), 120.0, move || {
                text_input(input)
            }),
            form_item("Default Slider:".to_string(), 120.0, move || {
                stack((
                    slider::slider(move || set_slider.get())
                        .style(|s| s.width(200))
                        .on_change_pct(move |val| set_slider.set(val)),
                    label(move || format!("{:.1}%", set_slider.get())),
                ))
                .style(|s| s.gap(10., 10))
            }),
            form_item("Unaligned Slider:".to_string(), 120.0, move || {
                stack((
                    slider::slider(move || set_slider.get())
                        .style(|s| {
                            s.width(200)
                                .class(slider::AccentBarClass, |s| s.height(30.pct()))
                                .class(slider::BarClass, |s| s.height(30.pct()))
                                .set(slider::EdgeAlign, false)
                        })
                        .on_change_pct(move |val| set_slider.set(val)),
                    label(move || format!("{:.1}%", set_slider.get())),
                ))
                .style(|s| s.gap(10., 10))
            }),
            form_item("Progress bar:".to_string(), 120.0, move || {
                stack((
                    slider::slider(move || set_slider.get())
                        .style(|s| s.width(200).set(Foreground, Color::GREEN))
                        .disable_events(|| true)
                        .on_change_pct(move |val| set_slider.set(val)),
                    label(move || format!("{:.1}%", set_slider.get())),
                ))
                .style(|s| s.gap(10., 10))
            }),
        )
    })
}
