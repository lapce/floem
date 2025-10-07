mod data;
mod view;

use crate::app_state::AppState;
use crate::context::StyleCx;
use crate::event::{Event, EventListener, EventPropagation};
use crate::id::ViewId;
use crate::style::{Style, StyleClassRef, StylePropRef, Transition};
use crate::view::{IntoView, View};
use crate::view_state::ChangeFlags;
use crate::views::{
    button, dyn_container, stack, static_label, text, v_stack, v_stack_from_iter, Decorators, Label,
};
use crate::{keyboard, style, Clipboard};
use floem_reactive::{batch, RwSignal, Scope, SignalGet, SignalUpdate};
use peniko::color::palette;
use peniko::kurbo::{Point, Rect, Size};
use peniko::Color;
use slotmap::Key;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::rc::Rc;
pub use view::capture;
use winit::keyboard::NamedKey;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use crate::inspector::data::CapturedDatas;
use taffy::prelude::Layout;
use taffy::style::FlexDirection;

#[derive(Clone, Debug)]
pub struct CapturedView {
    id: ViewId,
    name: String,
    id_data_str: String,
    custom_name: String,
    layout: Rect,
    taffy: Layout,
    clipped: Rect,
    children: Vec<Rc<CapturedView>>,
    direct_style: Style,
    requested_changes: ChangeFlags,
    keyboard_navigable: bool,
    classes: Vec<StyleClassRef>,
    focused: bool,
}

impl CapturedView {
    pub fn capture(id: ViewId, app_state: &mut AppState, clip: Rect) -> Self {
        let layout = id.layout_rect();
        let taffy = id.get_layout().unwrap_or_default();
        let view_state = id.state();
        let view_state = view_state.borrow();
        let combined_style = view_state.combined_style.clone();
        let keyboard_navigable = app_state.keyboard_navigable.contains(&id);
        let focused = app_state.focus == Some(id);
        let clipped = layout.intersect(clip);
        let custom_name = &view_state.debug_name;
        let classes = view_state.classes.clone();
        let view = id.view();
        let view = view.borrow();
        let name = custom_name
            .iter()
            .rev()
            .chain(std::iter::once(
                &View::debug_name(view.as_ref()).to_string(),
            ))
            .cloned()
            .collect::<Vec<_>>()
            .join(" - ");
        let custom_name = custom_name.iter().cloned().collect::<Vec<_>>().join(" - ");
        Self {
            id,
            name,
            id_data_str: id.data().as_ffi().to_string(),
            custom_name,
            layout,
            taffy,
            clipped,
            direct_style: combined_style,
            requested_changes: view_state.requested_changes,
            keyboard_navigable,
            focused,
            classes,
            children: id
                .children()
                .into_iter()
                .map(|view| Rc::new(CapturedView::capture(view, app_state, clipped)))
                .collect(),
        }
    }

    fn find(&self, id: ViewId) -> Option<&CapturedView> {
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
            .rev()
            .filter_map(|child| child.find_by_pos(pos))
            .next()
            .or_else(|| self.clipped.contains(pos).then_some(self))
    }

