use floem::{
    peniko::color::palette,
    prelude::ViewTuple,
    reactive::{create_effect, RwSignal, SignalGet, SignalUpdate},
    style::JustifyContent,
    text::Weight,
    views::{
        button, h_stack, h_stack_from_iter, v_stack, v_stack_from_iter, Checkbox, Decorators,
        LabelClass, ListExt, ScrollExt, VirtualStack, VirtualVector,
    },
    IntoView,
};

use crate::form::{form, form_item};

pub fn virt_list_view() -> impl IntoView {
    v_stack((
        h_stack({
            (
                form((form_item("Simple List".to_string(), 100.0, simple_list),)),
                form((form_item("Enhanced List".to_string(), 120.0, enhanced_list),)),
            )
        }),
        form((form_item(
            "Horizontal Stack from Iterator".to_string(),
            200.0,
            h_buttons_from_iter,
        ),)),
        form((form_item(
            "Vertical Stack from Iterator".to_string(),
            200.0,
            v_buttons_from_iter,
        ),)),
    ))
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
    let long_list: im::Vector<(bool, i32)> = (0..10000).map(|v| (true, v)).collect();
    let long_list = RwSignal::new(long_list);

    let list_width = 180.0;
    let item_height = 32.0;

    let checkmark = |checkbox_state| Checkbox::new_rw(checkbox_state).style(|s| s.margin_left(6));

    let label =
        |item: i32| item.style(|s| s.margin_left(6).height(32.0).font_size(22.0).items_center());

    let x_mark = move |index| {
        " X "
            .on_click_stop(move |_| {
                print!("Item Removed");
                long_list.update(|x| {
                    x.remove(index);
                });
            })
            .style(|s| {
                s.height(18.0)
                    .font_weight(Weight::BOLD)
                    .color(palette::css::RED)
                    .border(1.0)
                    .border_color(palette::css::RED)
                    .border_radius(16.0)
                    .margin_right(20.0)
                    .hover(|s| s.color(palette::css::WHITE).background(palette::css::RED))
            })
            .style(|s| {
                s.flex_basis(0)
                    .justify_content(Some(JustifyContent::FlexEnd))
            })
    };

    VirtualStack::list_with_view(
        move || long_list.get().enumerate(),
        move |(index, (state, item))| {
            let checkbox_state = RwSignal::new(state);
            create_effect(move |_| {
                let state = checkbox_state.get();
                long_list.update(|x| {
                    // because this is an immutable vector, getting the index will always result in the correct item even if we remove elements.
                    if let Some((s, _v)) = x.get_mut(index) {
                        *s = state;
                    };
                });
            });

            (checkmark(checkbox_state), label(item), x_mark(index))
                .h_stack()
                .style(move |s| {
                    s.items_center()
                        .height(item_height)
                        .apply_if(index != 0, |s| {
                            s.border_top(1.0).border_color(palette::css::LIGHT_GRAY)
                        })
                })
        },
    )
    .style(move |s| s.flex_col().flex_grow(1.0))
    .scroll()
    .style(move |s| s.width(list_width).height(200.0).border(1.0))
}

fn h_buttons_from_iter() -> impl IntoView {
    let button_iter = (0..3).map(|i| button(format!("Button {i}")));
    h_stack_from_iter(button_iter)
}

fn v_buttons_from_iter() -> impl IntoView {
    let button_iter = (0..3).map(|i| button(format!("Button {i}")));
    v_stack_from_iter(button_iter)
}
