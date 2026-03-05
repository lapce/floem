use floem::{imbl, prelude::*, style::CursorStyle, text::FontWeight, LazyView};

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
impl IntoView for Tab {
    type V = Label;

    type Intermediate = LazyView<String>;

    fn into_intermediate(self) -> Self::Intermediate {
        LazyView::new(self.to_string())
    }
}

fn tab_button(
    this_tab: Tab,
    tabs: RwSignal<imbl::Vector<Tab>>,
    active_tab: RwSignal<usize>,
) -> impl IntoView {
    Label::new(this_tab)
        .button() // by making this a button, the button class from the default theme will be applied and the focusable property will be set
        .action(move || {
            active_tab.update(|v: &mut usize| {
                *v = tabs
                    .get_untracked()
                    .iter()
                    .position(|it| *it == this_tab)
                    .unwrap();
            });
        })
        .style(move |s| {
            s.padding(10)
                .items_center()
                .justify_center()
                .hover(|s| s.font_weight(FontWeight::BOLD).cursor(CursorStyle::Pointer))
                .apply_if(
                    active_tab.get()
                        == tabs
                            .get_untracked()
                            .iter()
                            .position(|it| *it == this_tab)
                            .unwrap(),
                    |s| s.font_weight(FontWeight::BOLD),
                )
        })
}

const CONTENT_PADDING: f64 = 10.0;

pub fn tab_navigation_view() -> impl IntoView {
    let tabs = vec![Tab::General, Tab::Settings, Tab::Feedback]
        .into_iter()
        .collect::<imbl::Vector<Tab>>();
    let tabs = RwSignal::new(tabs);
    let active_tab = RwSignal::new(0);

    let tabs_bar = Stack::horizontal((
        tab_button(Tab::General, tabs, active_tab),
        tab_button(Tab::Settings, tabs, active_tab),
        tab_button(Tab::Feedback, tabs, active_tab),
    ))
    .style(|s| {
        s.flex_row()
            .width_full()
            .col_gap(5)
            .padding(CONTENT_PADDING)
            .border_bottom(1)
            .border_color(Color::from_rgb8(205, 205, 205))
    });

    let main_content = tab(
        move || Some(active_tab.get()),
        move || tabs.get(),
        |it| *it,
        |it| it,
    )
    .style(|s| s.padding(CONTENT_PADDING).padding_bottom(10.0))
    .scroll()
    .style(|s| s.flex_col().flex_basis(0).min_width(0).flex_grow(1.0))
    .container()
    .style(|s| s.size_full());

    Stack::vertical((tabs_bar, main_content))
        .style(|s| s.width_full().height_full())
        .on_event_stop(listener::KeyUp, move |_cx, KeyboardEvent { key, .. }| {
            if *key == Key::Named(NamedKey::F11) {
                floem::action::inspect();
            }
        })
}
