mod data;
mod view;

use crate::app_state::AppState;
use crate::context::StyleCx;
use crate::event::{Event, EventListener, EventPropagation};
use crate::id::ViewId;
use crate::prelude::ViewTuple;
use crate::style::{
    FontSize, OverflowX, OverflowY, Style, StyleClass, StyleClassRef, StyleKeyInfo, StylePropRef,
    Transition,
};
use crate::theme::StyleThemeExt as _;
use crate::view::{IntoView, View};
use crate::view_state::ChangeFlags;
use crate::views::{
    button, dyn_container, empty, stack, static_label, text, v_stack, v_stack_from_iter,
    ContainerExt, Decorators, Label, ScrollExt,
};
use crate::{keyboard, style, style_class, AnyView, Clipboard};
use floem_reactive::{batch, RwSignal, Scope, SignalGet, SignalUpdate};
use peniko::color::palette;
use peniko::kurbo::{Point, Rect, Size};
use peniko::Color;
use slotmap::Key;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::rc::Rc;
use taffy::AlignItems;
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
        .on_event_stop(EventListener::KeyDown, {
            let capture = capture.clone();
            move |event| {
                if let Event::KeyDown(key) = event {
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

fn info_row(name: String, view: impl View + 'static) -> impl View {
    let name = name
        .style(|s| {
            s.margin_right(5.0)
                .with_theme(|s, t| s.color(t.text_muted()))
        })
        .container()
        .style(|s| s.min_width(150.0).flex_direction(FlexDirection::RowReverse));
    (name, view).h_stack()
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
    v_stack_from_iter(
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

fn selected_view(capture: &Rc<Capture>, selected: RwSignal<Option<ViewId>>) -> impl IntoView {
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

            let style_header = header("View Style");
            let class_header = header("Classes");

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

            let style_list = v_stack_from_iter(style_list.into_iter().enumerate().map(
                |(idx, ((prop, name), value))| {
                    let name = name.strip_prefix("floem::style::").unwrap_or(&name);
                    let name = if direct.contains(&prop.key) {
                        text(name).into_any()
                    } else {
                        stack((
                            "Inherited".style(|s| {
                                s.margin_right(5.0)
                                    .border(1.)
                                    .border_radius(5.0)
                                    .border_color(palette::css::WHITE_SMOKE)
                                    .with_context_opt::<FontSize, _>(|s, fs| s.font_size(fs * 0.8))
                                    .with_theme(|s, t| {
                                        s.color(t.text_muted()).padding(t.padding() / 2.)
                                    })
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
                            "Transition".style(|s| {
                                s.margin_top(5.0)
                                    .margin_right(5.0)
                                    .border(1.)
                                    .border_radius(5.0)
                                    .padding(4.0)
                                    .with_theme(|s, t| {
                                        s.color(t.text_muted()).border_color(t.border())
                                    })
                                    .with_context_opt::<FontSize, _>(|s, fs| s.font_size(fs * 0.8))
                            }),
                            transition.debug_view(),
                        ))
                        .style(|s| s.items_center());
                        v = v_stack((v, transition)).into_any();
                    }
                    stack((
                        name.style(|s| {
                            s.margin_right(5.0)
                                .with_theme(|s, t| s.color(t.text_muted()))
                        })
                        .container()
                        .style(|s| s.min_width(150.0).flex_direction(FlexDirection::RowReverse)),
                        v,
                    ))
                    .style(move |s| {
                        s.padding(5.0)
                            .items_center()
                            .width_full()
                            .with_theme(move |s, t| {
                                s.apply_if(idx.is_multiple_of(2), |s| s.background(t.bg_base()))
                                    .apply_if(!idx.is_multiple_of(2), |s| {
                                        s.background(t.bg_elevated())
                                    })
                            })
                    })
                },
            ))
            .style(|s| s.height_full().flex_grow(1.))
            .scroll()
            .style(|s| {
                s.set(OverflowX, taffy::Overflow::Scroll)
                    .set(OverflowY, taffy::Overflow::Visible)
                    .height_full()
                    .padding_bottom(10)
                    .padding_right(10)
            });

            let selected_view_info = v_stack_from_iter(
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
                    .padding_bottom(10)
                    .padding_right(10)
            });

            let class_list_view =
                v_stack_from_iter(view.classes.clone().into_iter().enumerate().map(
                    |(idx, class_ref)| {
                        let class_style = capture.state.styles.get(&view.id).map(|style| {
                            Style::new().apply_classes_from_context(&[class_ref], style)
                        });

                        let class_name = format!("{:?}", class_ref.key);

                        let class_header = text(&class_name)
                            .style(|s| s.font_bold().with_theme(|s, t| s.color(t.text())));

                        if let Some(class_style) = class_style {
                            let mut props: Vec<_> = class_style
                                .map
                                .clone()
                                .into_iter()
                                .filter_map(|(k, v)| match k.info {
                                    StyleKeyInfo::Prop(..) => Some((StylePropRef { key: k }, v)),
                                    _ => None,
                                })
                                .map(|(p, v)| ((p, format!("{:?}", p.key)), v))
                                .collect();

                            let mut selectors: Vec<_> = class_style
                                .map
                                .clone()
                                .into_iter()
                                .filter_map(|(k, v)| match k.info {
                                    StyleKeyInfo::Selector(sel) => {
                                        Some((sel, v.downcast_ref::<Style>()?.clone()))
                                    }
                                    _ => None,
                                })
                                .collect();

                            props.sort_unstable_by(|a, b| a.0 .1.cmp(&b.0 .1));
                            selectors.sort_unstable_by(|a, b| {
                                a.0.debug_string().cmp(&b.0.debug_string())
                            });

                            let prop_count = props.len();
                            let selector_count = selectors.len();

                            let total_text = if prop_count > 0 && selector_count > 0 {
                                format!("{} properties, {} selectors", prop_count, selector_count)
                            } else if prop_count > 0 {
                                format!(
                                    "{} {}",
                                    prop_count,
                                    if prop_count == 1 {
                                        "property"
                                    } else {
                                        "properties"
                                    }
                                )
                            } else if selector_count > 0 {
                                format!(
                                    "{} {}",
                                    selector_count,
                                    if selector_count == 1 {
                                        "selector"
                                    } else {
                                        "selectors"
                                    }
                                )
                            } else {
                                "empty".to_string()
                            };

                            let count_badge = text(total_text).style(|s| {
                                s.padding(2.0)
                                    .padding_horiz(6.0)
                                    .border(1.)
                                    .border_radius(10.0)
                                    .margin_left(8.0)
                                    .with_theme(|s, t| {
                                        s.color(t.text_muted()).border_color(t.border())
                                    })
                                    .with_context_opt::<FontSize, _>(|s, fs| s.font_size(fs * 0.75))
                            });

                            let header_row =
                                stack((class_header, count_badge)).style(|s| s.items_center());

                            let props_view = if !props.is_empty() {
                                Some(
                                    v_stack_from_iter(props.into_iter().map(
                                        |((prop, name), value)| {
                                            let name = name
                                                .strip_prefix("floem::style::")
                                                .unwrap_or(&name);
                                            let mut v = (prop.info().debug_view)(&*value)
                                                .unwrap_or_else(|| {
                                                    static_label((prop.info().debug_any)(&*value))
                                                        .into_any()
                                                });

                                            if let Some(transition) = class_style
                                                .map
                                                .get(&prop.info().transition_key)
                                                .and_then(|v| v.downcast_ref::<Transition>())
                                            {
                                                let transition_badge = stack((
                                                    "Transition".style(|s| {
                                                        s.margin_top(5.0)
                                                            .margin_right(5.0)
                                                            .border(1.)
                                                            .border_radius(5.0)
                                                            .padding(4.0)
                                                            .with_theme(|s, t| {
                                                                s.color(t.text_muted())
                                                                    .border_color(t.border())
                                                            })
                                                            .with_context_opt::<FontSize, _>(
                                                                |s, fs| s.font_size(fs * 0.8),
                                                            )
                                                    }),
                                                    format!("{:?}", transition),
                                                ))
                                                .style(|s| s.items_center());
                                                v = v_stack((v, transition_badge)).into_any();
                                            }

                                            stack((
                                                text(name)
                                                    .style(|s| {
                                                        s.margin_right(5.0).with_theme(|s, t| {
                                                            s.color(t.text_muted())
                                                        })
                                                    })
                                                    .container()
                                                    .style(|s| {
                                                        s.min_width(120.0).flex_direction(
                                                            FlexDirection::RowReverse,
                                                        )
                                                    }),
                                                v,
                                            ))
                                            .style(
                                                |s| {
                                                    s.padding(4.0)
                                                        .padding_left(20.0)
                                                        .items_center()
                                                        .width_full()
                                                },
                                            )
                                        },
                                    ))
                                    .style(|s| s.width_full()),
                                )
                            } else {
                                None
                            };

                            let selectors_view = if !selectors.is_empty() {
                                Some(
                                    v_stack_from_iter(selectors.into_iter().map(
                                        |(selector_info, selector_style)| {
                                            let selector_name = selector_info.debug_string();

                                            let mut nested_props: Vec<_> = selector_style
                                                .map
                                                .clone()
                                                .into_iter()
                                                .filter_map(|(k, v)| match k.info {
                                                    StyleKeyInfo::Prop(..) => {
                                                        Some((StylePropRef { key: k }, v))
                                                    }
                                                    _ => None,
                                                })
                                                .map(|(p, v)| ((p, format!("{:?}", p.key)), v))
                                                .collect();

                                            nested_props
                                                .sort_unstable_by(|a, b| a.0 .1.cmp(&b.0 .1));

                                            let selector_header = text(selector_name).style(|s| {
                                                s.font_bold()
                                                    .with_theme(|s, t| s.color(t.text()))
                                                    .with_context_opt::<FontSize, _>(|s, fs| {
                                                    s.font_size(fs * 0.9)
                                                })
                                            });

                                            let nested_count = text(format!(
                                                "{} {}",
                                                nested_props.len(),
                                                if nested_props.len() == 1 {
                                                    "property"
                                                } else {
                                                    "properties"
                                                }
                                            ))
                                            .style(|s| {
                                                s.padding(1.0)
                                                    .padding_horiz(4.0)
                                                    .border(1.)
                                                    .border_radius(8.0)
                                                    .margin_left(6.0)
                                                    .with_theme(|s, t| {
                                                        s.color(t.text_muted())
                                                            .border_color(t.border())
                                                    })
                                                    .with_context_opt::<FontSize, _>(|s, fs| {
                                                        s.font_size(fs * 0.7)
                                                    })
                                            });

                                            let nested_header =
                                                stack((selector_header, nested_count)).style(|s| {
                                                    s.items_center()
                                                        .padding_left(20.0)
                                                        .padding_top(6.0)
                                                });

                                            let nested_props_view =
                                                v_stack_from_iter(nested_props.into_iter().map(
                                                    |((prop, name), value)| {
                                                        let name = name
                                                            .strip_prefix("floem::style::")
                                                            .unwrap_or(&name);
                                                        let v = (prop.info().debug_view)(&*value)
                                                            .unwrap_or_else(|| {
                                                                static_label((prop
                                                                    .info()
                                                                    .debug_any)(
                                                                    &*value
                                                                ))
                                                                .into_any()
                                                            });

                                                        stack((
                                                            text(name)
                                                                .style(|s| {
                                                                    s.margin_right(5.0).with_theme(
                                                                        |s, t| {
                                                                            s.color(t.text_muted())
                                                                        },
                                                                    )
                                                                })
                                                                .container()
                                                                .style(|s| {
                                                                    s.min_width(120.0)
                                                                        .flex_direction(
                                                                        FlexDirection::RowReverse,
                                                                    )
                                                                }),
                                                            v,
                                                        ))
                                                        .style(|s| {
                                                            s.padding(4.0)
                                                                .padding_left(40.0)
                                                                .items_center()
                                                                .width_full()
                                                        })
                                                    },
                                                ))
                                                .style(|s| s.width_full());

                                            v_stack((nested_header, nested_props_view))
                                                .style(|s| s.width_full())
                                        },
                                    ))
                                    .style(|s| s.width_full().gap(4)),
                                )
                            } else {
                                None
                            };

                            let content = match (props_view, selectors_view) {
                                (Some(props), Some(selectors)) => v_stack((props, selectors))
                                    .style(|s| s.width_full().gap(8))
                                    .into_any(),
                                (Some(props), None) => props.into_any(),
                                (None, Some(selectors)) => selectors.into_any(),
                                (None, None) => empty().into_any(),
                            };

                            v_stack((header_row, content)).style(move |s| {
                                s.padding(8.0).width_full().border_radius(5.0).with_theme(
                                    move |s, t| {
                                        s.apply_if(idx.is_multiple_of(2), |s| {
                                            s.background(t.bg_base())
                                        })
                                        .apply_if(!idx.is_multiple_of(2), |s| {
                                            s.background(t.bg_elevated())
                                        })
                                    },
                                )
                            })
                        } else {
                            stack((
                                class_header,
                                text("(no properties)").style(|s| {
                                    s.margin_left(8.0)
                                        .with_theme(|s, t| s.color(t.text_muted()))
                                        .with_context_opt::<FontSize, _>(|s, fs| {
                                            s.font_size(fs * 0.85)
                                        })
                                }),
                            ))
                            .style(move |s| {
                                s.padding(8.0)
                                    .width_full()
                                    .items_center()
                                    .border_radius(5.0)
                                    .with_theme(move |s, t| {
                                        s.apply_if(idx.is_multiple_of(2), |s| {
                                            s.background(t.bg_base())
                                        })
                                        .apply_if(!idx.is_multiple_of(2), |s| {
                                            s.background(t.bg_elevated())
                                        })
                                    })
                            })
                        }
                    },
                ))
                .style(|s| s.gap(4).width_full());

            v_stack((
                selected_view_info,
                style_header,
                style_list,
                class_header,
                class_list_view,
            ))
            .style(|s| s.width_full())
            .into_any()
        } else {
            text("No selection").style(|s| s.padding(5.0)).into_any()
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
