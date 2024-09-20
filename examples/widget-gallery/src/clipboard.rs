use floem::{
    reactive::{create_rw_signal, SignalGet, SignalUpdate},
    views::{button, h_stack, label, text_input, v_stack, Decorators},
    Clipboard, IntoView,
};

use crate::form::{form, form_item};

pub fn clipboard_view() -> impl IntoView {
    let text1 = create_rw_signal("".to_string());
    let text2 = create_rw_signal("-".to_string());

    form({
        (
            form_item("Simple copy".to_string(), 120.0, move || {
                button("Copy the answer").action(move || {
                    let _ = Clipboard::set_contents("42".to_string());
                })
            }),
            form_item("Copy from input".to_string(), 120.0, move || {
                h_stack((
                    text_input(text1).keyboard_navigatable(),
                    button("Copy").action(move || {
                        let _ = Clipboard::set_contents(text1.get());
                    }),
                ))
            }),
            form_item("Get clipboard".to_string(), 120.0, move || {
                v_stack((
                    button("Get clipboard").action(move || {
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
