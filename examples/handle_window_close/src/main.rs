use floem::{
    close_window, new_window,
    event::listener,
    kurbo::Size,
    prelude::{palette::css, *},
    reactive::{RwSignal, SignalGet},
    views::{Button, Empty, Label, Overlay, Stack},
    window::{WindowConfig, WindowId},
};

fn confirm_overlay(window_id: WindowId, show_confirm: RwSignal<bool>) -> Overlay {
    Overlay::new_dyn(move || {
        let visible = show_confirm.get();

        Stack::new((
            Empty::new()
            .style(|s| {
                s.absolute()
                .inset(0.0)
                .background(css::BLACK)
                .opacity(0.25)
                .z_index(1)
            })
            .on_event_cont(listener::Click, move |_, _| {
                show_confirm.set(false);
            }),
            Stack::vertical((
                Label::new("Close this window?")
                .style(|s| s.font_size(18.0)),
                             Label::new("Unsaved work in this window would be lost.")
                             .style(|s| s.color(css::DIM_GRAY)),
                             Stack::horizontal((
                                 Button::new("Yes").action(move || {
                                     close_window(window_id);
                                 }),
                                 Button::new("No").action(move || {
                                     show_confirm.set(false);
                                 }),
                             ))
                             .style(|s| s.col_gap(8.0)),
            ))
            .style(|s| {
                s.absolute()
                .inset_left(40.0)
                .inset_top(40.0)
                .width(320.0)
                .padding(16.0)
                .row_gap(12.0)
                .border(1.0)
                .border_radius(12.0)
                .border_color(css::LIGHT_GRAY)
                .background(css::WHITE)
                .z_index(10)
            }),
        ))
        .style(move |s| {
            s.fixed()
            .inset(0.0)
            .width_full()
            .height_full()
            .apply_if(!visible, |s| s.hide())
        })
    })
}

fn closable_window<V: IntoView + 'static>(window_id: WindowId, content: V) -> impl IntoView {
    let show_confirm = RwSignal::new(false);

    Stack::new((content, confirm_overlay(window_id, show_confirm)))
    .style(|s| s.size_full())
    .on_event_cont(listener::WindowCloseRequested, move |cx, _| {
        cx.prevent_default();
        show_confirm.set(true);
    })
}

fn second_window_view(window_id: WindowId) -> impl IntoView {
    let body = Stack::vertical((
        Label::new("Second window").style(|s| s.font_size(24.0)),
                                Label::new("Try closing this window from the title bar."),
    ))
    .style(|s| s.size_full().padding(24.0).row_gap(12.0));

    closable_window(window_id, body)
}

fn main_window_view(window_id: WindowId) -> impl IntoView {
    let body = Stack::vertical((
        Label::new("Main window").style(|s| s.font_size(24.0)),
                                Label::new("Use the button below to open a second window."),
                                Button::new("Open second window").action(|| {
                                    new_window(
                                        second_window_view,
                                        Some(
                                            WindowConfig::default()
                                            .size(Size::new(420.0, 240.0))
                                            .title("Second Window"),
                                        ),
                                    );
                                }),
    ))
    .style(|s| s.size_full().padding(24.0).row_gap(12.0));

    closable_window(window_id, body)
}

fn main() {
    floem::Application::new()
    .window(
        main_window_view,
        Some(
            WindowConfig::default()
            .size(Size::new(520.0, 320.0))
            .title("Main Window"),
        ),
    )
    .run();
}
