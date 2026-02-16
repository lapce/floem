use dropdown::Dropdown;
use strum::IntoEnumIterator;

use floem::{prelude::*, reactive::Effect, theme::StyleThemeExt, views::scroll::ScrollClass};

use crate::form::{self, form_item};

#[derive(strum::EnumIter, Debug, PartialEq, Clone, Copy, Hash)]
enum Value {
    One,
    Two,
    Three,
    Four,
    Five,
}
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{self:?}"))
    }
}

pub fn dropdown_view() -> impl IntoView {
    let dropdown_active_item = RwSignal::new(Value::Three);

    Effect::new(move |_| {
        let active_item = dropdown_active_item.get();
        println!("Selected: {active_item}");
    });

    form::form((form_item(
        "Dropdown",
        Dropdown::new_rw(dropdown_active_item, Value::iter()).style(|s| {
            s.font_size(15).class(ScrollClass, |s| {
                s.font_size(15).with_theme(|s, t| {
                    s.padding(t.padding())
                    // .class(ListItemClass, |s| s.padding(t.padding()))
                })
            })
        }),
    ),))
}
