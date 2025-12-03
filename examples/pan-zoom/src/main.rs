use floem::{kurbo, prelude::*, reactive::Effect};

mod pan_zoom_view;

use crate::pan_zoom_view::pan_zoom_view;

fn child_view() -> impl IntoView {
    let button = button("Click me").action(|| {
        println!("Button clicked!");
    });

    let ferris_png = include_bytes!("./../../widget-gallery/assets/ferris.png");
    let ferris_svg = include_str!("./../../widget-gallery/assets/ferris.svg");

    v_stack((
        "Try panning to move and scrolling to zoom this view",
        button,
        container(container("Clipping example").style(|s| {
            s.background(palette::css::TURQUOISE)
                .height(96.0)
                .width(96.0)
        }))
        .clip()
        .style(|s| s.border(1.0).border_radius(8.0).height(64.0).width(64.0)),
        h_stack((
            v_stack((
                img(move || ferris_png.to_vec()).style(|s| s.width(69.0).height(45.9)),
                "PNG".style(|s| s.justify_center()),
            )),
            v_stack((
                svg(ferris_svg).style(|s| s.unset_color().width(69.px()).height(45.9.px())),
                "SVG".style(|s| s.justify_center()),
            )),
        ))
        .style(|s| s.gap(16.0)),
    ))
    .style(|c| {
        c.background(palette::css::WHITE)
            .gap(16.0)
            .height(256.0)
            .padding(16.0)
    })
}

fn app_view() -> impl IntoView {
    let view_transform = RwSignal::new(kurbo::Affine::default());

    let child_view = child_view().into_view();
    let child_id = child_view.id();
    Effect::new(move |_| {
        let transform = view_transform.get().inverse();
        child_id.set_transform(transform);
    });

    pan_zoom_view(view_transform.get(), child_view.style(|s| s.size_full()))
        .style(|s| s.width_full().height_full().background(palette::css::BLACK))
        .on_pan_zoom(move |affine| view_transform.set(affine))
        .on_key_up(
            Key::Named(NamedKey::F11),
            |m| m.is_empty(),
            move |v, _| v.id().inspect(),
        )
}

fn main() {
    floem::launch(app_view);
}
