use floem::{
    reactive::create_signal,
    view::View,
    views::Decorators,
    widgets::{checkbox, labeled_checkbox},
};

use crate::form::{form, form_item};

pub fn checkbox_view() -> impl View {
    let width = 160.0;
    let (is_checked, set_is_checked) = create_signal(true);
    form({
        (
            form_item("Checkbox:".to_string(), width, move || {
                checkbox(move || is_checked.get())
                    .on_update(move |checked| {
                        set_is_checked.set(checked);
                    })
                    .style(|s| s.margin(5.0))
            }),
            form_item("Disabled Checkbox:".to_string(), width, move || {
                checkbox(move || is_checked.get())
                    .style(|s| s.margin(5.0))
                    .disabled(|| true)
            }),
            form_item("Labelled Checkbox:".to_string(), width, move || {
                labeled_checkbox(move || is_checked.get(), || "Check me!").on_update(
                    move |checked| {
                        set_is_checked.set(checked);
                    },
                )
            }),
            form_item(
                "Disabled Labelled Checkbox:".to_string(),
                width,
                move || {
                    labeled_checkbox(move || is_checked.get(), || "Check me!").disabled(|| true)
                },
            ),
        )
    })
}
