use crate::app::{add_app_update_event, AppUpdateEvent};
use crate::context::{AppState, ChangeFlags, StyleCx};
use crate::event::{Event, EventListener};
use crate::id::Id;
use crate::profiler::profiler;
use crate::style::{Style, StyleMapValue};
use crate::view::{view_children, View};
use crate::views::{
    container, dyn_container, empty, h_stack, img_dynamic, scroll, stack, static_label,
    static_list, tab, text, v_stack, Decorators, Label,
};
use crate::widgets::button;
use crate::window::WindowConfig;
use crate::{new_window, style, EventPropagation};
use floem_reactive::{create_effect, create_rw_signal, create_signal, RwSignal, Scope};
use image::DynamicImage;
use kurbo::{Point, Rect, Size};
use peniko::Color;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::rc::Rc;
use std::time::{Duration, Instant};
use taffy::prelude::Layout;
use taffy::style::{AlignItems, FlexDirection};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

#[derive(Clone, Debug)]
pub struct CapturedView {
    id: Id,
    name: String,
    layout: Rect,
    taffy: Layout,
    clipped: Rect,
    children: Vec<Rc<CapturedView>>,
    direct_style: Style,
    requested_changes: ChangeFlags,
    keyboard_navigable: bool,
    focused: bool,
}

impl CapturedView {
    pub fn capture(view: &dyn View, app_state: &mut AppState, clip: Rect) -> Self {
        let id = view.id();
        let layout = app_state.get_layout_rect(id);
        let taffy = app_state.get_layout(id).unwrap();
        let computed_style = app_state.get_computed_style(id).clone();
        let keyboard_navigable = app_state.keyboard_navigable.contains(&id);
        let focused = app_state.focus == Some(id);
        let state = app_state.view_state(id);
        let clipped = layout.intersect(clip);
        Self {
            id,
            name: view.debug_name().to_string(),
            layout,
            taffy,
            clipped,
            direct_style: computed_style,
            requested_changes: state.requested_changes,
            keyboard_navigable,
            focused,
            children: view_children(view)
                .into_iter()
                .map(|view| Rc::new(CapturedView::capture(view, app_state, clipped)))
                .collect(),
        }
    }

    fn find(&self, id: Id) -> Option<&CapturedView> {
        if self.id == id {
            return Some(self);
        }
        self.children
            .iter()
            .filter_map(|child| child.find(id))
            .next()
    }

    fn find_by_pos(&self, pos: Point) -> Option<&CapturedView> {
        self.children
            .iter()
            .filter_map(|child| child.find_by_pos(pos))
            .next()
            .or_else(|| self.clipped.contains(pos).then_some(self))
    }

    fn warnings(&self) -> bool {
        !self.requested_changes.is_empty() || self.children.iter().any(|child| child.warnings())
    }
}

pub struct Capture {
    pub root: Rc<CapturedView>,
    pub start: Instant,
    pub post_style: Instant,
    pub post_layout: Instant,
    pub end: Instant,
    pub taffy_duration: Duration,
    pub taffy_node_count: usize,
    pub taffy_depth: usize,
    pub window: Option<Rc<DynamicImage>>,
    pub window_size: Size,
    pub scale: f64,
    pub state: CaptureState,
}

#[derive(Default)]
pub struct CaptureState {
    styles: HashMap<Id, Style>,
}

impl CaptureState {
    pub(crate) fn capture_style(id: Id, cx: &mut StyleCx) {
        if cx.app_state_mut().capture.is_some() {
            let direct = cx.direct.clone();
            let mut current = (*cx.current).clone();
            current.apply_mut(direct);
            cx.app_state_mut()
                .capture
                .as_mut()
                .unwrap()
                .styles
                .insert(id, current);
        }
    }
}

