use floem::{
    event::EventListener,
    peniko::Color,
    reactive::{create_signal, ReadSignal, SignalGet, SignalUpdate, WriteSignal},
    style::{CursorStyle, Position},
    text::Weight,
    views::{container, h_stack, label, scroll, tab, v_stack, Decorators},
    IntoView, View,
};

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Tab {
    General,
    Settings,
    Feedback,
}

impl std::fmt::Display for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Tab::General => write!(f, "General"),
            Tab::Settings => write!(f, "Settings"),
            Tab::Feedback => write!(f, "Feedback"),
        }
    }
}

fn tab_button(
    this_tab: Tab,
    tabs: ReadSignal<im::Vector<Tab>>,
    set_active_tab: WriteSignal<usize>,
    active_tab: ReadSignal<usize>,
) -> impl IntoView {
    label(move || this_tab)
        .keyboard_navigable()
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
            s.width(70)
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
    let tabs = vec![Tab::General, Tab::Settings, Tab::Feedback]
        .into_iter()
        .collect::<im::Vector<Tab>>();
    let (tabs, _set_tabs) = create_signal(tabs);
    let (active_tab, set_active_tab) = create_signal(0);

    let tabs_bar = h_stack((
        tab_button(Tab::General, tabs, set_active_tab, active_tab),
        tab_button(Tab::Settings, tabs, set_active_tab, active_tab),
        tab_button(Tab::Feedback, tabs, set_active_tab, active_tab),
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
