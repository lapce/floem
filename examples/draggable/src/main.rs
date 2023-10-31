use floem::{
    peniko::Color,
    view::View,
    views::{label, Decorators},
};

fn app_view() -> impl View {
    label(|| "Drag me!")
        .style(|s| {
            s.border(1.0)
                .border_radius(2.0)
                .padding(10.0)
                .margin_left(10.0)
                .focus_visible(|s| s.border(2.).border_color(Color::BLUE))
                .hover(|s| {
                    s.background(Color::rgb8(244, 67, 54))
                        .border_radius(0.)
                        .border(2.)
                        .border_color(Color::BLUE)
                        .outline(2.)
                        .outline_color(Color::PALE_GREEN)
                })
                .active(|s| s.color(Color::WHITE).background(Color::RED))
        })
        .keyboard_navigatable()
        .draggable()
        .dragging_style(|s| {
            s.border(2.)
                .border_color(Color::BLACK)
                .outline(20.)
                .outline_color(Color::RED)
        })
}

fn main() {
    floem::launch(app_view);
}