fn captured_view_name(view: &CapturedView) -> impl View {
    let name = static_label(view.name.clone());
    let id = text(view.id.to_raw()).style(|s| {
        s.margin_right(5.0)
            .background(Color::BLACK.with_alpha_factor(0.02))
            .border(1.0)
            .border_radius(5.0)
            .border_color(Color::BLACK.with_alpha_factor(0.07))
            .padding(3.0)
            .padding_top(0.0)
            .padding_bottom(0.0)
            .font_size(12.0)
            .color(Color::BLACK.with_alpha_factor(0.6))
    });
    let tab: Box<dyn View> = if view.focused {
        Box::new(text("Focus").style(|s| {
            s.margin_right(5.0)
                .background(Color::rgb8(63, 81, 101).with_alpha_factor(0.6))
                .border_radius(5.0)
                .padding(1.0)
                .font_size(10.0)
                .color(Color::WHITE.with_alpha_factor(0.8))
        }))
    } else if view.keyboard_navigable {
        Box::new(text("Tab").style(|s| {
            s.margin_right(5.0)
                .background(Color::rgb8(204, 217, 221).with_alpha_factor(0.4))
                .border(1.0)
                .border_radius(5.0)
                .border_color(Color::BLACK.with_alpha_factor(0.07))
                .padding(1.0)
                .font_size(10.0)
                .color(Color::BLACK.with_alpha_factor(0.4))
        }))
    } else {
        Box::new(empty())
    };
    h_stack((id, tab, name)).style(|s| s.items_center())
}

// Outlined to reduce stack usage.
#[inline(never)]
fn captured_view_no_children(
    view: &CapturedView,
    depth: usize,
    capture_view: &CaptureView,
) -> Box<dyn View> {
    let offset = depth as f64 * 14.0;
    let name = captured_view_name(view);
    let name_id = name.id();
    let height = 20.0;
    let id = view.id;
    let selected = capture_view.selected;
    let highlighted = capture_view.highlighted;

    let row = Box::new(
        container(name)
            .style(move |s| {
                s.padding_left(20.0 + offset)
                    .hover(move |s| {
                        s.background(Color::rgba8(228, 237, 216, 160))
                            .apply_if(selected.get() == Some(id), |s| {
                                s.background(Color::rgb8(186, 180, 216))
                            })
                    })
                    .height(height)
                    .apply_if(highlighted.get() == Some(id), |s| {
                        s.background(Color::rgba8(228, 237, 216, 160))
                    })
                    .apply_if(selected.get() == Some(id), |s| {
                        if highlighted.get() == Some(id) {
                            s.background(Color::rgb8(186, 180, 216))
                        } else {
                            s.background(Color::rgb8(213, 208, 216))
                        }
                    })
            })
            .on_click_stop(move |_| selected.set(Some(id)))
            .on_event_cont(EventListener::PointerEnter, move |_| {
                highlighted.set(Some(id))
            }),
    );

    let row_id = row.id();
    let scroll_to = capture_view.scroll_to;
    let expanding_selection = capture_view.expanding_selection;
    create_effect(move |_| {
        if let Some(selection) = expanding_selection.get() {
            if selection == id {
                // Scroll to the row, then to the name part of the row.
                scroll_to.set(Some(row_id));
                scroll_to.set(Some(name_id));
            }
        }
    });

    row
}

