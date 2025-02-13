#![deny(missing_docs)]
//! A checkbox view for boolean selection.

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

style_class!(
    /// The style class that is applied to the checkbox.
    pub CheckboxClass
);

style_class!(
    /// The style class that is applied to the labeled checkbox stack.
    pub LabeledCheckboxClass
);

/// The default checkbox SVG
pub const DEFAULT_CHECKBOX_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-2 -2 16 16"><polygon points="5.19,11.83 0.18,7.44 1.82,5.56 4.81,8.17 10,1.25 12,2.75" /></svg>"#;

fn checkbox_svg(
    checked: impl SignalGet<bool> + 'static,
    check_svg: impl Into<String> + 'static,
) -> impl IntoView {
    let check_svg: String = check_svg.into();
    let update_svg = {
        let check_svg = check_svg.clone();
        move || {
            if checked.get() {
                check_svg.clone()
            } else {
                "".to_string()
            }
        }
    };
    svg(check_svg)
        .update_value(update_svg)
        .class(CheckboxClass)
        .keyboard_navigable()
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
    #[inline]
    pub fn new(checked: impl Fn() -> bool + 'static) -> ValueContainer<bool> {
        Self::new_custom(checked, DEFAULT_CHECKBOX_SVG)
    }

    /// Creates a new checkbox with a closure that determines its checked state and accepts a custom SVG
    ///
    /// The semantics are the same as [`Checkbox::new`].
    ///
    /// You can add an `on_update` handler to the returned `ValueContainer` to handle changes.
    pub fn new_custom(
        checked: impl Fn() -> bool + 'static,
        custom_check: impl Into<String> + Clone + 'static,
    ) -> ValueContainer<bool> {
        let (inbound_signal, outbound_signal) = create_value_container_signals(checked);

        value_container(
            checkbox_svg(inbound_signal.read_only(), custom_check).on_click_stop(move |_| {
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
    #[inline]
    pub fn new_rw(
        checked: impl SignalGet<bool> + SignalUpdate<bool> + Copy + 'static,
    ) -> impl IntoView {
        Self::new_rw_custom(checked, DEFAULT_CHECKBOX_SVG)
    }

    /// Creates a new checkbox with a signal that provides and updates its checked state and accepts a custom SVG for the symbol.
    ///
    /// The semantics are the same as [`Checkbox::new_rw`].
    pub fn new_rw_custom(
        checked: impl SignalGet<bool> + SignalUpdate<bool> + Copy + 'static,
        custom_check: impl Into<String> + Clone + 'static,
    ) -> impl IntoView {
        checkbox_svg(checked, custom_check).on_click_stop(move |_| {
            checked.update(|val| *val = !*val);
        })
    }

    /// Creates a new labeled checkbox with a closure that determines its checked state.
    ///
    /// This method is useful when you want a labeled checkbox whose state is determined by a closure.
    /// The label is also provided by a closure, allowing for dynamic updates.
    #[inline]
    pub fn labeled<S: Display + 'static>(
        checked: impl Fn() -> bool + 'static,
        label: impl Fn() -> S + 'static,
    ) -> ValueContainer<bool> {
        Self::custom_labeled(checked, label, DEFAULT_CHECKBOX_SVG)
    }

    /// Creates a new labeled checkbox with a closure that determines its checked state and accepts a custom SVG for the symbol.
    ///
    /// The semantics are the same as [`Checkbox::labeled`].
    pub fn custom_labeled<S: Display + 'static>(
        checked: impl Fn() -> bool + 'static,
        label: impl Fn() -> S + 'static,
        custom_check: impl Into<String> + Clone + 'static,
    ) -> ValueContainer<bool> {
        let (inbound_signal, outbound_signal) = create_value_container_signals(checked);

        value_container(
            h_stack((
                checkbox_svg(inbound_signal.read_only(), custom_check).on_click_stop(move |_| {
                    let checked = inbound_signal.get_untracked();
                    outbound_signal.set(!checked);
                }),
                views::label(label),
            ))
            .class(LabeledCheckboxClass)
            .on_click_stop(move |_| {
                let checked = inbound_signal.get_untracked();
                outbound_signal.set(!checked);
            })
            .style(|s| s.items_center()),
            move || outbound_signal.get(),
        )
    }

    /// Creates a new labeled checkbox with a signal that provides and updates its checked state.
    ///
    /// This method is ideal when you need a labeled checkbox that not only reflects a signal's state but also updates it.
    /// Clicking the checkbox will toggle its state and update the signal accordingly.
    #[inline]
    pub fn labeled_rw<S: Display + 'static>(
        checked: impl SignalGet<bool> + SignalUpdate<bool> + Copy + 'static,
        label: impl Fn() -> S + 'static,
    ) -> impl IntoView {
        Self::custom_labeled_rw(checked, label, DEFAULT_CHECKBOX_SVG)
    }

    /// Creates a new labeled checkbox with a signal that provides and updates its checked state and accepts a custom SVG.
    ///
    /// The semantics are the same as [`Checkbox::labeled_rw`].
    pub fn custom_labeled_rw<S: Display + 'static>(
        checked: impl SignalGet<bool> + SignalUpdate<bool> + Copy + 'static,
        label: impl Fn() -> S + 'static,
        custom_check: impl Into<String> + Clone + 'static,
    ) -> impl IntoView {
        h_stack((
            checkbox_svg(checked, custom_check).on_click_stop(move |_| {
                checked.update(|val| *val = !*val);
            }),
            views::label(label),
        ))
        .on_click_stop(move |_| {
            checked.update(|val| *val = !*val);
        })
        .class(LabeledCheckboxClass)
        .style(|s| s.items_center())
    }
}

/// Renders a checkbox the provided checked signal. See also [`Checkbox::new`] and [`Checkbox::new_rw`].
pub fn checkbox(checked: impl Fn() -> bool + 'static) -> ValueContainer<bool> {
    Checkbox::new(checked)
}

/// Renders a checkbox using a `checked` signal and custom SVG. See also [`Checkbox::new_rw`] and
pub fn custom_checkbox(
    checked: impl Fn() -> bool + 'static,
    custom_check: impl Into<String> + Clone + 'static,
) -> ValueContainer<bool> {
    Checkbox::new_custom(checked, custom_check)
}

/// Renders a checkbox using the provided checked signal. See also [`Checkbox::labeled`] and [`Checkbox::labeled_rw`].
pub fn labeled_checkbox<S: Display + 'static>(
    checked: impl Fn() -> bool + 'static,
    label: impl Fn() -> S + 'static,
) -> ValueContainer<bool> {
    Checkbox::labeled(checked, label)
}

/// Renders a checkbox using the a `checked` signal and a custom SVG. See also [`Checkbox::custom_labeled_rw`]
pub fn custom_labeled_checkbox<S: Display + 'static>(
    checked: impl Fn() -> bool + 'static,
    label: impl Fn() -> S + 'static,
    custom_check: impl Into<String> + Clone + 'static,
) -> ValueContainer<bool> {
    Checkbox::custom_labeled(checked, label, custom_check)
}
