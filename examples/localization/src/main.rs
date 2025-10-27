// #![allow(unused)]

use floem::fluent::*;
use floem::prelude::*;

fn main() {
    floem::launch(counter_view);
}

fn counter_view() -> impl IntoView {
    add_localizations(&[
        ("en-US", include_str!("../locales/en-US/app.ftl")),
        ("pl-PL", include_str!("../locales/pl-PL/app.ftl"))
    ]);
    set_default_language("en-US");
    let mut counter = RwSignal::new(0);

    v_stack((
        h_stack((
            button("pl").style(|s| s.padding_horiz(20.)).action(move || set_language("pl-PL")),
            button("en").style(|s| s.padding_horiz(20.)).action(move || set_language("en-US"))
        )).style(|s| s.width_full().padding_top(30.).justify_center().gap(10.)),
        h_stack((
            l10n("inc").class(ButtonClass).on_click_stop(move |_| counter += 1),
            l10n("val").with_arg("counter", move || counter.get().into()),
            l10n("dec").class(ButtonClass).on_click_stop(move |_| counter -= 1),
        )).style(|s| s.size_full().items_center().justify_center().gap(10.))
    )).style(|s| s.size_full().items_center().justify_center())
}


// fn l10nold(label_key: &str, args: Option<Vec<(&str, Box<dyn Fn() -> FluentValue<'static>>)>>) -> L10nold {
//     let id = ViewId::new();
//     let key2 = label_key.to_string();
//     let key3 = label_key.to_string();
//     let trigger = floem::fluent::get_refresh_trigger();
    
//     let l10n = L10nold {
//         id,
//         key: label_key.to_string(),
//         updater: RwSignal::new(String::new())
//     };

//     let label = match args {
//         Some(args) => {
//             for (arg_key, value) in args {
//                 let k1 = label_key.to_string();
//                 let k2 = arg_key.to_string();
//                 let initial_label = create_updater(
//                     move || {
//                         println!("updater: l10n from: `{k1}` `{k2}`");
//                         trigger.track();
//                         update_arg(&k1, &k2, value())
//                     },
//                     move |v| {
//                         l10n.updater.set(v);
//                     }
//                 );
//                 l10n.updater.set(initial_label);
//             }

//             label(move || {
//                 l10n.updater.get()
//             })
//         },
//         None => {
//             label(move || {
//                 trigger.track();
//                 get_locale_from_key(&key3)
//             })
//         }
//     };
//     id.add_child(Box::new(label));
//     l10n
// }