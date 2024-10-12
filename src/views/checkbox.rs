#![deny(missing_docs)]
//! A checkbox view for boolean selection.

use crate::{
    style_class,
    view::IntoView,
    views::{self, h_stack, svg, value_container::*, Decorators},
};
use floem_reactive::{SignalGet, SignalUpdate};
use std::fmt::Display;

style_class!(
    /// The Style class that is applied to the checkbox.
    pub CheckboxClass
);

style_class!(
    /// The Style class that is applied to the labeled checkbox stack.
    pub LabeledCheckboxClass
);

fn checkbox_svg(checked: impl SignalGet<bool> + 'static) -> impl IntoView {
    const CHECKBOX_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-2 -2 16 16"><polygon points="5.19,11.83 0.18,7.44 1.82,5.56 4.81,8.17 10,1.25 12,2.75" /></svg>"#;
    let svg_str = move || if checked.get() { CHECKBOX_SVG } else { "" }.to_string();
    svg(CHECKBOX_SVG)
        .update_value(svg_str)
        .class(CheckboxClass)
        .keyboard_navigatable()
}

/// # A customizable checkbox view for boolean selection.
///
/// The `Checkbox` struct provides several constructors, each offering different levels of
/// customization and ease of use. The simplest is the [Checkbox::new_rw] constructor, which gets direct access to a signal and will update it when the checkbox is clicked.
///
/// Choose the constructor that best fits your needs based on whether you require labeling
/// and how you prefer to manage the checkbox's state (via closure or direct signal manipulation).
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

    /// Creates a new checkbox with a signal that provides and updates its checked state.
    ///
    /// This method is ideal when you need a checkbox that not only reflects a signal's state but also updates it.
    /// Clicking the checkbox will toggle its state and update the signal accordingly.
    pub fn new_rw(
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
    pub fn labeled<S: Display + 'static>(
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

    /// Creates a new labeled checkbox with a signal that provides and updates its checked state.
    ///
    /// This method is ideal when you need a labeled checkbox that not only reflects a signal's state but also updates it.
    /// Clicking the checkbox will toggle its state and update the signal accordingly.
    pub fn labeled_rw<S: Display + 'static>(
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
