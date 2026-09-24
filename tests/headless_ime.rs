use floem::HasViewId;
use floem::headless::{HeadlessHarness, TestRoot};
use floem::reactive::{RwSignal, SignalGet};
use floem::views::{Decorators, TextInput};

/// Focusing a text input asks the window to enable the IME, and the
/// headless mock window cannot. That refusal used to be unwrapped, so any
/// test that clicked into a text input panicked; now the input takes focus
/// and the request is simply dropped.
#[test]
fn clicking_into_a_text_input_headlessly_does_not_panic() {
    let root = TestRoot::new();
    let value = RwSignal::new(String::from("hello"));
    let input = TextInput::new(value).style(|s| s.size(200.0, 30.0));
    let input_id = input.view_id();
    let mut harness = HeadlessHarness::new_with_size(root, input, 400.0, 200.0);
    harness.rebuild();
    harness.paint();

    // Down, then up: focus follows the press, and the FocusGained that
    // reaches the input asks for IME.
    harness.click(50.0, 15.0);
    harness.rebuild();
    harness.paint();

    assert!(harness.is_focused(input_id), "the input took focus");
    assert_eq!(value.get(), "hello", "and its text is untouched");

    // Losing focus asks for the IME to be disabled, which is refused too.
    harness.dispatch_event(floem::event::Event::Window(
        floem::event::WindowEvent::FocusLost,
    ));
    harness.rebuild();
    harness.paint();
}
