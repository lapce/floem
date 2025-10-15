use crate::app::{add_app_update_event, AppUpdateEvent};
use crate::event::{Event, EventListener, EventPropagation};
use crate::inspector::data::{CapturedData, CapturedDatas};
use crate::inspector::{
    add_event, find_view, header, selected_view, stats, update_select_view_id, Capture,
    CaptureView, CAPTURE, RUNNING,
};
use crate::prelude::{
    button, dyn_container, empty, h_stack, img_dynamic, scroll, stack, static_label, tab, text,
    text_input, v_stack, virtual_stack, ViewTuple,
};
use crate::profiler::profiler;
use crate::style::{FontSize, OverflowX, OverflowY, TextColor};
use crate::theme::StyleThemeExt as _;
use crate::unit::PxPctAuto;
use crate::views::{
    resizable, CheckboxClass, ContainerExt, Decorators, ListClass, ListItemClass, ScrollExt,
    TabSelectorClass, TooltipExt,
};
use crate::window::WindowConfig;
use crate::{keyboard, new_window, IntoView, View, ViewId};
use floem_reactive::{create_effect, create_rw_signal, RwSignal, SignalGet, SignalUpdate};
use peniko::color::palette;
use peniko::Color;
use std::rc::Rc;
use taffy::AlignItems;
use winit::keyboard::NamedKey;
use winit::window::WindowId;

