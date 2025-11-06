use std::time::Duration;

use floem::reactive::{create_memo, DerivedRwSignal};
use floem::unit::Pct;
use floem::{prelude::*, StreamSignal};
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
    let target_pct = RwSignal::new(100.pct());

    let stream = IntervalStream::new(tokio::time::interval(Duration::from_millis(4)));
    let now = Instant::now();
    let started_at = RwSignal::new(now);
    let current_instant = StreamSignal::with_initial(stream, Instant::now());
    let elapsed_time = create_memo(move |_| {
        current_instant
            .get()
            .duration_since(started_at.get())
            .as_secs_f64()
            .min(target_pct.get().0)
    });
    let is_active = move || elapsed_time.get() < target_pct.get().0;

    let progress = DerivedRwSignal::new(
        target_pct,
        move |val| Pct(elapsed_time.get() / val.0 * 100.),
        |val| *val,
    );

    let elapsed_time_label = label(move || {
        format!(
            "{:.1}s",
            if is_active() {
                elapsed_time.get()
            } else {
                target_pct.get().0
            }
        )
    });

    let elapsed_time_bar = gauge(progress);
    let el_label_bar = ("Elapsed Time: ", elapsed_time_bar)
        .h_stack()
        .style(|s| s.justify_between());

    let duration_slider = thin_slider(target_pct);
    let dur_label_slider = ("Duration: ", duration_slider)
        .h_stack()
        .style(|s| s.justify_between());

    let reset_button = button("Reset").action(move || started_at.set(Instant::now()));

    let view = (
        el_label_bar,
        elapsed_time_label,
        dur_label_slider,
        reset_button,
    )
        .v_stack()
        .style(|s| s.gap(5));

    view.container().style(|s| {
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
