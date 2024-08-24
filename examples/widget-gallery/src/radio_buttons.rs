use std::fmt::Display;

use floem::{
    reactive::RwSignal,
    views::{labeled_radio_button, radio_button, stack_from_iter, Decorators},
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
            OperatingSystem::MacOS => write!(f, "MacOS"),
            OperatingSystem::Linux => write!(f, "Linux"),
        }
    }
}

pub fn radio_buttons_view() -> impl IntoView {
    let width = 160.0;
    let operating_system = RwSignal::new(OperatingSystem::Windows);
    form({
        (
            form_item("Radio Buttons:".to_string(), width, move || {
                stack_from_iter(
                    OperatingSystem::iter().map(|os| radio_button(os, operating_system)),
                )
                .style(|s| s.flex_col().gap(10.).margin_left(5.))
            }),
            form_item("Disabled Radio Buttons:".to_string(), width, move || {
                stack_from_iter(
                    OperatingSystem::iter()
                        .map(|os| radio_button(os, operating_system).disabled(|| true)),
                )
                .style(|s| s.flex_col().gap(10.).margin_left(5.))
            }),
            form_item("Labelled Radio Buttons:".to_string(), width, move || {
                stack_from_iter(
                    OperatingSystem::iter()
                        .map(|os| labeled_radio_button(os, operating_system, move || os)),
                )
                .style(|s| s.flex_col().gap(10.).margin_left(5.))
            }),
            form_item(
                "Disabled Labelled Radio Buttons:".to_string(),
                width,
                move || {
                    stack_from_iter(OperatingSystem::iter().map(|os| {
                        labeled_radio_button(os, operating_system, move || os).disabled(|| true)
                    }))
                    .style(|s| s.flex_col().gap(10.).margin_left(5.))
                },
            ),
        )
    })
}