// Outlined to reduce stack usage.
#[inline(never)]
fn captured_view_with_children(
    view: &Rc<CapturedView>,
    depth: usize,
    capture_view: &CaptureView,
    children: Vec<Box<dyn View>>,
) -> Box<dyn View> {
    let offset = depth as f64 * 14.0;
    let name = captured_view_name(view);
    let height = 20.0;
    let id = view.id;
    let selected = capture_view.selected;
    let highlighted = capture_view.highlighted;
    let expanding_selection = capture_view.expanding_selection;
    let view_ = view.clone();

    let expanded = create_rw_signal(true);

    let name_id = name.id();
    let row = stack((
        empty()
            .style(move |s| {
                s.background(if expanded.get() {
                    Color::WHITE.with_alpha_factor(0.3)
                } else {
                    Color::BLACK.with_alpha_factor(0.3)
                })
                .border(1.0)
                .width(12.0)
                .height(12.0)
                .margin_left(offset)
                .margin_right(4.0)
                .border_color(Color::BLACK.with_alpha_factor(0.4))
                .border_radius(4.0)
                .hover(move |s| {
                    s.border_color(Color::BLACK.with_alpha_factor(0.6))
                        .background(if expanded.get() {
                            Color::WHITE.with_alpha_factor(0.5)
                        } else {
                            Color::BLACK.with_alpha_factor(0.5)
                        })
                })
            })
            .on_click_stop(move |_| {
                expanded.set(!expanded.get());
            }),
        name,
    ))
    .style(move |s| {
        s.padding_left(3.0)
            .align_items(AlignItems::Center)
            .hover(move |s| {
                s.background(Color::rgba8(228, 237, 216, 160))
                    .apply_if(selected.get() == Some(id), |s| {
                        s.background(Color::rgb8(186, 180, 216))
                    })
            })
            .height(height)
            .apply_if(highlighted.get() == Some(id), |s| {
                s.background(Color::rgba8(228, 237, 216, 160))
            })
            .apply_if(selected.get() == Some(id), |s| {
                if highlighted.get() == Some(id) {
                    s.background(Color::rgb8(186, 180, 216))
                } else {
                    s.background(Color::rgb8(213, 208, 216))
                }
            })
    })
    .on_click_stop(move |_| selected.set(Some(id)))
    .on_event_cont(EventListener::PointerEnter, move |_| {
        highlighted.set(Some(id))
    });

    let row_id = row.id();
    let scroll_to = capture_view.scroll_to;
    create_effect(move |_| {
        if let Some(selection) = expanding_selection.get() {
            if selection != id && view_.find(selection).is_some() {
                expanded.set(true);
            }
            if selection == id {
                // Scroll to the row, then to the name part of the row.
                scroll_to.set(Some(row_id));
                scroll_to.set(Some(name_id));
            }
        }
    });

    let line = empty().style(move |s| {
        s.absolute()
            .height_full()
            .width(1.0)
            .margin_left(9.0 + offset)
            .background(Color::BLACK.with_alpha_factor(0.1))
    });

    let list = static_list(children).style(move |s| {
        s.display(if expanded.get() {
            style::Display::Flex
        } else {
            style::Display::None
        })
    });

    let list = v_stack((line, list));

    Box::new(v_stack((row, list)).style(|s| s.min_width_full()))
}

fn captured_view(
    view: &Rc<CapturedView>,
    depth: usize,
    capture_view: &CaptureView,
) -> Box<dyn View> {
    if view.children.is_empty() {
        captured_view_no_children(view, depth, capture_view)
    } else {
        let children: Vec<_> = view
            .children
            .iter()
            .map(|view| captured_view(view, depth + 1, capture_view))
            .collect();
        captured_view_with_children(view, depth, capture_view, children)
    }
}

pub(crate) fn header(label: impl Display) -> Label {
    text(label).style(|s| {
        s.padding(5.0)
            .background(Color::WHITE_SMOKE)
            .width_full()
            .height(27.0)
            .border_bottom(1.0)
            .border_color(Color::LIGHT_GRAY)
    })
}

fn info(name: impl Display, value: String) -> impl View {
    info_row(name.to_string(), static_label(value))
}

fn info_row(name: String, view: impl View + 'static) -> impl View {
    stack((
        stack((static_label(name).style(|s| {
            s.margin_right(5.0)
                .color(Color::BLACK.with_alpha_factor(0.6))
        }),))
        .style(|s| s.min_width(150.0).flex_direction(FlexDirection::RowReverse)),
        view,
    ))
    .style(|s| {
        s.padding(5.0)
            .hover(|s| s.background(Color::rgba8(228, 237, 216, 160)))
    })
}

