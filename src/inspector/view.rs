use super::profiler::profiler;
use crate::{
    AnyView, ElementId, IntoView, View, ViewId,
    action::inspect,
    app::{AppUpdateEvent, add_app_update_event},
    event::{EventPropagation, listener},
    inspector::{
        CAPTURE, Capture, CaptureView, CapturedView, RUNNING, add_event,
        data::{CapturedData, CapturedDatas},
        find_view, header, selected_view, stats, update_select_view_id,
    },
    new_window,
    prelude::*,
    style::{FontSize, OverflowX, OverflowY, TextColor, theme::Theme},
    theme::StyleThemeExt as _,
    unit::LengthAuto,
    views::{
        Button, CheckboxClass, ContainerExt, Decorators, Label, ListClass, ListItemClass,
        ScrollExt, Stack, TabSelectorClass, TooltipExt, resizable::Resizable,
    },
    window::WindowConfig,
};
use floem_reactive::{Effect, Memo, RwSignal, SignalGet, SignalUpdate};
use peniko::{
    Color,
    color::palette::{self, css},
    kurbo::{Rect, Stroke},
};
use std::collections::HashMap;
use std::rc::Rc;
use taffy::AlignItems;
use understory_box_tree::NodeFlags;
use winit::window::WindowId;

const OS_MOD: Modifiers = if cfg!(target_os = "macos") {
    Modifiers::META
} else {
    Modifiers::CONTROL
};

pub fn capture(window_id: WindowId) {
    let capture = CAPTURE.with(|c| *c);

    if !RUNNING.get() {
        new_window(
            move |inspector_window_id| {
                let selected = RwSignal::new(0);

                let tab_item = |name, index| {
                    Label::new(name)
                        .class(TabSelectorClass)
                        .action(move || selected.set(index))
                        .style(move |s| s.set_selected(selected.get() == index))
                };

                let tabs = (tab_item("Views", 0), tab_item("Profiler", 1))
                    .h_stack()
                    .style(|s| s.with_theme(|s, t| s.background(t.bg_base())));

                let tab = tab(
                    move || Some(selected.get()),
                    move || [0, 1],
                    |it| *it,
                    move |it| match it {
                        0 => dyn_container(
                            move || capture.get(),
                            move |capture_value| {
                                inspector_view(window_id, capture, &capture_value).into_any()
                            },
                        )
                        .style(|s| s.width_full().height_full())
                        .into_any(),
                        1 => profiler(window_id).into_any(),
                        _ => panic!(),
                    },
                )
                .style(|s| s.flex_basis(0.0).min_height(0.0).flex_grow(1.0));

                let separator = ().style(move |s| {
                    s.width_full()
                        .min_height(1.0)
                        .with_theme(|s, t| s.background(t.border()))
                });

                let mut window_scale = RwSignal::new(1.);

                Stack::vertical((tabs, separator, tab))
                    .style(|s| s.width_full().height_full())
                    .on_event(
                        el::KeyUp,
                        move |_cx, KeyboardEvent { key, modifiers, .. }| {
                            if *key == ui_events::keyboard::Key::Named(NamedKey::F11)
                                && modifiers.shift()
                            {
                                inspect();
                                return EventPropagation::Stop;
                            }
                            EventPropagation::Continue
                        },
                    )
                    .on_event(el::WindowClosed, |_, _| {
                        RUNNING.set(false);
                        EventPropagation::Continue
                    })
                    .on_event_stop(
                        listener::KeyUp,
                        move |_cx, KeyboardEvent { modifiers, key, .. }| {
                            if *key == Key::Character("q".into()) && modifiers.contains(OS_MOD) {
                                crate::quit_app();
                            } else if *key == Key::Character("w".into())
                                && modifiers.contains(OS_MOD)
                            {
                                crate::close_window(inspector_window_id);
                            }
                        },
                    )
                    .on_event_stop(
                        el::KeyDown,
                        move |_, KeyboardEvent { key, modifiers, .. }| match key {
                            Key::Character(ch)
                                if (ch == "=" || ch == "+") && modifiers.contains(OS_MOD) =>
                            {
                                window_scale *= 1.1;
                                crate::action::set_window_scale(window_scale.get());
                            }

                            Key::Character(ch) if ch == "-" && *modifiers == OS_MOD => {
                                window_scale /= 1.1;
                                crate::action::set_window_scale(window_scale.get());
                            }

                            Key::Character(ch) if ch == "0" && *modifiers == OS_MOD => {
                                window_scale.set(1.);
                                crate::action::set_window_scale(window_scale.get());
                            }
                            _ => {}
                        },
                    )
            },
            Some(WindowConfig::default().size((1200.0, 800.0))),
        );
    }

    add_app_update_event(AppUpdateEvent::CaptureWindow {
        window_id,
        capture: capture.write_only(),
    })
}

