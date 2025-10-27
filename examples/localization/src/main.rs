// #![allow(unused)]

use floem::fluent::*;
use floem::prelude::*;

fn main() {
    floem::launch(counter_view);
}

fn counter_view() -> impl IntoView {
    add_localizations(&[
        ("en-US", include_str!("../locales/en-US/app.ftl")),
        ("pl-PL", include_str!("../locales/pl-PL/app.ftl")),
    ]);
    set_default_language("en-US");
    let mut counter = RwSignal::new(0);

    v_stack((
        h_stack((
            button("pl")
                .style(|s| s.padding_horiz(20.))
                .action(move || set_language("pl-PL")),
            button("en")
                .style(|s| s.padding_horiz(20.))
                .action(move || set_language("en-US")),
        ))
        .style(|s| s.width_full().padding_top(30.).justify_center().gap(10.)),
        h_stack((
            l10n("inc").button().action(move || counter += 1),
            l10n("val").with_arg("counter", move || counter.get().into()),
            l10n("dec").button().action(move || counter -= 1),
        ))
        .style(|s| s.size_full().items_center().justify_center().gap(10.)),
    ))
    .style(|s| s.size_full().items_center().justify_center())
}
