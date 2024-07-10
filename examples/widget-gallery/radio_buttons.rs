use std::fmt::Display;

use floem::{
    reactive::create_signal,
    views::{labeled_radio_button, radio_button, v_stack, Decorators},
    IntoView,
};

use crate::form::{form, form_item};

#[derive(PartialEq, Eq, Clone)]
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
    let (operating_system, set_operating_system) = create_signal(OperatingSystem::Windows);
    form({
        (
            form_item("Radio Buttons:".to_string(), width, move || {
                v_stack((
                    radio_button(OperatingSystem::Windows, move || operating_system.get())
                        .on_update(move |value| {
                            set_operating_system.set(value);
                        }),
                    radio_button(OperatingSystem::MacOS, move || operating_system.get()).on_update(
                        move |value| {
                            set_operating_system.set(value);
                        },
                    ),
                    radio_button(OperatingSystem::Linux, move || operating_system.get()).on_update(
                        move |value| {
                            set_operating_system.set(value);
                        },
                    ),
                ))
                .style(|s| s.column_gap(10.0).margin_left(5.0))
            }),
            form_item("Disabled Radio Buttons:".to_string(), width, move || {
                v_stack((
                    radio_button(OperatingSystem::Windows, move || operating_system.get())
                        .on_update(move |value| {
                            set_operating_system.set(value);
                        })
                        .disabled(|| true),
                    radio_button(OperatingSystem::MacOS, move || operating_system.get())
                        .on_update(move |value| {
                            set_operating_system.set(value);
                        })
                        .disabled(|| true),
                    radio_button(OperatingSystem::Linux, move || operating_system.get())
                        .on_update(move |value| {
                            set_operating_system.set(value);
                        })
                        .disabled(|| true),
                ))
                .style(|s| s.column_gap(10.0).margin_left(5.0))
            }),
            form_item("Labelled Radio Buttons:".to_string(), width, move || {
                v_stack((
                    labeled_radio_button(
                        OperatingSystem::Windows,
                        move || operating_system.get(),
                        || OperatingSystem::Windows,
                    )
                    .on_update(move |value| {
                        set_operating_system.set(value);
                    }),
                    labeled_radio_button(
                        OperatingSystem::MacOS,
                        move || operating_system.get(),
                        || OperatingSystem::MacOS,
                    )
                    .on_update(move |value| {
                        set_operating_system.set(value);
                    }),
                    labeled_radio_button(
                        OperatingSystem::Linux,
                        move || operating_system.get(),
                        || OperatingSystem::Linux,
                    )
                    .on_update(move |value| {
                        set_operating_system.set(value);
                    }),
                ))
            }),
            form_item(
                "Disabled Labelled Radio Buttons:".to_string(),
                width,
                move || {
                    v_stack((
                        labeled_radio_button(
                            OperatingSystem::Windows,
                            move || operating_system.get(),
                            || OperatingSystem::Windows,
                        )
                        .on_update(move |value| {
                            set_operating_system.set(value);
                        })
                        .disabled(|| true),
                        labeled_radio_button(
                            OperatingSystem::MacOS,
                            move || operating_system.get(),
                            || OperatingSystem::MacOS,
                        )
                        .on_update(move |value| {
                            set_operating_system.set(value);
                        })
                        .disabled(|| true),
                        labeled_radio_button(
                            OperatingSystem::Linux,
                            move || operating_system.get(),
                            || OperatingSystem::Linux,
                        )
                        .on_update(move |value| {
                            set_operating_system.set(value);
                        })
                        .disabled(|| true),
                    ))
                },
            ),
        )
    })
}
