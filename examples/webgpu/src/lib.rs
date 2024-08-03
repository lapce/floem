use floem::cosmic_text::load_font_data;
use floem::cosmic_text::FONT_SYSTEM;
use floem::kurbo::Size;
use floem::window::WindowConfig;
use floem::Application;
use floem::{
    keyboard::{Key, Modifiers, NamedKey},
    peniko::Color,
    reactive::create_signal,
    unit::UnitExt,
    views::{dyn_view, label, ButtonClass, Decorators, LabelClass, LabelCustomStyle},
    IntoView, View,
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

    load_font_data(Vec::from(FIRA_MONO));
    load_font_data(Vec::from(FIRA_SANS));
    load_font_data(Vec::from(DEJAVU_SERIF));

    let window_config = WindowConfig::default()
        .size(Size::new(800.0, 600.0))
        .with_web_config(|w| w.canvas_parent_id("canvas-container"));

    Application::new()
        .window(move |_| app_view(), Some(window_config))
        .run()
}
