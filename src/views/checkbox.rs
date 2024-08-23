use crate::{
    style_class,
    view::IntoView,
    views::{self, h_stack, svg, Decorators},
};
use floem_reactive::{SignalGet, SignalUpdate};
use std::fmt::Display;

style_class!(pub CheckboxClass);

style_class!(pub LabeledCheckboxClass);

fn checkbox_svg(checked: impl SignalGet<bool> + 'static) -> impl IntoView {
    const CHECKBOX_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-2 -2 16 16"><polygon points="5.19,11.83 0.18,7.44 1.82,5.56 4.81,8.17 10,1.25 12,2.75" /></svg>"#;
    let svg_str = move || if checked.get() { CHECKBOX_SVG } else { "" }.to_string();
    svg(svg_str).class(CheckboxClass)
}

/// Renders a checkbox the provided checked signal.
pub fn checkbox(
    checked: impl SignalGet<bool> + SignalUpdate<bool> + 'static + Copy,
) -> impl IntoView {
    checkbox_svg(checked)
        .keyboard_navigatable()
        .on_click_stop(move |_| {
            checked.update(|val| *val = !*val);
        })
}

/// Renders a checkbox using the provided checked signal.
pub fn labeled_checkbox<S: Display + 'static>(
    checked: impl SignalGet<bool> + SignalUpdate<bool> + Copy + 'static,
    label: impl Fn() -> S + 'static,
) -> impl IntoView {
    h_stack((checkbox_svg(checked), views::label(label)))
        .class(LabeledCheckboxClass)
        .keyboard_navigatable()
        .on_click_stop(move |_| {
            checked.update(|val| *val = !*val);
        })
        .style(|s| s.items_center().justify_center())
}
