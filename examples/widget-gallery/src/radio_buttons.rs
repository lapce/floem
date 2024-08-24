use std::fmt::Display;

use floem::{
    reactive::RwSignal,
    style_class,
    views::{Decorators, RadioButton, StackExt as _},
    IntoView,
};
use strum::IntoEnumIterator;

use crate::form::{form, form_item};

#[derive(PartialEq, Eq, Clone, Copy, strum::EnumIter)]
enum OperatingSystem {
    Windows,
    MacOS,
    Linux,
}

impl Display for OperatingSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            OperatingSystem::Windows => write!(f, "Windows"),
            OperatingSystem::MacOS => write!(f, "macOS"),
            OperatingSystem::Linux => write!(f, "Linux"),
        }
    }
}

style_class!(RadioButtonGroupClass);

pub fn radio_buttons_view() -> impl IntoView {
    let width = 160.0;
    let operating_system = RwSignal::new(OperatingSystem::Windows);
    form({
        (
            form_item("Radio Buttons:".to_string(), width, move || {
                OperatingSystem::iter()
                    .map(move |os| RadioButton::new_get_set(os, operating_system))
                    .v_stack()
                    .class(RadioButtonGroupClass)
            }),
            form_item("Disabled Radio Buttons:".to_string(), width, move || {
                OperatingSystem::iter()
                    .map(move |os| RadioButton::new_get(os, operating_system).disabled(|| true))
                    .v_stack()
                    .class(RadioButtonGroupClass)
            }),
            form_item("Labelled Radio Buttons:".to_string(), width, move || {
                OperatingSystem::iter()
                    .map(move |os| {
                        RadioButton::new_labeled_get_set(os, operating_system, move || os)
                    })
                    .v_stack()
                    .class(RadioButtonGroupClass)
            }),
            form_item(
                "Disabled Labelled Radio Buttons:".to_string(),
                width,
                move || {
                    OperatingSystem::iter()
                        .map(move |os| {
                            RadioButton::new_labeled_get(os, operating_system, move || os)
                                .disabled(|| true)
                        })
                        .v_stack()
                        .class(RadioButtonGroupClass)
                },
            ),
        )
    })
    .style(|s| s.class(RadioButtonGroupClass, |s| s.gap(10.).margin_left(5.)))
}
