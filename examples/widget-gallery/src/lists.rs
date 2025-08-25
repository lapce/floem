use floem::{
    prelude::*,
    reactive::create_effect,
    taffy::{prelude::*, Line},
    text::Weight,
};

use crate::{
    checkbox::CROSS_SVG,
    form::{form, form_item},
};

pub fn virt_list_view() -> impl IntoView {
    form((
        form_item(
            "Simple List".style(|s| s.grid_row(Line::from_line_index(1))),
            simple_list().style(|s| s.grid_row(Line::from_line_index(2))),
        ),
        form_item(
            "Enhanced List".style(|s| s.grid_row(Line::from_line_index(1))),
            enhanced_list().style(|s| s.grid_row(Line::from_line_index(2))),
        ),
        form_item(
            "Horizontal Stack from Iterator".style(|s| s.grid_row(Line::from_line_index(4))),
            h_buttons_from_iter().style(|s| s.grid_row(Line::from_line_index(5))),
        ),
        form_item(
            "Vertical Stack from Iterator".style(|s| s.grid_row(Line::from_line_index(4))),
            v_buttons_from_iter().style(|s| s.grid_row(Line::from_line_index(5))),
        ),
    ))
    .style(|s| {
        s.grid_template_columns([fr(1.), fr(1.), fr(1.), fr(1.)])
            .grid_template_rows([auto(), auto(), length(20.), auto(), auto()])
            .row_gap(20)
            .justify_items(JustifyItems::Center)
    })
}

fn simple_list() -> impl IntoView {
    (0..100)
        .list()
        .style(|s| {
            s.width_full()
                .class(LabelClass, |s| s.height(24).items_center())
        })
        .scroll()
        .style(|s| s.width(100.0).height(200.0).border(1.0))
}

fn enhanced_list() -> impl IntoView {
    let long_list: im::Vector<(bool, i32)> = (0..1000).map(|v| (true, v)).collect();
    let long_list = RwSignal::new(long_list);

    let list_width = 180.0;
    let item_height = 32.0;

    let checkmark = |checkbox_state| Checkbox::new_rw(checkbox_state).style(|s| s.margin_left(6));

    let label =
        |item: i32| item.style(|s| s.margin_left(6).height(32.0).font_size(22.0).items_center());

    let x_mark = move |index| {
        svg(CROSS_SVG)
            .on_click_stop(move |_| {
                print!("Item Removed");
                long_list.update(|list| {
                    list.remove(index);
                });
            })
            .style(|s| {
                s.size(18.0, 18.)
                    .font_weight(Weight::BOLD)
                    .color(palette::css::RED)
                    .border(1.0)
                    .border_color(palette::css::RED)
                    .border_radius(16.0)
                    .padding(2.)
                    .margin_right(20.0)
                    .hover(|s| s.color(palette::css::WHITE).background(palette::css::RED))
            })
    };

    let item_view = move |(index, (state, item))| {
        let checkbox_state = RwSignal::new(state);
        create_effect(move |_| {
            let state = checkbox_state.get();
            long_list.update(|list| {
                // because this is an immutable vector, getting the index will always result in the correct item even if we remove elements.
                if let Some((s, _v)) = list.get_mut(index) {
                    *s = state;
                };
            });
        });

        (checkmark(checkbox_state), label(item), x_mark(index))
            .h_stack()
            .style(move |s| {
                s.items_center()
                    .gap(5)
                    .height(item_height)
                    .apply_if(index != 0, |s| {
                        s.border_top(1.0).border_color(palette::css::LIGHT_GRAY)
                    })
            })
    };

    VirtualList::with_view(move || long_list.get().enumerate(), item_view)
        .style(move |s| s.flex_col().flex_grow(1.0))
        .scroll()
        .style(move |s| s.width(list_width).height(200.0).border(1.0))
}

fn h_buttons_from_iter() -> impl IntoView {
    let button_iter = (0..3).map(|i| button(format!("Button {i}")));
    h_stack_from_iter(button_iter).style(|s| s.gap(5))
}

fn v_buttons_from_iter() -> impl IntoView {
    let button_iter = (0..3).map(|i| button(format!("Button {i}")));
    v_stack_from_iter(button_iter).style(|s| s.gap(5))
}
