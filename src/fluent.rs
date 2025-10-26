use std::{cell::RefCell, collections::HashMap, fs::read_to_string, path::Path};
use fluent_bundle::{FluentBundle, FluentResource};

thread_local! {
    static LOCALE: RefCell<Localization> = RefCell::new(Localization::default());
}

#[derive(Default)]
pub struct Localization {
    pub(crate) locales: HashMap<String, FluentBundle<FluentResource>>,
    pub(crate) os_locale: Option<String>,
    pub(crate) current: String
}


pub(crate) fn add_localizations(locale_idents: &[&str]) {
    LOCALE.with(|l| {
        let mut lock = l.borrow_mut();
        lock.locales = locale_idents
            .into_iter()
            .filter_map(|lan| {
                let language = {
                    let lid = lan.parse().unwrap();
                    // let x = negotiate_languages();
                    let mut bundle = FluentBundle::new(vec!(lid));
                    let path = Path::new("resources").join(lan);
                    println!("path: {path:?}");
                    if !path.is_dir() {
                        eprintln!("path is not dir");
                        return None;
                    }
                    let path = path.join(lan);
                    if !path.is_file() {
                        eprintln!("path is not file");
                        return None;
                    }

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
        lock.os_locale = crate::fluent::get_os_language();
    });
}


pub(crate) fn set_default_language(locale: &str) {
    LOCALE.with(|l| {
        l.borrow_mut().current = locale.to_string();
    });
}

pub(crate) fn set_language(locale: &str) {
    LOCALE.with(|l| {
        l.borrow_mut().current = locale.to_string();
    });
}


pub(crate) fn get_os_language() -> Option<String> {
    None
}

pub(crate) fn add_key(key: impl Into<String>) {
    
}


pub trait Localize {
    fn localize(&self, key: impl Into<String>);
}