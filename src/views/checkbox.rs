use crate::{
    style_class,
    view::IntoView,
    views::{
        self, create_value_container_signals, h_stack, svg, value_container, Decorators,
        ValueContainer,
    },
};
use floem_reactive::{SignalGet, SignalUpdate};
use std::fmt::Display;

style_class!(pub CheckboxClass);

style_class!(pub LabeledCheckboxClass);

fn checkbox_svg(checked: impl SignalGet<bool> + 'static) -> impl IntoView {
    const CHECKBOX_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-2 -2 16 16"><polygon points="5.19,11.83 0.18,7.44 1.82,5.56 4.81,8.17 10,1.25 12,2.75" /></svg>"#;
    let svg_str = move || if checked.get() { CHECKBOX_SVG } else { "" }.to_string();
    svg(CHECKBOX_SVG)
        .update_value(svg_str)
        .class(CheckboxClass)
        .keyboard_navigatable()
}

/// The `Checkbox` struct provides various methods to create and manage checkboxes.
///
/// # Related Functions
/// - [`checkbox`]
/// - [`labeled_checkbox`]
pub struct Checkbox;

impl Checkbox {
    /// Creates a new checkbox with a closure that determines its checked state.
    ///
    /// This method is useful when you want to create a checkbox whose state is determined by a closure.
    /// The state can be dynamically updated by the closure, and the checkbox will reflect these changes.
    ///
    /// You can add an `on_update` handler to the returned `ValueContainer` to handle changes.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(checked: impl Fn() -> bool + 'static) -> ValueContainer<bool> {
        let (inbound_signal, outbound_signal) = create_value_container_signals(checked);

        value_container(
            checkbox_svg(inbound_signal.read_only()).on_click_stop(move |_| {
                let checked = inbound_signal.get_untracked();
                outbound_signal.set(!checked);
            }),
            move || outbound_signal.get(),
        )
    }

    /// Creates a new checkbox with a signal that provides its checked state.
    ///
    /// Use this method when you have a signal that provides the current state of the checkbox.
    /// The checkbox will automatically update its state based on the signal but nothing will happen when clicked.
    pub fn new_get(checked: impl SignalGet<bool> + 'static) -> impl IntoView {
        checkbox_svg(checked)
    }

    /// Creates a new checkbox with a signal that provides and updates its checked state.
    ///
    /// This method is ideal when you need a checkbox that not only reflects a signal's state but also updates it.
    /// Clicking the checkbox will toggle its state and update the signal accordingly.
    pub fn new_get_set(
        checked: impl SignalGet<bool> + SignalUpdate<bool> + Copy + 'static,
    ) -> impl IntoView {
        checkbox_svg(checked).on_click_stop(move |_| {
            checked.update(|val| *val = !*val);
        })
    }

    /// Creates a new labeled checkbox with a closure that determines its checked state.
    ///
    /// This method is useful when you want a labeled checkbox whose state is determined by a closure.
    /// The label is also provided by a closure, allowing for dynamic updates.
    pub fn new_labeled<S: Display + 'static>(
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
            .on_click_stop(move |_| {
                let checked = inbound_signal.get_untracked();
                outbound_signal.set(!checked);
            })
            .style(|s| s.items_center().justify_center()),
            move || outbound_signal.get(),
        )
    }

    /// Creates a new labeled checkbox with a signal that provides its checked state.
    ///
    /// Use this method when you have a signal that provides the current state of the checkbox and you also want a label.
    /// The checkbox and label will automatically update based on the signal.
    pub fn new_labeled_get<S: Display + 'static>(
        checked: impl SignalGet<bool> + 'static,
        label: impl Fn() -> S + 'static,
    ) -> impl IntoView {
        h_stack((checkbox_svg(checked), views::label(label)))
            .class(LabeledCheckboxClass)
            .style(|s| s.items_center().justify_center())
    }

    /// Creates a new labeled checkbox with a signal that provides and updates its checked state.
    ///
    /// This method is ideal when you need a labeled checkbox that not only reflects a signal's state but also updates it.
    /// Clicking the checkbox will toggle its state and update the signal accordingly.
    pub fn new_labeled_get_set<S: Display + 'static>(
        checked: impl SignalGet<bool> + SignalUpdate<bool> + Copy + 'static,
        label: impl Fn() -> S + 'static,
    ) -> impl IntoView {
        h_stack((checkbox_svg(checked), views::label(label)))
            .class(LabeledCheckboxClass)
            .style(|s| s.items_center().justify_center())
            .on_click_stop(move |_| {
                checked.update(|val| *val = !*val);
            })
    }
}

/// Renders a checkbox the provided checked signal.
pub fn checkbox(checked: impl Fn() -> bool + 'static) -> ValueContainer<bool> {
    Checkbox::new(checked)
}

/// Renders a checkbox using the provided checked signal.
pub fn labeled_checkbox<S: Display + 'static>(
    checked: impl Fn() -> bool + 'static,
    label: impl Fn() -> S + 'static,
) -> ValueContainer<bool> {
    Checkbox::new_labeled(checked, label)
}
