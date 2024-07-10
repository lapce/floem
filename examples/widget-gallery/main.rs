pub mod buttons;
pub mod checkbox;
pub mod clipboard;
pub mod context_menu;
pub mod dropdown;
pub mod form;
pub mod images;
pub mod inputs;
pub mod labels;
pub mod lists;
pub mod radio_buttons;
pub mod rich_text;
pub mod slider;

use floem::{
    event::{Event, EventListener, EventPropagation},
    keyboard::{Key, NamedKey},
    peniko::Color,
    reactive::create_signal,
    style::{Background, CursorStyle, Transition},
    unit::UnitExt,
    views::{
        button, container, h_stack, label, scroll, stack, tab, v_stack, virtual_stack, Decorators,
        VirtualDirection, VirtualItemSize,
    },
    IntoView, View,
};

fn app_view() -> impl IntoView {
    let tabs: im::Vector<&str> = vec![
        "Label",
        "Button",
        "Checkbox",
        "Radio",
        "Input",
        "List",
        "Menu",
        "RichText",
        "Image",
        "Clipboard",
        "Slider",
        "Dropdown",
    ]
    .into_iter()
    .collect();
    let (tabs, _set_tabs) = create_signal(tabs);

    let (active_tab, set_active_tab) = create_signal(0);

    let list = scroll({
        virtual_stack(
            VirtualDirection::Vertical,
            VirtualItemSize::Fixed(Box::new(|| 36.0)),
            move || tabs.get(),
            move |item| *item,
            move |item| {
                let index = tabs
                    .get_untracked()
                    .iter()
                    .position(|it| *it == item)
                    .unwrap();
                stack((label(move || item).style(|s| s.font_size(18.0)),))
                    .on_click_stop(move |_| {
                        set_active_tab.update(|v: &mut usize| {
                            *v = tabs
                                .get_untracked()
                                .iter()
                                .position(|it| *it == item)
                                .unwrap();
                        });
                    })
                    .on_event(EventListener::KeyDown, move |e| {
                        if let Event::KeyDown(key_event) = e {
                            let active = active_tab.get();
                            if key_event.modifiers.is_empty() {
                                match key_event.key.logical_key {
                                    Key::Named(NamedKey::ArrowUp) => {
                                        if active > 0 {
                                            set_active_tab.update(|v| *v -= 1)
                                        }
                                        EventPropagation::Stop
                                    }
                                    Key::Named(NamedKey::ArrowDown) => {
                                        if active < tabs.get().len() - 1 {
                                            set_active_tab.update(|v| *v += 1)
                                        }
                                        EventPropagation::Stop
                                    }
                                    _ => EventPropagation::Continue,
                                }
                            } else {
                                EventPropagation::Continue
                            }
                        } else {
                            EventPropagation::Continue
                        }
                    })
                    .keyboard_navigatable()
                    .draggable()
                    .style(move |s| {
                        s.flex_row()
                            .padding(5.0)
                            .width(100.pct())
                            .height(36.0)
                            .transition(Background, Transition::linear(0.4))
                            .items_center()
                            .border_bottom(1.0)
                            .border_color(Color::LIGHT_GRAY)
                            .apply_if(index == active_tab.get(), |s| {
                                s.background(Color::GRAY.with_alpha_factor(0.6))
                            })
                            .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                            .hover(|s| {
                                s.background(Color::LIGHT_GRAY)
                                    .apply_if(index == active_tab.get(), |s| {
                                        s.background(Color::GRAY)
                                    })
                                    .cursor(CursorStyle::Pointer)
                            })
                    })
            },
        )
        .style(|s| s.flex_col().width(140.0))
    })
    .style(|s| {
        s.flex_col()
            .width(140.0)
            .flex_grow(1.0)
            .min_height(0)
            .flex_basis(0)
    });

    let list = container(list).style(|s| {
        s.border(1.0)
            .border_color(Color::GRAY)
            .flex_grow(1.0)
            .min_height(0)
    });

    let id = list.id();
    let inspector = button(|| "Open Inspector")
        .on_click_stop(move |_| {
            id.inspect();
        })
        .style(|s| s);

    let left = v_stack((list, inspector)).style(|s| s.height_full().column_gap(5.0));

    let tab = tab(
        move || active_tab.get(),
        move || tabs.get(),
        |it| *it,
        |it| match it {
            "Label" => labels::label_view().into_any(),
            "Button" => buttons::button_view().into_any(),
            "Checkbox" => checkbox::checkbox_view().into_any(),
            "Radio" => radio_buttons::radio_buttons_view().into_any(),
            "Input" => inputs::text_input_view().into_any(),
            "List" => lists::virt_list_view().into_any(),
            "Menu" => context_menu::menu_view().into_any(),
            "RichText" => rich_text::rich_text_view().into_any(),
            "Image" => images::img_view().into_any(),
            "Clipboard" => clipboard::clipboard_view().into_any(),
            "Slider" => slider::slider_view().into_any(),
            "Dropdown" => dropdown::dropdown_view().into_any(),
            _ => label(|| "Not implemented".to_owned()).into_any(),
        },
    )
    .style(|s| s.flex_col().items_start());

    let tab = scroll(tab).style(|s| s.flex_basis(0).min_width(0).flex_grow(1.0));

    let view = h_stack((left, tab))
        .style(|s| s.padding(5.0).width_full().height_full().row_gap(5.0))
        .window_title(|| "Widget Gallery".to_owned());

    let id = view.id();
    view.on_event_stop(EventListener::KeyUp, move |e| {
        if let Event::KeyUp(e) = e {
            if e.key.logical_key == Key::Named(NamedKey::F11) {
                id.inspect();
            }
        }
    })
}

fn main() {
    floem::launch(app_view);
}
