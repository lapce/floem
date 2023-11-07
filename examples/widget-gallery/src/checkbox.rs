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
                checkbox(is_checked)
                    .style(|s| s.margin(5.0))
                    .on_click(move |_| {
                        set_is_checked.update(|checked| *checked = !*checked);
                        true
                    })
            }),
            form_item("Disabled Checkbox:".to_string(), width, move || {
                checkbox(is_checked)
                    .style(|s| s.margin(5.0))
                    .on_click(move |_| {
                        set_is_checked.update(|checked| *checked = !*checked);
                        true
                    })
                    .disabled(|| true)
            }),
            form_item("Labelled Checkbox:".to_string(), width, move || {
                labeled_checkbox(is_checked, || "Check me!").on_click(move |_| {
                    set_is_checked.update(|checked| *checked = !*checked);
                    true
                })
            }),
            form_item(
                "Disabled Labelled Checkbox:".to_string(),
                width,
                move || {
                    labeled_checkbox(is_checked, || "Check me!")
                        .on_click(move |_| {
                            set_is_checked.update(|checked| *checked = !*checked);
                            true
                        })
                        .disabled(|| true)
                },
            ),
        )
    })
}
