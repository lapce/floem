mod data;
pub(crate) mod profiler;
mod view;
use floem_reactive::{Effect, Scope};
use floem_renderer::text::FontWeight;
use peniko::kurbo::{Rect, Size};
use peniko::{
    Brush, Color,
    color::{HueDirection, Oklab, palette::css},
};
use slotmap::Key as _;
pub use view::capture;

use crate::{
    AnyView, Clipboard, ElementId, ViewId, WindowState,
    event::EventPropagation,
    inspector::data::CapturedDatas,
    platform::{Duration, Instant},
    prelude::*,
    style::{
        Background, BorderRadius, OverflowX, OverflowY, PxPct, PxPctAuto, StrokeWrap, Style,
        StyleCx, StyleThemeExt,
    },
};

use std::{cell::Cell, collections::HashMap, fmt::Display, rc::Rc};

use taffy::{
    prelude::{Layout, auto, fr},
    style::FlexDirection,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum BoxModelRegion {
    Position,
    Margin,
    Border,
    Padding,
    Content,
}

#[derive(Clone)]
struct BoxModelViewData {
    position: [PxPctAuto; 4],
    margin: [PxPctAuto; 4],
    border: [StrokeWrap; 4],
    border_radius: BorderRadius,
    padding: [PxPct; 4],
    content_width: f64,
    content_height: f64,
}

#[derive(Clone, Copy)]
struct BoxModelRegionIds {
    position: ElementId,
    margin: ElementId,
    border: ElementId,
    padding: ElementId,
    content: ElementId,
}

fn format_float(value: f64) -> String {
    if value.fract().abs() < 0.01 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.1}")
    }
}

fn format_px_pct(value: PxPct) -> String {
    match value {
        PxPct::Px(px) if px.abs() < 0.01 => "-".to_string(),
        PxPct::Pct(pct) if pct.abs() < 0.01 => "-".to_string(),
        PxPct::Px(px) => format!("{}px", format_float(px)),
        PxPct::Pct(pct) => format!("{}%", format_float(pct)),
    }
}

fn format_px_pct_auto(value: PxPctAuto) -> String {
    match value {
        PxPctAuto::Px(px) if px.abs() < 0.01 => "-".to_string(),
        PxPctAuto::Pct(pct) if pct.abs() < 0.01 => "-".to_string(),
        PxPctAuto::Px(px) => format!("{}px", format_float(px)),
        PxPctAuto::Pct(pct) => format!("{}%", format_float(pct)),
        PxPctAuto::Auto => "-".to_string(),
    }
}

fn resolve_px_pct(value: PxPct, basis: f64) -> f64 {
    match value {
        PxPct::Px(px) => px,
        PxPct::Pct(pct) => basis * (pct / 100.0),
    }
}

fn box_model_data(style: &Style, bounds: Rect) -> BoxModelViewData {
    let builtin = style.builtin();
    let border = [
        builtin.border_top(),
        builtin.border_right(),
        builtin.border_bottom(),
        builtin.border_left(),
    ];
    let padding = [
        builtin.padding_top(),
        builtin.padding_right(),
        builtin.padding_bottom(),
        builtin.padding_left(),
    ];
    let margin = [
        builtin.margin_top(),
        builtin.margin_right(),
        builtin.margin_bottom(),
        builtin.margin_left(),
    ];
    let position = [
        builtin.inset_top(),
        builtin.inset_right(),
        builtin.inset_bottom(),
        builtin.inset_left(),
    ];
    let border_radius = BorderRadius {
        top_left: Some(builtin.border_top_left_radius()),
        top_right: Some(builtin.border_top_right_radius()),
        bottom_left: Some(builtin.border_bottom_left_radius()),
        bottom_right: Some(builtin.border_bottom_right_radius()),
    };

    let horizontal_basis = bounds.width().max(0.0);
    let content_width = (bounds.width()
        - border[1].0.width
        - border[3].0.width
        - resolve_px_pct(padding[1], horizontal_basis)
        - resolve_px_pct(padding[3], horizontal_basis))
    .max(0.0);
    let content_height = (bounds.height()
        - border[0].0.width
        - border[2].0.width
        - resolve_px_pct(padding[0], horizontal_basis)
        - resolve_px_pct(padding[2], horizontal_basis))
    .max(0.0);

    BoxModelViewData {
        position,
        margin,
        border,
        border_radius,
        padding,
        content_width,
        content_height,
    }
}

fn format_border_radius(value: Option<PxPct>) -> String {
    value.map(format_px_pct).unwrap_or_else(|| "-".to_string())
}

fn format_border_width(value: f64) -> String {
    if value.abs() < 0.01 {
        "-".to_string()
    } else {
        format!("{}px", format_float(value))
    }
}

fn dashed_stroke(value: StrokeWrap) -> StrokeWrap {
    let mut stroke = value.0;
    if stroke.width > 0.0 && stroke.dash_pattern.is_empty() {
        stroke.dash_pattern = smallvec::smallvec![4.0, 3.0];
    }
    StrokeWrap(stroke)
}

