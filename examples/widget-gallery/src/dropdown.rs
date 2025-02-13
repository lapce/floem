use dropdown::Dropdown;
use strum::IntoEnumIterator;

use floem::{prelude::*, reactive::create_effect};

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

pub fn dropdown_view() -> impl IntoView {
    let dropdown_active_item = RwSignal::new(Values::Three);

    create_effect(move |_| {
        let active_item = dropdown_active_item.get();
        println!("Selected: {active_item}");
    });

    form::form((form_item(
        "Dropdown",
        Dropdown::new_rw(dropdown_active_item, Values::iter()),
    ),))
}
