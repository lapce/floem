use std::collections::hash_map::Entry;
use std::rc::Rc;
use std::{cell::RefCell, collections::HashMap};

use crate::ViewId;
use crate::prelude::*;
use floem_reactive::{Scope, Trigger, create_updater};
use fluent_bundle::{FluentBundle, FluentResource};

pub use fluent_bundle::FluentArgs;
pub use fluent_bundle::types::FluentValue;

thread_local! {
    static LOCALE: Rc<Localization> = Rc::new(Localization::default());
}

pub struct Localization {
    locales: RefCell<HashMap<String, FluentBundle<FluentResource>>>,
    args: RefCell<HashMap<String, FluentArgs<'static>>>,
    os_locale: RefCell<Option<String>>,
    current: RefCell<String>,
    refresh: Trigger,
}

impl Default for Localization {
    fn default() -> Self {
        Self {
            locales: Default::default(),
            os_locale: Default::default(),
            current: Default::default(),
            refresh: {
                let cx = Scope::new();
                cx.create_trigger()
            },
            args: Default::default(),
        }
    }
}

pub fn add_localizations(locales: &[(&str, &str)]) {
    LOCALE.with(|locale| {
        let mut lock = locale.locales.borrow_mut();
        *lock = locales
            .iter()
            .map(|(ident, lan)| {
                let language = {
                    let lid = ident.parse().unwrap();
                    let mut bundle = FluentBundle::new(vec![lid]);
                    let resource = FluentResource::try_new(lan.to_string())
                        .expect("Could not parse an FTL string.");
                    bundle
                        .add_resource(resource)
                        .expect("Failed to add FTL resources to the bundle.");
                    bundle
                };
                (ident.to_string(), language)
            })
            .collect();
        *locale.os_locale.borrow_mut() = crate::fluent::get_os_language();
    });
}

pub fn set_default_language(default: &str) {
    LOCALE.with(|locale| {
        *locale.current.borrow_mut() = default.to_string();
    });
}

pub fn set_language(new: &str) {
    let trigger = LOCALE.with(|locale| {
        *locale.current.borrow_mut() = new.to_string();
        locale.refresh
    });
    trigger.notify();
}

fn get_os_language() -> Option<String> {
    // TODO: use external crate for it?
    None
}

fn get_refresh_trigger() -> Trigger {
    LOCALE.with(|l| l.refresh)
}

fn update_arg(main_key: &str, arg_key: &str, value: impl Into<FluentValue<'static>>) -> String {
    println!("update_arg for: {main_key}");
    LOCALE.with(|loc| {
        let mut locales = loc.locales.borrow_mut();
        let bundle = locales.get_mut(&*loc.current.borrow()).unwrap();

        let msg = bundle.get_message(main_key).unwrap().value().unwrap();

        let mut args_mut = loc.args.borrow_mut();
        match args_mut.entry(main_key.to_string()) {
            Entry::Occupied(mut a) => {
                let a = a.get_mut();
                a.set(arg_key.to_string(), value);
            }
            Entry::Vacant(vacant) => {
                let mut args = FluentArgs::new();
                args.set(arg_key.to_string(), value);
                vacant.insert(args);
            }
        };
        let args = args_mut.get(main_key);

        let mut errors = vec![];
        let final_msg = bundle.format_pattern(msg, args, &mut errors);
        if !errors.is_empty() {
            eprintln!("errors: {errors:#?}");
        }
        final_msg.to_string()
    })
}

fn get_locale_from_key(key: &str) -> String {
    LOCALE.with(|loc| {
        let locales = loc.locales.borrow();
        let bundle = locales.get(&*loc.current.borrow()).unwrap();
        let msg = bundle.get_message(key).unwrap().value().unwrap();
        let args = loc.args.borrow();
        let args = args.get(key);

        let mut errors = vec![];
        let s = bundle.format_pattern(msg, args, &mut errors);
        if !errors.is_empty() {
            eprintln!("errors: {errors:#?}");
        }
        s.to_string()
    })
}

pub struct L10n {
    id: ViewId,
    key: String,
    label: RwSignal<String>,
    has_args: RwSignal<bool>,
}

impl crate::View for L10n {
    fn id(&self) -> ViewId {
        self.id
    }
}

pub fn l10n(label_key: &str) -> L10n {
    let id = ViewId::new();
    let key = label_key.to_string();
    let trigger = get_refresh_trigger();

    let l10n = L10n {
        id,
        key: key.clone(),
        label: RwSignal::new(String::new()),
        has_args: RwSignal::new(false),
    };

    let label = label(move || match l10n.has_args.get() {
        true => l10n.label.get(),
        false => {
            trigger.track();
            get_locale_from_key(&key)
        }
    });

    id.add_child(Box::new(label));
    l10n
}

pub trait LocalizeWithArgs {
    fn with_arg(
        self,
        arg: impl Into<String>,
        val: impl Fn() -> FluentValue<'static> + 'static,
    ) -> Self;
}

impl LocalizeWithArgs for L10n {
    fn with_arg(
        self,
        arg: impl Into<String>,
        val: impl Fn() -> FluentValue<'static> + 'static,
    ) -> Self {
        let trigger = get_refresh_trigger();
        let k1 = self.key.clone();
        let k2 = arg.into();
        self.has_args.set(true);

        let initial_label = create_updater(
            move || {
                println!("updater: l10n from: `{k1}` `{k2}`");
                trigger.track();
                update_arg(&k1, &k2, val())
            },
            move |v| {
                self.label.set(v);
            },
        );
        self.label.set(initial_label);
        self
    }
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