fn blend_box_model_color(background: Color, tint: Color, highlighted: bool) -> Color {
    let background_lightness = background.convert::<Oklab>().components[0];
    let blend = if highlighted {
        if background_lightness < 0.5 {
            0.3
        } else {
            0.22
        }
    } else if background_lightness < 0.5 {
        0.16
    } else {
        0.12
    };
    background.lerp(tint, blend, HueDirection::default())
}

fn hovered_box_model_region(
    hit_path: Option<&[ElementId]>,
    ids: BoxModelRegionIds,
) -> Option<BoxModelRegion> {
    let hit_path = hit_path?;
    if hit_path.contains(&ids.content) {
        Some(BoxModelRegion::Content)
    } else if hit_path.contains(&ids.padding) {
        Some(BoxModelRegion::Padding)
    } else if hit_path.contains(&ids.border) {
        Some(BoxModelRegion::Border)
    } else if hit_path.contains(&ids.margin) {
        Some(BoxModelRegion::Margin)
    } else if hit_path.contains(&ids.position) {
        Some(BoxModelRegion::Position)
    } else {
        None
    }
}

fn box_model_label(
    text: String,
    region: BoxModelRegion,
    hovered: RwSignal<Option<BoxModelRegion>>,
) -> impl View {
    Label::new(text).style(move |s| {
        s.font_size(11.0)
            .font_weight(if hovered.get() == Some(region) {
                FontWeight::SEMI_BOLD
            } else {
                FontWeight::NORMAL
            })
            .with_context_opt::<Background, _>(move |s, b| {
                let color = if let Brush::Solid(c) = b {
                    let l = c.convert::<Oklab>().components[0];
                    if l < 0.5 {
                        Some(css::WHITE)
                    } else {
                        Some(css::BLACK)
                    }
                } else {
                    None
                };
                s.apply_opt(color, |s, c| s.color(c))
            })
    })
}

fn box_model_layer(
    region: BoxModelRegion,
    color: Option<Color>,
    hovered: RwSignal<Option<BoxModelRegion>>,
    border: Option<[StrokeWrap; 4]>,
    values: [String; 4],
    child: AnyView,
) -> AnyView {
    let region_name = match region {
        BoxModelRegion::Position => "position",
        BoxModelRegion::Margin => "margin",
        BoxModelRegion::Border => "border",
        BoxModelRegion::Padding => "padding",
        BoxModelRegion::Content => "content",
    };
    let side_gap = if region == BoxModelRegion::Position {
        12.0
    } else {
        10.0
    };
    let top = box_model_label(values[0].clone(), region, hovered)
        .container()
        .style(|s| s.justify_center());
    let left = box_model_label(values[3].clone(), region, hovered)
        .container()
        .style(|s| s.justify_end().padding_right(2.0));
    let right = box_model_label(values[1].clone(), region, hovered)
        .container()
        .style(|s| s.justify_start().padding_left(2.0));
    let bottom = box_model_label(values[2].clone(), region, hovered)
        .container()
        .style(|s| s.justify_center());
    Stack::vertical((
        Stack::new((
            box_model_label(region_name.to_string(), region, hovered)
                .container()
                .style(|s| s.justify_start()),
            top,
            (),
        ))
        .style(|s| {
            s.grid()
                .grid_template_columns([fr(1.), auto(), fr(1.)])
                .width_full()
        }),
        Stack::new((left, child, right)).style(move |s| {
            s.grid()
                .grid_template_columns([fr(1.), auto(), fr(1.)])
                .items_center()
                .col_gap(side_gap)
        }),
        bottom,
    ))
    .style(move |s| {
        let highlighted = hovered.get() == Some(region);
        let any_hovered = hovered.get().is_some();
        let layer_padding = if region == BoxModelRegion::Position {
            14.0
        } else {
            12.0
        };
        let s =
            s.items_center()
                .gap(10.0)
                .padding(layer_padding)
                .apply_if(color.is_some(), move |s| {
                    let fill_color = color.unwrap();
                    s.with_theme(move |s, t| {
                        s.background(if highlighted {
                            blend_box_model_color(fill_color, t.bg_base(), true)
                        } else if any_hovered {
                            t.bg_base()
                        } else {
                            blend_box_model_color(fill_color, t.bg_base(), false)
                        })
                    })
                });
        if let Some(border) = border.as_ref() {
            s.border_top(border[0].clone())
                .border_right(border[1].clone())
                .border_bottom(border[2].clone())
                .border_left(border[3].clone())
                .border_color(if highlighted {
                    Color::BLACK.with_alpha(0.55)
                } else {
                    Color::WHITE.with_alpha(0.45)
                })
        } else {
            s.border(1.)
                .border_color(Color::WHITE.with_alpha(0.45))
                .apply_if(highlighted, |s| {
                    s.border_color(Color::BLACK.with_alpha(0.55))
                })
        }
    })
    .into_any()
}

