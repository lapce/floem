use floem::{
    unit::UnitExt,
    view::View,
    views::{img, scroll, Decorators},
};

use crate::form::{form, form_item};

pub fn img_view() -> impl View {
    let ferris = include_bytes!("./../assets/ferris.png");
    let sunflower = include_bytes!("./../assets/sunflower.jpg");

    scroll(form({
        (
            form_item("PNG:".to_string(), 120.0, move || {
                img(move || ferris.to_vec())
            }),
            form_item("PNG(resized):".to_string(), 120.0, move || {
                img(move || ferris.to_vec()).style(|s| s.width(230.px()).height(153.px()))
            }),
            form_item("JPG:".to_string(), 120.0, move || {
                img(move || sunflower.to_vec())
            }),
            form_item("JPG(resized):".to_string(), 120.0, move || {
                img(move || sunflower.to_vec()).style(|s| s.width(320.px()).height(490.px()))
            }),
            //TODO: support percentages for width/height
            //     img(move || ferris.to_vec()).style(|s| s.width(90.pct()).height(90.pct()))
            //
            //TODO: object fit and object position
            //     img(move || ferris.to_vec())
            //     .object_fit(ObjectFit::Contain).object_position(VertPosition::Top, HorizPosition::Left))
            //
        )
    }))
    .style(|s| s.flex_col().min_width(1000.px()))
}
