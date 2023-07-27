use floem::{
    peniko::Color,
    reactive::create_signal,
    style::Style,
    view::View,
    views::{checkbox, label, stack, Decorators},
};

use crate::form::{form, form_item};

pub fn checkbox_view() -> impl View {
    let (is_checked, set_is_checked) = create_signal(true);
    form(move || {
        (
            form_item("Basic Checkbox:".to_string(), 120.0, {
                let is_checked = is_checked.clone();
                let set_is_checked = set_is_checked.clone();
                move || {
                    checkbox(is_checked.clone())
                        .focus_visible_style(|| Style::BASE.border_color(Color::BLUE).border(2.))
                        .on_click({
                            let set_is_checked = set_is_checked.clone();
                            move |_| {
                                set_is_checked.update(|checked| *checked = !*checked);
                                true
                            }
                        })
                }
            }),
            form_item("Labelled Checkbox:".to_string(), 120.0, {
                let is_checked = is_checked.clone();
                let set_is_checked = set_is_checked.clone();
                move || {
                    stack(|| {
                        (
                            checkbox(is_checked.clone()).focus_visible_style(|| {
                                Style::BASE.border_color(Color::BLUE).border(2.)
                            }),
                            label(|| "Check me!".to_string()),
                        )
                    })
                    .on_click({
                        let set_is_checked = set_is_checked.clone();
                        move |_| {
                            set_is_checked.update(|checked| *checked = !*checked);
                            true
                        }
                    })
                }
            }),
            form_item("Disabled Checkbox:".to_string(), 120.0, {
                let is_checked = is_checked.clone();
                let set_is_checked = set_is_checked.clone();
                move || {
                    stack(|| {
                        (
                            checkbox(is_checked.clone()).focus_visible_style(|| {
                                Style::BASE.border_color(Color::BLUE).border(2.)
                            }),
                            label(|| "Check me!".to_string()),
                        )
                    })
                    .style(|| Style::BASE.color(Color::GRAY))
                    .disabled(|| true)
                    .on_click({
                        let set_is_checked = set_is_checked.clone();
                        move |_| {
                            set_is_checked.update(|checked| *checked = !*checked);
                            true
                        }
                    })
                }
            }),
        )
    })
}
