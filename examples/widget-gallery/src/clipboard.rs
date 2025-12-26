use floem::{prelude::*, Clipboard};

use crate::form::{form, form_item};

pub fn clipboard_view() -> impl IntoView {
    let text1 = RwSignal::new(String::new());
    let text2 = RwSignal::new("-".to_string());

    form((
        form_item(
            "Simple copy",
            Button::new("Copy the answer").action(move || {
                let _ = Clipboard::set_contents("42".to_string());
            }),
        ),
        form_item(
            "Copy from input",
            Stack::horizontal((
                text_input(text1).style(|s| s.width_full().min_width(150)),
                Button::new("Copy").action(move || {
                    let _ = Clipboard::set_contents(text1.get());
                }),
            ))
            .style(|s| s.gap(5)),
        ),
        form_item(
            "Get clipboard",
            Stack::vertical((
                Button::new("Get clipboard").action(move || {
                    if let Ok(content) = Clipboard::get_contents() {
                        text2.set(content);
                    }
                }),
                Label::derived(move || text2.get()),
            )),
        ),
    ))
}
