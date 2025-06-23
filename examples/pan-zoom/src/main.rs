use floem::kurbo;
use floem::prelude::*;

mod pan_zoom_view;
mod transform_view;

use crate::pan_zoom_view::pan_zoom_view;
use crate::transform_view::transform_view;

fn child_view() -> impl IntoView {
    let button = button("Click me").on_click_stop(|_| {
        println!("Button clicked!");
    });

    v_stack((
        "Try panning to move and scrolling to zoom this view",
        button,
    ))
    .style(|c| {
        c.background(palette::css::WHITE)
            .gap(16.0)
            .height(128.0)
            .padding(16.0)
    })
}

fn app_view() -> impl IntoView {
    let (view_transform, set_view_transform) = create_signal(kurbo::Affine::default());

    pan_zoom_view(
        view_transform.get(),
        transform_view(child_view(), move || view_transform.get().inverse()),
    )
    .style(|s| s.width_full().height_full().background(palette::css::BLACK))
    .on_pan_zoom(move |affine| set_view_transform.set(affine))
}

fn main() {
    floem::launch(app_view);
}
