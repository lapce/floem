#![allow(unused)]

use floem::fluent::add_localizations;
use floem::fluent::get_locale_from_key;
use floem::fluent::set_default_language;
use floem::fluent::set_language;
use floem::fluent::FluentValue;
use floem::fluent::update_arg;
use floem::prelude::*;
use floem::reactive::create_updater;
use floem::style::Style;
use floem::ViewId;

fn main() {
    floem::launch(counter_view);
}

fn counter_view() -> impl IntoView {
    add_localizations(&["en-US", "pl-PL"]);
    set_default_language("en-US");
    
    let mut counter = RwSignal::new(0);

    h_stack((
        l10n("inc", None).class(ButtonClass).on_click_stop(move |_| {
            println!("Button `inc` clicked");
            counter += 1
        }),
        l10n("val", Some(vec![("counter", Box::new(move || counter.get().into()))])),
        // label(move || format!("Value: {counter}")),
        l10n("dec", None).class(ButtonClass).on_click_stop(move |_| {
            println!("Button `inc` clicked");
            counter -= 1
        }),
        button("pl").action(move || set_language("pl-PL")),
        button("en").action(move || set_language("en-US"))
    ))
    .style(|s| s.size_full().items_center().justify_center().gap(10))
}


// fn app_view() -> impl IntoView {
//     // Create bundles for all supported languages
//     // let bundles = vec![
//     //     L10nBundle::new("en-US", &[
//     //         include_str!("locales/en-US.ftl"),
//     //         include_str!("locales/common.ftl"),
//     //     ]),
//     //     L10nBundle::new("fr-FR", &[
//     //         include_str!("locales/fr-FR.ftl"),
//     //         include_str!("locales/common.ftl"),
//     //     ]),
//     //     L10nBundle::new("ja-JP", &[
//     //         include_str!("locales/ja-JP.ftl"),
//     //         include_str!("locales/common.ftl"),
//     //     ]),
//     // ];

//     v_stack((
//         header_view(),
//         content_view(),
//         footer_view(),
//     ))
//     .style(|s| s
//         // .l10n_bundles(bundles)  // Inherited property - flows down to all children
//         // .locale("en-US")        // Inherited property - flows down to all children
//         .width_full()
//         .height_full()
//     )
// }

// fn content_view() -> impl IntoView {
//     let item_count = create_rw_signal(5);
//     let user_name = create_rw_signal("Alice");
//     let user_role = create_rw_signal("Admin");

//     v_stack((
//         // Simple localized text with fallback
//         l10n("welcome.title"),
//             // .style(|s| s.fallback("Welcome")),
        
//         // With reactive arguments
//         l10n("greeting"),
//             // .arg("name", move || user_name.get())
//             // .arg("role", move || user_role.get())
//             // .style(|s| s.fallback("Hello, User!")),
        
//         // With reactive plural count
//         l10n("item-count"),
//             // .arg("count", move || item_count.get())
//             // .style(|s| s.fallback("Items")),
        
//         // Increment button to show reactive plural updates
//         button("Add Item").on_click_stop(move |_| {
//             item_count.update(|n| *n += 1);
//         }),
        
//         // With multiple reactive args
//         l10n("notification")
//             // .arg("user", move || "Bob")
//             // .arg("action", move || "commented")
//             // .style(|s| s.fallback("New notification")),
//     ))
//     .style(|s| s.gap(10.0))
// }

// fn header_view() -> impl IntoView {
//     l10n("app.title")
//         .style(|s| s
//             .font_size(24.0)
//             // .fallback("My Application")
//         )
// }

// fn footer_view() -> impl IntoView {
//     // Can override locale for a specific subtree
//     // Uses the same inherited bundles but different locale
//     l10n("footer.copyright")
//         // .arg("year", move || "2025")
//         .style(|s| s
//             // .locale("ja-JP")  // Override inherited locale
//             .font_size(12.0)
//             // .fallback("© 2025")
//         )
// }

fn l10n(label_key: &str, args: Option<Vec<(&str, Box<dyn Fn() -> FluentValue<'static>>)>>) -> L10n {
    let id = ViewId::new();
    let key2 = label_key.to_string();
    let key3 = label_key.to_string();
    let trigger = floem::fluent::get_refresh_trigger();
    
    let mut l10n = L10n {
        id,
        key: label_key.to_string(),
        updater: RwSignal::new(String::new())
    };

    let label = match args {
        Some(args) => {
            for (arg_key, value) in args {
                let k1 = label_key.to_string();
                let k2 = arg_key.to_string();
                let initial_label = create_updater(
                    move || {
                        println!("updater: l10n from: `{k1}` `{k2}`");
                        trigger.track();
                        update_arg(&k1, &k2, value())
                    },
                    move |v| {
                        l10n.updater.set(v);
                    }
                );
                l10n.updater.set(initial_label);
            }

            label(move || {
                l10n.updater.get()
            })
        },
        None => {
            label(move || {
                trigger.track();
                get_locale_from_key(&key3)
            })
        }
    };
    
    // if let Some(args) = args {
    //     let mut fluent_args = FluentArgs::new();
    //     for (k, initial) in args {
    //         fluent_args.set(k.to_string(), initial());
    //     }
        
    //     // create_effect(move |_| {
    //     //     update_arg(&key3, v());
    //     //     let new_label = get_locale_from_key(&key3);
    //     //     id.update_state(new_label);
    //     // });
    //     let initial_label = create_updater(
    //         move || {
    //             println!("effect: l10n from: `{key2}`");
    //             // trigger.track();
    //             get_locale_from_key(&key2)
    //         },
    //         move |l| id.update_state(l)
    //     );
    //     provide_args_for_key(key3.clone(), fluent_args);
    // }
    // let label = label(move || {
    //     trigger.track();
    //     personal_trigger.track();
    //     get_locale_from_key(&key3)
    // });
    // // println!("initial_label: {initial_label}");
    id.add_child(Box::new(label));
    l10n
}




// #[derive(Clone)]
pub struct L10n {
    id: ViewId,
    key: String,
    updater: RwSignal<String>
}

impl View for L10n {
    fn id(&self) -> ViewId {
        self.id
    }
    
    fn view_style(&self) -> Option<floem::style::Style> {
        Some(Style::new().apply_class(ButtonClass))
    }
        
    // fn update(&mut self, cx: &mut floem::context::UpdateCx, state: Box<dyn std::any::Any>) {
    //     // these are here to just ignore these arguments in the default case
    //     let _ = cx;
    //     if let Ok(label) = state.downcast() {
    //         println!("update_state with: {}", *label);
    //         let c = self.id.parent().unwrap();
    //         self.id.request_layout();
    //     }
    // }
}

// impl Localize for L10n {
//     fn arg(mut self, arg: impl Into<String> + 'static, val: impl Fn() -> FluentValue<'static> + 'static) -> Self {
//         let key = self.key.clone();
//         let arg = arg.into();
//         create_effect(move |_| {
//             let key = key.clone();
//             let arg = arg.clone();
//             let v = val();
//             println!("arg: {v:?}");
//             add_args(key.clone(), arg, v);
//             // get_locale_from_key(&key);
//             // self.id.request_layout();
//         });
//         // self.personal_trigger.notify();
//         self
//     }
// }