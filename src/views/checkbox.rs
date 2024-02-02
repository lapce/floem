use crate::{
    style_class,
    view::View,
    views::{
        self, create_value_container_signals, h_stack, svg, value_container, Decorators,
        ValueContainer,
    },
};
use floem_reactive::ReadSignal;
use std::fmt::Display;

style_class!(pub CheckboxClass);

style_class!(pub LabeledCheckboxClass);

fn checkbox_svg(checked: ReadSignal<bool>) -> impl View {
    const CHECKBOX_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-2 -2 16 16"><polygon points="5.19,11.83 0.18,7.44 1.82,5.56 4.81,8.17 10,1.25 12,2.75" /></svg>"#;
    let svg_str = move || if checked.get() { CHECKBOX_SVG } else { "" }.to_string();
    svg(svg_str).class(CheckboxClass)
}

/// Renders a checkbox the provided checked signal.
pub fn checkbox(checked: impl Fn() -> bool + 'static) -> ValueContainer<bool> {
    let (inbound_signal, outbound_signal) = create_value_container_signals(checked);

    value_container(
        checkbox_svg(inbound_signal.read_only())
            .keyboard_navigatable()
            .on_click_stop(move |_| {
                let checked = inbound_signal.get_untracked();
                outbound_signal.set(!checked);
            }),
        move || outbound_signal.get(),
    )
}

/// Renders a checkbox using the provided checked signal.
pub fn labeled_checkbox<S: Display + 'static>(
    checked: impl Fn() -> bool + 'static,
    label: impl Fn() -> S + 'static,
) -> ValueContainer<bool> {
    let (inbound_signal, outbound_signal) = create_value_container_signals(checked);

    value_container(
        h_stack((
            checkbox_svg(inbound_signal.read_only()),
            views::label(label),
        ))
        .class(LabeledCheckboxClass)
        .keyboard_navigatable()
        .on_click_stop(move |_| {
            let checked = inbound_signal.get_untracked();
            outbound_signal.set(!checked);
        })
        .style(|s| s.items_center().justify_center()),
        move || outbound_signal.get(),
    )
}