    fn find_all_by_pos(&self, pos: Point) -> Vec<ViewId> {
        let mut match_ids = self
            .children
            .iter()
            .rev()
            .filter_map(|child| {
                let child_ids = child.find_all_by_pos(pos);
                if child_ids.is_empty() {
                    None
                } else {
                    Some(child_ids)
                }
            })
            .fold(Vec::new(), |mut init, mut item| {
                init.append(&mut item);
                init
            });
        if match_ids.is_empty() && self.layout.contains(pos) {
            match_ids.push(self.id);
        }
        match_ids
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
    pub window: Option<peniko::ImageBrush>,
    pub window_size: Size,
    pub scale: f64,
    pub state: CaptureState,
    pub renderer: String,
}

#[derive(Default)]
pub struct CaptureState {
    styles: HashMap<ViewId, Style>,
}

impl CaptureState {
    pub(crate) fn capture_style(id: ViewId, cx: &mut StyleCx, computed_style: Style) {
        if cx.app_state_mut().capture.is_some() {
            cx.app_state_mut()
                .capture
                .as_mut()
                .unwrap()
                .styles
                .insert(id, computed_style);
        }
    }
}

fn add_event(
    row: impl View + 'static,
    name: String,
    id: ViewId,
    capture_view: CaptureView,
    capture: &Rc<Capture>,
    datas: RwSignal<CapturedDatas>,
) -> impl View {
    let capture = capture.clone();
    row.keyboard_navigable()
        .on_secondary_click({
            let name = name.clone();
            move |_| {
                if !name.is_empty() {
                    // TODO: Log error
                    let _ = Clipboard::set_contents(name.clone());
                }
                EventPropagation::Stop
            }
        })
        .on_event_stop(EventListener::KeyUp, {
            let capture = capture.clone();
            move |event| {
                if let Event::KeyUp(key) = event {
                    match key.key.logical_key {
                        keyboard::Key::Named(NamedKey::ArrowUp) => {
                            let rs = find_relative_view_by_id_with_self(id, &capture.root);
                            let Some(ids) = rs else {
                                return;
                            };
                            if !key.modifiers.control() {
                                if let Some(id) = ids.big_brother_id {
                                    update_select_view_id(id, &capture_view, true, datas);
                                }
                            } else if let Some(id) = ids.parent_id {
                                update_select_view_id(id, &capture_view, true, datas);
                            }
                        }
                        keyboard::Key::Named(NamedKey::ArrowDown) => {
                            let rs = find_relative_view_by_id_with_self(id, &capture.root);
                            let Some(ids) = rs else {
                                return;
                            };
                            if !key.modifiers.control() {
                                if let Some(id) = ids.next_brother_id {
                                    update_select_view_id(id, &capture_view, true, datas);
                                }
                            } else if let Some(id) = ids.child_id {
                                update_select_view_id(id, &capture_view, true, datas);
                            }
                        }
                        _ => {}
                    }
                }
            }
        })
}

pub(crate) fn header(label: impl Display) -> Label {
    text(label).style(|s| {
        s.padding(5.0)
            .background(palette::css::WHITE_SMOKE)
            .width_full()
            .height(27.0)
            .border_bottom(1.)
            .border_color(palette::css::LIGHT_GRAY)
    })
}

fn info(name: impl Display, value: String) -> impl IntoView {
    info_row(name.to_string(), static_label(value))
}

fn info_row(name: String, view: impl View + 'static) -> impl View {
    stack((
        stack((static_label(name).style(|s| {
            s.margin_right(5.0)
                .color(palette::css::BLACK.with_alpha(0.6))
        }),))
        .style(|s| s.min_width(150.0).flex_direction(FlexDirection::RowReverse)),
        view,
    ))
    .style(|s| {
        s.padding(5.0)
            .hover(|s| s.background(Color::from_rgba8(228, 237, 216, 160)))
    })
}

fn stats(capture: &Capture) -> impl IntoView {
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

fn selected_view(capture: &Rc<Capture>, selected: RwSignal<Option<ViewId>>) -> impl IntoView {
    let capture = capture.clone();
    dyn_container(
        move || selected.get(),
        move |selected_value| {
            if let Some(view) = selected_value.and_then(|id| capture.root.find(id)) {
                let name = info("Type", view.name.clone());
                let id = info("Id", format!("{:?}", view.id));
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
                let clear = button("Clear selection")
                    .style(|s| s.margin(5.0))
                    .on_click_stop(move |_| selected.set(None));
                let clear = stack((clear,));

                let style_header = header("View Style");
                let class_header = header("Class Header");

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
                    .filter_map(|(p, v)| match p.info {
                        style::StyleKeyInfo::Prop(..) => Some((StylePropRef { key: p }, v)),
                        _ => None,
                    })
                    .map(|(p, v)| ((p, format!("{:?}", p.key)), v))
                    .collect::<Vec<_>>();

                style_list.sort_unstable_by(|a, b| a.0 .1.cmp(&b.0 .1));

                let mut class_list = view
                    .classes
                    .clone()
                    .into_iter()
                    .map(|val| StylePropRef { key: val.key })
                    .map(|val| format!("{:?}", val.key))
                    .collect::<Vec<_>>();

                class_list.sort_unstable();

                let style_list =
                    v_stack_from_iter(style_list.into_iter().map(|((prop, name), value)| {
                        let name = name.strip_prefix("floem::style::").unwrap_or(&name);
                        let name = if direct.contains(&prop.key) {
                            text(name).into_any()
                        } else {
                            stack((
                                text("Inherited").style(|s| {
                                    s.margin_right(5.0)
                                        .background(palette::css::WHITE_SMOKE.with_alpha(0.6))
                                        .border(1.)
                                        .border_radius(5.0)
                                        .border_color(palette::css::WHITE_SMOKE)
                                        .padding(1.0)
                                        .font_size(10.0)
                                        .color(palette::css::BLACK.with_alpha(0.4))
                                }),
                                text(name),
                            ))
                            .into_any()
                        };
                        let mut v = (prop.info().debug_view)(&*value).unwrap_or_else(|| {
                            static_label((prop.info().debug_any)(&*value)).into_any()
                        });
                        if let Some(transition) = style
                            .map
                            .get(&prop.info().transition_key)
                            .map(|v| v.downcast_ref::<Transition>().unwrap().clone())
                        {
                            let transition = stack((
                                text("Transition").style(|s| {
                                    s.margin_top(5.0)
                                        .margin_right(5.0)
                                        .background(palette::css::WHITE_SMOKE.with_alpha(0.6))
                                        .border(1.)
                                        .border_radius(5.0)
                                        .border_color(palette::css::WHITE_SMOKE)
                                        .padding(1.0)
                                        .font_size(10.0)
                                        .color(palette::css::BLACK.with_alpha(0.4))
                                }),
                                static_label(format!("{transition:?}")),
                            ))
                            .style(|s| s.items_center());
                            v = v_stack((v, transition)).into_any();
                        }
                        stack((
                            stack((name.style(|s| {
                                s.margin_right(5.0)
                                    .color(palette::css::BLACK.with_alpha(0.6))
                            }),))
                            .style(|s| {
                                s.min_width(150.0).flex_direction(FlexDirection::RowReverse)
                            }),
                            v,
                        ))
                        .style(|s| {
                            s.padding(5.0)
                                .items_center()
                                .hover(|s| s.background(Color::from_rgba8(228, 237, 216, 160)))
                        })
                    }))
                    .style(|s| s.width_full());

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
                    class_header,
                    v_stack_from_iter(class_list.iter().map(text)).style(|s| s.gap(10)),
                ))
                .style(|s| s.width_full())
                .into_any()
            } else {
                text("No selection").style(|s| s.padding(5.0)).into_any()
            }
        },
    )
    .into_view()
}

