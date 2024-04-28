use floem::{
    reactive::{create_rw_signal, RwSignal},
    view::ViewBuilder,
    views::{dyn_container, h_stack, label, v_stack, Decorators},
    widgets::button,
};

#[derive(Clone, PartialEq)]
enum ViewSwitcher {
    One,
    Two,
}

fn view_one() -> impl ViewBuilder {
    label(|| "A view")
}

fn view_two(view: RwSignal<ViewSwitcher>) -> impl ViewBuilder {
    v_stack((
        label(|| "Another view"),
        button(|| "Switch back").on_click_stop(move |_| {
            view.set(ViewSwitcher::One);
        }),
    ))
    .style(|s| s.gap(0.0, 10.0))
}

fn app_view() -> impl ViewBuilder {
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
            move |value| match value {
                ViewSwitcher::One => view_one().any(),
                ViewSwitcher::Two => view_two(view).any(),
            },
        )
        .style(|s| s.padding(10).border(1)),
    ))
    .style(|s| {
        s.width_full()
            .height_full()
            .items_center()
            .justify_center()
            .gap(10, 0)
    })
}

fn main() {
    floem::launch(app_view);
}
