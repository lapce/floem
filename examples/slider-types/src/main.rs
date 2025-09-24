use std::ops::RangeInclusive;

use floem::{
    prelude::{SignalGet, SignalUpdate},
    reactive::create_rw_signal,
    unit::{Pct, UnitExt},
    views::{container, h_stack, label, slider, v_stack, Decorators},
    IntoView,
};

fn main() {
    floem::launch(app_view);
}

fn app_view() -> impl IntoView {
    let regular_slider_value = create_rw_signal(75.pct());
    let auto_slider_value = create_rw_signal(40.pct());

    let ranged_slider_value_1 = create_rw_signal(20.0);
    let ranged_slider_range_1 = 0.0..=100.0;

    let ranged_slider_value_2 = create_rw_signal(-25.0);
    let ranged_slider_range_2 = -50.0..=100.0;

    let regular_slider_stack = h_stack((
        regular_slider(regular_slider_value),
        label(move || format!("{:.2} %", regular_slider_value.get().0))
            .style(|s| s.font_size(18.0)),
    ))
    .style(|s| s.justify_between());

    let auto_slider_stack = h_stack((
        auto_reactive_slider(auto_slider_value),
        label(move || format!("{:.2} %", auto_slider_value.get().0)).style(|s| s.font_size(18.0)),
    ))
    .style(|s| s.justify_between());

    let ranged_slider_1_stack = h_stack((
        ranged_slider(ranged_slider_value_1, ranged_slider_range_1, 10.0),
        label(move || format!("{}", ranged_slider_value_1.get())).style(|s| s.font_size(18.0)),
    ))
    .style(|s| s.justify_between());

    let ranged_slider_2_stack = h_stack((
        ranged_slider(ranged_slider_value_2, ranged_slider_range_2, 1.0),
        label(move || format!("{}", ranged_slider_value_2.get())).style(|s| s.font_size(18.0)),
    ))
    .style(|s| s.justify_between());

    let view = v_stack((
        label(|| "Regular slider").style(|s| s.font_size(20)),
        regular_slider_stack,
        label(|| "RW slider").style(|s| s.font_size(20)),
        auto_slider_stack,
        label(|| "Ranged sliders").style(|s| s.font_size(20)),
        ranged_slider_1_stack,
        ranged_slider_2_stack,
    ))
    .style(|s| s.gap(5));

    container(view).style(|s| {
        s.size(100.pct(), 100.pct())
            .flex_col()
            .items_center()
            .justify_center()
    })
}

fn regular_slider(
    fill_percent: impl SignalGet<Pct> + SignalUpdate<Pct> + Copy + 'static,
) -> slider::Slider {
    slider::Slider::new(move || fill_percent.get())
        .on_change_pct(move |v| fill_percent.set(v))
        .style(|s| s.width(300).height(20.0))
}

fn auto_reactive_slider(
    fill_percent: impl SignalGet<Pct> + SignalUpdate<Pct> + Copy + 'static,
) -> slider::Slider {
    slider::Slider::new_rw(fill_percent).style(|s| s.width(300).height(20.0))
}

fn ranged_slider(
    value: impl SignalGet<f64> + SignalUpdate<f64> + Copy + 'static,
    range: RangeInclusive<f64>,
    step: f64,
) -> slider::Slider {
    slider::Slider::new_ranged(move || value.get(), range)
        .step(step)
        .on_change_value(move |v| value.set(v))
        .style(|s| s.width(300).height(20.0))
}