fn stats(capture: &Capture) -> impl View {
    let style_time = capture.post_style.saturating_duration_since(capture.start);
    let layout_time = capture
        .post_layout
        .saturating_duration_since(capture.post_style);
    let paint_time = capture.end.saturating_duration_since(capture.post_layout);
    let style_time = info(
        "Style Time",
        format!("{:.4} ms", style_time.as_secs_f64() * 1000.0),
    );
    let layout_time = info(
        "Layout Time",
        format!("{:.4} ms", layout_time.as_secs_f64() * 1000.0),
    );
    let taffy_time = info(
        "Taffy Time",
        format!("{:.4} ms", capture.taffy_duration.as_secs_f64() * 1000.0),
    );
    let taffy_node_count = info("Taffy Node Count", capture.taffy_node_count.to_string());
    let taffy_depth = info("Taffy Depth", capture.taffy_depth.to_string());
    let paint_time = info(
        "Paint Time",
        format!("{:.4} ms", paint_time.as_secs_f64() * 1000.0),
    );
    let w = info("Window Width", format!("{}", capture.window_size.width));
    let h = info("Window Height", format!("{}", capture.window_size.height));
    v_stack((
        style_time,
        layout_time,
        taffy_time,
        taffy_node_count,
        taffy_depth,
        paint_time,
        w,
        h,
    ))
}

