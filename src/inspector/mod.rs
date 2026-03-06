mod data;
pub(crate) mod profiler;
mod view;
use floem_reactive::{Effect, Scope};
use peniko::kurbo::{Rect, Size};
use slotmap::Key as _;
pub use view::capture;

use crate::{
    AnyView, Clipboard, ViewId, WindowState,
    event::EventPropagation,
    inspector::data::CapturedDatas,
    platform::{Duration, Instant},
    prelude::*,
    style::{OverflowX, OverflowY, Style, StyleCx, StyleThemeExt},
};

use std::{
    cell::Cell,
    collections::HashMap,
    fmt::Display,
    rc::Rc,
};

use taffy::{prelude::Layout, style::FlexDirection};

#[derive(Clone, Debug)]
pub struct CapturedView {
    id: ViewId,
    name: String,
    id_data_str: String,
    world_bounds: Rect,
    taffy: Layout,
    children: Vec<Rc<CapturedView>>,
    direct_style: Style,
    keyboard_navigable: bool,
    focused: bool,
}

impl CapturedView {
    pub fn capture(id: ViewId, window_state: &mut WindowState) -> Self {
        let world_bounds = id.get_visual_rect_no_clip();
        let taffy = id.get_layout().unwrap_or_default();
        let view_state = id.state();
        let view_state = view_state.borrow();
        let combined_style = view_state.combined_style.clone();
        let focus = view_state.combined_style.builtin().set_focus();
        let focused = window_state.focus_state.current_path().last() == Some(&id.get_element_id());
        let custom_name = &view_state.debug_name;
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
        Self {
            id,
            name,
            id_data_str: id.data().as_ffi().to_string(),
            world_bounds,
            taffy,
            direct_style: combined_style,
            keyboard_navigable: focus.allows_keyboard_navigation(),
            focused,
            children: id
                .children()
                .into_iter()
                .map(|view| Rc::new(CapturedView::capture(view, window_state)))
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

    fn warnings(&self) -> bool {
        false
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
    computed_styles: HashMap<ViewId, Style>,
}

impl CaptureState {
    pub(crate) fn capture_style(id: ViewId, cx: &mut StyleCx, computed_style: Style) {
        if let Some(capture) = cx.window_state.capture.as_mut() {
            capture.computed_styles.insert(id, computed_style);
        }
    }
}

fn add_event<T: View + 'static>(
    row: T,
    name: String,
    id: ViewId,
    capture_view: CaptureView,
    capture: Rc<Capture>,
    datas: RwSignal<CapturedDatas>,
) -> impl View + use<T> {
    let capture = capture.clone();
    row.on_event(listener::SecondaryClick, {
        let name = name.clone();
        move |_, _| {
            if !name.is_empty() {
                // TODO: Log error
                let _ = Clipboard::set_contents(name.clone());
            }
            EventPropagation::Stop
        }
    })
    .on_event_stop(listener::KeyDown, {
        let capture = capture.clone();
        move |_cx, KeyboardEvent { key, modifiers, .. }| match key {
            Key::Named(NamedKey::ArrowUp) => {
                let rs = find_relative_view_by_id_with_self(id, &capture.root);
                let Some(ids) = rs else {
                    return;
                };
                if !modifiers.ctrl() {
                    if let Some(id) = ids.big_brother_id {
                        update_select_view_id(id, &capture_view, true, datas);
                    }
                } else if let Some(id) = ids.parent_id {
                    update_select_view_id(id, &capture_view, true, datas);
                }
            }
            Key::Named(NamedKey::ArrowDown) => {
                let rs = find_relative_view_by_id_with_self(id, &capture.root);
                let Some(ids) = rs else {
                    return;
                };
                if !modifiers.ctrl() {
                    if let Some(id) = ids.next_brother_id {
                        update_select_view_id(id, &capture_view, true, datas);
                    }
                } else if let Some(id) = ids.child_id {
                    update_select_view_id(id, &capture_view, true, datas);
                }
            }
            _ => {}
        }
    })
}

pub(crate) fn header(label: impl Display) -> Label {
    Label::new(label).style(|s| {
        s.padding(5.0)
            .width_full()
            .height(27.0)
            .border_bottom(1.)
            .font_bold()
            .with_theme(|s, t| s.border_color(t.border()).color(t.primary()))
    })
}

fn info(name: impl Display, value: String) -> AnyView {
    info_row(name.to_string(), value.style(|s| s.font_bold())).into_any()
}

fn info_row(name: String, view: impl IntoView + 'static) -> impl View {
    let name = name
        .style(|s| {
            s.margin_right(5.0)
                .with_theme(|s, t| s.color(t.text_muted()))
        })
        .container()
        .style(|s| s.min_width(150.0).flex_direction(FlexDirection::RowReverse));
    (name, view).h_stack()
}

fn stats(capture: &Capture) -> impl IntoView + use<> {
    let style_time = capture.post_style.saturating_duration_since(capture.start);
    let layout_time = capture
        .post_layout
        .saturating_duration_since(capture.post_style);
    let paint_time = capture.end.saturating_duration_since(capture.post_layout);
    let style_time = info(
        "Full Style Time",
        format!("{:.4} ms", style_time.as_secs_f64() * 1000.0),
    );
    let layout_time = info(
        "Full Layout Time",
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
    Stack::vertical_from_iter(
        [
            style_time,
            layout_time,
            taffy_time,
            taffy_node_count,
            taffy_depth,
            paint_time,
            w,
            h,
        ]
        .into_iter()
        .enumerate()
        .map(|(idx, v)| {
            v.style(move |s| {
                s.padding(3).with_theme(move |s, t| {
                    s.apply_if(idx.is_multiple_of(2), |s| s.background(t.bg_base()))
                        .apply_if(!idx.is_multiple_of(2), |s| s.background(t.bg_elevated()))
                })
            })
        }),
    )
    .style(|s| s.gap(5))
}

fn selected_view(
    capture: &Rc<Capture>,
    selected: RwSignal<Option<ViewId>>,
) -> impl IntoView + use<> {
    let capture = capture.clone();

    let dyn_view_builder = move |selected_value: Option<ViewId>| {
        if let Some(view) = selected_value.and_then(|id| capture.root.find(id)) {
            let name = info("View Debug", view.name.clone());
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
                    view.world_bounds.x0,
                    beyond(view.world_bounds.x0, capture.window_size.width)
                ),
            );
            let y = info(
                "Y",
                format!(
                    "{}{}",
                    view.world_bounds.y0,
                    beyond(view.world_bounds.y0, capture.window_size.height)
                ),
            );
            let w = info(
                "Width",
                format!(
                    "{}{}",
                    view.world_bounds.width(),
                    beyond(view.world_bounds.x1, capture.window_size.width)
                ),
            );
            let h = info(
                "Height",
                format!(
                    "{}{}",
                    view.world_bounds.height(),
                    beyond(view.world_bounds.y1, capture.window_size.height)
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

            let style_header = header("View Style");

            let style = capture
                .state
                .computed_styles
                .get(&view.id)
                .cloned()
                .unwrap_or_default();

            let style_list = style
                .debug_view(Some(&view.direct_style))
                .style(|s| s.height_full().flex_grow(1.))
                .scroll()
                .style(|s| {
                    s.set(OverflowX, taffy::Overflow::Scroll)
                        .set(OverflowY, taffy::Overflow::Visible)
                        .height_full()
                        .flex_grow(1.)
                });

            let selected_view_info = Stack::vertical_from_iter(
                [name, id, count, x, y, w, h, tx, ty, tw, th]
                    .into_iter()
                    .enumerate()
                    .map(|(idx, v)| {
                        v.style(move |s| {
                            s.padding(3).with_theme(move |s, t| {
                                s.apply_if(idx.is_multiple_of(2), |s| s.background(t.bg_base()))
                                    .apply_if(!idx.is_multiple_of(2), |s| {
                                        s.background(t.bg_elevated())
                                    })
                            })
                        })
                    }),
            )
            .style(|s| s.height_full().flex_grow(1.))
            .scroll()
            .style(|s| {
                s.set(OverflowX, taffy::Overflow::Scroll)
                    .set(OverflowY, taffy::Overflow::Visible)
                    .height_full()
                    .flex_grow(1.)
            });

            Stack::vertical((header("Selected View"), selected_view_info, style_header, style_list))
            .style(|s| s.width_full().flex_shrink(0.).gap(10))
            .into_any()
        } else {
            Label::new("No selection")
                .style(|s| s.padding(5.0))
                .into_any()
        }
    };

    dyn_container(move || selected.get(), dyn_view_builder)
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
            if ids.is_empty() { None } else { Some(ids) }
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
    Effect::batch(|| {
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