fn inspector_view(
    window_id: WindowId,
    capture_s: RwSignal<Option<Rc<Capture>>>,
    capture: &Option<Rc<Capture>>,
) -> impl IntoView {
    let view = if let Some(capture) = capture {
        capture_view(window_id, capture_s, capture.clone()).into_any()
    } else {
        Label::new("No capture").into_any()
    };

    view.container()
        .window_title(|| "Floem Inspector".to_owned())
        .style(|s| {
            s.width_full()
                .height_full()
                .with_theme(|s, t| s.background(t.bg_base()))
                .class(scroll::Handle, |s| {
                    s.border_radius(4.0)
                        .background(Color::from_rgba8(166, 166, 166, 140))
                        .set(scroll::Rounded, false)
                        .active(|s| s.background(Color::from_rgb8(166, 166, 166)))
                        .hover(|s| s.background(Color::from_rgb8(184, 184, 184)))
                })
                .class(scroll::Track, |s| {
                    s.hover(|s| s.background(Color::from_rgba8(166, 166, 166, 30)))
                })
        })
}

fn capture_view(
    window_id: WindowId,
    capture_s: RwSignal<Option<Rc<Capture>>>,
    capture: Rc<Capture>,
) -> impl IntoView {
    let capture_view = CaptureView {
        expanding_selection: RwSignal::new(None),
        scroll_to: RwSignal::new(None),
        selected: RwSignal::new(None),
        highlighted: RwSignal::new(None),
    };
    let datas = RwSignal::new(CapturedDatas::init_from_view(capture.root.clone()));
    let window = capture.window.clone();
    let capture_ = capture.clone();
    let size = capture_.window_size;
    let image_width = size.width;
    let image_height = size.height;
    let renderer = capture_.renderer.clone();

    let image = if let Some(window) = window {
        img_dynamic(move || window.clone()).into_any()
    } else {
        ().style(move |s| s.min_width(size.width).min_height(size.height))
            .into_any()
    }
    .style(move |s| {
        s.margin(5.0)
            .border(1.)
            .with_theme(|s, t| s.border_color(t.border()))
            .width(image_width + 2.0)
            .height(image_height + 2.0)
            .keyboard_navigable()
    });
    let image_view = InspectorImageView::new(image, capture.clone(), capture_view, datas);
    let recapture = Button::new("Recapture").action(move || {
        add_app_update_event(AppUpdateEvent::CaptureWindow {
            window_id,
            capture: capture_s.write_only(),
        })
    });

    let make_toggle_button = |signal: RwSignal<bool>, label: &'static str| {
        Stack::new((
            svg(move || {
                if !signal.get() {
                    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <rect x="3" y="3" width="18" height="8"/>
                        <line x1="3" y1="12" x2="21" y2="12"/>
                        <rect x="3" y="13" width="18" height="8"/>
                   </svg>"#
                } else {
                    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <rect x="3" y="3" width="8" height="18"/>
                        <line x1="12" y1="3" x2="12" y2="21"/>
                        <rect x="13" y="3" width="8" height="18"/>
                   </svg>"#
                }
            })
            .style(|s| s.size(12, 12)),
            Label::new(label),
        ))
        .style(|s| s.items_center().gap(3))
        .button()
        .action(move || signal.update(|s| *s = !*s))
    };

    let view_tree_horizontal_split = RwSignal::new(false);
    let selected_view_horizontal_split = RwSignal::new(image_height <= image_width);

    let tree_button = make_toggle_button(view_tree_horizontal_split, "Tree");
    let selected_button = make_toggle_button(selected_view_horizontal_split, "Selected");

    let active_tab = RwSignal::new(0);
    let capture_sig = RwSignal::new(capture.clone());

    let tab = tab(
        move || Some(active_tab.get()),
        move || [0, 1],
        |it| *it,
        move |it| {
            match it {
                0 => selected_view(&capture_sig.get(), capture_view.selected).into_any(),
                1 => Stack::vertical((
                    header("Stats"),
                    stats(&capture_sig.get()),
                    header("Renderer"),
                    Label::new(renderer.clone()).style(|s| s.padding(5.0)),
                ))
                .into_any(),
                _ => panic!(),
            }
            .style(|s| s.min_size(0, 0.).flex_grow(1.))
            .scroll()
            .style(|s| {
                s.width_full()
                    .set(OverflowX, taffy::Overflow::Visible)
                    .set(OverflowY, taffy::Overflow::Scroll)
            })
        },
    )
    .style(|s| s.size_full().min_size(0, 0.));

    let clear = Button::new("Clear selection")
        .style(move |s| s.apply_if(capture_view.selected.get().is_none(), |s| s.hide()))
        .action(move || capture_view.selected.set(None));

    let tabs = Stack::vertical((
        Stack::horizontal((
            recapture,
            tree_button,
            selected_button,
            clear,
            "selected"
                .style(move |s| {
                    s.apply_if(active_tab.get() == 0, |s| s.set_selected(true))
                        .margin_left(LengthAuto::Auto)
                })
                .class(TabSelectorClass)
                .action(move || active_tab.set(0)),
            "stats"
                .style(move |s| {
                    s.apply_if(active_tab.get() == 1, |s| s.set_selected(true))
                        .margin_right(LengthAuto::Auto)
                })
                .class(TabSelectorClass)
                .action(move || active_tab.set(1)),
        ))
        .style(|s| s.items_end().gap(10).padding_top(5)),
        tab,
    ))
    .style(|s| s.size_full().min_size(0., 0.));

    let left = Stack::vertical((
        header("Captured Window"),
        Resizable::new((
            image_view.scroll().style(|s| {
                s.min_size(0, 0)
                    .flex_grow(1.)
                    .grid()
                    .items_center()
                    .justify_items(AlignItems::Center)
            }),
            tabs,
        ))
        .custom_sizes(move || vec![(0, size.height.min(500.))])
        .style(move |s| {
            s.size_full()
                .apply_if(selected_view_horizontal_split.get(), |s| s.flex_col())
                .min_size(0., 0.)
        }),
    ))
    .style(|s| s.min_size(0., 0.).flex_grow(1.));

    let root = capture.root.clone();
    let tree = view_tree(capture.clone(), capture_view, datas);

    let search_str = RwSignal::new("".to_string());
    let inner_search = search_str;
    let match_ids = RwSignal::new((0, Vec::<ViewId>::new()));

    let search = TextInput::new(search_str)
        .style(|s| s.width_full())
        .placeholder("View Search...")
        .on_event_stop(
            listener::KeyUp,
            move |_cx, KeyboardEvent { key, .. }| match key {
                Key::Named(NamedKey::ArrowUp) => {
                    let id = match_ids.try_update(|(match_index, ids)| {
                        if !ids.is_empty() {
                            if *match_index == 0 {
                                *match_index = ids.len() - 1;
                            } else {
                                *match_index -= 1;
                            }
                        }
                        ids.get(*match_index).copied()
                    });
                    if let Some(Some(id)) = id {
                        update_select_view_id(id, &capture_view, false, datas);
                    }
                }
                Key::Named(NamedKey::ArrowDown) => {
                    let id = match_ids.try_update(|(match_index, ids)| {
                        if !ids.is_empty() {
                            *match_index = (*match_index + 1) % ids.len();
                        }
                        ids.get(*match_index).copied()
                    });
                    if let Some(Some(id)) = id {
                        update_select_view_id(id, &capture_view, false, datas);
                    }
                }
                _ => {
                    let content = inner_search.get();
                    let ids = find_view(&content, &root);
                    let first = match_ids.try_update(|(index, match_ids)| {
                        *index = 0;
                        let _ = std::mem::replace(match_ids, ids);
                        match_ids.first().copied()
                    });
                    if let Some(Some(id)) = first {
                        update_select_view_id(id, &capture_view, false, datas);
                    }
                }
            },
        );
    let tree = if capture.root.warnings() {
        Stack::vertical((
            header("Warnings")
                .style(|s| s.with_theme(|s, t| s.color(t.warning_base())))
                .tooltip(|| "requested changes is not empty"),
            header("View Tree"),
            search,
            tree,
        ))
        .into_view()
    } else {
        Stack::vertical((header("View Tree"), search, tree)).into_view()
    };

    let tree = tree.style(|s| s.height_full().min_width(0).flex_basis(0).flex_grow(1.0));

    Resizable::new((left, tree))
        .style(move |s| {
            s.size_full()
                .max_width_full()
                .apply_if(view_tree_horizontal_split.get(), |s| s.flex_col())
        })
        .custom_sizes(move || vec![(0, size.width.min(800.))])
        .on_event_stop(
            el::KeyUp,
            move |_, KeyboardEvent { key, modifiers, .. }| {
                if *key == Key::Named(NamedKey::F5)
                    || (*key == Key::Character("r".to_string()) && modifiers.contains(OS_MOD))
                {
                    add_app_update_event(AppUpdateEvent::CaptureWindow {
                        window_id,
                        capture: capture_s.write_only(),
                    });
                }
            },
        )
}

