use crate::{
    style_class,
    view::View,
    views::{self, h_stack, svg, Decorators},
};
use floem_reactive::ReadSignal;
use std::fmt::Display;

style_class!(pub FocusClass);

style_class!(pub CheckboxClass);

style_class!(pub LabeledCheckboxClass);

fn checkbox_svg(checked: ReadSignal<bool>) -> impl View {
    const CHECKBOX_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-2 -2 16 16"><polygon points="5.19,11.83 0.18,7.44 1.82,5.56 4.81,8.17 10,1.25 12,2.75" /></svg>"#;
    let svg_str = move || if checked.get() { CHECKBOX_SVG } else { "" }.to_string();
    svg(svg_str).class(CheckboxClass)
}

/// Renders a checkbox the provided checked signal.
/// Can be combined with a label and a stack with a click event (as in `examples/widget-gallery`).
pub fn checkbox(checked: ReadSignal<bool>) -> impl View {
    checkbox_svg(checked).keyboard_navigatable()
}

/// Renders a checkbox using the provided checked signal.
pub fn labeled_checkbox<S: Display + 'static>(
    checked: ReadSignal<bool>,
    label: impl Fn() -> S + 'static,
) -> impl View {
    h_stack((checkbox_svg(checked), views::label(label)))
        .class(LabeledCheckboxClass)
        .style(|s| s.items_center().justify_center())
        .keyboard_navigatable()
}
