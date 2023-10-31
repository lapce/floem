use floem::{
    cosmic_text::Weight,
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    peniko::Color,
    reactive::create_signal,
    style::{CursorStyle, JustifyContent},
    unit::UnitExt,
    view::View,
    views::{
        checkbox, container, label, scroll, stack, virtual_list, Decorators, VirtualListDirection,
        VirtualListItemSize,
    },
};

use crate::form::{form, form_item};

pub fn virt_list_view() -> impl View {
    stack({
        (
            form((form_item("Simple List".to_string(), 120.0, simple_list),)),
            form((form_item("Enhanced List".to_string(), 120.0, enhanced_list),)),
        )
    })
}

fn simple_list() -> impl View {
    let long_list: im::Vector<i32> = (0..100).collect();
    let (long_list, _set_long_list) = create_signal(long_list);
    scroll(
        virtual_list(
            VirtualListDirection::Vertical,
            VirtualListItemSize::Fixed(Box::new(|| 20.0)),
            move || long_list.get(),
            move |item| *item,
            move |item| label(move || item.to_string()).style(|s| s.height(24.0)),
        )
        .style(|s| s.flex_col()),
    )
    .style(|s| s.width(100.0).height(300.0).border(1.0))
}

fn enhanced_list() -> impl View {
    let long_list: im::Vector<i32> = (0..100).collect();
    let (long_list, set_long_list) = create_signal(long_list);

    let (selected, set_selected) = create_signal(0);
    let list_width = 180.0;
    let item_height = 32.0;
    scroll(
        virtual_list(
            VirtualListDirection::Vertical,
            VirtualListItemSize::Fixed(Box::new(|| 32.0)),
            move || long_list.get(),
            move |item| *item,
            move |item| {
                let index = long_list
                    .get_untracked()
                    .iter()
                    .position(|it| *it == item)
                    .unwrap();
                let (is_checked, set_is_checked) = create_signal(true);
                container({
                    stack({
                        (
                            checkbox(is_checked).on_click(move |_| {
                                set_is_checked.update(|checked: &mut bool| *checked = !*checked);
                                true
                            }),
                            label(move || item.to_string())
                                .style(|s| s.height(32.0).font_size(32.0)),
                            container({
                                label(move || " X ")
                                    .on_click(move |_| {
                                        print!("Item Removed");
                                        set_long_list.update(|x| {
                                            x.remove(index);
                                        });
                                        true
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
                    .style(move |s| s.height(item_height).width(list_width).items_center())
                })
                .on_click(move |_| {
                    set_selected.update(|v: &mut usize| {
                        *v = long_list.get().iter().position(|it| *it == item).unwrap();
                    });
                    true
                })
                .on_event(EventListener::KeyDown, move |e| {
                    if let Event::KeyDown(key_event) = e {
                        let sel = selected.get();
                        match key_event.key.logical_key {
                            Key::Named(NamedKey::ArrowUp) => {
                                if sel > 0 {
                                    set_selected.update(|v| *v -= 1);
                                }
                                true
                            }
                            Key::Named(NamedKey::ArrowDown) => {
                                if sel < long_list.get().len() - 1 {
                                    set_selected.update(|v| *v += 1);
                                }
                                true
                            }
                            _ => false,
                        }
                    } else {
                        false
                    }
                })
                .keyboard_navigatable()
                .style(move |s| {
                    s.flex_row()
                        .width(list_width.pct())
                        .height(item_height)
                        .apply_if(index == selected.get(), |s| s.background(Color::GRAY))
                        .apply_if(index != 0, |s| {
                            s.border_top(1.0).border_color(Color::LIGHT_GRAY)
                        })
                        .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                        .hover(|s| s.background(Color::LIGHT_GRAY).cursor(CursorStyle::Pointer))
                })
            },
        )
        .style(move |s| s.flex_col().width(list_width)),
    )
    .style(move |s| s.width(list_width).height(300.0).border(1.0))
}
