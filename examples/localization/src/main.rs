// #![allow(unused)]

use floem::action::inspect;
use floem::fluent::*;
use floem::prelude::*;

fn main() {
    floem::launch(counter_view);
}

fn counter_view() -> impl IntoView {
    let localizations = LanguageMap::from_resources([
        ("en-US", include_str!("../locales/en-US/app.ftl")),
        ("pl-PL", include_str!("../locales/pl-PL/app.ftl")),
    ])
    .unwrap();

    let langauge = RwSignal::new("en-US");

    let mut counter = RwSignal::new(0);

    v_stack((
        h_stack((
            button("pl")
                .style(|s| s.padding_horiz(20.))
                .action(move || langauge.set("pl-PL")),
            button("en")
                .style(|s| s.padding_horiz(20.))
                .action(move || langauge.set("en-US")),
        ))
        .style(|s| s.width_full().padding_top(30.).justify_center().gap(10.)),
        h_stack((
            l10n("inc").button().action(move || counter += 1),
            l10n("val").arg("counter", move || counter.get()),
            l10n("dec").button().action(move || counter -= 1),
        ))
        .style(|s| s.size_full().items_center().justify_center().gap(10.)),
    ))
    .style(move |s| {
        s.size_full()
            .items_center()
            .justify_center()
            .set(L10nBundle, localizations.clone())
            .set(
                L10nLanguage,
                langauge.get().parse::<LanguageIdentifier>().unwrap(),
            )
    })
    .on_key_down(
        floem::keyboard::Key::Named(floem::keyboard::NamedKey::F11),
        |_| true,
        |_| {
            inspect();
        },
    )
}
