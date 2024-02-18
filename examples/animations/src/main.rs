use std::time::Duration;

use floem::{
    animate::{animation, EasingFn},
    event::EventListener,
    peniko::Color,
    reactive::create_signal,
    view::View,
    views::{empty, label, stack, text, v_stack, Decorators},
};

fn app_view() -> impl View {
    v_stack((progress_bar_container(), cube_container()))
}

fn progress_bar_container() -> impl View {
    let width = 300.0;
    let (anim_id, set_anim_id) = create_signal(None);
    let (is_paused, set_is_paused) = create_signal(false);

    v_stack((
        text("Progress bar"),
        empty()
            .style(|s| {
                s.border(1.0)
                    .border_color(Color::DIM_GRAY)
                    .background(Color::LIME_GREEN)
                    .border_radius(10)
                    .width(0)
                    .height(20.)
                    .margin_vert(10)
                    .active(|s| s.color(Color::BLACK))
            })
            .animation(
                //TODO: add on_update so we can track the animation state(completed/paused/running etc.)
                animation()
                    .on_create(move |id| set_anim_id.update(|aid| *aid = Some(id)))
                    // Animate from 0 to 300px in 10 seconds
                    .width(move || width)
                    .easing_fn(EasingFn::Quartic)
                    .ease_in_out()
                    .duration(Duration::from_secs(10)),
            ),
        //TODO: add restart
        label(move || if is_paused.get() { "Resume" } else { "Pause" })
            .on_click_stop(move |_| {
                if let Some(anim_id) = anim_id.get() {
                    let paused = is_paused.get();
                    if paused {
                        anim_id.resume()
                    } else {
                        anim_id.pause()
                    }
                    set_is_paused.update(|val| *val = !paused);
                }
            })
            .style(|s| s.width(70).border(1.0).padding_left(10).border_radius(5)),
    ))
    .style(|s| s.margin_bottom(80).padding_left(5))
}

fn cube_container() -> impl View {
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
