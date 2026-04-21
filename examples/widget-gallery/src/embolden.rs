use floem::{kurbo::Vec2, prelude::*, style::FontEmbolden, unit::Pct};

use crate::form::{form, form_item};

const MAX_EMBOLDEN_PX: f64 = 1.5;
const LOREM_IPSUM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed non neque ut nibh aliquet volutpat. Integer bibendum, velit et interdum tincidunt, lectus arcu feugiat nisl, vitae tempus sem arcu ac enim. Vestibulum ante ipsum primis in faucibus orci luctus et ultrices posuere cubilia curae; Morbi pretium, lacus sed feugiat malesuada, justo orci volutpat nibh, vitae sollicitudin justo mauris non ipsum. Donec id est sed eros sagittis congue. Integer posuere, nibh in laoreet viverra, risus mauris sodales purus, id porttitor sapien lorem eget odio.\n\nSuspendisse potenti. Praesent nec mi quis mauris lacinia porttitor. Pellentesque habitant morbi tristique senectus et netus et malesuada fames ac turpis egestas. Fusce vitae orci consequat, pretium ligula et, malesuada ipsum. Curabitur non turpis nisl. Duis non magna id neque ullamcorper tristique. Nulla facilisi. Mauris commodo sem et lectus tincidunt, in ultrices enim bibendum. Quisque lacinia volutpat augue, non interdum metus bibendum ut.\n\nAliquam erat volutpat. Vestibulum sodales tincidunt magna, at pharetra enim viverra nec. Nam feugiat, risus a efficitur luctus, lectus justo suscipit lacus, id pulvinar nibh mauris ac purus. Cras sit amet dui sed neque accumsan commodo. In nec luctus ipsum. Phasellus viverra felis non elit sagittis, at hendrerit risus dignissim. Donec faucibus orci nec lectus volutpat, eget vulputate ipsum molestie.";

fn slider_to_px(slider_pct: Pct) -> f64 {
    slider_pct.0 * MAX_EMBOLDEN_PX / 100.0
}

fn amount_label(name: &'static str, amount: impl Fn() -> f64 + 'static) -> impl IntoView {
    Label::derived(move || format!("{name}: {:.3}px", amount()))
}

pub fn embolden_view() -> impl IntoView {
    let x_pct = RwSignal::new(Pct(50.0));
    let y_pct = RwSignal::new(Pct(50.0));

    let sample = Label::new(LOREM_IPSUM)
        .style(move |s| {
            s.font_size(18.0)
                .line_height(1.45)
                .text_wrap()
                .width(720.0)
                .padding(20.0)
                .border(1.0)
                .border_radius(10.0)
                .set(
                    FontEmbolden,
                    Vec2::new(slider_to_px(x_pct.get()), slider_to_px(y_pct.get())),
                )
        })
        .scroll()
        .style(|s| s.height(340.0).width_full());

    form((
        form_item(
            "X Amount:",
            (
                slider::Slider::new_rw(x_pct).style(|s| s.width(200).height_full()),
                amount_label("X", move || slider_to_px(x_pct.get())),
            )
                .h_stack()
                .style(|s| s.gap(12).items_center()),
        ),
        form_item(
            "Y Amount:",
            (
                slider::Slider::new_rw(y_pct).style(|s| s.width(200).height_full()),
                amount_label("Y", move || slider_to_px(y_pct.get())),
            )
                .h_stack()
                .style(|s| s.gap(12).items_center()),
        ),
        form_item(
            "Combined:",
            Label::derived(move || {
                let embolden = Vec2::new(slider_to_px(x_pct.get()), slider_to_px(y_pct.get()));
                format!(
                    "FontEmbolden = Vec2::new({:.3}, {:.3}) px",
                    embolden.x, embolden.y
                )
            }),
        ),
        form_item("Sample:", sample),
    ))
}
