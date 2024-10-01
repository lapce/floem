use floem::{
    kurbo::{self},
    reactive::{create_rw_signal, SignalGet},
    unit::UnitExt,
    views::{button, container, h_stack, text_input, transform, v_stack, Decorators},
    window::WindowConfig,
    Application, IntoView, View,
};

fn frame<V: IntoView + 'static>(child: V) -> impl IntoView {
    container(child).style(|s| s.border(1.px()))
}

fn app_view() -> impl IntoView {
    let child = || {
        container((
            button("Button".into_view().style(|s| s.width(90).height(40))).on_click_stop(|_| {
                println!("Button clicked");
            }),
        ))
        .style(|s| s.width(100).height(50))
    };

    let x_translation = create_rw_signal("50".to_string());
    let x_translation_input = text_input(x_translation);

    let y_translation = create_rw_signal("50".to_string());
    let y_translation_input = text_input(y_translation);

    let scale = create_rw_signal("0.5".to_string());
    let scale_input = text_input(scale);

    let rotation = create_rw_signal("0".to_string());
    let rotation_input = text_input(rotation);

    let view = h_stack((
        v_stack((
            v_stack((
                h_stack(("Translate (x): ", x_translation_input)),
                h_stack(("Translate (y): ", y_translation_input)),
            ))
            .style(|s| {
                s.margin(8)
                    .align_items(Some(floem::taffy::AlignItems::Center))
                    .height(48)
            }),
            frame(transform(child(), move || {
                kurbo::Affine::translate((
                    x_translation.get().parse().unwrap_or_default(),
                    y_translation.get().parse().unwrap_or_default(),
                ))
            }))
            .style(|s| s.width(200).height(200)),
            h_stack(("Scale", scale_input)).style(|s| {
                s.margin(8)
                    .align_items(Some(floem::taffy::AlignItems::Center))
                    .height(48)
            }),
            frame(transform(child(), move || {
                kurbo::Affine::scale(scale.get().parse().unwrap_or(1.0))
            }))
            .style(|s| s.width(200).height(200)),
        )),
        v_stack((
            h_stack(("Rotate: ", rotation_input)).style(|s| {
                s.margin(8)
                    .align_items(Some(floem::taffy::AlignItems::Center))
                    .height(48)
            }),
            frame(transform(child(), move || {
                kurbo::Affine::rotate(rotation.get().parse().unwrap_or_default())
            }))
            .style(|s| s.width(200).height(200)),
            container("Translate, then scale, then rotate").style(|s| {
                s.margin(8)
                    .height(48)
                    .align_items(Some(floem::taffy::AlignItems::Center))
            }),
            frame(transform(child(), move || {
                kurbo::Affine::translate((
                    x_translation.get().parse().unwrap_or_default(),
                    y_translation.get().parse().unwrap_or_default(),
                ))
                .then_scale(scale.get().parse().unwrap_or(1.0))
                .then_rotate(rotation.get().parse().unwrap_or_default())
            }))
            .style(|s| s.width(200).height(200)),
        )),
    ))
    .style(|s| s.width_full().height_full());

    let id = view.id();

    v_stack((
        view,
        button("Open Inspector".into_view()).on_click_stop(move |_| {
            id.inspect();
        }),
    ))
}

fn main() {
    let config = WindowConfig::default().size((800.0, 600.0));
    Application::new()
        .window(move |_| app_view(), Some(config))
        .run()
}
