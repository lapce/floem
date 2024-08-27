use floem::{
    reactive::RwSignal,
    views::{Checkbox, Decorators},
    IntoView,
};

use crate::form::{form, form_item};

pub fn checkbox_view() -> impl IntoView {
    let width = 160.0;
    let is_checked = RwSignal::new(true);
    form({
        (
            form_item("Checkbox:".to_string(), width, move || {
                Checkbox::new_get_set(is_checked).style(|s| s.margin(5.0))
            }),
            form_item("Disabled Checkbox:".to_string(), width, move || {
                Checkbox::new_get(is_checked)
                    .style(|s| s.margin(5.0))
                    .disabled(|| true)
            }),
            form_item("Labelled Checkbox:".to_string(), width, move || {
                Checkbox::new_labeled_get_set(is_checked, || "Check me!")
            }),
            form_item(
                "Disabled Labelled Checkbox:".to_string(),
                width,
                move || Checkbox::new_labeled_get(is_checked, || "Check me!").disabled(|| true),
            ),
        )
    })
}
