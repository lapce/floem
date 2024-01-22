use crate::{
    id::Id,
    style_class,
    view::{delegate_view, View, ViewData},
    views::{self, h_stack, svg, Decorators, Svg},
};
use floem_reactive::ReadSignal;
use std::fmt::Display;

style_class!(pub CheckboxClass);

style_class!(pub LabeledCheckboxClass);

fn checkbox_svg(checked: ReadSignal<bool>) -> Svg {
    const CHECKBOX_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-2 -2 16 16"><polygon points="5.19,11.83 0.18,7.44 1.82,5.56 4.81,8.17 10,1.25 12,2.75" /></svg>"#;
    let svg_str = move || if checked.get() { CHECKBOX_SVG } else { "" }.to_string();
    svg(svg_str).class(CheckboxClass)
}

pub struct Checkbox {
    child: Svg,
    data: ViewData,
    checked: ReadSignal<bool>,
}

impl Checkbox {
    pub fn on_update(mut self, on_update: impl Fn(bool) + 'static) -> Self {
        self.child = self
            .child
            .on_click_stop(move |_| on_update(!self.checked.get()));
        self
    }
}

delegate_view!(Checkbox, data, child);

/// Render a checkbox with the provided signal.
pub fn checkbox(checked: ReadSignal<bool>) -> Checkbox {
    Checkbox {
        child: checkbox_svg(checked).keyboard_navigatable(),
        data: ViewData::new(Id::next()),
        checked,
    }
}

/// Renders a checkbox using the provided checked signal.
pub fn labeled_checkbox<S: Display + 'static>(
    checked: ReadSignal<bool>,
    label: impl Fn() -> S + 'static,
) -> impl View {
    h_stack((checkbox_svg(checked), views::label(label)))
        .class(LabeledCheckboxClass)
        .style(|s| s.items_center().justify_center())
        .keyboard_navigatable()
}
