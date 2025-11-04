use floem::{action::inspect, prelude::*};
use localization::*;

fn main() {
    floem::launch(counter_view);
}

fn counter_view() -> impl IntoView {
    let localizations = LocaleMap::from_resources([
        ("en-US", include_str!("../locales/en-US/app.ftl")),
        ("pl-PL", include_str!("../locales/pl-PL/app.ftl")),
    ])
    .unwrap();

    let langauge = RwSignal::new(None);

    let mut counter = RwSignal::new(0);

    let lang_tabs = h_stack((
        "System Default"
            .class(TabSelectorClass)
            .style(move |s| s.apply_if(langauge.get().is_none(), |s| s.set_selected(true)))
            .action(move || langauge.set(None)),
        "pl-PL"
            .class(TabSelectorClass)
            .style(move |s| {
                s.apply_opt(langauge.get(), |s, l| {
                    s.apply_if(l == "pl-PL", |s| s.set_selected(true))
                })
            })
            .action(move || langauge.set(Some("pl-PL"))),
        "en-US"
            .class(TabSelectorClass)
            .style(move |s| {
                s.apply_opt(langauge.get(), |s, l| {
                    s.apply_if(l == "en-US", |s| s.set_selected(true))
                })
            })
            .action(move || langauge.set(Some("en-US"))),
    ))
    .style(|s| s.width_full().padding_top(30.).justify_center().gap(10.));

    let value_controls = h_stack((
        l10n("inc")
            .fallback(|| "increment")
            .button()
            .action(move || counter += 1),
        l10n("val")
            .fallback(move || format!("{counter}"))
            .arg("counter", move || counter.get()),
        l10n("dec")
            .fallback(|| "decrement")
            .button()
            .action(move || counter -= 1),
    ))
    .style(|s| s.size_full().items_center().justify_center().gap(10.));

    (lang_tabs, value_controls)
        .v_stack()
        .style(move |s| {
            s.size_full()
                .items_center()
                .justify_center()
                .custom(|ls: L10nCustomStyle| {
                    ls.bundle(localizations.clone())
                        .apply_opt(langauge.get(), |ls, locale| {
                            ls.locale(locale.parse::<LanguageIdentifier>().unwrap())
                        })
                })
        })
        .on_key_down(
            Key::Named(NamedKey::F11),
            |_| true,
            |_| {
                inspect();
            },
        )
}