fn view_tree(
    capture: Rc<Capture>,
    capture_signal: CaptureView,
    datas: RwSignal<CapturedDatas>,
) -> impl View {
    let capture_signal_clone = capture_signal;
    let focus_line = datas.get_untracked().focus_line;
    virtual_stack(
        move || datas.get(),
        move |(_, _, data)| data.id,
        move |(_, level, rw_data)| {
            let capture = capture.clone();
            tree_node(&rw_data, capture_signal, capture, level, datas).class(ListItemClass)
        },
    )
    .class(ListClass)
    .style(|s| {
        s.flex_col()
            .flex_grow(1.)
            .min_size(0., 0.)
            .class(ListItemClass, |s| {
                s.width_full()
                    .hover(|s| s.with_theme(|s, t| s.background(t.bg_elevated())))
            })
    })
    .scroll()
    .style(|s| s.size_full())
    // .custom_style(|s| s.shrink_to_fit())
    .on_event_cont(listener::PointerLeave, move |_, _| {
        capture_signal_clone.highlighted.set(None)
    })
    .action(move || capture_signal_clone.selected.set(None))
    .scroll_to(move || {
        let focus_line = focus_line.get();
        Some((0.0, focus_line.saturating_sub(1) as f64 * 20.0).into())
    })
    // .scroll_to_view(move || {
    //     let view_id = capture_signal_clone.scroll_to.get();
    //     println!("{view_id:?}");
    //     view_id
    // })
}

