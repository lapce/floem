use floem::{
    peniko::Color,
    reactive::create_signal,
    view::View,
    views::{checkbox, label, stack, Decorators},
};

use crate::form::{form, form_item};

pub fn checkbox_view() -> impl View {
    let (is_checked, set_is_checked) = create_signal(true);
    form({
        (
            form_item("Basic Checkbox:".to_string(), 120.0, move || {
                checkbox(is_checked)
                    .style(|s| s.focus_visible(|s| s.border(2.).border_color(Color::BLUE)))
                    .on_click(move |_| {
                        set_is_checked.update(|checked| *checked = !*checked);
                        true
                    })
            }),
            form_item("Labelled Checkbox:".to_string(), 120.0, move || {
                stack({
                    (
                        checkbox(is_checked)
                            .style(|s| s.focus_visible(|s| s.border(2.).border_color(Color::BLUE))),
                        label(|| "Check me!"),
                    )
                })
                .on_click(move |_| {
                    set_is_checked.update(|checked| *checked = !*checked);
                    true
                })
            }),
            form_item("Disabled Checkbox:".to_string(), 120.0, move || {
                stack({
                    (
                        checkbox(is_checked)
                            .style(|s| s.focus_visible(|s| s.border(2.).border_color(Color::BLUE))),
                        label(|| "Check me!"),
                    )
                })
                .style(|s| s.color(Color::GRAY))
                .disabled(|| true)
                .on_click(move |_| {
                    set_is_checked.update(|checked| *checked = !*checked);
                    true
                })
            }),
        )
    })
}
