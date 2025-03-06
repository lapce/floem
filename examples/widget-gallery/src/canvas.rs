use floem::{kurbo::Rect, prelude::*};
use palette::css;

use crate::form::{form, form_item};

pub fn canvas_view() -> impl IntoView {
    let rounded = RwSignal::new(true);

    form((form_item(
        "Simple Canvas:",
        h_stack((
            canvas(move |cx, size| {
                cx.fill(
                    &Rect::ZERO
                        .with_size(size)
                        .to_rounded_rect(if rounded.get() { 8. } else { 0. }),
                    css::PURPLE,
                    0.,
                );
            })
            .style(|s| s.size(100, 300)),
            button("toggle")
                .action(move || rounded.update(|s| *s = !*s))
                .style(|s| s.height(30)),
        ))
        .style(|s| s.gap(10).items_center()),
    ),))
}
