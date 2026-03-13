use floem::{
    easing,
    event::DragConfig,
    peniko::{Brush, color::Oklab},
    prelude::{palette::css, *},
    style::{Background, CursorStyle, TextColor},
    style_class,
    theme::StyleThemeExt,
};

style_class!(DraggableItem);

pub fn get_easing(idx: usize) -> Box<dyn easing::Easing> {
    match idx {
        0 => Box::new(easing::Linear),
        1 => Box::new(easing::Bezier::ease_in()) as Box<dyn easing::Easing>,
        2 => Box::new(easing::Bezier::ease_out()),
        3 => Box::new(easing::Bezier::ease_in_out()),
        4 => Box::new(easing::Spring::gentle()),
        5 => Box::new(easing::Spring::bouncy()),
        6 => Box::new(easing::Spring::snappy()),
        7 => Box::new(easing::Step::new(3, easing::StepPosition::Start)),
        _ => panic!(),
    }
}

fn sortable_item(
    name: &str,
    sortable_items: RwSignal<Vec<usize>>,
    easing_idx: RwSignal<usize>,
    item_id: usize,
) -> impl IntoView {
    let name = String::from(name);
    let colors = [
        palette::css::WHITE,
        palette::css::BEIGE,
        palette::css::REBECCA_PURPLE,
        palette::css::TEAL,
        palette::css::PALE_GREEN,
        palette::css::YELLOW,
        palette::css::DODGER_BLUE,
        palette::css::KHAKI,
        palette::css::WHEAT,
        palette::css::DARK_SALMON,
        palette::css::HOT_PINK,
    ];

    Stack::horizontal((
        Label::derived(move || format!("Selectable item {name}"))
            .style(|s| s.padding(5).width_full())
            .on_event_stop(
                listener::PointerDown,
                |_, _| { /* Disable dragging for this view */ },
            ),
        "drag me".style(|s| {
            s.selectable(false)
                .padding(2)
                .flex_shrink(0.)
                .cursor(CursorStyle::RowResize)
        }),
    ))
    .on_event_stop(listener::DragTargetEnter, move |_, drag_enter| {
        if let Some(custom_data) = &drag_enter.custom_data
                && let Some(dragged_id) = custom_data.downcast_ref::<usize>() // in a different app with more draggable items you would want to use a unique type for your custom data
                && *dragged_id != item_id
        {
            let dragger_pos = sortable_items
                .get()
                .iter()
                .position(|id| id == dragged_id)
                .unwrap();
            let hover_pos = sortable_items
                .get()
                .iter()
                .position(|id| *id == item_id)
                .unwrap();

            sortable_items.update(|items| {
                items.remove(dragger_pos);
                items.insert(hover_pos, *dragged_id);
            });
        }
    })
    .dragging_style(|s| {
        s.box_shadow_blur(3)
            .box_shadow_color(Color::from_rgb8(100, 100, 100))
            .box_shadow_spread(2)
    })
    .style(move |s| {
        s.background(colors[item_id])
            .selectable(false)
            .col_gap(5)
            .items_center()
            .border(2)
            .border_color(palette::css::RED)
    })
    .draggable_with_config(move || {
        DragConfig::default()
            .with_custom_data(item_id)
            .with_easing(get_easing(easing_idx.get()))
    })
    .class(DraggableItem)
}

pub fn draggable_view() -> impl IntoView {
    let easing = RwSignal::new(0);
    let easings = [
        "linear",
        "ease_in",
        "ease_out",
        "ease_in_out",
        "spring_gentle",
        "spring_bouncy",
        "spring_snappy",
        "step_3",
    ]
    .map(|i| {
        i.style(|s| {
            s.text_clip()
                .items_center()
                .justify_center()
                .padding_vert(10.)
                .padding_horiz(5.)
                .selectable(false)
                .cursor(CursorStyle::Pointer)
        })
    })
    .list()
    .on_select(move |idx| {
        if let Some(idx) = idx {
            easing.set(idx);
        }
    })
    .style(|s| s.flex_row().gap(5))
    .scroll()
    .style(|s| {
        s.max_width(400.)
            .flex_row()
            .padding_right(3.)
            .scrollbar_width(0.)
            .border_horiz(3.)
            .with_theme(|s, t| s.border_color(t.border()))
    });

    let easings = ("Drag release easing:", easings).style(|s| s.gap(10).items_center());

    let items = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    let sortable_items = RwSignal::new((0..items.len()).collect::<Vec<usize>>());

    let items_stack = dyn_stack(
        move || sortable_items.get(),
        move |item_id| *item_id,
        move |item_id| sortable_item(items[item_id], sortable_items, easing, item_id),
    )
    .style(|s| {
        s.flex_col()
            .row_gap(5)
            .padding(10)
            .max_width(300.)
            .class(DraggableItem, |s| {
                s.with::<Background>(move |s, b| {
                    s.set_context_opt(
                        TextColor,
                        b.def(|b| {
                            b.and_then(|b| {
                                if let Brush::Solid(c) = b {
                                    let l = c.convert::<Oklab>().components[0];
                                    Some(if l < 0.5 { css::WHITE } else { css::BLACK })
                                } else {
                                    None
                                }
                            })
                        }),
                    )
                })
            })
    });

    Stack::vertical((easings, items_stack)).style(|s| s.items_center())
}
