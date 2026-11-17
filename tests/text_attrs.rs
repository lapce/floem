use floem::text::{Attrs, AttrsList, TextLayout};

fn layout_width(attrs: Attrs) -> f64 {
    let layout = TextLayout::new_with_text("mmmm mmmm", AttrsList::new(attrs), None);
    layout.size().width
}

#[test]
fn letter_spacing_widens_layout() {
    let base = layout_width(Attrs::new().font_size(16.0));
    let spaced = layout_width(Attrs::new().font_size(16.0).letter_spacing(5.0));
    // "mmmm mmmm" has 9 characters; 5px of extra tracking per character must
    // widen the layout by a clearly measurable amount regardless of the
    // platform's default font.
    assert!(
        spaced - base > 20.0,
        "expected letter spacing to widen layout: base={base}, spaced={spaced}"
    );
}

#[test]
fn letter_spacing_applies_per_span() {
    let attrs = Attrs::new().font_size(16.0);
    let mut plain = AttrsList::new(attrs.clone());
    plain.add_span(0..4, attrs.clone());
    let mut spaced = AttrsList::new(attrs.clone());
    spaced.add_span(0..4, attrs.letter_spacing(10.0));

    let base = TextLayout::new_with_text("mmmm mmmm", plain, None)
        .size()
        .width;
    let wide = TextLayout::new_with_text("mmmm mmmm", spaced, None)
        .size()
        .width;
    assert!(
        wide - base > 20.0,
        "expected spaced span to widen layout: base={base}, wide={wide}"
    );
}
