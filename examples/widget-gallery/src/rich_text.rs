use std::ops::Range;

use floem::{
    IntoView,
    peniko::color::palette,
    text::{Attrs, AttrsList, FontStyle},
    views::{Decorators, RichTextExt, Scroll, Stack, rich_text},
};

pub fn rich_text_view() -> impl IntoView {
    let builder =
        "This".red().italic() + " is rich text".blue() + "\nTest value: " + 5.to_string().green();

    let text = "
    // floem is a ui lib, homepage https://github.com/lapce/floem
    fn main() {
        println(\"Hello World!\");
    }";

    let create_attrs = || {
        let attrs = Attrs::new().color(palette::css::BLACK);
        let mut attrs_list = AttrsList::new(attrs);

        attrs_list.add_span(
            Range { start: 5, end: 66 },
            Attrs::new()
                .color(palette::css::GRAY)
                .font_style(FontStyle::Italic),
        );

        attrs_list.add_span(
            Range { start: 36, end: 66 },
            Attrs::new().color(palette::css::BLUE),
        );

        attrs_list.add_span(
            Range { start: 71, end: 73 },
            Attrs::new().color(palette::css::PURPLE),
        );

        attrs_list.add_span(
            Range { start: 74, end: 78 },
            Attrs::new().color(palette::css::SKY_BLUE),
        );

        attrs_list.add_span(
            Range { start: 78, end: 80 },
            Attrs::new().color(palette::css::GOLDENROD),
        );

        attrs_list.add_span(
            Range { start: 91, end: 98 },
            Attrs::new().color(palette::css::GOLD),
        );

        attrs_list.add_span(
            Range { start: 98, end: 99 },
            Attrs::new().color(palette::css::PURPLE),
        );

        attrs_list.add_span(
            Range {
                start: 100,
                end: 113,
            },
            Attrs::new().color(palette::css::DARK_GREEN),
        );

        attrs_list.add_span(
            Range {
                start: 113,
                end: 114,
            },
            Attrs::new().color(palette::css::PURPLE),
        );

        attrs_list.add_span(
            Range {
                start: 114,
                end: 115,
            },
            Attrs::new().color(palette::css::GRAY),
        );

        attrs_list
    };

    let initial_attrs = create_attrs();

    Scroll::new({
        Stack::vertical((
            rich_text(text.to_string(), initial_attrs, move || {
                (text.to_string(), create_attrs())
            }),
            builder.style(|s| s.padding_left(15)),
        ))
        .style(|s| s.gap(20))
    })
}
