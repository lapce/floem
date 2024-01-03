use floem::{
    cosmic_text::Weight,
    peniko::Color,
    reactive::create_signal,
    style::JustifyContent,
    view::View,
    views::{
        container, label, scroll, stack, Decorators, VirtualDirection, VirtualItemSize,
        VirtualVector,
    },
    widgets::{checkbox, list, virtual_list},
};

use crate::form::{form, form_item};

pub fn virt_list_view() -> impl View {
    stack({
        (
            form((form_item("Simple List".to_string(), 100.0, simple_list),)),
            form((form_item("Enhanced List".to_string(), 120.0, enhanced_list),)),
        )
    })
}

fn simple_list() -> impl View {
    scroll(
        list((0..100).map(|i| label(move || i.to_string()).style(|s| s.height(24.0))))
            .style(|s| s.width_full()),
    )
    .style(|s| s.width(100.0).height(300.0).border(1.0))
}

fn enhanced_list() -> impl View {
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
                let (is_checked, set_is_checked) = create_signal(true);
                container({
                    stack({
                        (
                            checkbox(is_checked).on_click_stop(move |_| {
                                set_is_checked.update(|checked: &mut bool| *checked = !*checked);
                            }),
                            label(move || item.to_string())
                                .style(|s| s.height(32.0).font_size(22.0)),
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
                                            .margin_right(5.0)
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
                    s.flex_row().height(item_height).apply_if(index != 0, |s| {
                        s.border_top(1.0).border_color(Color::LIGHT_GRAY)
                    })
                })
            },
        )
        .style(move |s| s.flex_col().flex_grow(1.0)),
    )
    .style(move |s| s.width(list_width).height(300.0).border(1.0))
}
