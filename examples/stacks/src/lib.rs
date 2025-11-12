use floem::prelude::*;

pub mod dyn_stack;
mod stack;
mod stack_from_iter;
mod virtual_stack;

pub fn stacks_view() -> impl IntoView {
    let simple_stack = h_stack((
        "stack".style(|s| s.font_size(16.0)),
        "From signal: false",
        "From iter: false",
        "Renders off-screen: true",
        stack::stack_view(),
    ))
    .style(|s| s.flex_col().row_gap(5).width_pct(25.0));

    let stack_from_iter = h_stack((
        "stack_from_iter".style(|s| s.font_size(16.0)),
        "From signal: false",
        "From iter: true",
        "Renders off-screen: true",
        stack_from_iter::stack_from_iter_view(),
    ))
    .style(|s| s.flex_col().row_gap(5).width_pct(25.0));

    let simple_dyn_stack = h_stack((
        "dyn_stack".style(|s| s.font_size(16.0)),
        "From signal: true",
        "From iter: true",
        "Renders off-screen: true",
        dyn_stack::dyn_stack_view(),
    ))
    .style(|s| s.flex_col().row_gap(5).width_pct(25.0));

    let virtual_stack = h_stack((
        "virtual_stack".style(|s| s.font_size(16.0)),
        "From signal: true",
        "From iter: true",
        "Renders off-screen: false",
        virtual_stack::virtual_stack_view(),
    ))
    .style(|s| s.flex_col().row_gap(5).width_pct(25.0));

    let view = h_stack((
        simple_stack,
        stack_from_iter,
        simple_dyn_stack,
        virtual_stack,
    ))
    .style(|s| s.margin(20).width_full().height_full().col_gap(10))
    .into_view();

    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_, _| id.inspect(),
    )
}
