use floem::{
    prelude::*,
    style::{CursorStyle, ObjectFit, ObjectPosition},
    theme::StyleThemeExt,
};

use crate::form::{form, form_item};

pub fn img_view() -> impl IntoView {
    let ferris_png = include_bytes!("./../assets/ferris.png");
    let ferris_svg = include_str!("./../assets/ferris.svg");
    let svg_str = r##"<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="#000">
      <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12c0 1.268-.63 2.39-1.593 3.068a3.745 3.745 0 01-1.043 3.296 3.745 3.745 0 01-3.296 1.043A3.745 3.745 0 0112 21c-1.268 0-2.39-.63-3.068-1.593a3.746 3.746 0 01-3.296-1.043 3.745 3.745 0 01-1.043-3.296A3.745 3.745 0 013 12c0-1.268.63-2.39 1.593-3.068a3.745 3.745 0 011.043-3.296 3.746 3.746 0 013.296-1.043A3.746 3.746 0 0112 3c1.268 0 2.39.63 3.068 1.593a3.746 3.746 0 013.296 1.043 3.746 3.746 0 011.043 3.296A3.745 3.745 0 0121 12z" />
    </svg>"##;
    let sunflower = include_bytes!("./../assets/sunflower.jpg");

    form((
        form_item(
            "PNG:",
            img(move || ferris_png.to_vec()).style(|s| s.aspect_ratio(1.5)),
        ),
        form_item(
            "PNG(resized):",
            img(move || ferris_png.to_vec()).style(|s| s.width(230.pt()).height(153.pt())),
        ),
        form_item(
            "SVG(from file):",
            svg(ferris_svg).style(|s| s.unset_color().width(230.pt())),
        ),
        form_item("Image Fit", object_fit_position_picker(ferris_png)),
        form_item("SVG(from string):", svg(svg_str).style(|s| s.width(100))),
        form_item("JPG:", img(move || sunflower.to_vec())),
        form_item(
            "JPG(resized):",
            img(move || sunflower.to_vec()).style(|s| s.width(320.pt()).height(490.pt())),
        ),
    ))
}

fn object_fit_position_picker(image: &'static [u8]) -> impl IntoView {
    let object_fit = RwSignal::new(ObjectFit::Cover);
    let object_position = RwSignal::new(ObjectPosition::Center);

    let fit_options = [
        ("Fill", ObjectFit::Fill),
        ("Contain", ObjectFit::Contain),
        ("Cover", ObjectFit::Cover),
        ("ScaleDown", ObjectFit::ScaleDown),
        ("None", ObjectFit::None),
    ];
    let position_options = [
        ("TopLeft", ObjectPosition::TopLeft),
        ("Top", ObjectPosition::Top),
        ("TopRight", ObjectPosition::TopRight),
        ("Left", ObjectPosition::Left),
        ("Center", ObjectPosition::Center),
        ("Right", ObjectPosition::Right),
        ("BottomLeft", ObjectPosition::BottomLeft),
        ("Bottom", ObjectPosition::Bottom),
        ("BottomRight", ObjectPosition::BottomRight),
    ];

    let fit_picker = fit_options
        .map(|(label, _)| {
            label.style(|s| {
                s.text_clip()
                    .items_center()
                    .justify_center()
                    .padding_vert(10.)
                    .padding_horiz(8.)
                    .selectable(false)
                    .cursor(CursorStyle::Pointer)
            })
        })
        .list()
        .on_select(move |idx| {
            if let Some(idx) = idx {
                object_fit.set(fit_options[idx].1);
            }
        })
        .style(|s| s.flex_row().gap(5))
        .scroll()
        .style(|s| {
            s.max_width(320.)
                .flex_row()
                .padding_right(3.)
                .scrollbar_width(0.)
                .border_horiz(3.)
                .with_theme(|s, t| s.border_color(t.border()))
        });

    let position_picker = position_options
        .map(|(label, _)| {
            label.style(|s| {
                s.text_clip()
                    .items_center()
                    .justify_center()
                    .padding_vert(10.)
                    .padding_horiz(8.)
                    .selectable(false)
                    .cursor(CursorStyle::Pointer)
            })
        })
        .list()
        .on_select(move |idx| {
            if let Some(idx) = idx {
                object_position.set(position_options[idx].1);
            }
        })
        .style(|s| s.flex_row().gap(5))
        .scroll()
        .style(|s| {
            s.max_width(320.)
                .flex_row()
                .padding_right(3.)
                .scrollbar_width(0.)
                .border_horiz(3.)
                .with_theme(|s, t| s.border_color(t.border()))
        });

    let controls = Stack::vertical((
        ("Object fit:".style(|s| s.width(110.0)), fit_picker).style(|s| s.gap(10).items_center()),
        (
            "Object position:".style(|s| s.width(110.0)),
            position_picker,
        )
            .style(|s| s.gap(10).items_center()),
    ))
    .style(|s| s.gap(10).items_start());

    let preview = img(move || image.to_vec()).style(move |s| {
        s.object_fit(object_fit.get())
            .object_position(object_position.get())
            .size(300, 300)
            .border(2)
            .border_color(css::RED)
    });

    Stack::vertical((controls, preview)).style(|s| s.gap(12).items_center())
}