fn tree_node(
    view: &CapturedData,
    capture_signal: CaptureView,
    capture: Rc<Capture>,
    level: usize,
    datas: RwSignal<CapturedDatas>,
) -> impl View + use<> {
    let name = tree_node_name(view, level as f64 * 10.0).into_view();
    let name_id = name.id();
    let height = 20.0;
    let id = view.id;
    let selected = capture_signal.selected;
    let highlighted = capture_signal.highlighted;

    let row = name
        .container()
        .style(move |s| {
            s.height(height)
                .width_full()
                .keyboard_navigable()
                .text_clip()
                .apply_if(selected.get() == Some(id), |s| s.set_selected(true))
        })
        .action(move || selected.set(Some(id)))
        .on_event_cont(el::PointerEnter, move |_, _| highlighted.set(Some(id)));
    let row = add_event(
        row,
        view.view_conf.name.clone(),
        id,
        capture_signal,
        capture.clone(),
        datas,
    );
    let row_id = row.id();
    let scroll_to = capture_signal.scroll_to;
    let expanding_selection = capture_signal.expanding_selection;
    Effect::new(move |_| {
        if let Some((selection, request_focus)) = expanding_selection.get()
            && selection == id
        {
            // Scroll to the row, then to the name part of the row.
            scroll_to.set(Some(row_id));
            scroll_to.set(Some(name_id));
            if request_focus {
                row_id.request_focus();
            }
        }
    });

    row
}

