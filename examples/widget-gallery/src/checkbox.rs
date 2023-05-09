use floem::{
    peniko::Color,
    reactive::{create_signal, SignalUpdate},
    style::Style,
    view::View,
    views::{checkbox, label, stack, Decorators},
    AppContext,
};

use crate::form::{form, form_item};

pub fn checkbox_view() -> impl View {
    let cx = AppContext::get_current();
    let (is_checked, set_is_checked) = create_signal(cx.scope, true);
    form(move || {
        (
            form_item("Basic Checkbox:".to_string(), 120.0, move || {
                checkbox(is_checked)
                    .focus_visible_style(|| Style::BASE.border_color(Color::BLUE).border(2.))
                    .on_click(move |_| {
                        set_is_checked.update(|checked| *checked = !*checked);
                        true
                    })
            }),
            form_item("Labelled Checkbox:".to_string(), 120.0, move || {
                stack(|| {
                    (
                        checkbox(is_checked).focus_visible_style(|| {
                            Style::BASE.border_color(Color::BLUE).border(2.)
                        }),
                        label(|| "Check me!".to_string()),
                    )
                })
                .on_click(move |_| {
                    set_is_checked.update(|checked| *checked = !*checked);
                    true
                })
            }),
            form_item("Disabled Checkbox:".to_string(), 120.0, move || {
                stack(|| {
                    (
                        checkbox(is_checked).focus_visible_style(|| {
                            Style::BASE.border_color(Color::BLUE).border(2.)
                        }),
                        label(|| "Check me!".to_string()),
                    )
                })
                .style(|| Style::BASE.color(Color::GRAY))
                .disabled(|| true)
                .on_click(move |_| {
                    set_is_checked.update(|checked| *checked = !*checked);
                    true
                })
            }),
        )
    })
}
