use floem::event::{Event, EventListener};
use floem::reactive::create_rw_signal;
use floem::views::label;
use floem::{
    view::View,
    views::{v_stack, Decorators},
    widgets::text_input,
    EventPropagation,
};

fn app_view() -> impl View {
    let text = create_rw_signal("".to_string());
    let keyboard_signal = create_rw_signal("".to_string());
    v_stack((
        text_input(text)
            .placeholder("Write here")
            .keyboard_navigatable(),
        label(move || format!("Key Pressed: {}", keyboard_signal.get()))
            .keyboard_listenable()
            .on_event(EventListener::KeyDown, move |e| {
                if let Event::KeyDown(e) = e {
                    keyboard_signal.set(e.key.logical_key.to_text().unwrap().to_string());
                }
                EventPropagation::Continue
            }),
    ))
}

fn main() {
    floem::launch(app_view);
}
