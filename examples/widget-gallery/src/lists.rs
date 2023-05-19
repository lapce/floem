use floem::{
    cosmic_text::Weight,
    event::{Event, EventListner},
    glazier::keyboard_types::Key,
    peniko::Color,
    reactive::{create_signal, SignalGet, SignalUpdate},
    style::{CursorStyle, Dimension, JustifyContent, Style},
    view::View,
    views::{
        checkbox, container, label, scroll, stack, virtual_list, Decorators, VirtualListDirection,
        VirtualListItemSize,
    },
    AppContext,
};

use crate::form::{form, form_item};

pub fn virt_list_view() -> impl View {
    stack(|| {
        (
            form(|| (form_item("Simple List".to_string(), 120.0, simple_list),)),
            form(|| (form_item("Enhanced List".to_string(), 120.0, enhanced_list),)),
        )
    })
}

fn simple_list() -> impl View {
    let cx = AppContext::get_current();
    let long_list: im::Vector<i32> = (0..100).collect();
    let (long_list, _set_long_list) = create_signal(cx.scope, long_list);
    scroll(move || {
        virtual_list(
            VirtualListDirection::Vertical,
            VirtualListItemSize::Fixed(Box::new(|| 20.0)),
            move || long_list.get(),
            move |item| *item,
            move |item| label(move || item.to_string()).style(|| Style::BASE.height_px(24.0)),
        )
        .style(|| Style::BASE.flex_col())
    })
    .style(|| Style::BASE.width_px(100.0).height_px(300.0).border(1.0))
}

fn enhanced_list() -> impl View {
    let cx = AppContext::get_current();
    let long_list: im::Vector<i32> = (0..100).collect();
    let (long_list, set_long_list) = create_signal(cx.scope, long_list);

    let (selected, set_selected) = create_signal(cx.scope, 0);
    let list_width = 180.0;
    let item_height = 32.0;
    scroll(move || {
        virtual_list(
            VirtualListDirection::Vertical,
            VirtualListItemSize::Fixed(Box::new(|| 32.0)),
            move || long_list.get(),
            move |item| *item,
            move |item| {
                let index = long_list.get().iter().position(|it| *it == item).unwrap();
                let (is_checked, set_is_checked) = create_signal(cx.scope, true);
                container(move || {
                    stack(move || {
                        (
                            checkbox(is_checked).on_click(move |_| {
                                set_is_checked.update(|checked: &mut bool| *checked = !*checked);
                                true
                            }),
                            label(move || item.to_string())
                                .style(|| Style::BASE.height_px(32.0).font_size(32.0)),
                            container(move || {
                                label(move || " X ".to_string())
                                    .on_click(move |_| {
                                        print!("Item Removed");
                                        set_long_list.update(|x| {
                                            x.remove(index);
                                        });
                                        true
                                    })
                                    .style(|| {
                                        Style::BASE
                                            .height_px(18.0)
                                            .font_weight(Weight::BOLD)
                                            .color(Color::RED)
                                            .border(1.0)
                                            .border_color(Color::RED)
                                            .border_radius(16.0)
                                            .margin_right_px(5.0)
                                    })
                                    .hover_style(|| {
                                        Style::BASE.color(Color::WHITE).background(Color::RED)
                                    })
                            })
                            .style(|| {
                                Style::BASE
                                    .flex_basis(Dimension::Points(0.0))
                                    .flex_grow(1.0)
                                    .justify_content(Some(JustifyContent::FlexEnd))
                            }),
                        )
                    })
                    .style(move || {
                        Style::BASE
                            .height_px(item_height)
                            .width_px(list_width)
                            .items_center()
                    })
                })
                .on_click(move |_| {
                    set_selected.update(|v: &mut usize| {
                        *v = long_list.get().iter().position(|it| *it == item).unwrap();
                    });
                    true
                })
                .on_event(EventListner::KeyDown, move |e| {
                    if let Event::KeyDown(key_event) = e {
                        let sel = selected.get();
                        match key_event.key {
                            Key::ArrowUp => {
                                if sel > 0 {
                                    set_selected.update(|v| *v -= 1);
                                }
                                true
                            }
                            Key::ArrowDown => {
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
                .focus_visible_style(|| Style::BASE.border(2.).border_color(Color::BLUE))
                .style(move || {
                    Style::BASE
                        .flex_row()
                        .width_pct(list_width)
                        .height_px(item_height)
                        .apply_if(index == selected.get(), |s| s.background(Color::GRAY))
                        .apply_if(index != 0, |s| {
                            s.border_top(1.0).border_color(Color::LIGHT_GRAY)
                        })
                })
                .hover_style(|| {
                    Style::BASE
                        .background(Color::LIGHT_GRAY)
                        .cursor(CursorStyle::Pointer)
                })
            },
        )
        .style(move || Style::BASE.flex_col().width_px(list_width))
    })
    .style(move || {
        Style::BASE
            .width_px(list_width)
            .height_px(300.0)
            .border(1.0)
    })
}
