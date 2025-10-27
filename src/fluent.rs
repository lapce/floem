use std::collections::hash_map::Entry;
use std::rc::Rc;
use std::{cell::RefCell, collections::HashMap, fs::read_to_string, path::Path};
use floem_reactive::{Scope, Trigger};
use fluent_bundle::{FluentBundle, FluentResource};

pub use fluent_bundle::types::FluentValue;
pub use fluent_bundle::FluentArgs;


thread_local! {
    static LOCALE: Rc<Localization> = Rc::new(Localization::default());
}

// #[derive(Default)]
pub struct Localization {
    pub(crate) locales: RefCell<HashMap<String, FluentBundle<FluentResource>>>,
    pub(crate) os_locale: RefCell<Option<String>>,
    pub(crate) current: RefCell<String>,
    pub(crate) refresh: Trigger,
    pub(crate) args: RefCell<HashMap<String, FluentArgs<'static>>>
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
            args: Default::default()
        }
    }
}


pub fn add_localizations(locale_idents: &[&str]) {
    println!("add_localizations");
    LOCALE.with(|locale| {
        let mut lock = locale.locales.borrow_mut();
        *lock = locale_idents
            .into_iter()
            .filter_map(|lan| {
                let language = {
                    let lid = lan.parse().unwrap();
                    // let x = negotiate_languages();
                    let dir = std::env::current_dir().unwrap().join("examples/localization");
                    println!("dir: {}", dir.display());
                    let mut bundle = FluentBundle::new(vec!(lid));
                    let path = Path::new("locales").join(lan);
                    println!("path: {path:?}");
                    // if !path.is_dir() {
                    //     eprintln!("path is not dir");
                    //     return None;
                    // }
                    let path = dir.join(path).join("app.ftl");
                    println!("path: {path:?}");
                    // if !path.exists() {
                    //     eprintln!("path does NOT exist");
                    //     return None;
                    // }

                    let source = read_to_string(&path).expect("Failed to read file.");
                    let resource = FluentResource::try_new(source)
                        .expect("Could not parse an FTL string.");
                    bundle
                        .add_resource(resource)
                        .expect("Failed to add FTL resources to the bundle.");
                    bundle
                };
                Some((lan.to_string(), language))
            })
            .collect();
        *locale.os_locale.borrow_mut() = crate::fluent::get_os_language();
    });
}


pub fn set_default_language(default: &str) {
    println!("set_default_language");
    LOCALE.with(|locale| {
        *locale.current.borrow_mut() = default.to_string();
    });
}

pub fn set_language(new: &str) {
    println!("set_language");
    let trigger = LOCALE.with(|locale| {
        *locale.current.borrow_mut() = new.to_string();
        locale.refresh
    });
    trigger.notify();
}


pub(crate) fn get_os_language() -> Option<String> {
    None
}

pub trait Localize {
    fn arg(self, arg: impl Into<String> + 'static, val: impl Fn() -> FluentValue<'static> + 'static) -> Self;
}


pub fn get_refresh_trigger() -> Trigger {
    println!("get_refresh_trigger");
    LOCALE.with(|l| l.refresh)
}


pub fn update_arg(main_key: &str, arg_key: &str, value: impl Into<FluentValue<'static>>) -> String {
    println!("update_arg for: {main_key}");
    LOCALE.with(|loc| {
        // println!("current_len: {}, total: {}", &lock.current, lock.locales.len());
        let mut locales = loc.locales.borrow_mut();
        let bundle = locales.get_mut(&*loc.current.borrow()).unwrap();
        
        let msg = bundle.get_message(main_key).unwrap().value().unwrap();

        let mut args_mut = loc.args.borrow_mut();
        match args_mut.entry(main_key.to_string()) {
            Entry::Occupied(mut a) => {
                let a = a.get_mut();
                a.set(arg_key.to_string(), value);
            },
            Entry::Vacant(vacant) => {
                let mut args = FluentArgs::new();
                args.set(arg_key.to_string(), value);
                vacant.insert(args);
            }
        };
        let args = args_mut.get(main_key);
        
        println!("args: {args:#?}");
        let mut errors = vec!();
        let s = bundle.format_pattern(msg, args.as_deref(), &mut errors);
        if !errors.is_empty() {
            eprintln!("errors: {errors:#?}");
        }
        s.to_string()
    })
}


pub fn get_locale_from_key(key: &str) -> String {
    println!("get_locale_from_key: `{key}`");
    LOCALE.with(|loc| {
        let locales = loc.locales.borrow();
        // println!("current_len: {}, total: {}", &lock.current, lock.locales.len());
        let bundle = locales.get(&*loc.current.borrow()).unwrap();
        let msg = bundle.get_message(key).unwrap().value().unwrap();
        let args = loc.args.borrow();
        let args = args.get(key);
        println!("args: {args:#?}");
        let mut errors = vec!();
        let s = bundle.format_pattern(msg, args, &mut errors);
        if !errors.is_empty() {
            eprintln!("errors: {errors:#?}");
        }
        s.to_string()
    })
}


pub fn provide_args_for_key(key: String, args: FluentArgs<'static>) {
    println!("provide_args_for_key for {key}");
    
    LOCALE.with(|locale| {
        let mut lock = locale.args.borrow_mut();
        match lock.entry(key) {
            Entry::Occupied(mut a) => { let a = a.get_mut(); *a = args; },
            Entry::Vacant(vacant) => { vacant.insert(args); }
        }
    });
}


pub fn add_args(key: String, arg: String, val: FluentValue<'static>) {
    println!("add_args for {key}");
    // let k = key.clone();
    LOCALE.with(|locale| {
        let mut lock = locale.args.borrow_mut();
        match lock.entry(key) {
            Entry::Occupied(mut args) => args.get_mut().set(arg, val),
            Entry::Vacant(vacant) => vacant.insert(FluentArgs::new()).set(arg, val)
        }
    });
}