fn selected_view(capture: &Rc<Capture>, selected: RwSignal<Option<Id>>) -> impl View {
    let capture = capture.clone();
    dyn_container(
        move || selected.get(),
        move |current| {
            if let Some(view) = current.and_then(|id| capture.root.find(id)) {
                let name = info("Type", view.name.clone());
                let id = info("Id", view.id.to_raw().to_string());
                let count = info("Child Count", format!("{}", view.children.len()));
                let beyond = |view: f64, window| {
                    if view > window {
                        format!(" ({} after window edge)", view - window)
                    } else if view < 0.0 {
                        format!(" ({} before window edge)", -view)
                    } else {
                        String::new()
                    }
                };
                let x = info(
                    "X",
                    format!(
                        "{}{}",
                        view.layout.x0,
                        beyond(view.layout.x0, capture.window_size.width)
                    ),
                );
                let y = info(
                    "Y",
                    format!(
                        "{}{}",
                        view.layout.y0,
                        beyond(view.layout.y0, capture.window_size.height)
                    ),
                );
                let w = info(
                    "Width",
                    format!(
                        "{}{}",
                        view.layout.width(),
                        beyond(view.layout.x1, capture.window_size.width)
                    ),
                );
                let h = info(
                    "Height",
                    format!(
                        "{}{}",
                        view.layout.height(),
                        beyond(view.layout.y1, capture.window_size.height)
                    ),
                );
                let tx = info(
                    "Taffy X",
                    format!(
                        "{}{}",
                        view.taffy.location.x,
                        beyond(
                            view.taffy.location.x as f64 + view.taffy.size.width as f64,
                            capture.window_size.width
                        )
                    ),
                );
                let ty = info(
                    "Taffy Y",
                    format!(
                        "{}{}",
                        view.taffy.location.y,
                        beyond(
                            view.taffy.location.x as f64 + view.taffy.size.width as f64,
                            capture.window_size.width
                        )
                    ),
                );
                let tw = info(
                    "Taffy Width",
                    format!(
                        "{}{}",
                        view.taffy.size.width,
                        beyond(
                            view.taffy.location.x as f64 + view.taffy.size.width as f64,
                            capture.window_size.width
                        )
                    ),
                );
                let th = info(
                    "Taffy Height",
                    format!(
                        "{}{}",
                        view.taffy.size.height,
                        beyond(
                            view.taffy.location.y as f64 + view.taffy.size.height as f64,
                            capture.window_size.height
                        )
                    ),
                );
                let clear = button(|| "Clear selection")
                    .style(|s| s.margin(5.0))
                    .on_click_stop(move |_| selected.set(None));
                let clear = stack((clear,));

                let style_header = header("View Style");

                let direct: HashSet<_> = view.direct_style.map.keys().copied().collect();

                let style = capture
                    .state
                    .styles
                    .get(&view.id)
                    .cloned()
                    .unwrap_or_default();

                let mut style_list = style
                    .map
                    .clone()
                    .into_iter()
                    .map(|(p, v)| ((p, format!("{p:?}")), v))
                    .collect::<Vec<_>>();

                style_list.sort_unstable_by(|a, b| a.0 .1.cmp(&b.0 .1));

                let style_list =
                    static_list(style_list.into_iter().map(|((prop, name), value)| {
                        let name = name.strip_prefix("floem::style::").unwrap_or(&name);
                        let name: Box<dyn View> = if direct.contains(&prop) {
                            Box::new(text(name))
                        } else {
                            Box::new(stack((
                                text("Inherited").style(|s| {
                                    s.margin_right(5.0)
                                        .background(Color::WHITE_SMOKE.with_alpha_factor(0.6))
                                        .border(1.0)
                                        .border_radius(5.0)
                                        .border_color(Color::WHITE_SMOKE)
                                        .padding(1.0)
                                        .font_size(10.0)
                                        .color(Color::BLACK.with_alpha_factor(0.4))
                                }),
                                text(name),
                            )))
                        };
                        let mut v: Box<dyn View> = match value {
                            StyleMapValue::Val(v) => {
                                let v = &*v;
                                (prop.info.debug_view)(v).unwrap_or_else(|| {
                                    Box::new(static_label((prop.info.debug_any)(v)))
                                })
                            }
                            StyleMapValue::Unset => Box::new(text("Unset")),
                        };
                        if let Some(transition) = style.transitions.get(&prop).cloned() {
                            let transition = stack((
                                text("Transition").style(|s| {
                                    s.margin_top(5.0)
                                        .margin_right(5.0)
                                        .background(Color::WHITE_SMOKE.with_alpha_factor(0.6))
                                        .border(1.0)
                                        .border_radius(5.0)
                                        .border_color(Color::WHITE_SMOKE)
                                        .padding(1.0)
                                        .font_size(10.0)
                                        .color(Color::BLACK.with_alpha_factor(0.4))
                                }),
                                static_label(format!("{transition:?}")),
                            ))
                            .style(|s| s.items_center());
                            v = Box::new(v_stack((v, transition)));
                        }
                        stack((
                            stack((name.style(|s| {
                                s.margin_right(5.0)
                                    .color(Color::BLACK.with_alpha_factor(0.6))
                            }),))
                            .style(|s| {
                                s.min_width(150.0).flex_direction(FlexDirection::RowReverse)
                            }),
                            v,
                        ))
                        .style(|s| {
                            s.padding(5.0)
                                .items_center()
                                .hover(|s| s.background(Color::rgba8(228, 237, 216, 160)))
                        })
                    }))
                    .style(|s| s.width_full());

                Box::new(
                    v_stack((
                        name,
                        id,
                        count,
                        x,
                        y,
                        w,
                        h,
                        tx,
                        ty,
                        tw,
                        th,
                        clear,
                        style_header,
                        style_list,
                    ))
                    .style(|s| s.width_full()),
                )
            } else {
                Box::new(text("No selection").style(|s| s.padding(5.0)))
            }
        },
    )
}

#[derive(Clone, Copy)]
struct CaptureView {
    expanding_selection: RwSignal<Option<Id>>,
    scroll_to: RwSignal<Option<Id>>,
    selected: RwSignal<Option<Id>>,
    highlighted: RwSignal<Option<Id>>,
}

