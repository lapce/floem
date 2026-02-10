use floem::{
    IntoView,
    peniko::{Color, color::palette},
    prelude::{
        palette::css::{DARK_GRAY, WHITE_SMOKE},
        *,
    },
    style::CursorStyle,
    theme::StyleThemeExt,
    views::{Button, Decorators, ToggleButton, ToggleHandleBehavior},
};

use crate::form::{form, form_item};

pub fn button_view() -> impl IntoView {
    let state = RwSignal::new(false);
    form((
        form_item(
            "Basic Button:",
            Button::new("Click me").action(|| println!("Button clicked")),
        ),
        form_item(
            "Styled Button:",
            Button::new("Click me")
                .action(|| println!("Button clicked"))
                .style(|s| {
                    s.border(1.0)
                        .border_radius(10.0)
                        .padding(10.0)
                        .background(palette::css::YELLOW_GREEN)
                        .color(palette::css::DARK_GREEN)
                        .cursor(CursorStyle::Pointer)
                        .active(|s| s.color(palette::css::WHITE).background(palette::css::RED))
                        .hover(|s| s.background(Color::from_rgb8(244, 67, 54)))
                        .focus_visible(|s| s.outline(2.).outline_color(palette::css::BLUE))
                }),
        ),
        form_item(
            "Disabled Button:",
            Button::new("Unclickable")
                .style(|s| s.set_disabled(true))
                .action(|| println!("Button clicked")),
        ),
        form_item(
            "Secondary click button:",
            Button::new("Right click me").on_event_stop(listener::SecondaryClick, |_, _| {
                println!("Secondary mouse button click.");
            }),
        ),
        form_item(
            "Toggle button - Snap:",
            ToggleButton::new(|| true)
                .on_toggle(|_| {
                    println!("Button Toggled");
                })
                .toggle_style(|s| s.behavior(ToggleHandleBehavior::Snap)),
        ),
        form_item(
            "Toggle button - Follow:",
            ToggleButton::new(|| true)
                .on_toggle(|_| {
                    println!("Button Toggled");
                })
                .toggle_style(|s| s.behavior(ToggleHandleBehavior::Follow)),
        ),
        form_item(
            "Toggle button - toggle background:",
            ToggleButton::new_rw(state).toggle_style(move |s| {
                s.apply_if(state.get(), |s| {
                    s.accent_color(DARK_GRAY).handle_color(WHITE_SMOKE)
                })
                .behavior(ToggleHandleBehavior::Snap)
            }),
        ),
    ))
    .style(move |s| {
        s.apply_if(state.get(), |s| {
            s.with_theme(|s, t| s.background(t.bg_elevated()))
        })
    })
}