fn box_model_view(data: BoxModelViewData) -> impl View {
    let hovered = RwSignal::new(None);
    let content_radius = data.border_radius;
    let content = Label::new(format!(
        "{} x {}",
        format_float(data.content_width),
        format_float(data.content_height)
    ))
    .style(move |s| {
        s.font_size(12.0)
            .font_weight(if hovered.get() == Some(BoxModelRegion::Content) {
                FontWeight::SEMI_BOLD
            } else {
                FontWeight::NORMAL
            })
            .with_context_opt::<Background, _>(move |s, b| {
                let color = if let Brush::Solid(c) = b {
                    let l = c.convert::<Oklab>().components[0];
                    if l < 0.5 {
                        Some(css::WHITE)
                    } else {
                        Some(css::BLACK)
                    }
                } else {
                    None
                };
                s.apply_opt(color, |s, c| s.color(c))
            })
    })
    .container()
    .style(move |s| {
        let highlighted = hovered.get() == Some(BoxModelRegion::Content);
        let any_hovered = hovered.get().is_some();
        s.with_theme(move |s, t| {
            s.items_center()
                .justify_center()
                .gap(4.0)
                .padding_vert(6.)
                .padding_horiz(12.0)
                .apply_border_radius(content_radius)
                .border(1.)
                .border_color(Color::WHITE.with_alpha(0.45))
                .apply_if(highlighted, |s| {
                    s.border_color(Color::BLACK.with_alpha(0.55))
                })
                .background(if highlighted {
                    blend_box_model_color(Color::from_rgb8(111, 168, 220), t.bg_base(), true)
                } else if any_hovered {
                    t.bg_base()
                } else {
                    blend_box_model_color(Color::from_rgb8(111, 168, 220), t.bg_base(), false)
                })
        })
    })
    .into_any();
    let content_id = content.view_id().get_element_id();

    let padding = box_model_layer(
        BoxModelRegion::Padding,
        Some(Color::from_rgba8(183, 195, 125, 180)),
        hovered,
        None,
        data.padding.map(format_px_pct),
        content,
    );
    let padding_id = padding.view_id().get_element_id();
    let dashed_border = data.border.clone().map(dashed_stroke);
    let border_values = data.border.map(|value| format_border_width(value.0.width));
    let border = box_model_layer(
        BoxModelRegion::Border,
        Some(Color::from_rgba8(255, 229, 153, 190)),
        hovered,
        Some(dashed_border),
        border_values,
        padding,
    );
    let border_id = border.view_id().get_element_id();
    let margin = box_model_layer(
        BoxModelRegion::Margin,
        Some(Color::from_rgba8(246, 178, 107, 170)),
        hovered,
        None,
        data.margin.map(format_px_pct_auto),
        border,
    );
    let margin_id = margin.view_id().get_element_id();
    let position = box_model_layer(
        BoxModelRegion::Position,
        None,
        hovered,
        None,
        data.position.map(format_px_pct_auto),
        margin,
    );
    let position_id = position.view_id().get_element_id();
    let region_ids = BoxModelRegionIds {
        position: position_id,
        margin: margin_id,
        border: border_id,
        padding: padding_id,
        content: content_id,
    };

    Stack::vertical((
        Label::new("Box Model").style(|s| {
            s.font_bold()
                .with_theme(|s, t| s.color(t.primary()))
                .padding_bottom(6.0)
        }),
        Stack::vertical((
            info_row(
                "Radius".to_string(),
                format!(
                    "{} {} {} {}",
                    format_border_radius(data.border_radius.top_left),
                    format_border_radius(data.border_radius.top_right),
                    format_border_radius(data.border_radius.bottom_right),
                    format_border_radius(data.border_radius.bottom_left),
                )
                .style(|s| s.font_size(11.0).font_bold()),
            )
            .style(|s| s.padding_bottom(8.0)),
            position,
        )),
    ))
    .style(|s| {
        s.padding(10.0).border_radius(8.0).with_theme(|s, t| {
            s.background(t.bg_base())
                .border(1.)
                .border_color(t.border())
        })
    })
    .on_event_cont(crate::event::listener::PointerMove, move |cx, _| {
        hovered.set(hovered_box_model_region(cx.hit_path.as_deref(), region_ids));
    })
    .on_event_cont(crate::event::listener::PointerLeave, move |_, _| {
        hovered.set(None);
    })
}

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
            .style(|s| s.flex_grow(1.).height_full().gap(2.0));

            let selected_view_summary = Stack::horizontal((
                selected_view_info,
                box_model_view(box_model_data(&style, view.world_bounds)),
            ))
            .style(|s| {
                s.items_start()
                    .gap(16.0)
                    .flex_grow(1.)
                    .justify_between()
                    .padding_right(15)
            })
            .scroll()
            .style(|s| {
                s.set(OverflowX, taffy::Overflow::Scroll)
                    .set(OverflowY, taffy::Overflow::Visible)
                    .width_full()
            });

            let selected_view_panel =
                Stack::vertical((header("Selected View"), selected_view_summary))
                    .style(|s| s.width_full().min_size(0., 0.).flex_grow(1.));
            let style_panel = Stack::vertical((header("View Style"), style_list))
                .style(|s| s.width_full().min_size(0., 0.).flex_grow(1.));

            Stack::vertical((selected_view_panel, style_panel))
                .style(|s| s.width_full().flex_shrink(0.).gap(10).min_size(0., 0.))
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
