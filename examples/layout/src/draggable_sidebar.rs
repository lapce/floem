use floem::{
    event::{Event, EventListener, EventPropagation},
    peniko::Color,
    reactive::{create_rw_signal, create_signal, SignalGet, SignalUpdate},
    style::{CursorStyle, Position},
    views::{
        container, h_stack, label, scroll, virtual_stack, Decorators, VirtualDirection,
        VirtualItemSize,
    },
    IntoView, View,
};

const SIDEBAR_WIDTH: f64 = 100.0;

pub fn draggable_sidebar_view() -> impl IntoView {
    let long_list: im::Vector<i32> = (0..100).collect();
    let (long_list, _set_long_list) = create_signal(long_list);
    let sidebar_width = create_rw_signal(SIDEBAR_WIDTH);
    let is_sidebar_dragging = create_rw_signal(false);

    let side_bar = scroll({
        virtual_stack(
            VirtualDirection::Vertical,
            VirtualItemSize::Fixed(Box::new(|| 22.0)),
            move || long_list.get(),
            move |item| *item,
            move |item| {
                label(move || format!("Item {} with long lines", item)).style(move |s| {
                    s.text_ellipsis()
                        .height(22)
                        .padding(10.0)
                        .padding_top(3.0)
                        .padding_bottom(3.0)
                        .width(sidebar_width.get())
                        .items_start()
                        .border_bottom(1.0)
                        .border_color(Color::rgb8(205, 205, 205))
                })
            },
        )
        .style(move |s| s.flex_col().width(sidebar_width.get() - 1.0))
    })
    .style(move |s| {
        s.width(sidebar_width.get())
            .border_right(1.0)
            .border_top(1.0)
            .border_color(Color::rgb8(205, 205, 205))
    });

    let main_window = scroll(
        container(
            label(move || String::from("<-- drag me!\n \n(double click to return to default)"))
                .style(|s| s.padding(10.0)),
        )
        .style(|s| s.flex_col().items_start().padding_bottom(10.0)),
    )
    .style(|s| {
        s.flex_col()
            .flex_basis(0)
            .min_width(0)
            .flex_grow(1.0)
            .border_top(1.0)
            .border_color(Color::rgb8(205, 205, 205))
    });

    let dragger = label(|| "")
        .style(move |s| {
            s.position(Position::Absolute)
                .z_index(10)
                .inset_top(0)
                .inset_bottom(0)
                .inset_left(sidebar_width.get())
                .width(10)
                .border_left(1)
                .border_color(Color::rgb8(205, 205, 205))
                .hover(|s| {
                    s.border_left(2)
                        .border_color(Color::rgb8(41, 98, 218))
                        .cursor(CursorStyle::ColResize)
                })
                .apply_if(is_sidebar_dragging.get(), |s| {
                    s.border_left(2).border_color(Color::rgb8(41, 98, 218))
                })
        })
        .draggable()
        .dragging_style(|s| s.border_color(Color::TRANSPARENT))
        .on_event(EventListener::DragStart, move |_| {
            is_sidebar_dragging.set(true);
            EventPropagation::Continue
        })
        .on_event(EventListener::DragEnd, move |_| {
            is_sidebar_dragging.set(false);
            EventPropagation::Continue
        })
        .on_event(EventListener::DoubleClick, move |_| {
            sidebar_width.set(SIDEBAR_WIDTH);
            EventPropagation::Continue
        });

    let view = h_stack((side_bar, dragger, main_window))
        .on_event(EventListener::PointerMove, move |event| {
            let pos = match event {
                Event::PointerMove(p) => p.pos,
                _ => (0.0, 0.0).into(),
            };

            if is_sidebar_dragging.get() {
                sidebar_width.set(pos.x);
            }
            EventPropagation::Continue
        })
        .style(|s| s.width_full().height_full());

    let id = view.id();
    view.on_event_stop(EventListener::KeyUp, move |e| {
        if let floem::event::Event::KeyUp(e) = e {
            if e.key.logical_key == floem::keyboard::Key::Named(floem::keyboard::NamedKey::F11) {
                id.inspect();
            }
        }
    })
}
