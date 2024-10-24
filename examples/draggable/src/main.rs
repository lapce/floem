use floem::{
    keyboard::{Key, NamedKey},
    peniko::Color,
    reactive::{create_rw_signal, RwSignal, SignalGet, SignalUpdate},
    style::CursorStyle,
    views::{dyn_stack, label, Decorators},
    IntoView, View,
};

fn sortable_item(
    name: &str,
    sortable_items: RwSignal<Vec<usize>>,
    dragger_id: RwSignal<usize>,
    item_id: usize,
) -> impl IntoView {
    let name = String::from(name);
    let colors = [
        Color::WHITE,
        Color::BEIGE,
        Color::REBECCA_PURPLE,
        Color::TEAL,
        Color::PALE_GREEN,
        Color::YELLOW,
        Color::DODGER_BLUE,
        Color::KHAKI,
        Color::WHEAT,
        Color::DARK_SALMON,
        Color::HOT_PINK,
    ];

    (
        label(move || format!("Selectable item {name}"))
            .style(|s| s.padding(5).width_full())
            .on_event_stop(
                floem::event::EventListener::PointerDown,
                |_| { /* Disable dragging for this view */ },
            ),
        label(|| "drag me").style(|s| {
            s.selectable(false)
                .padding(2)
                .cursor(CursorStyle::RowResize)
        }),
    )
        .draggable()
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
                .box_shadow_color(Color::rgb8(100, 100, 100))
                .box_shadow_spread(2)
        })
        .style(move |s| {
            s.background(colors[item_id])
                .selectable(false)
                .row_gap(5)
                .items_center()
                .border(2)
                .border_color(Color::RED)
        })
}

fn app_view() -> impl IntoView {
    let items = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    let sortable_items = create_rw_signal((0..items.len()).collect::<Vec<usize>>());
    let dragger_id = create_rw_signal(0);

    let view = dyn_stack(
        move || sortable_items.get(),
        move |item_id| *item_id,
        move |item_id| sortable_item(items[item_id], sortable_items, dragger_id, item_id),
    )
    .style(|s| s.flex_col().column_gap(5).padding(10))
    .into_view();

    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect(),
    )
}

fn main() {
    floem::launch(app_view);
}