fn capture_view(capture: &Rc<Capture>) -> impl View {
    let capture_view = CaptureView {
        expanding_selection: create_rw_signal(None),
        scroll_to: create_rw_signal(None),
        selected: create_rw_signal(None),
        highlighted: create_rw_signal(None),
    };

    let window = capture.window.clone();
    let capture_ = capture.clone();
    let capture__ = capture.clone();
    let (image_width, image_height) = capture
        .window
        .as_ref()
        .map(|img| {
            (
                img.width() as f64 / capture.scale,
                img.height() as f64 / capture.scale,
            )
        })
        .unwrap_or_default();
    let image = img_dynamic(move || window.clone())
        .style(move |s| {
            s.margin(5.0)
                .border(1.0)
                .border_color(Color::BLACK.with_alpha_factor(0.5))
                .width(image_width + 2.0)
                .height(image_height + 2.0)
                .margin_bottom(21.0)
                .margin_right(21.0)
        })
        .on_event(EventListener::PointerMove, move |e| {
            if let Event::PointerMove(e) = e {
                if let Some(view) = capture_.root.find_by_pos(e.pos) {
                    if capture_view.highlighted.get() != Some(view.id) {
                        capture_view.highlighted.set(Some(view.id));
                    }
                    return EventPropagation::Continue;
                }
            }
            if capture_view.highlighted.get().is_some() {
                capture_view.highlighted.set(None);
            }
            EventPropagation::Continue
        })
        .on_click(move |e| {
            if let Event::PointerUp(e) = e {
                if let Some(view) = capture__.root.find_by_pos(e.pos) {
                    capture_view.selected.set(Some(view.id));
                    capture_view.expanding_selection.set(Some(view.id));
                    return EventPropagation::Stop;
                }
            }
            if capture_view.selected.get().is_some() {
                capture_view.selected.set(None);
            }
            EventPropagation::Stop
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
                .margin_left(5.0 + view.layout.x0)
                .margin_top(5.0 + view.layout.y0)
                .width(view.layout.width())
                .height(view.layout.height())
                .background(Color::rgb8(186, 180, 216).with_alpha_factor(0.5))
                .border_color(Color::rgb8(186, 180, 216).with_alpha_factor(0.7))
                .border(1.0)
        } else {
            s
        }
    });

    let capture_ = capture.clone();
    let highlighted_overlay = empty().style(move |s| {
        if let Some(view) = capture_view
            .highlighted
            .get()
            .and_then(|id| capture_.root.find(id))
        {
            s.absolute()
                .margin_left(5.0 + view.layout.x0)
                .margin_top(5.0 + view.layout.y0)
                .width(view.layout.width())
                .height(view.layout.height())
                .background(Color::rgba8(228, 237, 216, 120))
                .border_color(Color::rgba8(75, 87, 53, 120))
                .border(1.0)
        } else {
            s
        }
    });

    let image = stack((image, selected_overlay, highlighted_overlay));

    let left_scroll = scroll(
        v_stack((
            header("Selected View"),
            selected_view(capture, capture_view.selected),
            header("Stats"),
            stats(capture),
        ))
        .style(|s| s.width_full()),
    )
    .style(|s| s.width_full().flex_basis(0).min_height(0).flex_grow(1.0));

    let seperator = empty().style(move |s| {
        s.width_full()
            .min_height(1.0)
            .background(Color::BLACK.with_alpha_factor(0.2))
    });

    let left = v_stack((
        header("Captured Window"),
        scroll(image).style(|s| s.max_height_pct(60.0)),
        seperator,
        left_scroll,
    ))
    .style(|s| s.max_width_pct(60.0));

    let tree = scroll(captured_view(&capture.root, 0, &capture_view))
        .style(|s| s.width_full().min_height(0).flex_basis(0).flex_grow(1.0))
        .on_event_cont(EventListener::PointerLeave, move |_| {
            capture_view.highlighted.set(None)
        })
        .on_click_stop(move |_| capture_view.selected.set(None))
        .scroll_to_view(move || capture_view.scroll_to.get());

    let tree: Box<dyn View> = if capture.root.warnings() {
        Box::new(v_stack((header("Warnings"), header("View Tree"), tree)))
    } else {
        Box::new(v_stack((header("View Tree"), tree)))
    };

    let tree = tree.style(|s| s.height_full().min_width(0).flex_basis(0).flex_grow(1.0));

    let seperator = empty().style(move |s| {
        s.height_full()
            .min_width(1.0)
            .background(Color::BLACK.with_alpha_factor(0.2))
    });

    h_stack((left, seperator, tree)).style(|s| s.height_full().width_full().max_width_full())
}