#[derive(Clone, Copy)]
struct CaptureView {
    expanding_selection: RwSignal<Option<(ViewId, bool)>>,
    scroll_to: RwSignal<Option<ViewId>>,
    selected: RwSignal<Option<ViewId>>,
    highlighted: RwSignal<Option<ViewId>>,
}

thread_local! {
    pub(crate) static RUNNING: Cell<bool> = const { Cell::new(false) };
    pub(crate) static CAPTURE: RwSignal<Option<Rc<Capture>>> = {
        Scope::new().create_rw_signal(None)
    };
}

fn find_view(name: &str, views: &Rc<CapturedView>) -> Vec<ViewId> {
    let mut ids = Vec::new();
    if name.is_empty() {
        return ids;
    }
    if views
        .name
        .to_lowercase()
        .contains(name.to_lowercase().as_str())
        || views.id_data_str.contains(name)
    {
        ids.push(views.id);
    }
    views
        .children
        .iter()
        .filter_map(|x| {
            let ids = find_view(name, x);
            if ids.is_empty() {
                None
            } else {
                Some(ids)
            }
        })
        .fold(ids, |mut init, mut item| {
            init.append(&mut item);
            init
        })
}

fn find_relative_view_by_id_without_self(
    id: ViewId,
    views: &Rc<CapturedView>,
) -> Option<RelativeViewId> {
    let mut parent_id = None;
    let mut big_brother_id = None;
    let mut next_brother_id = None;
    let mut first_child_id = None;
    let mut found = false;
    let mut previous = None;
    for child in &views.children {
        if child.id == id {
            parent_id = Some(views.id);
            big_brother_id = previous;
            first_child_id = child.children.first().map(|x| x.id);
            found = true;
        } else if found {
            next_brother_id = Some(child.id);
            break;
        } else {
            previous = Some(child.id);
        }
    }
    if found {
        Some(RelativeViewId::new(
            parent_id,
            big_brother_id,
            next_brother_id,
            first_child_id,
        ))
    } else {
        for child in &views.children {
            let rs = find_relative_view_by_id_without_self(id, child);
            if rs.is_some() {
                return rs;
            }
        }
        None
    }
}

fn find_relative_view_by_id_with_self(
    id: ViewId,
    views: &Rc<CapturedView>,
) -> Option<RelativeViewId> {
    if views.id == id {
        let first_child_id = views.children.first().map(|x| x.id);
        Some(RelativeViewId::new(None, None, None, first_child_id))
    } else {
        find_relative_view_by_id_without_self(id, views)
    }
}

fn update_select_view_id(
    id: ViewId,
    capture: &CaptureView,
    request_focus: bool,
    datas: RwSignal<CapturedDatas>,
) {
    capture.selected.set(Some(id));
    capture.highlighted.set(Some(id));
    capture.expanding_selection.set(Some((id, request_focus)));
    batch(|| {
        datas.update(|x| {
            x.focus(id);
        });
    });
}

#[derive(Debug, Default, Clone)]
struct RelativeViewId {
    pub parent_id: Option<ViewId>,
    pub big_brother_id: Option<ViewId>,
    pub next_brother_id: Option<ViewId>,
    pub child_id: Option<ViewId>,
}

impl RelativeViewId {
    pub fn new(
        parent_id: Option<ViewId>,
        big_brother_id: Option<ViewId>,
        next_brother_id: Option<ViewId>,
        child_id: Option<ViewId>,
    ) -> Self {
        Self {
            parent_id,
            big_brother_id,
            next_brother_id,
            child_id,
        }
    }
}
