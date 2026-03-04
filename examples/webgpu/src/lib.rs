use floem::text::FONT_CONTEXT;
use floem::views::Label;
use floem::window::WindowConfig;
use floem::Application;
use floem::{
    reactive::{RwSignal, SignalGet, SignalUpdate},
    views::{ButtonClass, Decorators},
    IntoView,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const FIRA_MONO: &[u8] = include_bytes!("../fonts/FiraMono-Medium.ttf");
const FIRA_SANS: &[u8] = include_bytes!("../fonts/FiraSans-Medium.ttf");
const DEJAVU_SERIF: &[u8] = include_bytes!("../fonts/DejaVuSerif.ttf");

pub fn app_view() -> impl IntoView {
    // Create a reactive signal with a counter value, defaulting to 0
    let counter = RwSignal::new(0);

    // Create a vertical layout
    (
        // The counter value updates automatically, thanks to reactivity
        Label::derived(move || format!("Value: {}", counter.get())),
        // Create a horizontal layout
        (
            "Increment".class(ButtonClass).action(move || {
                counter.update(|value| *value += 1);
            }),
            "Decrement".class(ButtonClass).action(move || {
                counter.update(|value| *value -= 1);
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
        let mut font_cx = FONT_CONTEXT.lock();
        font_cx
            .collection
            .register_fonts(FIRA_MONO.to_vec().into(), None);
        font_cx
            .collection
            .register_fonts(FIRA_SANS.to_vec().into(), None);
        font_cx
            .collection
            .register_fonts(DEJAVU_SERIF.to_vec().into(), None);
    }

    let window_config = WindowConfig::default().with_web_config(|w| w.canvas_id("the-canvas"));

    Application::new()
        .window(move |_| app_view(), Some(window_config))
        .run()
}
