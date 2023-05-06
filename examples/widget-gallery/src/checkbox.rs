use floem::{
    peniko::Color,
    reactive::{create_signal, SignalUpdate},
    style::Style,
    view::View,
    views::{checkbox, label, stack, Decorators},
    AppContext,
};

use crate::form::{form, form_item};

pub fn checkbox_view(cx: AppContext) -> impl View {
    form(cx, |cx| {
        let (is_checked, set_is_checked) = create_signal(cx.scope, true);
        (
            form_item(cx, "Basic Checkbox:".to_string(), 120.0, move |cx| {
                checkbox(cx, is_checked)
                    .focus_visible_style(cx, || Style::BASE.border_color(Color::BLUE).border(2.))
                    .on_click(move |_| {
                        set_is_checked.update(|checked| *checked = !*checked);
                        true
                    })
            }),
            form_item(cx, "Labelled Checkbox:".to_string(), 120.0, move |cx| {
                stack(cx, |cx| {
                    (
                        checkbox(cx, is_checked).focus_visible_style(cx, || {
                            Style::BASE.border_color(Color::BLUE).border(2.)
                        }),
                        label(cx, || "Check me!".to_string()),
                    )
                })
                .on_click(move |_| {
                    set_is_checked.update(|checked| *checked = !*checked);
                    true
                })
            }),
        )
    })
}
