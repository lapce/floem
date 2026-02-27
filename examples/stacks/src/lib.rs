use floem::{action::inspect, prelude::*};

mod dyn_stack;
mod stack;
mod stack_from_iter;
mod virtual_stack;

pub fn stacks_view() -> impl IntoView {
    let basic_stack = Stack::vertical((
        "stack".style(|s| s.font_size(16.0)),
        "From signal: false",
        "From iter: false",
        "Renders off-screen: true",
        stack::stack_view(),
    ))
    .style(|s| s.gap(5).width_pct(25.0));

    let stack_from_iter = Stack::vertical((
        "stack_from_iter".style(|s| s.font_size(16.0)),
        "From signal: false",
        "From iter: true",
        "Renders off-screen: true",
        stack_from_iter::stack_from_iter_view(),
    ))
    .style(|s| s.gap(5).width_pct(25.0));

    let dyn_stack = Stack::vertical((
        "dyn_stack".style(|s| s.font_size(16.0)),
        "From signal: true",
        "From iter: true",
        "Renders off-screen: true",
        dyn_stack::dyn_stack_view(),
    ))
    .style(|s| s.gap(5).width_pct(25.0));

    let virtual_stack = Stack::vertical((
        "virtual_stack".style(|s| s.font_size(16.0)),
        "From signal: true",
        "From iter: true",
        "Renders off-screen: false",
        virtual_stack::virtual_stack_view(),
    ))
    .style(|s| s.flex_col().row_gap(5).width_pct(25.0));

    (basic_stack, stack_from_iter, dyn_stack, virtual_stack)
        .h_stack()
        .style(|s| s.flex().margin(20).width_full().height_full().col_gap(10))
        .on_event_stop(el::KeyUp, |_, KeyboardEvent { key, .. }| {
            if *key == Key::Named(NamedKey::F11) {
                inspect();
            }
        })
}
