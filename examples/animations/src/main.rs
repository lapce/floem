use std::time::Duration;

use floem::{
    animate::{animation, EasingFn},
    event::EventListener,
    peniko::Color,
    reactive::{create_rw_signal, create_signal},
    style_class,
    view::ViewBuilder,
    views::{container, empty, h_stack, label, stack, static_label, text, v_stack, Decorators},
};

fn app_view() -> impl ViewBuilder {
    v_stack((progress_bar_container(), cube_container()))
}

style_class!(pub Button);
fn progress_bar_container() -> impl ViewBuilder {
    let width = 300.0;
    let anim_id = create_rw_signal(None);
    let is_stopped = create_rw_signal(false);
    let is_paused = create_rw_signal(false);

    v_stack((
        text("Progress bar"),
        container(
            empty()
                .style(|s| {
                    s.border_color(Color::DIM_GRAY)
                        .background(Color::LIME_GREEN)
                        .border_radius(3)
                        .width(0)
                        .height(20.)
                        .active(|s| s.color(Color::BLACK))
                })
                .animation(
                    animation()
                        .on_create(move |id| anim_id.update(|aid| *aid = Some(id)))
                        // Animate from 0 to 300px in 10 seconds
                        .width(move || width)
                        .easing_fn(EasingFn::Quartic)
                        .ease_in_out()
                        .duration(Duration::from_secs(10)),
                ),
        )
        .style(move |s| {
            s.width(width)
                .border(1.0)
                .border_radius(2)
                .box_shadow_blur(3.0)
                .border_color(Color::DIM_GRAY)
                .background(Color::DIM_GRAY)
                .margin_vert(10)
        }),
        h_stack((
            label(move || if is_stopped.get() { "Start" } else { "Stop" })
                .on_click_stop(move |_| {
                    let anim_id = anim_id.get().expect("id should be set in on_create");
                    let stopped = is_stopped.get();
                    if stopped {
                        anim_id.start()
                    } else {
                        anim_id.stop()
                    }
                    is_stopped.update(|val| *val = !stopped);
                    is_paused.update(|val| *val = false);
                })
                .class(Button),
            label(move || if is_paused.get() { "Resume" } else { "Pause" })
                .on_click_stop(move |_| {
                    let anim_id = anim_id.get().expect("id should be set in on_create");
                    let paused = is_paused.get();
                    if paused {
                        anim_id.resume()
                    } else {
                        anim_id.pause()
                    }
                    is_paused.update(|val| *val = !paused);
                })
                .disabled(move || is_stopped.get())
                .class(Button),
            static_label("Restart")
                .on_click_stop(move |_| {
                    let anim_id = anim_id.get().expect("id should be set in on_create");
                    anim_id.stop();
                    anim_id.start();
                    is_stopped.update(|val| *val = false);
                    is_paused.update(|val| *val = false);
                })
                .class(Button),
        )),
    ))
    .style(|s| {
        s.margin_vert(20)
            .margin_horiz(10)
            .padding(8)
            .class(Button, |s| {
                s.width(70)
                    .border(1.0)
                    .padding_left(10)
                    .border_radius(5)
                    .margin_left(5.)
                    .disabled(|s| s.background(Color::DIM_GRAY))
            })
            .width(400)
            .border(1.0)
            .border_color(Color::DIM_GRAY)
    })
}

fn cube_container() -> impl ViewBuilder {
    let (counter, set_counter) = create_signal(0.0);
    let (is_hovered, set_is_hovered) = create_signal(false);

    stack({
        (label(|| "Hover or click me!")
            .on_click_stop(move |_| {
                set_counter.update(|value| *value += 1.0);
            })
            .on_event_stop(EventListener::PointerEnter, move |_| {
                set_is_hovered.update(|val| *val = true);
            })
            .on_event_stop(EventListener::PointerLeave, move |_| {
                set_is_hovered.update(|val| *val = false);
            })
            .style(|s| {
                s.border(1.0)
                    .background(Color::RED)
                    .color(Color::BLACK)
                    .padding(10.0)
                    .margin(20.0)
                    .size(120.0, 120.0)
                    .active(|s| s.color(Color::BLACK))
            })
            .animation(
                animation()
                    //TODO:
                    // .border_radius(move || if is_hovered.get() { 1.0 } else { 40.0 })
                    .border_color(|| Color::CYAN)
                    .color(|| Color::CYAN)
                    .background(move || {
                        if is_hovered.get() {
                            Color::DEEP_PINK
                        } else {
                            Color::DARK_ORANGE
                        }
                    })
                    .easing_fn(EasingFn::Quartic)
                    .ease_in_out()
                    .duration(Duration::from_secs(1)),
            ),)
    })
    .style(|s| {
        s.border(5.0)
            .background(Color::BLUE)
            .padding(10.0)
            .size(400.0, 400.0)
            .color(Color::BLACK)
    })
    .animation(
        animation()
            .width(move || {
                if counter.get() % 2.0 == 0.0 {
                    400.0
                } else {
                    600.0
                }
            })
            .height(move || {
                if counter.get() % 2.0 == 0.0 {
                    200.0
                } else {
                    500.0
                }
            })
            .border_color(|| Color::CYAN)
            .color(|| Color::CYAN)
            .background(|| Color::LAVENDER)
            .easing_fn(EasingFn::Cubic)
            .ease_in_out()
            .auto_reverse(true)
            .duration(Duration::from_secs(2)),
    )
}

fn main() {
    floem::launch(app_view);
}
