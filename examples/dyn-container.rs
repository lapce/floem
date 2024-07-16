use floem::{
    reactive::{create_rw_signal, RwSignal},
    views::{button, dyn_container, h_stack, label, v_stack, Decorators},
    IntoView,
};

#[derive(Clone, PartialEq)]
enum ViewSwitcher {
    One,
    Two,
}

fn view_one() -> impl IntoView {
    label(|| "A view")
}

fn view_two(view: RwSignal<ViewSwitcher>) -> impl IntoView {
    v_stack((
        label(|| "Another view"),
        button(|| "Switch back").on_click_stop(move |_| {
            view.set(ViewSwitcher::One);
        }),
    ))
    .style(|s| s.column_gap(10.0))
}

fn app_view() -> impl IntoView {
    let view = create_rw_signal(ViewSwitcher::One);

    v_stack((
        h_stack((
            label(|| "Swap views:").style(|s| s.padding(5)),
            button(|| "Switch views")
                .on_click_stop(move |_| {
                    if view.get() == ViewSwitcher::One {
                        view.set(ViewSwitcher::Two);
                    } else {
                        view.set(ViewSwitcher::One);
                    }
                })
                .style(|s| s.margin_bottom(20)),
        )),
        dyn_container(
            move || view.get(),
            move |view_value| match view_value {
                ViewSwitcher::One => view_one().into_any(),
                ViewSwitcher::Two => view_two(view).into_any(),
            },
        )
        .style(|s| s.padding(10).border(1)),
    ))
    .style(|s| {
        s.width_full()
            .height_full()
            .items_center()
            .justify_center()
            .row_gap(10)
    })
}

fn main() {
    floem::launch(app_view);
}
