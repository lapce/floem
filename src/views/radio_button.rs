use crate::{
    style_class,
    view::View,
    views::{self, container, empty, h_stack, Decorators},
    IntoView,
};
use floem_reactive::{SignalGet, SignalUpdate};

use super::{create_value_container_signals, value_container, ValueContainer};

style_class!(pub RadioButtonClass);
style_class!(pub RadioButtonDotClass);
style_class!(pub RadioButtonDotSelectedClass);
style_class!(pub LabeledRadioButtonClass);

fn radio_button_svg<T>(represented_value: T, actual_value: impl SignalGet<T> + 'static) -> impl View
where
    T: Eq + PartialEq + Clone + 'static,
{
    container(empty().class(RadioButtonDotClass).style(move |s| {
        s.apply_if(actual_value.get() != represented_value, |s| {
            s.display(taffy::style::Display::None)
        })
    }))
    .class(RadioButtonClass)
}

/// The `RadioButton` struct provides various methods to create and manage radio buttons.
///
/// # Related Functions
/// - [`radio_button`]
/// - [`labeled_radio_button`]
pub struct RadioButton;

impl RadioButton {
    /// Creates a new radio button with a closure that determines its selected state.
    ///
    /// This method is useful when you want a radio button whose state is determined by a closure.
    /// The state can be dynamically updated by the closure, and the radio button will reflect these changes.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<T>(represented_value: T, actual_value: impl Fn() -> T + 'static) -> ValueContainer<T>
    where
        T: Eq + PartialEq + Clone + 'static,
    {
        let (inbound_signal, outbound_signal) = create_value_container_signals(actual_value);

        value_container(
            radio_button_svg(represented_value.clone(), inbound_signal.read_only())
                .keyboard_navigatable()
                .on_click_stop(move |_| {
                    outbound_signal.set(represented_value.clone());
                }),
            move || outbound_signal.get(),
        )
    }

    /// Creates a new radio button with a signal that provides its selected state.
    ///
    /// Use this method when you have a signal that provides the current state of the radio button.
    /// The radio button will automatically update its state based on the signal.
    pub fn new_get<T>(
        represented_value: T,
        actual_value: impl SignalGet<T> + 'static,
    ) -> impl IntoView
    where
        T: Eq + PartialEq + Clone + 'static,
    {
        radio_button_svg(represented_value, actual_value).keyboard_navigatable()
    }

    /// Creates a new radio button with a signal that provides and updates its selected state.
    ///
    /// This method is ideal when you need a radio button that not only reflects a signal's state but also updates it.
    /// Clicking the radio button will set the signal to the represented value.
    pub fn new_rw<T>(
        represented_value: T,
        actual_value: impl SignalGet<T> + SignalUpdate<T> + Copy + 'static,
    ) -> impl IntoView
    where
        T: Eq + PartialEq + Clone + 'static,
    {
        let cloneable_represented_value = represented_value.clone();

        radio_button_svg(cloneable_represented_value.clone(), actual_value)
            .keyboard_navigatable()
            .on_click_stop(move |_| {
                actual_value.set(cloneable_represented_value.clone());
            })
    }

    /// Creates a new labeled radio button with a closure that determines its selected state.
    ///
    /// This method is useful when you want a labeled radio button whose state is determined by a closure.
    /// The label is also provided by a closure, allowing for dynamic updates.
    pub fn new_labeled<S: std::fmt::Display + 'static, T>(
        represented_value: T,
        actual_value: impl Fn() -> T + 'static,
        label: impl Fn() -> S + 'static,
    ) -> ValueContainer<T>
    where
        T: Eq + PartialEq + Clone + 'static,
    {
        let (inbound_signal, outbound_signal) = create_value_container_signals(actual_value);

        value_container(
            h_stack((
                radio_button_svg(represented_value.clone(), inbound_signal.read_only()),
                views::label(label),
            ))
            .class(LabeledRadioButtonClass)
            .style(|s| s.items_center())
            .keyboard_navigatable()
            .on_click_stop(move |_| {
                outbound_signal.set(represented_value.clone());
            }),
            move || outbound_signal.get(),
        )
    }

    /// Creates a new labeled radio button with a signal that provides its selected state.
    ///
    /// Use this method when you have a signal that provides the current state of the radio button and you also want a label.
    /// The radio button and label will automatically update based on the signal.
    pub fn new_labeled_get<S: std::fmt::Display + 'static, T>(
        represented_value: T,
        actual_value: impl SignalGet<T> + 'static,
        label: impl Fn() -> S + 'static,
    ) -> impl IntoView
    where
        T: Eq + PartialEq + Clone + 'static,
    {
        h_stack((
            radio_button_svg(represented_value, actual_value),
            views::label(label),
        ))
        .class(LabeledRadioButtonClass)
        .style(|s| s.items_center())
        .keyboard_navigatable()
    }

    /// Creates a new labeled radio button with a signal that provides and updates its selected state.
    ///
    /// This method is ideal when you need a labeled radio button that not only reflects a signal's state but also updates it.
    /// Clicking the radio button will set the signal to the represented value.
    pub fn new_labeled_rw<S: std::fmt::Display + 'static, T>(
        represented_value: T,
        actual_value: impl SignalGet<T> + SignalUpdate<T> + Copy + 'static,
        label: impl Fn() -> S + 'static,
    ) -> impl IntoView
    where
        T: Eq + PartialEq + Clone + 'static,
    {
        let cloneable_represented_value = represented_value.clone();

        h_stack((
            radio_button_svg(cloneable_represented_value.clone(), actual_value),
            views::label(label),
        ))
        .class(LabeledRadioButtonClass)
        .style(|s| s.items_center())
        .keyboard_navigatable()
        .on_click_stop(move |_| {
            actual_value.set(cloneable_represented_value.clone());
        })
    }
}

/// Renders a radio button that appears as selected if the signal equals the given enum value.
/// Can be combined with a label and a stack with a click event (as in `examples/widget-gallery`).
pub fn radio_button<T>(
    represented_value: T,
    actual_value: impl Fn() -> T + 'static,
) -> ValueContainer<T>
where
    T: Eq + PartialEq + Clone + 'static,
{
    RadioButton::new(represented_value, actual_value)
}

/// Renders a radio button that appears as selected if the signal equals the given enum value.
pub fn labeled_radio_button<S: std::fmt::Display + 'static, T>(
    represented_value: T,
    actual_value: impl Fn() -> T + 'static,
    label: impl Fn() -> S + 'static,
) -> ValueContainer<T>
where
    T: Eq + PartialEq + Clone + 'static,
{
    RadioButton::new_labeled(represented_value, actual_value, label)
}
