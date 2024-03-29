use floem::{
    Application,
    event::{Event, EventListener},
    keyboard::Key,
    peniko::Color,
    quit_app,
    style::CursorStyle,
    unit::UnitExt,
    views::{text, Decorators, container},
    view::View,
};
use floem_winit::event::WindowEvent;

const MESSAGE: &str = "
Keyboard event handler example
 
Focus-based keyboard handling: Click inside this frame to get keyboard focus, then hit 'q' to quit.
Global keyboard handling: Hit 'z' at anytime to quit as well (no focus needed).
";

fn app_view() -> impl View {
    let frame = text(MESSAGE)
        .style(|s| s
            .size(90.pct(), 90.pct())
            .cursor(CursorStyle::Pointer)
            .border(3.0)
            .focus(|s| s.border_color(Color::BLUE).background(Color::LIGHT_GRAY))
            .padding(10.0)
        )
        .keyboard_navigatable() // A view must be navigatable in order to receive keyboard events
        .on_event_stop(EventListener::KeyDown, move |e| {
            if let Event::KeyDown(e) = e {
                if e.key.logical_key == Key::Character("q".into()) {
                    println!("Goodbye :)");
                    quit_app();
                }
                println!("Key pressed in KeyCode: {:?}", e.key.physical_key);
            }
        });

    container(frame)
        .style(|s| s
            .size_full()
            .items_center()
            .justify_center()
        )
}

fn main() {
    Application::new()
        .window(|_| app_view(), None)
        .on_window_event(|event| {
            if let WindowEvent::KeyboardInput { device_id: _ , event, is_synthetic: _ } = event {
                if event.logical_key == Key::Character("z".into()) {
                    quit_app();
                    return true; // Event was handled
                }
            }
            false // Event was not handled, let floem handle it
        })
        .run();
}
