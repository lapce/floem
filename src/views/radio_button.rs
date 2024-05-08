use crate::{
    style_class,
    view::View,
    views::{
        self, container, create_value_container_signals, empty, h_stack, value_container,
        Decorators, ValueContainer,
    },
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
pub fn radio_button<T>(
    represented_value: T,
    actual_value: impl Fn() -> T + 'static,
) -> ValueContainer<T>
where
    T: Eq + PartialEq + Clone + 'static,
{
    let (inbound_signal, outbound_signal) = create_value_container_signals(actual_value);
    let cloneable_represented_value = represented_value.clone();

    value_container(
        radio_button_svg(
            cloneable_represented_value.clone(),
            inbound_signal.read_only(),
        )
        .keyboard_navigatable()
        .on_click_stop(move |_| {
            outbound_signal.set(cloneable_represented_value.clone());
        }),
        move || outbound_signal.get(),
    )
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
    let (inbound_signal, outbound_signal) = create_value_container_signals(actual_value);
    let cloneable_represented_value = represented_value.clone();

    value_container(
        h_stack((
            radio_button_svg(
                cloneable_represented_value.clone(),
                inbound_signal.read_only(),
            ),
            views::label(label),
        ))
        .class(LabeledRadioButtonClass)
        .style(|s| s.items_center())
        .keyboard_navigatable()
        .on_click_stop(move |_| {
            outbound_signal.set(cloneable_represented_value.clone());
        }),
        move || outbound_signal.get(),
    )
}
