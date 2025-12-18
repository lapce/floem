use std::time::{Duration, Instant};

use floem::{
    action::exec_after,
    reactive::{DerivedRwSignal, Effect, RwSignal, SignalGet, SignalTrack, SignalUpdate},
    unit::{Pct, UnitExt},
    views::{slider, stack, v_stack, Button, Container, Decorators, Label},
    IntoView,
};

fn main() {
    floem::launch(app_view);
}

fn app_view() -> impl IntoView {
    // We take maximum duration as 100s for convenience so that
    // one percent represents one second.
    let target_duration = RwSignal::new(100.pct());
    let duration_slider = thin_slider(target_duration);

    let elapsed_time = RwSignal::new(Duration::ZERO);
    let is_active = move || elapsed_time.get().as_secs_f64() < target_duration.get().0;

    let elapsed_time_label = Label::derived(move || {
        format!(
            "{:.1}s",
            if is_active() {
                elapsed_time.get().as_secs_f64()
            } else {
                target_duration.get().0
            }
        )
    });

    let tick = RwSignal::new(());
    Effect::new(move |_| {
        tick.track();
        let before_exec = Instant::now();

        exec_after(Duration::from_millis(100), move |_| {
            if is_active() {
                elapsed_time.update(|d| *d += before_exec.elapsed());
            }
            tick.set(());
        });
    });

    let progress = DerivedRwSignal::new(
        target_duration,
        move |val| Pct(elapsed_time.get().as_secs_f64() / val.0 * 100.),
        |val| *val,
    );
    let elapsed_time_bar = gauge(progress);

    let reset_button = Button::new("Reset").action(move || elapsed_time.set(Duration::ZERO));

    let view = v_stack((
        stack((Label::new("Elapsed Time: "), elapsed_time_bar)).style(|s| s.justify_between()),
        elapsed_time_label,
        stack((Label::new("Duration: "), duration_slider)).style(|s| s.justify_between()),
        reset_button,
    ))
    .style(|s| s.gap(5));

    Container::new(view).style(|s| {
        s.size(100.pct(), 100.pct())
            .flex_col()
            .items_center()
            .justify_center()
    })
}

/// A slider with a thin bar instead of the default thick bar.
fn thin_slider(
    fill_percent: impl SignalGet<Pct> + SignalUpdate<Pct> + Copy + 'static,
) -> slider::Slider {
    slider::Slider::new_rw(fill_percent)
        .slider_style(|s| s.accent_bar_height(30.pct()).bar_height(30.pct()))
        .style(|s| s.width(200))
}

/// A non-interactive slider that has been repurposed into a progress bar.
fn gauge(fill_percent: impl SignalGet<Pct> + 'static) -> slider::Slider {
    slider::Slider::new(move || fill_percent.get())
        .slider_style(|s| {
            s.handle_radius(0)
                .bar_radius(25.pct())
                .accent_bar_radius(25.pct())
        })
        .style(|s| s.width(200).set_disabled(true))
}