fn inspector_view(capture: &Option<Rc<Capture>>) -> impl View {
    let view: Box<dyn View> = if let Some(capture) = capture {
        Box::new(capture_view(capture))
    } else {
        Box::new(text("No capture"))
    };

    stack((view,))
        .window_title(|| "Floem Inspector".to_owned())
        .style(|s| {
            s.width_full()
                .height_full()
                .background(Color::WHITE)
                .class(scroll::Handle, |s| {
                    s.border_radius(4.0)
                        .background(Color::rgba8(166, 166, 166, 140))
                        .set(scroll::Thickness, 16.0)
                        .set(scroll::Rounded, false)
                        .active(|s| s.background(Color::rgb8(166, 166, 166)))
                        .hover(|s| s.background(Color::rgb8(184, 184, 184)))
                })
                .class(scroll::Track, |s| {
                    s.hover(|s| s.background(Color::rgba8(166, 166, 166, 30)))
                })
        })
}

thread_local! {
    pub(crate) static RUNNING: Cell<bool> = Cell::new(false);
    pub(crate) static CAPTURE: RwSignal<Option<Rc<Capture>>> = {
        Scope::new().create_rw_signal(None)
    };
}

pub fn capture(window_id: WindowId) {
    let capture = CAPTURE.with(|c| *c);

    if !RUNNING.get() {
        RUNNING.set(true);
        new_window(
            move |_| {
                let (selected, set_selected) = create_signal(0);

                let tab_item = |name, index| {
                    text(name)
                        .on_click_stop(move |_| set_selected.set(index))
                        .style(move |s| {
                            s.padding(5.0)
                                .border_right(1)
                                .border_color(Color::BLACK.with_alpha_factor(0.2))
                                .hover(move |s| {
                                    s.background(Color::rgba8(228, 237, 216, 160))
                                        .apply_if(selected.get() == index, |s| {
                                            s.background(Color::rgb8(186, 180, 216))
                                        })
                                })
                                .apply_if(selected.get() == index, |s| {
                                    s.background(Color::rgb8(213, 208, 216))
                                })
                        })
                };

                let tabs = h_stack((tab_item("Views", 0), tab_item("Profiler", 1)))
                    .style(|s| s.background(Color::WHITE));

                let tab = tab(
                    move || selected.get(),
                    move || [0, 1].into_iter(),
                    |it| *it,
                    move |it| -> Box<dyn View> {
                        match it {
                            0 => Box::new(
                                dyn_container(
                                    move || capture.get(),
                                    |capture| Box::new(inspector_view(&capture)),
                                )
                                .style(|s| s.width_full().height_full()),
                            ),
                            1 => Box::new(profiler(window_id)),
                            _ => panic!(),
                        }
                    },
                )
                .style(|s| s.flex_basis(0.0).min_height(0.0).flex_grow(1.0));

                let seperator = empty().style(move |s| {
                    s.width_full()
                        .min_height(1.0)
                        .background(Color::BLACK.with_alpha_factor(0.2))
                });

                let stack = v_stack((tabs, seperator, tab));
                let id = stack.id();
                stack
                    .style(|s| s.width_full().height_full())
                    .on_event(EventListener::KeyUp, move |e| {
                        if let Event::KeyUp(e) = e {
                            if e.key.logical_key == Key::Named(NamedKey::F11)
                                && e.modifiers.shift_key()
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
            Some(WindowConfig {
                size: Some(Size {
                    width: 1200.0,
                    height: 800.0,
                }),
                ..Default::default()
            }),
        );
    }

    add_app_update_event(AppUpdateEvent::CaptureWindow {
        window_id,
        capture: capture.write_only(),
    })
}
