use floem::{prelude::*, style::Style, taffy::FlexWrap, IntoView};
mod music_player;

#[derive(Clone, Copy, PartialEq)]
enum ViewSwitcher {
    One,
    Two,
}
impl ViewSwitcher {
    fn toggle(&mut self) {
        *self = match self {
            Self::One => Self::Two,
            Self::Two => Self::One,
        };
    }

    fn view(self, state: RwSignal<Self>) -> impl IntoView {
        match self {
            Self::One => music_player::music_player().into_any(),
            Self::Two => view_two(state).into_any(),
        }
        .style(|s| s.scale(100.pct()))
        .animation(|a| a.scale_effect().keyframe(0, |s| s.style(|s| s.size(0, 0))))
        .clip()
        .style(|s| s.padding(20))
        .animation(|a| {
            a.view_transition()
                .keyframe(0, |f| f.style(|s| s.padding(0)))
        })
    }
}

fn main() {
    floem::launch(app_view);
}

fn app_view() -> impl IntoView {
    let state = RwSignal::new(ViewSwitcher::One);

    v_stack((
        Button::new("Switch views").action(move || state.update(ViewSwitcher::toggle)),
        h_stack((
            dyn_container(move || state.get(), move |which| which.view(state)),
            empty()
                .animation(move |a| {
                    a.scale_effect()
                        .with_duration(|a, d| a.delay(d))
                        .keyframe(0, |s| s.style(|s| s.size(0, 0)))
                })
                .style(move |s| {
                    s.size(100, 100)
                        .scale(100.pct())
                        .border_radius(5)
                        .background(palette::css::RED)
                        .apply_if(state.get() == ViewSwitcher::Two, |s| s.hide())
                        .apply(box_shadow())
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
        Button::new("Switch back").action(move || view.set(ViewSwitcher::One)),
    ))
    .style(|s| {
        s.row_gap(10.0)
            .size(150, 100)
            .items_center()
            .justify_center()
            .border(1.)
            .border_radius(5)
    })
}

fn box_shadow() -> Style {
    Style::new()
        .box_shadow_color(palette::css::BLACK.with_alpha(0.5))
        .box_shadow_h_offset(5.)
        .box_shadow_v_offset(10.)
        // .box_shadow_spread(1)
        .box_shadow_blur(1.5)
}
