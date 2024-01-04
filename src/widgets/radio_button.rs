use crate::{
    style_class,
    view::View,
    views::{self, container, empty, h_stack, Decorators},
};
use floem_reactive::ReadSignal;

style_class!(pub RadioButtonClass);
style_class!(pub RadioButtonDotClass);
style_class!(pub RadioButtonDotSelectedClass);
style_class!(pub LabeledRadioButtonClass);

fn radio_button_svg<T>(represented_value: T, actual_value: ReadSignal<T>) -> impl View
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
pub fn radio_button<T>(represented_value: T, actual_value: ReadSignal<T>) -> impl View
where
    T: Eq + PartialEq + Clone + 'static,
{
    radio_button_svg(represented_value, actual_value).keyboard_navigatable()
}

/// Renders a radio button that appears as selected if the signal equals the given enum value.
pub fn labeled_radio_button<S: std::fmt::Display + 'static, T>(
    represented_value: T,
    actual_value: ReadSignal<T>,
    label: impl Fn() -> S + 'static,
) -> impl View
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
