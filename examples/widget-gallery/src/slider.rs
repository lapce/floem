use floem::{
    reactive::{create_effect, create_rw_signal},
    unit::UnitExt,
    views::{label, slider, stack, text_input, Decorators},
    IntoView,
};

use crate::form::{self, form_item};

pub fn slider_view() -> impl IntoView {
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
                .style(|s| s.gap(10))
            }),
            form_item("Unaligned Slider:".to_string(), 120.0, move || {
                stack((
                    slider::slider(move || set_slider.get())
                        .slider_style(|s| {
                            s.accent_bar_height(30.pct())
                                .bar_height(30.pct())
                                .edge_align(false)
                                .style(|s| s.width(200))
                        })
                        .on_change_pct(move |val| set_slider.set(val)),
                    label(move || format!("{:.1}%", set_slider.get())),
                ))
                .style(|s| s.gap(10))
            }),
            form_item("Progress bar:".to_string(), 120.0, move || {
                stack((
                    slider::slider(move || set_slider.get())
                        .slider_style(|s| {
                            s.handle_radius(0).edge_align(true).style(|s| s.width(200))
                        })
                        .disabled(|| true)
                        .on_change_pct(move |val| set_slider.set(val)),
                    label(move || format!("{:.1}%", set_slider.get())),
                ))
                .style(|s| s.gap(10))
            }),
        )
    })
}
