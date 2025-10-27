use floem::prelude::palette::css;
use floem::prelude::*;
use floem::reactive::create_effect;
use floem::style::BoxShadow;
use floem::taffy::AlignContent;
use floem::theme::StyleThemeExt;

#[derive(Clone)]
struct TabContent {
    idx: usize,
    name: String,
}

impl TabContent {
    fn new(tabs_count: usize) -> Self {
        Self {
            idx: tabs_count,
            name: "Tab with index".to_string(),
        }
    }
}

#[derive(Clone)]
enum Action {
    Add,
    Remove,
    None,
}

pub fn tab_view() -> impl IntoView {
    let tabs = RwSignal::new(vec![]);
    let active_tab = RwSignal::new(None::<usize>);
    let tab_action = RwSignal::new(Action::None);

    create_effect(move |_| match tab_action.get() {
        Action::Add => {
            tabs.update(|tabs| tabs.push(TabContent::new(tabs.len())));
        }
        Action::Remove => {
            tabs.update(|tabs| {
                tabs.pop();
            });
        }
        Action::None => (),
    });

    let tabs_view = dyn_stack(
        move || tabs.get(),
        |tab| tab.idx,
        move |tab| {
            tab_side_item(tab.clone(), active_tab).on_click_stop(move |_| {
                active_tab.update(|a| {
                    *a = Some(tab.idx);
                });
            })
        },
    )
    .style(|s| s.flex_col().width_full().row_gap(5.))
    .scroll()
    .on_click_stop(move |_| {
        if active_tab.with_untracked(|act| act.is_some()) {
            active_tab.set(None)
        }
    })
    .style(|s| s.size_full().padding(5.).padding_right(7.))
    .scroll_style(|s| s.handle_thickness(6.).shrink_to_fit())
    .style(|s| {
        s.width(140.)
            .min_width(140.)
            .height_full()
            .border_right(1.)
            .with_theme(|s, t| s.border_color(t.border_muted()))
    });

    let tabs_content_view = stack((tab(
        move || active_tab.get(),
        move || tabs.get(),
        |tab| tab.idx,
        show_tab_content,
    )
    .style(|s| s.size_full()),))
    .style(|s| s.size_full());

    v_stack((
        h_stack((
            button("add tab").action(move || {
                tab_action.update(|a| {
                    *a = Action::Add;
                })
            }),
            button("remove tab").action(move || {
                tab_action.update(|a| {
                    *a = Action::Remove;
                })
            }),
        ))
        .style(|s| {
            s.height(40.px())
                .width_full()
                .border_bottom(1.)
                .with_theme(|s, t| s.border_color(t.border_muted()))
                .padding(5.)
                .col_gap(5.)
                .items_center()
                .align_content(AlignContent::SpaceAround)
        }),
        stack((tabs_view, tabs_content_view)).style(|s| s.height(400.px()).width(500.px())),
    ))
    .style(|s| s.size_full())
    .container()
    .style(|s| {
        s.size_full()
            .padding(10.)
            .with_theme(|s, t| s.border_color(t.border_muted()))
    })
}

fn show_tab_content(tab: TabContent) -> impl IntoView {
    v_stack((
        tab.name.style(|s| s.font_size(15.).font_bold()),
        label(move || format!("{}", tab.idx)).style(|s| s.font_size(20.).font_bold()),
        "is now active".style(|s| s.font_size(13.)),
    ))
    .style(|s| {
        s.size(150.px(), 150.px())
            .items_center()
            .justify_center()
            .row_gap(10.)
            .border_radius(7.)
            .border_top(0.6)
            .with_theme(|s, t| {
                s.background(t.bg_elevated())
                    .border_top_color(css::WHITE)
                    .apply_if(t.is_dark, |s| s.border_top_color(t.border()))
            })
            .border_bottom_color(css::BLACK.multiply_alpha(0.7))
            .apply_box_shadows(vec![
                BoxShadow::new()
                    .color(css::BLACK.multiply_alpha(0.3))
                    .top_offset(-13.)
                    .bottom_offset(0.4)
                    .right_offset(-4.)
                    .left_offset(-4.)
                    .blur_radius(2.)
                    .spread(1.),
                BoxShadow::new()
                    .color(css::BLACK.multiply_alpha(0.3))
                    .top_offset(-15.)
                    .bottom_offset(4.)
                    .right_offset(-6.)
                    .left_offset(-6.)
                    .blur_radius(5.)
                    .spread(6.),
            ])
    })
    .container()
    .style(|s| s.size_full().items_center().justify_center())
}

fn tab_side_item(tab: TabContent, act_tab: RwSignal<Option<usize>>) -> impl IntoView {
    text(format!("{} {}", tab.name, tab.idx))
        .button()
        .style(move |s| {
            s.width_full()
                .height(36.px())
                .apply_if(act_tab.get().is_some_and(|a| a == tab.idx), |s| {
                    s.with_theme(|s, t| s.border_color(t.primary()))
                })
        })
}