fn tree_node_name(view: &CapturedData, marge_left: f64) -> impl IntoView {
    let name = Label::new(view.view_conf.name.clone());
    let id = Label::new(format!("ViewId {{{:?}}}", view.id.0)).style(|s| {
        s.margin_right(5.0)
            .background(palette::css::BLACK.with_alpha(0.02))
            .border(1.)
            .border_radius(5.0)
            .with_theme(|s, t| s.border_color(t.border()))
            .padding(3.0)
            .padding_top(0.0)
            .padding_bottom(0.0)
            .font_size(12.0)
            .with::<TextColor>(|s, tc| {
                s.set_context_opt(TextColor, tc.def(|tc| tc.map(|tc| tc.with_alpha(0.6))))
            })
    });
    let tab = if view.view_conf.focused {
        "Focus"
            .style(|s| {
                s.margin_right(5.0)
                    .background(Color::from_rgb8(63, 81, 101).with_alpha(0.6))
                    .border_radius(5.0)
                    .padding(1.0)
                    .with::<FontSize>(|s, fs| s.set_context(FontSize, fs.def(|fs| fs * 0.8)))
                    .color(palette::css::WHITE.with_alpha(0.8))
            })
            .into_any()
    } else if view.view_conf.keyboard_navigable {
        "Tab"
            .style(|s| {
                s.margin_right(5.0)
                    .background(Color::from_rgb8(204, 217, 221).with_alpha(0.4))
                    .border(1.)
                    .border_radius(5.0)
                    .with_theme(|s, t| s.border_color(t.border()))
                    .padding(1.0)
                    .font_size(10.0)
                    .color(palette::css::BLACK.with_alpha(0.4))
            })
            .into_any()
    } else {
        ().into_any()
    };
    let ty = view.expanded();
    // let click_ty = view.ty.clone();
    let checkbox = ()
        .class_if(move || ty.is_some(), CheckboxClass)
        .style(move |s| match ty {
            Some(expanded) => {
                let expanded = expanded.get();
                s.apply_if(expanded, |s| s.set_selected(true))
                    .size(12, 12)
                    .margin_right(4.0)
                    .with_theme(move |s, t| {
                        s.background(t.text_muted())
                            .border_radius(t.border_radius())
                            .apply_if(expanded, |s| s.background(t.text()))
                    })
                    .border(1.0)
            }
            None => s.width(12.0).height(12.0).margin_right(4.0),
        })
        .action(move || {
            if let Some(expanded) = ty {
                expanded.set(!expanded.get_untracked());
            }
        });
    Stack::horizontal((checkbox, id, tab, name))
        .style(move |s| s.items_center().margin_left(marge_left))
}

pub struct InspectorImageView {
    id: ElementId,
    capture: Rc<Capture>,
    capture_view: CaptureView,
    datas: RwSignal<CapturedDatas>,
    data_id_to_view: HashMap<String, ViewId>,
    element_to_data_id: HashMap<ElementId, String>,
    contain_ids: Vec<ViewId>,
    contain_index: usize,
    selected_overlay_color: Color,
    selected_overlay_border_color: Color,
    highlighted_overlay_color: Color,
    highlighted_overlay_border_color: Color,
}

