use floem::{
    reactive::create_rw_signal,
    view::View,
    views::{h_stack, label, v_stack, Decorators},
    widgets::{button, text_input},
    Clipboard,
};

use crate::form::{form, form_item};

pub fn clipboard_view() -> impl View {
    let text1 = create_rw_signal("".to_string());
    let text2 = create_rw_signal("-".to_string());

    form({
        (
            form_item("Simple copy".to_string(), 120.0, move || {
                button(|| "Copy the answer").on_click_stop(move |_| {
                    let _ = Clipboard::set_contents("42".to_string());
                })
            }),
            form_item("Copy from input".to_string(), 120.0, move || {
                h_stack((
                    text_input(text1).keyboard_navigatable(),
                    button(|| "Copy").on_click_stop(move |_| {
                        let _ = Clipboard::set_contents(text1.get());
                    }),
                ))
            }),
            form_item("Get clipboard".to_string(), 120.0, move || {
                v_stack((
                    button(|| "Get clipboard").on_click_stop(move |_| {
                        if let Ok(content) = Clipboard::get_contents() {
                            text2.set(content);
                        }
                    }),
                    label(move || text2.get()),
                ))
            }),
        )
    })
}
