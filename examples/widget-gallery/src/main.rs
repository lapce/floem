pub mod animation;
pub mod buttons;
pub mod checkbox;
pub mod clipboard;
pub mod context_menu;
pub mod draggable;
pub mod dropdown;
pub mod dropped_file;
pub mod form;
pub mod images;
pub mod inputs;
pub mod labels;
pub mod lists;
pub mod radio_buttons;
pub mod rich_text;
pub mod slider;

use floem::{
    event::{Event, EventListener},
    keyboard::{Key, NamedKey},
    kurbo::Size,
    new_window,
    prelude::*,
    style::{Background, CursorStyle, Transition},
    window::WindowConfig,
};

fn app_view() -> impl IntoView {
    let tabs: Vec<&'static str> = vec![
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
        "Animation",
        "Draggable",
        "DroppedFile",
        "Files",
    ];

    let create_view = |it: &str| match it {
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
        "Animation" => animation::animation_view().into_any(),
        "Draggable" => draggable::draggable_view().into_any(),
        "DroppedFile" => dropped_file::dropped_file_view().into_any(),
        "Files" => files::files_view().into_any(),
        _ => label(|| "Not implemented".to_owned()).into_any(),
    };

    let tabs = RwSignal::new(tabs);

    let (active_tab, set_active_tab) = create_signal(0);

    let side_tab_bar = list(tabs.get().into_iter().enumerate().map(move |(idx, item)| {
        label(move || item)
            .draggable()
            .style(move |s| {
                s.flex_row()
                    .font_size(18.)
                    .padding(5.0)
                    .width(100.pct())
                    .height(36.0)
                    .transition(Background, Transition::ease_in_out(100.millis()))
                    .items_center()
                    .border_bottom(1.)
                    .border_color(Color::LIGHT_GRAY)
                    .selected(|s| {
                        s.border(2.)
                            .border_color(Color::BLUE)
                            .background(Color::GRAY.multiply_alpha(0.6))
                    })
                    .hover(|s| {
                        s.background(Color::LIGHT_GRAY)
                            .apply_if(idx == active_tab.get(), |s| s.background(Color::GRAY))
                            .cursor(CursorStyle::Pointer)
                    })
            })
            .dragging_style(|s| s.background(Color::GRAY.multiply_alpha(0.6)))
    }))
    .on_select(move |idx| {
        if let Some(idx) = idx {
            set_active_tab.set(idx);
        }
    })
    .keyboard_navigable()
    .style(|s| s.flex_col().width(140.0))
    .scroll()
    .debug_name("Side Tab Bar")
    .scroll_style(|s| s.shrink_to_fit())
    .style(|s| {
        s.border(1.)
            .padding(3.)
            .border_color(Color::GRAY)
            .class(LabelClass, |s| s.selectable(false))
    });

    let id = side_tab_bar.id();
    let inspector = button("Open Inspector")
        .action(move || id.inspect())
        .style(|s| s);

    let new_window = button("Open In Window").action(move || {
        let name = tabs.with(|tabs| tabs.get(active_tab.get()).copied());
        new_window(
            move |_| create_view(name.unwrap_or_default()),
            Some(
                WindowConfig::default()
                    .size(Size::new(700.0, 400.0))
                    .title(name.unwrap_or_default()),
            ),
        );
    });

    let left_side_bar = (side_tab_bar, new_window, inspector)
        .v_stack()
        .debug_name("Left Side Bar")
        .style(|s| s.height_full().column_gap(5.0));

    let tab = tab(
        move || Some(active_tab.get()),
        move || tabs.get(),
        |it| *it,
        create_view,
    )
    .debug_name("Active Tab")
    .style(|s| s.flex_col().items_start());

    let tab = scroll(tab).scroll_style(|s| s.shrink_to_fit());

    let view = (left_side_bar, tab)
        .h_stack()
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
