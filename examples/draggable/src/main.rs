use floem::{
    peniko::Color,
    style::Style,
    view::View,
    views::{label, Decorators},
};

fn app_view() -> impl View {
    label(|| "Drag me!".to_string())
        .style(|| {
            Style::BASE
                .border(1.0)
                .border_radius(2.0)
                .padding_px(10.0)
                .margin_left_px(10.0)
        })
        .hover_style(|| {
            Style::BASE
                .background(Color::rgb8(244, 67, 54))
                .border_radius(0.)
                .border(2.)
                .border_color(Color::BLUE)
                .outline(2.)
                .outline_color(Color::PALE_GREEN)
        })
        .active_style(|| Style::BASE.color(Color::WHITE).background(Color::RED))
        .keyboard_navigatable()
        .focus_visible_style(|| Style::BASE.border_color(Color::BLUE).border(2.))
        .draggable()
        .dragging_style(|| {
            Style::BASE
                .border(2.)
                .border_color(Color::BLACK)
                .outline(20.)
                .outline_color(Color::RED)
        })
}

fn main() {
    floem::launch(app_view);
}
