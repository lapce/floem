use floem::{
    reactive::{create_get_update, create_rw_signal, SignalGet},
    unit::UnitExt,
    views::{label, slider, stack, text_input, Decorators},
    IntoView,
};

use crate::form::{self, form_item};

pub fn slider_view() -> impl IntoView {
    let input = create_rw_signal(String::from("50"));
    let slider_state = create_get_update(
        input,
        |val| val.parse::<f64>().unwrap_or_default().pct(),
        |val| val.0.to_string(),
    );
    form::form({
        (
            form_item("Input Control:".to_string(), 120.0, move || {
                text_input(input)
            }),
            form_item("Default Slider:".to_string(), 120.0, move || {
                stack((
                    slider::Slider::new_rw(slider_state).style(|s| s.width(200)),
                    label(move || format!("{:.1}%", slider_state.get().0)),
                ))
                .style(|s| s.gap(10))
            }),
            form_item("Unaligned Slider:".to_string(), 120.0, move || {
                stack((
                    slider::Slider::new_rw(slider_state)
                        .slider_style(|s| {
                            s.accent_bar_height(30.pct())
                                .bar_height(30.pct())
                                .edge_align(false)
                        })
                        .style(|s| s.width(200)),
                    label(move || format!("{:.1}%", slider_state.get().0)),
                ))
                .style(|s| s.gap(10))
            }),
            form_item("Progress bar:".to_string(), 120.0, move || {
                stack((
                    slider::Slider::new(move || slider_state.get())
                        .slider_style(|s| s.handle_radius(0).edge_align(true))
                        .style(|s| s.width(200))
                        .disabled(|| true),
                    label(move || format!("{:.1}%", slider_state.get().0)),
                ))
                .style(|s| s.gap(10))
            }),
        )
    })
}
