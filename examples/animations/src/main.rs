use floem::{
    animate::Animation,
    event::EventListener as EL,
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
                    .easing_in()
            })
            .keyframe(100, |kf| {
                kf.style(|s| s.background(Color::AQUAMARINE).size(10, 300))
                    .easing_out()
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
            }),
        empty()
            .style(|s| s.background(Color::GREEN).size(100, 300))
            .animation(move |_| {
                animation
                    .get()
                    .pause(move || pause.track())
                    .resume(move || resume.track())
                    .delay(3.seconds())
            })
            .on_event_stop(EL::PointerEnter, move |_| {
                pause.notify();
            })
            .on_event_stop(EL::PointerLeave, move |_| {
                resume.notify();
            }),
    ))
    .style(|s| s.size_full().gap(10).items_center().justify_center())
}

fn main() {
    floem::launch(app_view);
}
