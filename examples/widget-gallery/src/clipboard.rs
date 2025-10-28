use floem::{prelude::*, Clipboard};

use crate::form::{form, form_item};

pub fn clipboard_view() -> impl IntoView {
    let text1 = create_rw_signal("".to_string());
    let text2 = create_rw_signal("-".to_string());

    form((
        form_item(
            "Simple copy",
            button("Copy the answer").action(move || {
                let _ = Clipboard::set_contents("42".to_string());
            }),
        ),
        form_item(
            "Copy from input",
            h_stack((
                text_input(text1),
                button("Copy").action(move || {
                    let _ = Clipboard::set_contents(text1.get());
                }),
            )),
        ),
        form_item(
            "Get clipboard",
            v_stack((
                button("Get clipboard").action(move || {
                    if let Ok(content) = Clipboard::get_contents() {
                        text2.set(content);
                    }
                }),
                label(move || text2.get()),
            )),
        ),
    ))
}
