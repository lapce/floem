use floem::{
    peniko::Color,
    reactive::{create_signal, RwSignal, SignalGet, SignalUpdate},
    style::JustifyContent,
    text::Weight,
    views::{
        button, container, h_stack, h_stack_from_iter, label, list, scroll, stack, v_stack,
        v_stack_from_iter, virtual_list, Checkbox, Decorators, VirtualDirection, VirtualItemSize,
        VirtualVector,
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
    scroll(
        list(
            (0..100).map(|i| label(move || i.to_string()).style(|s| s.height(24.0).items_center())),
        )
        .style(|s| s.width_full()),
    )
    .style(|s| s.width(100.0).height(200.0).border(1.0))
}

fn enhanced_list() -> impl IntoView {
    let long_list: im::Vector<i32> = (0..100).collect();
    let (long_list, set_long_list) = create_signal(long_list);

    let list_width = 180.0;
    let item_height = 32.0;
    scroll(
        virtual_list(
            VirtualDirection::Vertical,
            VirtualItemSize::Fixed(Box::new(|| 32.0)),
            move || long_list.get().enumerate(),
            move |(_, item)| *item,
            move |(index, item)| {
                let checkbox_state = RwSignal::new(true);
                container({
                    stack({
                        (
                            Checkbox::new_get_set(checkbox_state).style(|s| s.margin_left(6)),
                            label(move || item.to_string()).style(|s| {
                                s.margin_left(6).height(32.0).font_size(22.0).items_center()
                            }),
                            container({
                                label(move || " X ")
                                    .on_click_stop(move |_| {
                                        print!("Item Removed");
                                        set_long_list.update(|x| {
                                            x.remove(index);
                                        });
                                    })
                                    .style(|s| {
                                        s.height(18.0)
                                            .font_weight(Weight::BOLD)
                                            .color(Color::RED)
                                            .border(1.0)
                                            .border_color(Color::RED)
                                            .border_radius(16.0)
                                            .margin_right(20.0)
                                            .hover(|s| s.color(Color::WHITE).background(Color::RED))
                                    })
                            })
                            .style(|s| {
                                s.flex_basis(0)
                                    .flex_grow(1.0)
                                    .justify_content(Some(JustifyContent::FlexEnd))
                            }),
                        )
                    })
                    .style(move |s| s.height_full().width_full().items_center())
                })
                .style(move |s| {
                    s.flex_row()
                        .items_center()
                        .height(item_height)
                        .apply_if(index != 0, |s| {
                            s.border_top(1.0).border_color(Color::LIGHT_GRAY)
                        })
                })
            },
        )
        .style(move |s| s.flex_col().flex_grow(1.0)),
    )
    .style(move |s| s.width(list_width).height(200.0).border(1.0))
}

fn h_buttons_from_iter() -> impl IntoView {
    let button_iter = (0..3).map(|i| button(move || format!("Button {}", i)));
    h_stack_from_iter(button_iter)
}

fn v_buttons_from_iter() -> impl IntoView {
    let button_iter = (0..3).map(|i| button(move || format!("Button {}", i)));
    v_stack_from_iter(button_iter)
}
