use floem::text::FONT_SYSTEM;
use floem::window::WindowConfig;
use floem::Application;
use floem::{
    reactive::{create_signal, SignalGet, SignalUpdate},
    views::{label, ButtonClass, Decorators},
    IntoView,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const FIRA_MONO: &[u8] = include_bytes!("../fonts/FiraMono-Medium.ttf");
const FIRA_SANS: &[u8] = include_bytes!("../fonts/FiraSans-Medium.ttf");
const DEJAVU_SERIF: &[u8] = include_bytes!("../fonts/DejaVuSerif.ttf");

pub fn app_view() -> impl IntoView {
    // Create a reactive signal with a counter value, defaulting to 0
    let (counter, set_counter) = create_signal(0);

    // Create a vertical layout
    (
        // The counter value updates automatically, thanks to reactivity
        label(move || format!("Value: {}", counter.get())),
        // Create a horizontal layout
        (
            "Increment".class(ButtonClass).on_click_stop(move |_| {
                set_counter.update(|value| *value += 1);
            }),
            "Decrement".class(ButtonClass).on_click_stop(move |_| {
                set_counter.update(|value| *value -= 1);
            }),
        ),
    )
        .style(|s| s.flex_col())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn run() {
    #[cfg(target_family = "wasm")]
    console_error_panic_hook::set_once();

    {
        let mut font_system = FONT_SYSTEM.lock();
        let font_db = font_system.db_mut();
        font_db.load_font_data(Vec::from(FIRA_MONO));
        font_db.load_font_data(Vec::from(FIRA_SANS));
        font_db.load_font_data(Vec::from(DEJAVU_SERIF));
    }

    let window_config = WindowConfig::default().with_web_config(|w| w.canvas_id("the-canvas"));

    Application::new()
        .window(move |_| app_view(), Some(window_config))
        .run()
}
