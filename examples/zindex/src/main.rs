use floem::event::{Event, EventListener, EventPropagation};
use floem::views::h_stack;
use floem::{
    peniko::Color,
    reactive::provide_context,
    views::{empty, v_stack, Decorators},
    IntoView,
};

fn context_container(
    name: String,
    merge_left: f64,
    merge_top: f64,
    zindex: Option<i32>,
) -> impl IntoView {
    empty()
        .style(move |s| {
            let s = s
                .absolute()
                .border(1)
                .width(80.0)
                .height(80.0)
                .inset_left(merge_left)
                .inset_top(merge_top);
            if let Some(zindex) = zindex {
                s.z_index(zindex)
            } else {
                s
            }
        })
        .on_event(EventListener::PointerUp, {
            let name = name.clone();
            move |x| {
                if let Event::PointerUp(_) = x {
                    println!("{}-zindex {:?}", name, zindex);
                }
                EventPropagation::Continue
            }
        })
        .debug_name(name)
}

fn app_view() -> impl IntoView {
    provide_context(Color::BLACK);

    h_stack((
        v_stack((
            context_container("1.1".into(), 0.0, 0.0, Some(20)),
            context_container("1.2".into(), 60.0, 0.0, None),
        )),
        v_stack((
            context_container("2.1".into(), 0.0, 60.0, Some(10)),
            context_container("2.2".into(), 60.0, 60.0, Some(40)),
        )),
    ))
    .style(|s| s.absolute().inset_top(10.0).inset_left(10.0))
}

fn main() {
    floem::launch(app_view);
}