impl InspectorImageView {
    pub fn new(
        child: AnyView,
        capture: Rc<Capture>,
        capture_view: CaptureView,
        datas: RwSignal<CapturedDatas>,
    ) -> Self {
        let id = ViewId::new();
        let selected = Memo::new(move |_| capture_view.selected.get());
        let highlighted = Memo::new(move |_| capture_view.highlighted.get());
        Effect::new(move |_| {
            selected.track();
            highlighted.track();
            id.request_paint();
        });
        id.add_child(child);
        let data_id_to_view = datas.get_untracked().visible_data_id_map();
        let mut element_to_data_id = HashMap::new();
        register_capture_elements(id, &capture.root, &mut element_to_data_id);
        id.request_box_tree_commit();
        Self {
            id: id.get_element_id(),
            capture,
            capture_view,
            datas,
            data_id_to_view,
            element_to_data_id,
            contain_ids: Vec::new(),
            contain_index: 0,
            selected_overlay_color: css::DODGER_BLUE.with_alpha(0.5),
            selected_overlay_border_color: css::DODGER_BLUE.with_alpha(0.7),
            highlighted_overlay_color: css::DEEP_SKY_BLUE.with_alpha(0.5),
            highlighted_overlay_border_color: css::DEEP_SKY_BLUE.with_alpha(0.7),
        }
    }

    fn overlay_rect(&self, id: Option<ViewId>) -> Option<Rect> {
        let view = id.and_then(|id| self.capture.root.find(id))?;
        Some(Rect::new(
            5.0 + view.world_bounds.x0 + 1.0,
            5.0 + view.world_bounds.y0 + 1.0,
            5.0 + view.world_bounds.x1 + 1.0,
            5.0 + view.world_bounds.y1 + 1.0,
        ))
    }
}

fn register_capture_elements(
    owner_id: ViewId,
    root: &Rc<CapturedView>,
    element_to_data_id: &mut HashMap<ElementId, String>,
) {
    fn register_one(
        owner_id: ViewId,
        captured: &Rc<CapturedView>,
        parent_element: ElementId,
        element_to_data_id: &mut HashMap<ElementId, String>,
    ) {
        let is_visible = captured.direct_style.builtin().display() != taffy::Display::None
            && captured.world_bounds.area() > 0.0;

        let mut next_parent = parent_element;
        if is_visible {
            let element = owner_id.create_child_element_id(0);
            let rect = Rect::new(
                6.0 + captured.world_bounds.x0,
                6.0 + captured.world_bounds.y0,
                6.0 + captured.world_bounds.x1,
                6.0 + captured.world_bounds.y1,
            );
            let box_tree = owner_id.box_tree();
            let mut bt = box_tree.borrow_mut();
            bt.reparent(element.0, Some(parent_element.0));
            bt.set_local_bounds(element.0, rect);
            bt.set_flags(element.0, NodeFlags::VISIBLE | NodeFlags::PICKABLE);
            bt.set_element_meta(
                element.0,
                Some(crate::ElementMeta::new(ElementId(
                    element.0, owner_id, false,
                ))),
            );
            drop(bt);
            element_to_data_id.insert(element, captured.id_data_str.clone());
            next_parent = element;
        }
        for child in &captured.children {
            register_one(owner_id, child, next_parent, element_to_data_id);
        }
    }

    let root_element = owner_id.get_element_id();
    register_one(owner_id, root, root_element, element_to_data_id);
}

impl View for InspectorImageView {
    fn id(&self) -> ViewId {
        self.id.owning_id()
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Inspector Image Viewer".into()
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if let Some(theme) = cx.get_prop(Theme) {
            self.selected_overlay_color = theme.info().with_alpha(0.5);
            self.selected_overlay_border_color = theme.info().with_alpha(0.7);
            self.highlighted_overlay_color = theme.primary_muted().with_alpha(0.5);
            self.highlighted_overlay_border_color = theme.primary_muted().with_alpha(0.7);
        }
    }

