use crate::{
    style_class,
    view::View,
    views::{self, container, empty, h_stack, Decorators},
    IntoView,
};
use floem_reactive::{SignalGet, SignalUpdate};

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

/// Renders a radio button that appears as selected if the signal equals the given enum value.
/// Can be combined with a label and a stack with a click event (as in `examples/widget-gallery`).
pub fn radio_button<T>(
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

/// Renders a radio button that appears as selected if the signal equals the given enum value.
pub fn labeled_radio_button<S: std::fmt::Display + 'static, T>(
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
