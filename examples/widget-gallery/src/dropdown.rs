use strum::IntoEnumIterator;

use floem::{
    peniko::Color,
    reactive::create_rw_signal,
    unit::UnitExt,
    view::View,
    views::{container, label, stack, svg, Decorators},
    widgets::dropdown::dropdown,
};

use crate::form::{self, form_item};

#[derive(strum::EnumIter, Debug, PartialEq, Clone, Copy)]
enum Values {
    One,
    Two,
    Three,
    Four,
    Five,
}
impl std::fmt::Display for Values {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

const CHEVRON_DOWN: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xml:space="preserve" viewBox="0 0 185.344 185.344">
  <path fill="#010002" d="M92.672 144.373a10.707 10.707 0 0 1-7.593-3.138L3.145 59.301c-4.194-4.199-4.194-10.992 0-15.18a10.72 10.72 0 0 1 15.18 0l74.347 74.341 74.347-74.341a10.72 10.72 0 0 1 15.18 0c4.194 4.194 4.194 10.981 0 15.18l-81.939 81.934a10.694 10.694 0 0 1-7.588 3.138z"/>
</svg>"##;

pub fn dropdown_view() -> impl View {
    let show_dropdown = create_rw_signal(false);

    let main_drop_view = move |item| {
        stack((
            label(move || item),
            container(
                svg(|| String::from(CHEVRON_DOWN)).style(|s| s.size(12, 12).color(Color::BLACK)),
            )
            .style(|s| {
                s.items_center()
                    .padding(3.)
                    .border_radius(7.pct())
                    .hover(move |s| s.background(Color::LIGHT_GRAY))
            }),
        ))
        .style(|s| s.items_center().justify_between().size_full())
        .any()
    };

    form::form({
        (form_item("Dropdown".to_string(), 120.0, move || {
            dropdown(
                // drivign function
                move || Values::Three,
                // main view
                main_drop_view,
                // iterator to build list in dropdown
                Values::iter(),
                // view for each item in the list
                |item| label(move || item).any(),
            )
            .show_list(move || show_dropdown.get())
            .on_accept(move |_val| show_dropdown.set(false))
        }),)
    })
}
