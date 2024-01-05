use std::fmt::Display;

use floem::{
    reactive::create_signal,
    view::View,
    views::{v_stack, Decorators},
    widgets::{labeled_radio_button, radio_button},
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

pub fn radio_buttons_view() -> impl View {
    let width = 160.0;
    let (operating_system, set_operating_system) = create_signal(OperatingSystem::Windows);
    form({
        (
            form_item("Radio Buttons:".to_string(), width, move || {
                v_stack((
                    radio_button(OperatingSystem::Windows, operating_system).on_click_stop(
                        move |_| {
                            set_operating_system.set(OperatingSystem::Windows);
                        },
                    ),
                    radio_button(OperatingSystem::MacOS, operating_system).on_click_stop(
                        move |_| {
                            set_operating_system.set(OperatingSystem::MacOS);
                        },
                    ),
                    radio_button(OperatingSystem::Linux, operating_system).on_click_stop(
                        move |_| {
                            set_operating_system.set(OperatingSystem::Linux);
                        },
                    ),
                ))
                .style(|s| s.gap(0.0, 10.0).margin_left(5.0))
            }),
            form_item("Disabled Radio Buttons:".to_string(), width, move || {
                v_stack((
                    radio_button(OperatingSystem::Windows, operating_system)
                        .on_click_stop(move |_| {
                            set_operating_system.set(OperatingSystem::Windows);
                        })
                        .disabled(|| true),
                    radio_button(OperatingSystem::MacOS, operating_system)
                        .on_click_stop(move |_| {
                            set_operating_system.set(OperatingSystem::MacOS);
                        })
                        .disabled(|| true),
                    radio_button(OperatingSystem::Linux, operating_system)
                        .on_click_stop(move |_| {
                            set_operating_system.set(OperatingSystem::Linux);
                        })
                        .disabled(|| true),
                ))
                .style(|s| s.gap(0.0, 10.0).margin_left(5.0))
            }),
            form_item("Labelled Radio Buttons:".to_string(), width, move || {
                v_stack((
                    labeled_radio_button(OperatingSystem::Windows, operating_system, || {
                        OperatingSystem::Windows
                    })
                    .on_click_stop(move |_| {
                        set_operating_system.set(OperatingSystem::Windows);
                    }),
                    labeled_radio_button(OperatingSystem::MacOS, operating_system, || {
                        OperatingSystem::MacOS
                    })
                    .on_click_stop(move |_| {
                        set_operating_system.set(OperatingSystem::MacOS);
                    }),
                    labeled_radio_button(OperatingSystem::Linux, operating_system, || {
                        OperatingSystem::Linux
                    })
                    .on_click_stop(move |_| {
                        set_operating_system.set(OperatingSystem::Linux);
                    }),
                ))
            }),
            form_item(
                "Disabled Labelled Radio Buttons:".to_string(),
                width,
                move || {
                    v_stack((
                        labeled_radio_button(OperatingSystem::Windows, operating_system, || {
                            OperatingSystem::Windows
                        })
                        .on_click_stop(move |_| {
                            set_operating_system.set(OperatingSystem::Windows);
                        })
                        .disabled(|| true),
                        labeled_radio_button(OperatingSystem::MacOS, operating_system, || {
                            OperatingSystem::MacOS
                        })
                        .on_click_stop(move |_| {
                            set_operating_system.set(OperatingSystem::MacOS);
                        })
                        .disabled(|| true),
                        labeled_radio_button(OperatingSystem::Linux, operating_system, || {
                            OperatingSystem::Linux
                        })
                        .on_click_stop(move |_| {
                            set_operating_system.set(OperatingSystem::Linux);
                        })
                        .disabled(|| true),
                    ))
                },
            ),
        )
    })
}
