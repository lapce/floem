use floem::{
    keyboard::{Key, NamedKey},
    views::Decorators,
    IntoView, View,
};

mod dyn_stack;
mod stack;
mod stack_from_iter;
mod virtual_stack;

fn app_view() -> impl IntoView {
    let view = (
        (
            "stack".style(|s| s.font_size(16.0)),
            "From signal: false",
            "From iter: false",
            "Renders off-screen: true",
            stack::stack_view(),
        )
            .style(|s| s.flex_col().row_gap(5).width_pct(25.0)),
        (
            "stack_from_iter".style(|s| s.font_size(16.0)),
            "From signal: false",
            "From iter: true",
            "Renders off-screen: true",
            stack_from_iter::stack_from_iter_view(),
        )
            .style(|s| s.flex_col().row_gap(5).width_pct(25.0)),
        (
            "dyn_stack".style(|s| s.font_size(16.0)),
            "From signal: true",
            "From iter: true",
            "Renders off-screen: true",
            dyn_stack::dyn_stack_view(),
        )
            .style(|s| s.flex_col().row_gap(5).width_pct(25.0)),
        (
            "virtual_stack".style(|s| s.font_size(16.0)),
            "From signal: true",
            "From iter: true",
            "Renders off-screen: false",
            virtual_stack::virtual_stack_view(),
        )
            .style(|s| s.flex_col().row_gap(5).width_pct(25.0)),
    )
        .style(|s| s.flex().margin(20).width_full().height_full().col_gap(10))
        .into_view();

    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect(),
    )
}

fn main() {
    floem::launch(app_view);
}
