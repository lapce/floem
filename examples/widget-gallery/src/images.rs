use floem::{
    unit::UnitExt,
    view::View,
    views::{img, svg, Decorators},
};

use crate::form::{form, form_item};

pub fn img_view() -> impl View {
    let ferris_png = include_bytes!("./../assets/ferris.png");
    let ferris_svg = include_str!("./../assets/ferris.svg");
    let svg_str = r##"<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="#000">
      <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12c0 1.268-.63 2.39-1.593 3.068a3.745 3.745 0 01-1.043 3.296 3.745 3.745 0 01-3.296 1.043A3.745 3.745 0 0112 21c-1.268 0-2.39-.63-3.068-1.593a3.746 3.746 0 01-3.296-1.043 3.745 3.745 0 01-1.043-3.296A3.745 3.745 0 013 12c0-1.268.63-2.39 1.593-3.068a3.745 3.745 0 011.043-3.296 3.746 3.746 0 013.296-1.043A3.746 3.746 0 0112 3c1.268 0 2.39.63 3.068 1.593a3.746 3.746 0 013.296 1.043 3.746 3.746 0 011.043 3.296A3.745 3.745 0 0121 12z" />
    </svg>"##;
    let sunflower = include_bytes!("./../assets/sunflower.jpg");

    form({
        (
            form_item("PNG:".to_string(), 120.0, move || {
                img(move || ferris_png.to_vec())
            }),
            form_item("PNG(resized):".to_string(), 120.0, move || {
                img(move || ferris_png.to_vec()).style(|s| s.width(230.px()).height(153.px()))
            }),
            form_item("SVG(from file):".to_string(), 120.0, move || {
                svg(move || ferris_svg.to_string()).style(|s| s.width(230.px()).height(153.px()))
            }),
            form_item("SVG(from string):".to_string(), 120.0, move || {
                svg(move || svg_str.to_string()).style(|s| s.width(100.px()).height(100.px()))
            }),
            form_item("JPG:".to_string(), 120.0, move || {
                img(move || sunflower.to_vec())
            }),
            form_item("JPG(resized):".to_string(), 120.0, move || {
                img(move || sunflower.to_vec()).style(|s| s.width(320.px()).height(490.px()))
            }),
            //TODO: support percentages for width/height
            //     img(move || ferris_png.to_vec()).style(|s| s.width(90.pct()).height(90.pct()))
            //
            //TODO: object fit and object position
            //     img(move || ferris_png.to_vec())
            //     .object_fit(ObjectFit::Contain).object_position(VertPosition::Top, HorizPosition::Left))
            //
        )
    })
}
