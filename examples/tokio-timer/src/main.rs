use std::time::Duration;

use floem::{
    ext_event::create_signal_from_stream,
    reactive::{create_get_update, create_rw_signal, SignalGet, SignalUpdate},
    unit::UnitExt,
    views::{button, container, label, slider, stack, text, v_stack, Decorators},
    IntoView,
};
use tokio::runtime::Runtime;
use tokio::time::Instant;
use tokio_stream::wrappers::IntervalStream;

fn main() {
    // Multi threaded runtime is required because the main thread is not a real tokio task
    let runtime = Runtime::new().expect("Could not start tokio runtime");

    // We must make it so that the main task is under the tokio runtime so that APIs like
    // tokio::spawn work
    runtime.block_on(async { tokio::task::block_in_place(|| floem::launch(app_view)) })
}

fn app_view() -> impl IntoView {
    // We take maximum duration as 100s for convenience so that
    // one percent represents one second.
    let target_duration = create_rw_signal(100.0);
    let duration_slider = thin_slider(target_duration);

    let stream = IntervalStream::new(tokio::time::interval(Duration::from_millis(100)));
    let now = Instant::now();
    let started_at = create_rw_signal(now);
    let current_instant = create_signal_from_stream(now, stream);
    let elapsed_time = move || current_instant.get().duration_since(started_at.get());
    let is_active = move || elapsed_time().as_secs_f32() < target_duration.get();

    let elapsed_time_label = label(move || {
        format!(
            "{:.1}s",
            if is_active() {
                elapsed_time().as_secs_f32()
            } else {
                target_duration.get()
            }
        )
    });

    let progress = create_get_update(
        target_duration,
        move |val| elapsed_time().as_secs_f32() / val * 100.,
        |val| *val,
    );
    let elapsed_time_bar = gauge(progress);

    let reset_button = button("Reset").action(move || started_at.set(Instant::now()));

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
fn thin_slider(
    fill_percent: impl SignalGet<f32> + SignalUpdate<f32> + Copy + 'static,
) -> slider::Slider {
    slider::Slider::new_get_update(fill_percent)
        .slider_style(|s| s.accent_bar_height(30.pct()).bar_height(30.pct()))
        .style(|s| s.width(200))
}

/// A non-interactive slider that has been repurposed into a progress bar.
fn gauge(fill_percent: impl SignalGet<f32> + Copy + 'static) -> slider::Slider {
    slider::Slider::new_get(fill_percent)
        .disabled(|| true)
        .slider_style(|s| {
            s.handle_radius(0)
                .bar_radius(25.pct())
                .accent_bar_radius(25.pct())
        })
        .style(|s| s.width(200))
}