pub fn capture(window_id: WindowId) {
    let capture = CAPTURE.with(|c| *c);

    if !RUNNING.get() {
        new_window(
            move |_| {
                let selected = RwSignal::new(0);

                let tab_item = |name, index| {
                    text(name)
                        .class(TabSelectorClass)
                        .on_click_stop(move |_| selected.set(index))
                        .style(move |s| s.set_selected(selected.get() == index))
                };

                let tabs = (tab_item("Views", 0), tab_item("Profiler", 1))
                    .h_stack()
                    .style(|s| s.with_theme(|s, t| s.background(t.bg_base())));

                let tab = tab(
                    move || selected.get(),
                    move || [0, 1].into_iter(),
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

                let separator = empty().style(move |s| {
                    s.width_full()
                        .min_height(1.0)
                        .background(palette::css::BLACK.with_alpha(0.2))
                });

                let stack = v_stack((tabs, separator, tab));
                let id = stack.id();
                stack
                    .style(|s| s.width_full().height_full())
                    .on_event(EventListener::KeyUp, move |e| {
                        if let Event::KeyUp(e) = e {
                            if e.key.logical_key == keyboard::Key::Named(NamedKey::F11)
                                && e.modifiers.shift()
                            {
                                id.inspect();
                                return EventPropagation::Stop;
                            }
                        }
                        EventPropagation::Continue
                    })
                    .on_event(EventListener::WindowClosed, |_| {
                        RUNNING.set(false);
                        EventPropagation::Continue
                    })
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
        capture_view(window_id, capture_s, capture).into_any()
    } else {
        text("No capture").into_any()
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
                        .set(scroll::Thickness, 16.0)
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
    capture: &Rc<Capture>,
) -> impl IntoView {
    let capture_view = CaptureView {
        expanding_selection: create_rw_signal(None),
        scroll_to: create_rw_signal(None),
        selected: create_rw_signal(None),
        highlighted: create_rw_signal(None),
    };
    let datas = create_rw_signal(CapturedDatas::init_from_view(capture.root.clone()));
    let window = capture.window.clone();
    let capture_ = capture.clone();
    let (image_width, image_height) = capture
        .window
        .as_ref()
        .map(|img| {
            (
                img.image.width as f64 / capture.scale,
                img.image.height as f64 / capture.scale,
            )
        })
        .unwrap_or_default();
    let size = capture_.window_size;
    let renderer = capture_.renderer.clone();

    let contain_ids = create_rw_signal((0, Vec::<ViewId>::new()));

    let image = if let Some(window) = window {
        img_dynamic(move || window.clone()).into_any()
    } else {
        empty()
            .style(move |s| s.min_width(size.width).min_height(size.height))
            .into_any()
    }
    .style(move |s| {
        s.margin(5.0)
            .border(1.)
            .border_color(palette::css::BLACK.with_alpha(0.5))
            .width(image_width + 2.0)
            .height(image_height + 2.0)
            .margin_bottom(21.0)
            .margin_right(21.0)
    })
    .keyboard_navigable()
    .on_event_stop(EventListener::KeyUp, {
        move |event: &Event| {
            if let Event::KeyUp(key) = event {
                match key.key.logical_key {
                    keyboard::Key::Named(NamedKey::ArrowUp) => {
                        let id = contain_ids.try_update(|(match_index, ids)| {
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
                    keyboard::Key::Named(NamedKey::ArrowDown) => {
                        let id = contain_ids.try_update(|(match_index, ids)| {
                            if !ids.is_empty() {
                                *match_index = (*match_index + 1) % ids.len();
                            }
                            ids.get(*match_index).copied()
                        });
                        if let Some(Some(id)) = id {
                            update_select_view_id(id, &capture_view, false, datas);
                        }
                    }
                    _ => {}
                }
            }
        }
    })
    .on_event_stop(EventListener::PointerUp, {
        let capture_ = capture_.clone();
        move |event: &Event| {
            if let Event::PointerUp(e) = event {
                let find_ids = capture_
                    .root
                    .find_all_by_pos(e.pos)
                    .iter()
                    .filter(|id| !id.is_hidden_recursive())
                    .cloned()
                    .collect::<Vec<_>>();
                if !find_ids.is_empty() {
                    let first = contain_ids.try_update(|(index, ids)| {
                        *index = 0;
                        let _ = std::mem::replace(ids, find_ids);
                        ids.first().copied()
                    });
                    if let Some(Some(id)) = first {
                        update_select_view_id(id, &capture_view, false, datas);
                    }
                }
            }
        }
    })
    .on_event_stop(EventListener::PointerMove, {
        move |event: &Event| {
            if let Event::PointerMove(e) = event {
                let find_ids = capture_
                    .root
                    .find_all_by_pos(e.pos)
                    .iter()
                    .filter(|id| !id.is_hidden_recursive())
                    .cloned()
                    .collect::<Vec<_>>();
                if !find_ids.is_empty() {
                    if let Some(Some(first)) = contain_ids.try_update(|(index, ids)| {
                        *index = 0;
                        let _ = std::mem::replace(ids, find_ids);
                        ids.first().copied()
                    }) {
                        if capture_view.highlighted.get() != Some(first) {
                            capture_view.highlighted.set(Some(first));
                        }
                    } else {
                        capture_view.highlighted.set(None);
                    }
                } else {
                    capture_view.highlighted.set(None);
                }
            }
        }
    })
    .on_event_cont(EventListener::PointerLeave, move |_| {
        capture_view.highlighted.set(None)
    });

    let capture_ = capture.clone();
    let selected_overlay = empty().style(move |s| {
        if let Some(view) = capture_view
            .selected
            .get()
            .and_then(|id| capture_.root.find(id))
        {
            s.absolute()
                // the plus ones here might be because of the border 1... I'm not sure though
                .margin_left(5.0 + view.layout.x0 + 1.)
                .margin_top(5.0 + view.layout.y0 + 1.)
                .width(view.layout.width())
                .height(view.layout.height())
                .with_theme(|s, t| {
                    s.background(t.info().with_alpha(0.5))
                        .border_color(t.info().with_alpha(0.7))
                })
                .border(1.)
        } else {
            s
        }
        .pointer_events_none()
    });

    let capture_ = capture.clone();
    let highlighted_overlay = empty().style(move |s| {
        if let Some(view) = capture_view
            .highlighted
            .get()
            .and_then(|id| capture_.root.find(id))
        {
            s.absolute()
                .margin_left(5.0 + view.layout.x0 + 1.)
                .margin_top(5.0 + view.layout.y0 + 1.)
                .width(view.layout.width())
                .height(view.layout.height())
                .with_theme(|s, t| {
                    s.background(t.primary_muted().with_alpha(0.5))
                        .border_color(t.primary_muted().with_alpha(0.7))
                })
                .border(1.)
        } else {
            s
        }
        .pointer_events_none()
    });

    let image = stack((image, selected_overlay, highlighted_overlay));

    let recapture = button("Recapture").on_click_stop(move |_| {
        add_app_update_event(AppUpdateEvent::CaptureWindow {
            window_id,
            capture: capture_s.write_only(),
        })
    });

    let active_tab = RwSignal::new(0);
    let capture_sig = RwSignal::new(capture.clone());

    let tab = tab(
        move || active_tab.get(),
        move || [0, 1].into_iter(),
        |it| *it,
        move |it| {
            match it {
                0 => v_stack((
                    header("Selected View"),
                    selected_view(&capture_sig.get(), capture_view.selected),
                ))
                .into_any(),
                1 => v_stack((
                    header("Stats"),
                    stats(&capture_sig.get()),
                    header("Renderer"),
                    text(renderer.clone()).style(|s| s.padding(5.0)),
                ))
                .into_any(),
                _ => panic!(),
            }
            .style(|s| s.width_full().flex_grow(1.))
            .scroll()
            .style(|s| {
                s.width_full()
                    .flex_grow(1.)
                    .set(OverflowX, taffy::Overflow::Visible)
                    .set(OverflowY, taffy::Overflow::Scroll)
            })
        },
    )
    .style(|s| s.size_full());

    let clear = button("Clear selection")
        .style(move |s| s.apply_if(capture_view.selected.get().is_none(), |s| s.hide()))
        .action(move || capture_view.selected.set(None));

    let tabs = v_stack((
        h_stack((
            recapture,
            clear,
            "selected"
                .style(move |s| {
                    s.apply_if(active_tab.get() == 0, |s| s.set_selected(true))
                        .margin_left(PxPctAuto::Auto)
                })
                .class(TabSelectorClass)
                .on_click_stop(move |_| active_tab.set(0)),
            "stats"
                .style(move |s| {
                    s.apply_if(active_tab.get() == 1, |s| s.set_selected(true))
                        .margin_right(PxPctAuto::Auto)
                })
                .class(TabSelectorClass)
                .on_click_stop(move |_| active_tab.set(1)),
        ))
        .style(|s| s.items_end().gap(10).padding_top(5)),
        tab,
    ))
    .style(|s| s.size_full());

    let left = v_stack((
        header("Captured Window"),
        resizable::resizable((scroll(image).style(|s| s.max_height_pct(60.0)), tabs))
            .style(|s| s.size_full().flex_col()),
    ));

    let root = capture.root.clone();
    let tree = view_tree(capture.clone(), capture_view, datas);

    let search_str = create_rw_signal("".to_string());
    let inner_search = search_str;
    let match_ids = create_rw_signal((0, Vec::<ViewId>::new()));

    let search = text_input(search_str)
        .placeholder("View Search...")
        .on_event_stop(EventListener::KeyUp, move |event: &Event| {
            if let Event::KeyUp(key) = event {
                match key.key.logical_key {
                    keyboard::Key::Named(NamedKey::ArrowUp) => {
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
                    keyboard::Key::Named(NamedKey::ArrowDown) => {
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
                }
            }
        });
    let tree = if capture.root.warnings() {
        v_stack((
            header("Warnings")
                .style(|s| s.with_theme(|s, t| s.color(t.warning_base)))
                .tooltip(|| "requested changes is not empty"),
            header("View Tree"),
            search,
            tree,
        ))
        .into_view()
    } else {
        v_stack((header("View Tree"), search, tree)).into_view()
    };

    let tree = tree.style(|s| s.height_full().min_width(0).flex_basis(0).flex_grow(1.0));

    resizable::resizable((left, tree)).style(|s| s.size_full().max_width_full())
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
        s.flex_col().flex_grow(1.).class(ListItemClass, |s| {
            s.hover(|s| s.with_theme(|s, t| s.background(t.bg_elevated())))
        })
    })
    .scroll()
    .style(|s| s.flex_grow(1.0).size_full())
    .scroll_style(|s| s.shrink_to_fit())
    .on_event_cont(EventListener::PointerLeave, move |_| {
        capture_signal_clone.highlighted.set(None)
    })
    .on_click_stop(move |_| capture_signal_clone.selected.set(None))
    .scroll_to(move || {
        let focus_line = focus_line.get();
        Some((0.0, focus_line as f64 * 20.0).into())
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
) -> impl IntoView {
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
                //     .apply_if(highlighted.get() == Some(id), |s| {
                //         s.background(Color::from_rgba8(228, 237, 216, 160))
                //     })
                .apply_if(selected.get() == Some(id), |s| {
                    s.set_selected(true)
                    // if highlighted.get() == Some(id) {
                    //     s.background(Color::from_rgb8(186, 180, 216))
                    // } else {
                    //     s.background(Color::from_rgb8(213, 208, 216))
                    // }
                })
        })
        .on_click_stop(move |_| selected.set(Some(id)))
        .on_event_cont(EventListener::PointerEnter, move |_| {
            highlighted.set(Some(id))
        });
    let row = add_event(
        row,
        view.view_conf.custom_name.clone(),
        id,
        capture_signal,
        &capture,
        datas,
    );
    let row_id = row.id();
    let scroll_to = capture_signal.scroll_to;
    let expanding_selection = capture_signal.expanding_selection;
    create_effect(move |_| {
        if let Some((selection, request_focus)) = expanding_selection.get() {
            if selection == id {
                // Scroll to the row, then to the name part of the row.
                scroll_to.set(Some(row_id));
                scroll_to.set(Some(name_id));
                if request_focus {
                    row_id.request_focus();
                }
            }
        }
    });

    row
}

fn tree_node_name(view: &CapturedData, marge_left: f64) -> impl IntoView {
    let name = static_label(view.view_conf.name.clone());
    let id = text(format!("{:?}", view.id)).style(|s| {
        s.margin_right(5.0)
            .background(palette::css::BLACK.with_alpha(0.02))
            .border(1.)
            .border_radius(5.0)
            .border_color(palette::css::BLACK.with_alpha(0.07))
            .padding(3.0)
            .padding_top(0.0)
            .padding_bottom(0.0)
            .font_size(12.0)
            .with_context::<TextColor>(|s, tc| {
                s.apply_opt(*tc, |s, tc| s.color(tc.with_alpha(0.6)))
            })
    });
    let tab = if view.view_conf.focused {
        "Focus"
            .style(|s| {
                s.margin_right(5.0)
                    .background(Color::from_rgb8(63, 81, 101).with_alpha(0.6))
                    .border_radius(5.0)
                    .padding(1.0)
                    .with_context_opt::<FontSize, _>(|s, fs| s.font_size(fs * 0.8))
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
                    .border_color(palette::css::BLACK.with_alpha(0.07))
                    .padding(1.0)
                    .font_size(10.0)
                    .color(palette::css::BLACK.with_alpha(0.4))
            })
            .into_any()
    } else {
        empty().into_any()
    };
    let ty = view.expanded();
    // let click_ty = view.ty.clone();
    let checkbox = empty()
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
        .on_click_stop(move |_| {
            if let Some(expanded) = ty {
                expanded.set(!expanded.get_untracked());
            }
        });
    h_stack((checkbox, id, tab, name)).style(move |s| s.items_center().margin_left(marge_left))
}
