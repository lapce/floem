use floem::{
    animate::Animation,
    peniko::Color,
    reactive::{RwSignal, SignalGet, Trigger},
    unit::DurationUnitExt,
    views::{empty, h_stack, Decorators},
    IntoView,
};

fn app_view() -> impl IntoView {
    let animation = RwSignal::new(
        Animation::new()
            .duration(5.seconds())
            .keyframe(50, |kf| {
                kf.style(|s| s.background(Color::BLACK).size(30, 30))
                    .ease_fn_sine()
                    .ease_in_out()
            })
            .keyframe(100, |kf| {
                kf.style(|s| s.background(Color::AQUAMARINE).size(10, 300))
                    .ease_fn_sine()
                    .ease_in_out()
            })
            .repeat(true)
            .auto_reverse(true),
    );

    let pause = Trigger::new();
    let resume = Trigger::new();

    h_stack((
        empty()
            .style(|s| s.background(Color::RED).size(500, 100))
            .animation(move |_| animation.get().duration(10.seconds())),
        empty()
            .style(|s| s.background(Color::BLUE).size(50, 100))
            .animation(move |_| animation.get())
            .animation(move |a| {
                a.keyframe(100, |kf| {
                    kf.style(|s| s.border(5).border_color(Color::PURPLE))
                })
                .duration(5.seconds())
                .repeat(true)
                .auto_reverse(true)
                .animate(floem::animate::Animate::ToDefault)
            }),
        empty()
            .style(|s| s.background(Color::GREEN).size(100, 300))
            .animation(move |_| {
                animation
                    .get()
                    .pause(move || pause.track())
                    .resume(move || resume.track())
            })
            .on_event_stop(floem::event::EventListener::PointerEnter, move |_| {
                pause.notify();
            })
            .on_event_stop(floem::event::EventListener::PointerLeave, move |_| {
                resume.notify();
            }),
    ))
    .style(|s| s.size_full().gap(10).items_center().justify_center())
}

fn main() {
    floem::launch(app_view);
}
