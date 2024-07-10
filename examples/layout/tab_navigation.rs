use floem::{
    cosmic_text::Weight,
    event::EventListener,
    peniko::Color,
    reactive::{create_signal, ReadSignal, WriteSignal},
    style::{CursorStyle, Position},
    views::{container, h_stack, label, scroll, tab, v_stack, Decorators},
    IntoView, View,
};

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Tabs {
    General,
    Settings,
    Feedback,
}

impl std::fmt::Display for Tabs {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Tabs::General => write!(f, "General"),
            Tabs::Settings => write!(f, "Settings"),
            Tabs::Feedback => write!(f, "Feedback"),
        }
    }
}

fn tab_button(
    this_tab: Tabs,
    tabs: ReadSignal<im::Vector<Tabs>>,
    set_active_tab: WriteSignal<usize>,
    active_tab: ReadSignal<usize>,
) -> impl IntoView {
    label(move || this_tab)
        .style(|s| s.justify_center())
        .keyboard_navigatable()
        .on_click_stop(move |_| {
            set_active_tab.update(|v: &mut usize| {
                *v = tabs
                    .get_untracked()
                    .iter()
                    .position(|it| *it == this_tab)
                    .unwrap();
            });
        })
        .style(move |s| {
            s.width(50)
                .items_center()
                .hover(|s| s.font_weight(Weight::BOLD).cursor(CursorStyle::Pointer))
                .apply_if(
                    active_tab.get()
                        == tabs
                            .get_untracked()
                            .iter()
                            .position(|it| *it == this_tab)
                            .unwrap(),
                    |s| s.font_weight(Weight::BOLD),
                )
        })
}

const TABBAR_HEIGHT: f64 = 37.0;
const CONTENT_PADDING: f64 = 10.0;

pub fn tab_navigation_view() -> impl IntoView {
    let tabs = vec![Tabs::General, Tabs::Settings, Tabs::Feedback]
        .into_iter()
        .collect::<im::Vector<Tabs>>();
    let (tabs, _set_tabs) = create_signal(tabs);
    let (active_tab, set_active_tab) = create_signal(0);

    let tabs_bar = h_stack((
        tab_button(Tabs::General, tabs, set_active_tab, active_tab),
        tab_button(Tabs::Settings, tabs, set_active_tab, active_tab),
        tab_button(Tabs::Feedback, tabs, set_active_tab, active_tab),
    ))
    .style(|s| {
        s.flex_row()
            .width_full()
            .height(TABBAR_HEIGHT)
            .row_gap(5)
            .padding(CONTENT_PADDING)
            .border_bottom(1)
            .border_color(Color::rgb8(205, 205, 205))
    });

    let main_content = container(
        scroll(
            tab(
                move || active_tab.get(),
                move || tabs.get(),
                |it| *it,
                |it| container(label(move || format!("{}", it))),
            )
            .style(|s| s.padding(CONTENT_PADDING).padding_bottom(10.0)),
        )
        .style(|s| s.flex_col().flex_basis(0).min_width(0).flex_grow(1.0)),
    )
    .style(|s| {
        s.position(Position::Absolute)
            .inset_top(TABBAR_HEIGHT)
            .inset_bottom(0.0)
            .width_full()
    });

    let settings_view = v_stack((tabs_bar, main_content)).style(|s| s.width_full().height_full());

    let id = settings_view.id();
    settings_view.on_event_stop(EventListener::KeyUp, move |e| {
        if let floem::event::Event::KeyUp(e) = e {
            if e.key.logical_key == floem::keyboard::Key::Named(floem::keyboard::NamedKey::F11) {
                id.inspect();
            }
        }
    })
}