    fn event(&mut self, cx: &mut crate::context::EventCx) -> EventPropagation {
        use crate::event::{Event, Phase};
        use ui_events::keyboard::{Key, KeyState, NamedKey};
        use ui_events::pointer::PointerEvent;

        if cx.phase != Phase::Target {
            return EventPropagation::Continue;
        }

        match &cx.event {
            Event::Key(KeyboardEvent { key, state, .. }) => {
                if *state != KeyState::Up {
                    return EventPropagation::Continue;
                }
                match key {
                    Key::Named(NamedKey::ArrowUp) => {
                        if !self.contain_ids.is_empty() {
                            self.contain_index = if self.contain_index == 0 {
                                self.contain_ids.len() - 1
                            } else {
                                self.contain_index - 1
                            };
                            if let Some(id) = self.contain_ids.get(self.contain_index).copied() {
                                cx.window_state.request_paint = true;
                                self.id.owning_id().request_paint();
                                update_select_view_id(id, &self.capture_view, false, self.datas);
                                return EventPropagation::Stop;
                            }
                        }
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        if !self.contain_ids.is_empty() {
                            self.contain_index = (self.contain_index + 1) % self.contain_ids.len();
                            if let Some(id) = self.contain_ids.get(self.contain_index).copied() {
                                cx.window_state.request_paint = true;
                                self.id.owning_id().request_paint();
                                update_select_view_id(id, &self.capture_view, false, self.datas);
                                return EventPropagation::Stop;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Pointer(PointerEvent::Move(_)) => {
                self.contain_ids.clear();
                if let Some(hit_path) = &cx.hit_path {
                    self.contain_ids.extend(
                        hit_path
                            .iter()
                            .filter_map(|id| self.element_to_data_id.get(id))
                            .filter_map(|data_id| self.data_id_to_view.get(data_id).copied()),
                    );
                }
                cx.window_state.request_paint = true;
                self.contain_index = 0;
                self.capture_view
                    .highlighted
                    .set(self.contain_ids.last().copied());
            }
            Event::Pointer(PointerEvent::Up(_)) => {
                self.contain_ids.clear();
                if let Some(hit_path) = &cx.hit_path {
                    self.contain_ids.extend(
                        hit_path
                            .iter()
                            .filter_map(|id| self.element_to_data_id.get(id))
                            .filter_map(|data_id| self.data_id_to_view.get(data_id).copied()),
                    );
                }
                self.contain_index = 0;
                if let Some(id) = self.contain_ids.last().copied() {
                    cx.window_state.request_paint = true;
                    self.id.owning_id().request_paint();
                    update_select_view_id(id, &self.capture_view, false, self.datas);
                    return EventPropagation::Stop;
                }
            }
            Event::Pointer(PointerEvent::Leave(_)) => {
                self.capture_view.highlighted.set(None);
            }
            _ => {}
        }

        EventPropagation::Continue
    }

    fn post_paint(&mut self, cx: &mut crate::paint::PaintCx) {
        if cx.target_id == self.id {
            if let Some(selected_overlay) = self.overlay_rect(self.capture_view.selected.get()) {
                cx.fill(&selected_overlay, self.selected_overlay_color, 0.);
                cx.stroke(
                    &selected_overlay,
                    self.selected_overlay_border_color,
                    &Stroke::new(1.0),
                );
            }
            if let Some(highlighted_overlay) =
                self.overlay_rect(self.capture_view.highlighted.get())
            {
                cx.fill(&highlighted_overlay, self.highlighted_overlay_color, 0.);
                cx.stroke(
                    &highlighted_overlay,
                    self.highlighted_overlay_border_color,
                    &Stroke::new(1.0),
                );
            }
        }
    }
}
