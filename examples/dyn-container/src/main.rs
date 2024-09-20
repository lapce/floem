use floem::{
    animate::Animation,
    reactive::{create_rw_signal, RwSignal, SignalGet, SignalUpdate},
    views::*,
    IntoView,
};

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
            ViewSwitcher::One => view_one().into_any(),
            ViewSwitcher::Two => view_two(state).into_any(),
        }
        .animation(Animation::scale_effect)
        .clip()
    }
}

fn main() {
    floem::launch(app_view);
}

fn app_view() -> impl IntoView {
    let view = create_rw_signal(ViewSwitcher::One);

    v_stack((
        button("Switch views").action(move || view.update(|which| which.toggle())),
        dyn_container(move || view.get(), move |which| which.view(view))
            .style(|s| s.border(1).border_radius(5)),
    ))
    .style(|s| {
        s.width_full()
            .height_full()
            .items_center()
            .justify_center()
            .gap(20)
    })
}

fn view_one() -> impl IntoView {
    // container used to make the text clip evenly on both sides while animating
    container("A view").style(|s| s.size(100, 100).items_center().justify_center())
}

fn view_two(view: RwSignal<ViewSwitcher>) -> impl IntoView {
    v_stack((
        "Another view",
        button("Switch back").action(move || view.set(ViewSwitcher::One)),
    ))
    .style(|s| {
        s.column_gap(10.0)
            .size(150, 100)
            .items_center()
            .justify_center()
    })
}
