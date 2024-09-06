use floem::{
    animate::Animation,
    peniko::Color,
    reactive::{RwSignal, SignalGet, SignalUpdate},
    taffy::FlexWrap,
    views::*,
    IntoView,
};
mod music_player;

#[derive(Clone, Copy, PartialEq)]
enum ViewSwitcher {
    One,
    Two,
}
impl ViewSwitcher {
    fn toggle(&mut self) {
        *self = match self {
            ViewSwitcher::One => ViewSwitcher::Two,
            ViewSwitcher::Two => ViewSwitcher::One,
        };
    }

    fn view(&self, state: RwSignal<Self>) -> impl IntoView {
        match self {
            ViewSwitcher::One => music_player::music_player().into_any(),
            ViewSwitcher::Two => view_two(state).into_any(),
        }
        .animation(Animation::scale)
        .clip()
        .style(|s| s.padding(8))
        .animation(|a| {
            a.view_transition()
                .keyframe(0, |kf| kf.style(|s| s.padding(0)))
        })
    }
}

fn main() {
    floem::launch(app_view);
}

fn app_view() -> impl IntoView {
    let state = RwSignal::new(ViewSwitcher::One);

    v_stack((
        button(|| "Switch views").on_click_stop(move |_| {
            state.update(|which| which.toggle());
        }),
        h_stack((
            dyn_container(move || state.get(), move |which| which.view(state)).style(|s| s),
            empty()
                .animation(move |a| {
                    Animation::scale(a)
                        .run_on_remove(false)
                        .with_duration(|a, d| a.delay(d))
                        .debug_name("Take double time time while entering")
                })
                .animation(move |a| Animation::scale(a).run_on_create(false))
                .style(move |s| {
                    s.size(100, 100)
                        .border_radius(5)
                        .background(Color::RED)
                        .apply_if(state.get() == ViewSwitcher::Two, |s| s.hide())
                        .box_shadow_color(Color::BLACK.with_alpha_factor(0.7))
                        .box_shadow_h_offset(3)
                        .box_shadow_v_offset(3.)
                        .box_shadow_blur(1.5)
                }),
        ))
        .style(|s| s.items_center().justify_center().flex_wrap(FlexWrap::Wrap)),
    ))
    .style(|s| {
        s.width_full()
            .height_full()
            .items_center()
            .justify_center()
            .gap(20)
    })
}

fn view_two(view: RwSignal<ViewSwitcher>) -> impl IntoView {
    v_stack((
        "Another view",
        button(|| "Switch back").on_click_stop(move |_| {
            view.set(ViewSwitcher::One);
        }),
    ))
    .style(|s| {
        s.column_gap(10.0)
            .size(150, 100)
            .items_center()
            .justify_center()
            .border(1)
            .border_radius(5)
    })
}
