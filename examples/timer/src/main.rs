use std::time::{Duration, Instant};

use floem::{
    action::exec_after,
    reactive::{create_effect, create_rw_signal},
    unit::UnitExt,
    views::{button, container, label, slider, stack, text, v_stack, Decorators},
    IntoView,
};

fn main() {
    floem::launch(app_view);
}

fn app_view() -> impl IntoView {
    // We take maximum duration as 100s for convenience so that
    // one percent represents one second.
    let target_duration = create_rw_signal(100.0);
    let duration_slider = thin_slider(move || target_duration.get())
        .on_change_pct(move |new| target_duration.set(new));

    let elapsed_time = create_rw_signal(Duration::ZERO);
    let is_active = move || elapsed_time.get().as_secs_f32() < target_duration.get();

    let elapsed_time_label = label(move || {
        format!(
            "{:.1}s",
            if is_active() {
                elapsed_time.get().as_secs_f32()
            } else {
                target_duration.get()
            }
        )
    });

    let tick = create_rw_signal(());
    create_effect(move |_| {
        tick.track();
        let before_exec = Instant::now();

        exec_after(Duration::from_millis(100), move |_| {
            if is_active() {
                elapsed_time.update(|d| *d += before_exec.elapsed());
            }
            tick.set(());
        });
    });

    let progress = move || elapsed_time.get().as_secs_f32() / target_duration.get() * 100.0;
    let elapsed_time_bar = gauge(progress);

    let reset_button = button(|| "Reset").on_click_stop(move |_| elapsed_time.set(Duration::ZERO));

    let view = v_stack((
        stack((text("Elapsed Time: "), elapsed_time_bar)).style(|s| s.justify_between()),
        elapsed_time_label,
        stack((text("Duration: "), duration_slider)).style(|s| s.justify_between()),
        reset_button,
    ))
    .style(|s| s.gap(5));

    container(view).style(|s| {
        s.size(100.pct(), 100.pct())
            .flex_col()
            .items_center()
            .justify_center()
    })
}

/// A slider with a thin bar instead of the default thick bar.
fn thin_slider(fill_percent: impl Fn() -> f32 + 'static) -> slider::Slider {
    slider::slider(fill_percent)
        .slider_style(|s| s.accent_bar_height(30.pct()).bar_height(30.pct()))
        .style(|s| s.width(200))
}

/// A non-interactive slider that has been repurposed into a progress bar.
fn gauge(fill_percent: impl Fn() -> f32 + 'static) -> slider::Slider {
    slider::slider(fill_percent)
        .disabled(|| true)
        .slider_style(|s| {
            s.handle_radius(0)
                .bar_radius(25.pct())
                .accent_bar_radius(25.pct())
        })
        .style(|s| s.width(200))
}
