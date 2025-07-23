//! Module for creating and managing radio buttons with optional labels, reactivity, and styling.
//!
//! This module includes [`RadioButton`] and helper functions for building both standalone and labeled radio buttons.
//!
//! It supports multiple levels of reactivity using closures or reactive signals (`RwSignal`, etc.).
//!
//! # Usage
//!
//! ```rust
//! # use floem::views::radio_button;
//! use floem_reactive::{RwSignal, SignalGet};
//! let selected = RwSignal::new("A".to_string());
//! radio_button("A".to_string(), move || selected.get());
//! ```
//!
//! For labels:
//! ```rust
//! # use floem::views::labeled_radio_button;
//! use floem_reactive::{RwSignal, SignalGet};
//! let selected = RwSignal::new("A".to_string());
//! labeled_radio_button("A".to_string(), move || selected.get(), || "Option A");
//! ```
#[deny(missing_docs)]
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

/// Internal helper to create the visual representation of a radio button.
///
/// Conditionally shows the selection dot based on whether `actual_value == represented_value`.
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

/// A struct for building radio buttons using different data update strategies.
///
/// The radio button is visually selectable and supports keyboard navigation.
///
/// # Reactivity Options
///
/// - [`RadioButton::new`] – for closures returning a value.
/// - [`RadioButton::new_get`] – for read-only reactive signals.
/// - [`RadioButton::new_rw`] – for read-write reactive signals.
///
/// # Related
/// See [`radio_button`] and [`labeled_radio_button`] for simplified constructors.
pub struct RadioButton;

impl RadioButton {
    /// Creates a new radio button using a closure for its current value.
    ///
    /// The returned value is wrapped in a [`ValueContainer`] so it can be managed declaratively.
    ///
    /// # Example
    /// ```rust
    /// use floem::views::RadioButton;
    /// use floem_reactive::{RwSignal, SignalGet};
    /// let selected = RwSignal::new("A".to_string());
    /// RadioButton::new("A".to_string(), move || selected.get());
    /// ```
    pub fn new<T>(represented_value: T, actual_value: impl Fn() -> T + 'static) -> ValueContainer<T>
    where
        T: Eq + PartialEq + Clone + 'static,
    {
        let (inbound_signal, outbound_signal) = create_value_container_signals(actual_value);

        value_container(
            radio_button_svg(represented_value.clone(), inbound_signal.read_only())
                .keyboard_navigable()
                .on_click_stop(move |_| {
                    outbound_signal.set(represented_value.clone());
                }),
            move || outbound_signal.get(),
        )
    }

    /// Creates a read-only reactive radio button.
    ///
    /// Useful for when the button state is externally managed and shouldn't be changed by the user.
    pub fn new_get<T>(
        represented_value: T,
        actual_value: impl SignalGet<T> + 'static,
    ) -> impl IntoView
    where
        T: Eq + PartialEq + Clone + 'static,
    {
        radio_button_svg(represented_value, actual_value).keyboard_navigable()
    }

    /// Creates a reactive radio button with two-way binding.
    ///
    /// When selected, the radio button will set the underlying signal to its represented value.
    ///
    /// # Example
    /// ```rust
    /// use floem::views::RadioButton;
    /// use floem_reactive::RwSignal;
    /// let selected = RwSignal::new("Option1".to_string());
    /// RadioButton::new_rw("Option2".to_string(), selected);
    /// ```
    pub fn new_rw<T>(
        represented_value: T,
        actual_value: impl SignalGet<T> + SignalUpdate<T> + Copy + 'static,
    ) -> impl IntoView
    where
        T: Eq + PartialEq + Clone + 'static,
    {
        let cloneable_represented_value = represented_value.clone();

        radio_button_svg(cloneable_represented_value.clone(), actual_value)
            .keyboard_navigable()
            .on_click_stop(move |_| {
                actual_value.set(cloneable_represented_value.clone());
            })
    }

    /// Creates a new **labeled** radio button from a closure and label generator.
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
            .keyboard_navigable()
            .on_click_stop(move |_| {
                outbound_signal.set(represented_value.clone());
            }),
            move || outbound_signal.get(),
        )
    }

    /// Creates a read-only **labeled** radio button from a signal and label.
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
        .keyboard_navigable()
    }

    /// Creates a reactive **labeled** radio button with two-way binding and dynamic label.
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
        .keyboard_navigable()
        .on_click_stop(move |_| {
            actual_value.set(cloneable_represented_value.clone());
        })
    }
}

/// Shorthand for [`RadioButton::new`] to create a reactive radio button.
pub fn radio_button<T>(
    represented_value: T,
    actual_value: impl Fn() -> T + 'static,
) -> ValueContainer<T>
where
    T: Eq + PartialEq + Clone + 'static,
{
    RadioButton::new(represented_value, actual_value)
}

/// Shorthand for [`RadioButton::new_labeled`] to create a reactive labeled radio button.
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

#[cfg(test)]
mod test {
    use super::*;
    use floem_reactive::{create_rw_signal, SignalGet, SignalUpdate};

    #[test]
    fn test_radio_button_new_initial_value() {
        let actual_value = create_rw_signal(String::from("Option1"));
        let _radio_button = RadioButton::new_rw("Option1".to_string(), actual_value);
        assert_eq!(actual_value.get(), "Option1");
    }

    #[test]
    fn test_radio_button_new_changes_state() {
        let actual_value = create_rw_signal(String::from("Option1"));
        let _radio_button = RadioButton::new_rw("Option2".to_string(), actual_value);
        actual_value.set("Option2".to_string());
        assert_eq!(actual_value.get(), "Option2");
    }

    #[test]
    fn test_labeled_radio_button_initial_value() {
        let actual_value = create_rw_signal(String::from("OptionA"));
        let _labeled_radio_button =
            RadioButton::new_labeled_rw("OptionA".to_string(), actual_value, || {
                "Label for Option A"
            });

        assert_eq!(actual_value.get(), "OptionA");
    }

    #[test]
    fn test_labeled_radio_button_changes_state() {
        let actual_value = create_rw_signal(String::from("OptionA"));
        let _labeled_radio_button =
            RadioButton::new_labeled_rw("OptionB".to_string(), actual_value, || {
                "Label for Option B"
            });

        actual_value.set("OptionB".to_string());

        assert_eq!(actual_value.get(), "OptionB");
    }

    #[test]
    fn test_radio_button_new_get() {
        let actual_value = create_rw_signal(String::from("Option1"));
        let _radio_button = RadioButton::new_get("Option1".to_string(), actual_value);
        assert_eq!(actual_value.get(), "Option1");
    }

    #[test]
    fn test_radio_button_new_labeled_get() {
        let actual_value = create_rw_signal(String::from("OptionA"));
        let _labeled_radio_button =
            RadioButton::new_labeled_get("OptionA".to_string(), actual_value, || {
                "Label for Option A"
            });

        assert_eq!(actual_value.get(), "OptionA");
    }
}
