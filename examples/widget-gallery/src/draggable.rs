use floem::{prelude::*, style::CursorStyle};

fn sortable_item(
    name: &str,
    sortable_items: RwSignal<Vec<usize>>,
    dragger_id: RwSignal<usize>,
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

    (
        label(move || format!("Selectable item {name}"))
            .style(|s| s.padding(5).width_full())
            .on_event_stop(
                floem::event::EventListener::PointerDown,
                |_| { /* Disable dragging for this view */ },
            ),
        "drag me".style(|s| {
            s.selectable(false)
                .padding(2)
                .cursor(CursorStyle::RowResize)
        }),
    )
        .on_event(floem::event::EventListener::DragStart, move |_| {
            dragger_id.set(item_id);
            floem::event::EventPropagation::Continue
        })
        .on_event(floem::event::EventListener::DragOver, move |_| {
            if dragger_id.get_untracked() != item_id {
                let dragger_pos = sortable_items
                    .get()
                    .iter()
                    .position(|id| *id == dragger_id.get_untracked())
                    .unwrap();
                let hover_pos = sortable_items
                    .get()
                    .iter()
                    .position(|id| *id == item_id)
                    .unwrap();

                sortable_items.update(|items| {
                    items.remove(dragger_pos);
                    items.insert(hover_pos, dragger_id.get_untracked());
                });
            }
            floem::event::EventPropagation::Continue
        })
        .dragging_style(|s| {
            s.box_shadow_blur(3)
                .box_shadow_color(Color::from_rgb8(100, 100, 100))
                .box_shadow_spread(2)
        })
        .style(move |s| {
            s.background(colors[item_id])
                .selectable(false)
                .draggable(true)
                .col_gap(5)
                .items_center()
                .border(2)
                .border_color(palette::css::RED)
        })
}

pub fn draggable_view() -> impl IntoView {
    let items = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    let sortable_items = RwSignal::new((0..items.len()).collect::<Vec<usize>>());
    let dragger_id = RwSignal::new(0);

    dyn_stack(
        move || sortable_items.get(),
        move |item_id| *item_id,
        move |item_id| sortable_item(items[item_id], sortable_items, dragger_id, item_id),
    )
    .style(|s| s.flex_col().row_gap(5).padding(10))
    .into_view()
}
