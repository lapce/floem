use super::{TimingKind, TimingReport};
use crate::app::{AppUpdateEvent, add_app_update_event};
use crate::context::{LayoutChanged, LayoutChangedListener, PaintCx, StyleCx};
use crate::event::{Event, EventPropagation, PointerScrollEventExt, listener};
use crate::prelude::EventListenerTrait;
use crate::prelude::palette::css;
use crate::style::theme::Theme;
use crate::text::{Attrs, AttrsList, TextLayout};
use crate::theme::StyleThemeExt;
use crate::ui_events::pointer::{PointerButton, PointerEvent, PointerGesture};
use crate::view::{IntoView, View, ViewId};
use crate::views::resizable::Resizable;
use crate::views::{
    Button, ContainerExt, Decorators, Label, ListItemClass, Scroll, ScrollExt, Stack,
    dyn_container, list,
};
use crate::{ElementId, box_tree::ElementMeta};
use floem_reactive::{Effect, Memo, RwSignal, Scope, SignalGet, SignalUpdate};
use peniko::kurbo::{Affine, Line, Point, Rect, Size, Stroke};
use peniko::{Brush, Color};
use std::collections::HashMap;
use std::fmt::Display;
use std::mem;
use std::rc::Rc;
use taffy::style::FlexDirection;
use understory_box_tree::NodeFlags;
use understory_view2d::Viewport1D;
use winit::window::WindowId;

use crate::platform::{Duration, Instant};
use peniko::color::HueDirection;

#[derive(Clone)]
pub struct ProfileEvent {
    pub start: Instant,
    pub end: Instant,
    pub name: String,
    pub depth: usize,
}

#[derive(Default)]
pub struct ProfileFrame {
    pub events: Vec<ProfileEvent>,
    pub timing: Option<TimingReport>,
}

#[derive(Default)]
pub struct Profile {
    pub current: ProfileFrame,
    timeline_origin: Option<Instant>,
    events: Vec<ProfileEvent>,
    frames: Vec<ProfileFrameSummary>,
}

impl Profile {
    pub fn next_frame(&mut self) {
        let frame = mem::take(&mut self.current);
        if frame.events.is_empty() && frame.timing.is_none() {
            return;
        }

        let frame_start = frame
            .events
            .iter()
            .map(|event| event.start)
            .chain(frame.timing.iter().filter_map(|timing| timing.anchor))
            .min();
        let Some(frame_start) = frame_start else {
            return;
        };
        let frame_end = frame
            .events
            .iter()
            .map(|event| event.end)
            .chain(
                frame
                    .timing
                    .iter()
                    .filter_map(|timing| timing.anchor.map(|anchor| anchor + timing.total)),
            )
            .max()
            .unwrap_or(frame_start);
        match self.timeline_origin {
            Some(origin) if frame_start < origin => self.timeline_origin = Some(frame_start),
            None => self.timeline_origin = Some(frame_start),
            _ => {}
        }
        let event_sum = frame
            .events
            .iter()
            .map(|event| event.end.saturating_duration_since(event.start))
            .sum();
        let event_count = frame.events.len();
        self.events.extend(frame.events);
        self.frames.push(ProfileFrameSummary {
            start: Some(frame_start),
            duration: frame_end.saturating_duration_since(frame_start),
            sum: event_sum,
            event_count,
            timing: frame.timing,
        });
    }
}

struct ProfileFrameData {
    index: usize,
    start: Duration,
    duration: Duration,
    sum: Duration,
    event_count: usize,
}

#[derive(Default)]
struct ProfileFrameSummary {
    start: Option<Instant>,
    duration: Duration,
    sum: Duration,
    event_count: usize,
    timing: Option<TimingReport>,
}

#[derive(Clone)]
struct TimelineFrameMarker {
    index: usize,
    start: Duration,
}

#[derive(Clone)]
struct TimelineInstantMarker {
    start: Duration,
    color: Color,
}

#[derive(Clone)]
enum TimelineItemKind {
    Event,
    Timing(TimingKind),
}

#[derive(Clone)]
struct TimelineItem {
    label: String,
    source: &'static str,
    start: Duration,
    duration: Duration,
    depth: usize,
    kind: TimelineItemKind,
}

fn timeline_item_color(item: &TimelineItem) -> Color {
    match &item.kind {
        TimelineItemKind::Event if item.label == "VSync" => Color::from_rgb8(153, 97, 191),
        TimelineItemKind::Event if item.label == "FramePresented" => Color::from_rgb8(48, 166, 127),
        TimelineItemKind::Event => Color::from_rgb8(54, 111, 196),
        TimelineItemKind::Timing(TimingKind::Total) => Color::from_rgb8(41, 78, 163),
        TimelineItemKind::Timing(TimingKind::Style) => Color::from_rgb8(29, 142, 120),
        TimelineItemKind::Timing(TimingKind::Layout) => Color::from_rgb8(201, 138, 33),
        TimelineItemKind::Timing(TimingKind::BoxTree) => Color::from_rgb8(188, 130, 36),
        TimelineItemKind::Timing(TimingKind::Paint) => Color::from_rgb8(203, 92, 73),
        TimelineItemKind::Timing(TimingKind::Present) => Color::from_rgb8(64, 157, 163),
        TimelineItemKind::Timing(TimingKind::Renderer) => Color::from_rgb8(62, 126, 214),
    }
}

fn instant_marker_color(label: &str) -> Color {
    match label {
        "VSync" => Color::from_rgb8(153, 97, 191),
        "FramePresented" => Color::from_rgb8(48, 166, 127),
        _ => Color::from_rgb8(54, 111, 196),
    }
}

fn profiler_panel(view: impl IntoView + 'static) -> impl IntoView {
    view.into_view().style(|s| {
        s.border(1.0).border_radius(22.0).with_theme(|s, t| {
            s.background(t.bg_elevated())
                .border_color(t.border_muted())
                .color(t.text())
        })
    })
}

fn profiler_chip(
    label: impl Into<String>,
    value: impl Into<String>,
    accent: Color,
) -> impl IntoView {
    let label = label.into();
    let value = value.into();
    Stack::vertical((
        Label::new(label).style(|s| s.font_size(10.5).with_theme(|s, t| s.color(t.text_muted()))),
        Label::new(value).style(move |s| {
            s.font_size(13.0)
                .font_bold()
                .color(accent)
                .text_ellipsis()
                .min_width(0.0)
        }),
    ))
    .style(move |s| {
        s.gap(4.0)
            .padding_horiz(12.0)
            .padding_vert(10.0)
            .border_radius(16.0)
            .border(1.0)
            .background(accent.with_alpha(0.10))
            .border_color(accent.with_alpha(0.18))
            .min_width(0.0)
    })
}

fn transform_max_scale(transform: Affine) -> f64 {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    let s1 = a * a + b * b;
    let s2 = c * c + d * d;
    let trace = s1 + s2;
    let det = a * d - b * c;
    let disc = (trace * trace - 4.0 * det * det).max(0.0);
    ((trace + disc.sqrt()) * 0.5).sqrt()
}

fn build_timeline_lanes(profile: &Profile) -> Vec<Vec<TimelineItem>> {
    let mut items = Vec::new();

    let Some(origin) = profile.timeline_origin else {
        return Vec::new();
    };

    items.extend(profile.events.iter().filter_map(|event| {
        let duration = event.end.saturating_duration_since(event.start);
        (!duration.is_zero()).then(|| TimelineItem {
            label: event.name.to_string(),
            source: "Profiler Event",
            start: event.start.saturating_duration_since(origin),
            duration,
            depth: event.depth,
            kind: TimelineItemKind::Event,
        })
    }));

    for frame in &profile.frames {
        let Some(report) = &frame.timing else {
            continue;
        };
        let Some(anchor) = report.anchor else {
            continue;
        };
        items.extend(
            report
                .flattened_spans()
                .into_iter()
                .map(|span| TimelineItem {
                    label: span.label.to_string(),
                    source: "Timing Span",
                    start: anchor.saturating_duration_since(origin) + span.start,
                    duration: span.duration,
                    depth: span.depth,
                    kind: TimelineItemKind::Timing(span.kind),
                }),
        );
    }

    items.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| b.duration.cmp(&a.duration))
            .then_with(|| a.depth.cmp(&b.depth))
            .then_with(|| a.label.cmp(&b.label))
    });

    let mut lane_ends: Vec<Duration> = Vec::new();
    let mut lanes: Vec<Vec<TimelineItem>> = Vec::new();

    for item in items {
        let item_end = item.start.saturating_add(item.duration);
        if let Some((lane_idx, lane_end)) = lane_ends
            .iter_mut()
            .enumerate()
            .find(|(_, lane_end)| **lane_end <= item.start)
        {
            *lane_end = item_end;
            lanes[lane_idx].push(item);
        } else {
            lane_ends.push(item_end);
            lanes.push(vec![item]);
        }
    }

    lanes
}

fn profile_total_duration(profile: &Profile) -> Duration {
    let Some(origin) = profile.timeline_origin else {
        return Duration::ZERO;
    };

    let event_end = profile
        .events
        .iter()
        .map(|event| event.end.saturating_duration_since(origin))
        .max()
        .unwrap_or_default();

    let frame_end = profile
        .frames
        .iter()
        .filter_map(|frame| {
            frame
                .start
                .map(|start| start.saturating_duration_since(origin) + frame.duration)
        })
        .max()
        .unwrap_or_default();

    let timing_end = profile
        .frames
        .iter()
        .filter_map(|frame| {
            frame.timing.as_ref().and_then(|timing| {
                timing
                    .anchor
                    .map(|anchor| anchor.saturating_duration_since(origin) + timing.total)
            })
        })
        .max()
        .unwrap_or_default();

    event_end.max(frame_end).max(timing_end)
}

const TIMELINE_PADDING: f64 = 8.0;
const TIMELINE_LANE_HEIGHT: f64 = 18.0;
const TIMELINE_LANE_GAP: f64 = 4.0;
const TIMELINE_LABEL_FONT_SIZE: f32 = 11.0;
const OVERVIEW_HEIGHT: f64 = 84.0;
const OVERVIEW_INSET: f64 = 8.0;
const OVERVIEW_BIN_COUNT: usize = 360;

#[derive(Clone, Copy)]
struct TimelinePalette {
    lane_bg: Color,
    border: Color,
    text: Color,
    selected: Color,
}

fn format_duration_short(duration: Duration) -> String {
    let micros = duration.as_secs_f64() * 1_000_000.0;
    if micros >= 1_000.0 {
        format!("{:.2} ms", micros / 1_000.0)
    } else {
        format!("{micros:.0} us")
    }
}

fn frame_for_visible_range(
    frames: &[Rc<ProfileFrameData>],
    visible_range: (f64, f64),
) -> Option<Rc<ProfileFrameData>> {
    let center = (visible_range.0 + visible_range.1) * 0.5;
    let containing = frames
        .iter()
        .find(|frame| {
            let start = frame.start.as_secs_f64();
            let end = start + frame.duration.as_secs_f64();
            center >= start && center <= end
        })
        .cloned();
    containing.or_else(|| {
        frames
            .iter()
            .min_by(|a, b| {
                let a_center = a.start.as_secs_f64() + a.duration.as_secs_f64() * 0.5;
                let b_center = b.start.as_secs_f64() + b.duration.as_secs_f64() * 0.5;
                (a_center - center)
                    .abs()
                    .total_cmp(&(b_center - center).abs())
            })
            .cloned()
    })
}

fn frame_for_cursor_time(
    frames: &[Rc<ProfileFrameData>],
    cursor_time: f64,
) -> Option<Rc<ProfileFrameData>> {
    frames
        .iter()
        .min_by(|a, b| {
            (a.start.as_secs_f64() - cursor_time)
                .abs()
                .total_cmp(&(b.start.as_secs_f64() - cursor_time).abs())
        })
        .cloned()
}

struct TimelineElement {
    element_id: ElementId,
    lane_idx: usize,
    item: TimelineItem,
    label_layout: TextLayout,
}

enum ProfilerTimelineUpdate {
    SelectedFrame(Option<Rc<ProfileFrameData>>),
    ViewportRequest(ProfilerViewportRequest),
}

#[derive(Clone, Copy)]
enum ProfilerViewportRequest {
    Center(f64),
    PanByWorld(f64),
    ZoomAround { anchor_time: f64, factor: f64 },
}

struct ProfilerTimelineView {
    id: ViewId,
    hovered_event: RwSignal<Option<TimelineItem>>,
    cursor_time: RwSignal<Option<f64>>,
    selected_frame: RwSignal<Option<Rc<ProfileFrameData>>>,
    active_frame_index: RwSignal<Option<usize>>,
    visible_range: RwSignal<(f64, f64)>,
    viewport_request: RwSignal<Option<ProfilerViewportRequest>>,
    viewport: Viewport1D,
    viewport_initialized: bool,
    content_origin_x: f64,
    size: Size,
    content_element: ElementId,
    elements: Vec<TimelineElement>,
    element_indices: HashMap<ElementId, usize>,
    hovered_element: Option<ElementId>,
    lane_count: usize,
    frame_markers: Rc<[TimelineFrameMarker]>,
    instant_markers: Rc<[TimelineInstantMarker]>,
    last_selected_frame: Option<usize>,
    palette: TimelinePalette,
}

impl ProfilerTimelineView {
    fn new(
        lanes: Rc<[Vec<TimelineItem>]>,
        frame_markers: Rc<[TimelineFrameMarker]>,
        instant_markers: Rc<[TimelineInstantMarker]>,
        total_duration: Duration,
        hovered_event: RwSignal<Option<TimelineItem>>,
        cursor_time: RwSignal<Option<f64>>,
        selected_frame: RwSignal<Option<Rc<ProfileFrameData>>>,
        active_frame_index: RwSignal<Option<usize>>,
        visible_range: RwSignal<(f64, f64)>,
        viewport_request: RwSignal<Option<ProfilerViewportRequest>>,
    ) -> Self {
        let id = ViewId::new();
        id.register_listener(LayoutChangedListener::listener_key());

        let mut viewport = Viewport1D::new(0.0..1.0);
        viewport.set_world_bounds(Some(0.0..total_duration.as_secs_f64()));
        viewport.set_zoom_limits(1e-6, 1e12);

        let content_element = id.create_child_element_id(0);
        let pending_elements: Vec<_> = lanes
            .iter()
            .enumerate()
            .flat_map(|(lane_idx, lane)| {
                lane.iter().cloned().map(move |item| {
                    let element_id = id.create_child_element_id(1);
                    (lane_idx, element_id, item)
                })
            })
            .collect();
        let mut elements = Vec::new();
        let mut element_indices = HashMap::new();

        {
            let box_tree = id.box_tree();
            let mut bt = box_tree.borrow_mut();
            bt.set_flags(content_element.0, NodeFlags::VISIBLE);
            bt.set_element_meta(content_element.0, Some(ElementMeta::new(content_element)));
            bt.set_local_bounds(
                content_element.0,
                Rect::new(
                    0.0,
                    0.0,
                    total_duration.as_secs_f64().max(1e-6),
                    Self::total_height_for_lane_count(lanes.len()),
                ),
            );

            for (lane_idx, element_id, item) in pending_elements {
                bt.reparent(element_id.0, Some(content_element.0));
                bt.set_flags(element_id.0, NodeFlags::VISIBLE | NodeFlags::PICKABLE);
                bt.set_element_meta(element_id.0, Some(ElementMeta::new(element_id)));
                let x0 = item.start.as_secs_f64();
                let width = item.duration.as_secs_f64().max(1e-6);
                let lane_y =
                    TIMELINE_PADDING + lane_idx as f64 * (TIMELINE_LANE_HEIGHT + TIMELINE_LANE_GAP);
                bt.set_local_bounds(
                    element_id.0,
                    Rect::new(x0, lane_y, x0 + width, lane_y + TIMELINE_LANE_HEIGHT),
                );
                element_indices.insert(element_id, elements.len());
                elements.push(TimelineElement {
                    element_id,
                    lane_idx,
                    item,
                    label_layout: TextLayout::new(),
                });
            }
        }

        let mut this = Self {
            id,
            hovered_event,
            cursor_time,
            selected_frame,
            active_frame_index,
            visible_range,
            viewport_request,
            viewport,
            viewport_initialized: false,
            content_origin_x: 0.0,
            size: Size::ZERO,
            content_element,
            elements,
            element_indices,
            hovered_element: None,
            lane_count: lanes.len(),
            frame_markers,
            instant_markers,
            last_selected_frame: None,
            palette: TimelinePalette {
                lane_bg: css::LIGHT_GRAY.with_alpha(0.3),
                border: css::GRAY,
                text: css::BLACK,
                selected: css::TOMATO,
            },
        };
        let effect_id = this.id;
        let effect_selected_frame = this.selected_frame;
        Effect::new(move |_| {
            effect_id.update_state(ProfilerTimelineUpdate::SelectedFrame(
                effect_selected_frame.get(),
            ));
        });
        let effect_id = this.id;
        let effect_active_frame_index = this.active_frame_index;
        Effect::new(move |_| {
            let _ = effect_active_frame_index.get();
            effect_id.request_paint();
        });
        let effect_id = this.id;
        let effect_viewport_request = this.viewport_request;
        Effect::new(move |_| {
            if let Some(request) = effect_viewport_request.get() {
                effect_id.update_state(ProfilerTimelineUpdate::ViewportRequest(request));
            }
        });
        this.rebuild_label_layouts();
        this
    }

    fn total_height_for_lane_count(lane_count: usize) -> f64 {
        let lane_count = lane_count as f64;
        if lane_count == 0.0 {
            TIMELINE_PADDING * 2.0
        } else {
            TIMELINE_PADDING * 2.0
                + lane_count * TIMELINE_LANE_HEIGHT
                + (lane_count - 1.0) * TIMELINE_LANE_GAP
        }
    }

    fn sync_viewport_size(&mut self) {
        self.viewport.set_view_span(0.0..self.size.width.max(1.0));
        if !self.viewport_initialized {
            self.viewport.fit_world();
            self.viewport_initialized = true;
        }
    }

    fn total_duration_secs(&self) -> f64 {
        self.viewport
            .world_bounds()
            .map(|bounds| (bounds.end - bounds.start).abs())
            .unwrap_or(1.0)
    }

    fn root_local_x(&self, cx: &crate::context::EventCx<'_>, point: Point) -> f64 {
        if cx.target == self.id.get_element_id() {
            return point.x;
        }

        let window_point = cx.world_transform.inverse() * point;
        let box_tree = self.id.box_tree();
        let box_tree = box_tree.borrow();
        let root_world = box_tree
            .world_transform(self.id.get_element_id().0)
            .unwrap_or_default();
        (root_world.inverse() * window_point).x
    }

    fn rebuild_label_layouts(&mut self) {
        let attrs = Attrs::new()
            .font_size(TIMELINE_LABEL_FONT_SIZE)
            .color(self.palette.text);
        let attrs_list = AttrsList::new(attrs);
        for element in &mut self.elements {
            element
                .label_layout
                .set_text(&element.item.label, attrs_list.clone(), None);
        }
    }

    fn time_at_root_local_x(&self, x: f64) -> Option<f64> {
        if self.size.width <= f64::EPSILON {
            return None;
        }
        Some(
            self.viewport
                .view_to_world_x(x.clamp(0.0, self.size.width))
                .clamp(0.0, self.total_duration_secs()),
        )
    }

    fn fit_selected_frame(&mut self, frame: &ProfileFrameData) {
        if !self.viewport_initialized {
            return;
        }
        let visible = self.viewport.visible_world_range();
        let visible_width = (visible.end - visible.start).abs().max(f64::MIN_POSITIVE);
        let center = frame.start.as_secs_f64() + frame.duration.as_secs_f64() * 0.5;
        self.viewport
            .fit_range((center - visible_width * 0.5)..(center + visible_width * 0.5));
    }

    fn center_viewport_on(&mut self, center: f64) {
        if !self.viewport_initialized {
            return;
        }
        let visible = self.viewport.visible_world_range();
        let total = self.total_duration_secs().max(f64::MIN_POSITIVE);
        let visible_width = (visible.end - visible.start)
            .abs()
            .clamp(f64::MIN_POSITIVE, total);
        let mut start = center - visible_width * 0.5;
        let mut end = center + visible_width * 0.5;

        if start < 0.0 {
            end -= start;
            start = 0.0;
        }
        if end > total {
            start -= end - total;
            end = total;
        }
        if start < 0.0 {
            start = 0.0;
        }
        self.viewport
            .fit_range(start..end.max(start + f64::MIN_POSITIVE));
    }

    fn pan_viewport_by_world(&mut self, delta: f64) {
        if !self.viewport_initialized {
            return;
        }
        let visible = self.viewport.visible_world_range();
        let visible_width = (visible.end - visible.start).abs().max(f64::MIN_POSITIVE);
        self.center_viewport_on((visible.start + visible_width * 0.5) + delta);
    }

    fn zoom_viewport_around_time(&mut self, anchor_time: f64, factor: f64) {
        if !self.viewport_initialized {
            return;
        }
        let total = self.total_duration_secs().max(f64::MIN_POSITIVE);
        let visible = self.viewport.visible_world_range();
        let width = (visible.end - visible.start)
            .abs()
            .clamp(f64::MIN_POSITIVE, total);
        let factor = factor.max(0.01);
        let new_width = (width / factor).clamp(f64::MIN_POSITIVE, total);
        let anchor_time = anchor_time.clamp(0.0, total);
        let anchor_ratio = if width <= f64::MIN_POSITIVE {
            0.5
        } else {
            ((anchor_time - visible.start) / width).clamp(0.0, 1.0)
        };
        let mut start = anchor_time - new_width * anchor_ratio;
        let mut end = start + new_width;
        if start < 0.0 {
            end -= start;
            start = 0.0;
        }
        if end > total {
            start -= end - total;
            end = total;
        }
        if start < 0.0 {
            start = 0.0;
        }
        self.viewport
            .fit_range(start..end.max(start + f64::MIN_POSITIVE));
    }

    fn sync_selected_frame(&mut self, selected_frame: Option<Rc<ProfileFrameData>>) -> bool {
        let selected_index = selected_frame.as_ref().map(|frame| frame.index);
        if selected_index == self.last_selected_frame {
            return false;
        }
        self.last_selected_frame = selected_index;
        if let Some(frame) = selected_frame.as_ref() {
            self.fit_selected_frame(frame);
        }
        true
    }

    fn refresh_viewport(&mut self, window_state: &mut crate::WindowState) {
        let visible = self.viewport.visible_world_range();
        self.visible_range.set((visible.start, visible.end));
        self.sync_content_transform(window_state);
        window_state.request_paint(self.id.get_element_id());
    }

    fn invalidate_timeline_paint(&self, window_state: &mut crate::WindowState) {
        window_state.request_paint(self.id.get_element_id());
        window_state.request_paint(self.content_element);
        for element in &self.elements {
            window_state.request_paint(element.element_id);
        }
    }

    fn maybe_rebase_content_geometry(&mut self, window_state: &mut crate::WindowState) -> bool {
        if !self.viewport_initialized {
            return false;
        }

        let visible = self.viewport.visible_world_range();
        let desired_origin = visible.start.max(0.0);
        let rebase_threshold =
            (256.0 / self.viewport.zoom().abs().max(f64::MIN_POSITIVE)).max(1e-9);
        if (desired_origin - self.content_origin_x).abs() < rebase_threshold {
            return false;
        }

        self.content_origin_x = desired_origin;
        let total_duration = self.total_duration_secs();
        let total_height = Self::total_height_for_lane_count(self.lane_count);

        let box_tree = self.id.box_tree();
        let mut bt = box_tree.borrow_mut();
        bt.set_local_bounds(
            self.content_element.0,
            Rect::new(
                -self.content_origin_x,
                0.0,
                total_duration - self.content_origin_x,
                total_height,
            ),
        );

        for element in &self.elements {
            let x0 = element.item.start.as_secs_f64() - self.content_origin_x;
            let width = element.item.duration.as_secs_f64().max(1e-6);
            let lane_y = TIMELINE_PADDING
                + element.lane_idx as f64 * (TIMELINE_LANE_HEIGHT + TIMELINE_LANE_GAP);
            bt.set_local_bounds(
                element.element_id.0,
                Rect::new(x0, lane_y, x0 + width, lane_y + TIMELINE_LANE_HEIGHT),
            );
        }
        drop(bt);

        self.id.request_box_tree_commit();
        self.invalidate_timeline_paint(window_state);
        true
    }

    fn sync_content_transform(&mut self, window_state: &mut crate::WindowState) {
        let rebased = self.maybe_rebase_content_geometry(window_state);
        let pan = self.viewport.world_to_view_x(self.content_origin_x);
        let transform = Affine::new([self.viewport.zoom(), 0.0, 0.0, 1.0, pan, 0.0]);
        let box_tree = self.id.box_tree();
        let transform_changed = box_tree
            .borrow_mut()
            .set_local_transform(self.content_element.0, transform);
        if transform_changed || rebased {
            self.id.request_box_tree_commit();
            window_state.request_paint(self.id.get_element_id());
        }
    }

    fn set_hovered_element(
        &mut self,
        hovered: Option<ElementId>,
        window_state: &mut crate::WindowState,
    ) {
        if hovered == self.hovered_element {
            return;
        }
        let previous = self.hovered_element;
        self.hovered_element = hovered;
        if let Some(previous) = previous
            && previous != self.id.get_element_id()
        {
            window_state.request_paint(previous);
        }
        if let Some(current) = hovered {
            window_state.request_paint(current);
        }
        let event = hovered
            .and_then(|element_id| self.element_indices.get(&element_id).copied())
            .map(|idx| self.elements[idx].item.clone());
        self.hovered_event.set(event);
    }

    fn update_from_layout(
        &mut self,
        layout: &LayoutChanged,
        window_state: &mut crate::WindowState,
    ) {
        let new_size = layout.new_box.size();
        if self.viewport_initialized && self.size == new_size {
            return;
        }

        self.size = new_size;
        self.sync_viewport_size();
        let selected_frame = self.selected_frame.get_untracked();
        self.sync_selected_frame(selected_frame);
        self.refresh_viewport(window_state);
    }
}

impl View for ProfilerTimelineView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Profiler Timeline View".into()
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(update) = state.downcast::<ProfilerTimelineUpdate>() {
            match *update {
                ProfilerTimelineUpdate::SelectedFrame(selected_frame) => {
                    if self.sync_selected_frame(selected_frame) {
                        self.refresh_viewport(cx.window_state);
                    }
                }
                ProfilerTimelineUpdate::ViewportRequest(request) => {
                    self.viewport_request.set(None);
                    match request {
                        ProfilerViewportRequest::Center(center) => {
                            self.center_viewport_on(center);
                        }
                        ProfilerViewportRequest::PanByWorld(delta) => {
                            self.pan_viewport_by_world(delta);
                        }
                        ProfilerViewportRequest::ZoomAround {
                            anchor_time,
                            factor,
                        } => {
                            self.zoom_viewport_around_time(anchor_time, factor);
                        }
                    }
                    self.refresh_viewport(cx.window_state);
                }
            }
        }
    }

    fn style_pass(&mut self, cx: &mut StyleCx<'_>) {
        let Some(theme) = cx.get_prop(Theme) else {
            return;
        };
        let palette = TimelinePalette {
            lane_bg: theme.bg_elevated(),
            border: theme.border(),
            text: theme.text(),
            selected: css::TOMATO,
        };
        let palette_changed = palette.lane_bg != self.palette.lane_bg
            || palette.border != self.palette.border
            || palette.text != self.palette.text
            || palette.selected != self.palette.selected;
        if palette_changed {
            self.palette = palette;
            self.rebuild_label_layouts();
            cx.window_state.request_paint(self.id);
        }
    }

    fn event(&mut self, cx: &mut crate::context::EventCx) -> EventPropagation {
        if let Some(layout) = LayoutChangedListener::extract(&cx.event) {
            self.update_from_layout(layout, cx.window_state);
            return EventPropagation::Continue;
        }

        match &cx.event {
            Event::Pointer(PointerEvent::Enter(_)) => {
                let hovered = self
                    .element_indices
                    .contains_key(&cx.target)
                    .then_some(cx.target);
                if hovered.is_some() {
                    self.set_hovered_element(hovered, cx.window_state);
                    EventPropagation::Stop
                } else {
                    EventPropagation::Continue
                }
            }
            Event::Pointer(PointerEvent::Move(event)) => {
                let root_local_x = self.root_local_x(cx, event.current.logical_point());
                self.cursor_time
                    .set(self.time_at_root_local_x(root_local_x));
                EventPropagation::Continue
            }
            Event::Pointer(PointerEvent::Scroll(event)) => {
                let delta = event.resolve_to_points(None, None);
                let anchor_x = self.root_local_x(cx, event.state.logical_point());
                let mut changed = false;
                let mut zoomed = false;

                if delta.x.abs() > delta.y.abs() && delta.x.abs() > f64::EPSILON {
                    self.viewport.pan_by_view(delta.x);
                    changed = true;
                } else if delta.y.abs() > f64::EPSILON {
                    let factor = (1.0 - delta.y / 400.0).max(0.01);
                    self.viewport.zoom_about_view_point(anchor_x, factor);
                    changed = true;
                    zoomed = true;
                }

                if changed {
                    if zoomed {
                        self.invalidate_timeline_paint(cx.window_state);
                    }
                    self.refresh_viewport(cx.window_state);
                    EventPropagation::Stop
                } else {
                    EventPropagation::Continue
                }
            }
            Event::Pointer(PointerEvent::Gesture(gesture)) => {
                let PointerGesture::Pinch(delta) = gesture.gesture else {
                    return EventPropagation::Continue;
                };
                let factor = (1.0 + f64::from(delta)).max(0.01);
                let anchor_x = self.root_local_x(cx, gesture.state.logical_point());
                self.viewport.zoom_about_view_point(anchor_x, factor);
                self.invalidate_timeline_paint(cx.window_state);
                self.refresh_viewport(cx.window_state);
                EventPropagation::Stop
            }
            Event::Pointer(PointerEvent::Leave(_)) if cx.target == self.id.get_element_id() => {
                self.cursor_time.set(None);
                EventPropagation::Continue
            }
            _ => EventPropagation::Continue,
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        if cx.target_id == self.id.get_element_id() {
            for lane_idx in 0..self.lane_count {
                let y =
                    TIMELINE_PADDING + lane_idx as f64 * (TIMELINE_LANE_HEIGHT + TIMELINE_LANE_GAP);
                let lane_rect = Rect::new(0.0, y, self.size.width, y + TIMELINE_LANE_HEIGHT);
                cx.painter
                    .fill(lane_rect, &Brush::Solid(self.palette.lane_bg))
                    .draw();
            }
            let total_height = Self::total_height_for_lane_count(self.lane_count);
            let selected_index = self.active_frame_index.get_untracked();
            for marker in self.frame_markers.iter() {
                let x = self.viewport.world_to_view_x(marker.start.as_secs_f64());
                if x < 0.0 || x > self.size.width {
                    continue;
                }
                let is_selected = selected_index == Some(marker.index);
                let color = if is_selected {
                    self.palette.selected
                } else {
                    self.palette.border.with_alpha(0.35)
                };
                cx.painter
                    .stroke(
                        Line::new(
                            (x, TIMELINE_PADDING - 2.0),
                            (x, total_height - TIMELINE_PADDING + 2.0),
                        ),
                        &Stroke::new(if is_selected { 1.5 } else { 1.0 }),
                        &Brush::Solid(color),
                    )
                    .draw();
                cx.painter
                    .fill(
                        Rect::new(x - 2.5, 2.0, x + 2.5, 8.0).to_rounded_rect(2.0),
                        &Brush::Solid(color),
                    )
                    .draw();
            }
            for marker in self.instant_markers.iter() {
                let x = self.viewport.world_to_view_x(marker.start.as_secs_f64());
                if x < 0.0 || x > self.size.width {
                    continue;
                }
                let color = marker.color.with_alpha(0.7);
                cx.painter
                    .stroke(
                        Line::new(
                            (x, TIMELINE_PADDING - 2.0),
                            (x, total_height - TIMELINE_PADDING + 2.0),
                        ),
                        &Stroke::new(1.0),
                        &Brush::Solid(color),
                    )
                    .draw();
                cx.painter
                    .fill(
                        Rect::new(x - 2.5, 2.0, x + 2.5, 8.0).to_rounded_rect(2.0),
                        &Brush::Solid(color),
                    )
                    .draw();
            }
            return;
        }

        if let Some(&element_idx) = self.element_indices.get(&cx.target_id) {
            let element = &self.elements[element_idx];
            let rect = cx.layout_rect_local;
            let rounded_rect = rect.to_rounded_rect(2.0);
            let hovered = self.hovered_element == Some(element.element_id);
            let color = timeline_item_color(&element.item);
            cx.painter
                .fill(
                    rounded_rect,
                    &Brush::Solid(color.with_alpha(if hovered { 0.9 } else { 0.7 })),
                )
                .draw();
        }
    }

    fn post_paint(&mut self, cx: &mut PaintCx) {
        if cx.target_id != self.id.get_element_id() {
            return;
        }

        let visible = self.viewport.visible_world_range();

        for element in &self.elements {
            let start = element.item.start.as_secs_f64();
            let end = start + element.item.duration.as_secs_f64();
            if end <= visible.start || start >= visible.end {
                continue;
            }

            let x0 = self.viewport.world_to_view_x(start);
            let x1 = self.viewport.world_to_view_x(end);
            let lane_y = TIMELINE_PADDING
                + element.lane_idx as f64 * (TIMELINE_LANE_HEIGHT + TIMELINE_LANE_GAP);
            let rect = Rect::new(x0, lane_y, x1, lane_y + TIMELINE_LANE_HEIGHT);
            if rect.width() < 10.0 || rect.x1 <= 0.0 || rect.x0 >= self.size.width {
                continue;
            }

            let text_origin = Point::new(
                rect.x0 + 6.0,
                rect.y0
                    + ((TIMELINE_LANE_HEIGHT.min(rect.height()))
                        - element.label_layout.size().height)
                        * 0.5,
            );
            cx.painter.with_fill_clip(rect, |p| {
                element
                    .label_layout
                    .draw_with_painter(p.as_dyn(), text_origin, cx.font_embolden);
            });
        }
    }
}

#[derive(Clone, Copy)]
struct OverviewPalette {
    background: Color,
    border: Color,
    textured_base: Color,
    viewport_fill: Color,
    viewport_stroke: Color,
}

#[derive(Clone, Copy)]
struct OverviewBin {
    density: f64,
    color: Color,
}

#[derive(Default)]
struct OverviewAccum {
    density: f64,
    weight: f64,
    color_r: f64,
    color_g: f64,
    color_b: f64,
}

fn mix_overview_color(base: Color, accent: Color, amount: f64) -> Color {
    base.lerp(
        accent,
        amount.clamp(0.0, 1.0) as f32,
        HueDirection::default(),
    )
}

fn smooth_overview_bins(bins: &mut [OverviewBin]) {
    if bins.len() < 3 {
        return;
    }
    let source = bins.to_vec();
    for idx in 0..bins.len() {
        let prev = source[idx.saturating_sub(1)];
        let curr = source[idx];
        let next = source[(idx + 1).min(source.len() - 1)];
        bins[idx].density = prev.density * 0.2 + curr.density * 0.6 + next.density * 0.2;
        bins[idx].color = mix_overview_color(
            mix_overview_color(curr.color, prev.color, 0.25),
            next.color,
            0.25,
        );
    }
}

fn build_overview_bins(
    lanes: &[Vec<TimelineItem>],
    total_duration_secs: f64,
    lane_count: usize,
) -> Rc<[OverviewBin]> {
    if lanes.is_empty() || total_duration_secs <= f64::EPSILON {
        return Rc::from(Vec::<OverviewBin>::new());
    }

    let mut accum = (0..OVERVIEW_BIN_COUNT)
        .map(|_| OverviewAccum::default())
        .collect::<Vec<_>>();
    let bin_span = total_duration_secs / OVERVIEW_BIN_COUNT as f64;
    let lane_norm = lane_count.max(1) as f64;

    for lane in lanes {
        for item in lane {
            let color = timeline_item_color(item).to_rgba8();
            let start = item.start.as_secs_f64().clamp(0.0, total_duration_secs);
            let end =
                (start + item.duration.as_secs_f64().max(1e-6)).clamp(0.0, total_duration_secs);
            if end <= start {
                continue;
            }

            let start_idx =
                ((start / total_duration_secs) * OVERVIEW_BIN_COUNT as f64).floor() as usize;
            let end_idx = (((end / total_duration_secs) * OVERVIEW_BIN_COUNT as f64).ceil()
                as usize)
                .min(OVERVIEW_BIN_COUNT.saturating_sub(1));

            for (idx, acc) in accum
                .iter_mut()
                .enumerate()
                .take(end_idx + 1)
                .skip(start_idx.min(OVERVIEW_BIN_COUNT - 1))
            {
                let bin_start = idx as f64 * bin_span;
                let bin_end = bin_start + bin_span;
                let overlap = (end.min(bin_end) - start.max(bin_start)).max(0.0);
                if overlap <= 0.0 {
                    continue;
                }
                let weight = overlap / bin_span.max(f64::MIN_POSITIVE);
                acc.density += weight / lane_norm;
                acc.weight += weight;
                acc.color_r += f64::from(color.r) * weight;
                acc.color_g += f64::from(color.g) * weight;
                acc.color_b += f64::from(color.b) * weight;
            }
        }
    }

    let max_density = accum
        .iter()
        .map(|acc| acc.density)
        .fold(0.0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let mut bins = accum
        .into_iter()
        .map(|acc| {
            let avg_color = if acc.weight > 0.0 {
                Color::from_rgb8(
                    (acc.color_r / acc.weight).clamp(0.0, 255.0) as u8,
                    (acc.color_g / acc.weight).clamp(0.0, 255.0) as u8,
                    (acc.color_b / acc.weight).clamp(0.0, 255.0) as u8,
                )
            } else {
                Color::from_rgb8(145, 155, 168)
            };
            OverviewBin {
                density: (acc.density / max_density).sqrt().clamp(0.0, 1.0),
                color: avg_color,
            }
        })
        .collect::<Vec<_>>();
    smooth_overview_bins(&mut bins);
    Rc::from(bins)
}

struct ProfilerOverviewView {
    id: ViewId,
    bins: Rc<[OverviewBin]>,
    total_duration_secs: f64,
    cursor_time: RwSignal<Option<f64>>,
    visible_range: RwSignal<(f64, f64)>,
    viewport_request: RwSignal<Option<ProfilerViewportRequest>>,
    dragging: bool,
    size: Size,
    palette: OverviewPalette,
}

impl ProfilerOverviewView {
    fn new(
        bins: Rc<[OverviewBin]>,
        total_duration: Duration,
        cursor_time: RwSignal<Option<f64>>,
        visible_range: RwSignal<(f64, f64)>,
        viewport_request: RwSignal<Option<ProfilerViewportRequest>>,
    ) -> Self {
        let id = ViewId::new();
        id.register_listener(LayoutChangedListener::listener_key());

        let effect_id = id;
        let effect_visible_range = visible_range;
        Effect::new(move |_| {
            let _ = effect_visible_range.get();
            effect_id.request_paint();
        });

        Self {
            id,
            bins,
            total_duration_secs: total_duration.as_secs_f64().max(f64::MIN_POSITIVE),
            cursor_time,
            visible_range,
            viewport_request,
            dragging: false,
            size: Size::ZERO,
            palette: OverviewPalette {
                background: Color::from_rgb8(245, 247, 250),
                border: Color::from_rgb8(202, 208, 216),
                textured_base: Color::from_rgb8(178, 186, 197),
                viewport_fill: Color::from_rgba8(54, 111, 196, 28),
                viewport_stroke: Color::from_rgba8(54, 111, 196, 180),
            },
        }
    }

    fn update_from_layout(
        &mut self,
        layout: &LayoutChanged,
        window_state: &mut crate::WindowState,
    ) {
        let new_size = layout.new_box.size();
        if self.size == new_size {
            return;
        }
        self.size = new_size;
        window_state.request_paint(self.id);
    }

    fn inner_rect(&self) -> Rect {
        Rect::new(
            OVERVIEW_INSET,
            OVERVIEW_INSET,
            (self.size.width - OVERVIEW_INSET).max(OVERVIEW_INSET),
            (self.size.height - OVERVIEW_INSET).max(OVERVIEW_INSET),
        )
    }

    fn time_at_local_point(&self, point: Point) -> Option<f64> {
        let inner = self.inner_rect();
        if inner.width() <= f64::EPSILON {
            return None;
        }
        let t = ((point.x - inner.x0) / inner.width()).clamp(0.0, 1.0);
        Some(self.total_duration_secs * t)
    }

    fn visible_width_secs(&self) -> f64 {
        let (start, end) = self.visible_range.get_untracked();
        (end - start).abs().clamp(
            f64::MIN_POSITIVE,
            self.total_duration_secs.max(f64::MIN_POSITIVE),
        )
    }

    fn request_center_at(&mut self, point: Point, window_state: &mut crate::WindowState) {
        if let Some(time) = self.time_at_local_point(point) {
            self.viewport_request
                .set(Some(ProfilerViewportRequest::Center(time)));
        }
        window_state.request_paint(self.id);
    }

    fn request_pan_by(&mut self, delta_x: f64, window_state: &mut crate::WindowState) {
        let inner = self.inner_rect();
        if inner.width() <= f64::EPSILON {
            return;
        }
        let world_delta = delta_x * (self.visible_width_secs() / inner.width());
        self.viewport_request
            .set(Some(ProfilerViewportRequest::PanByWorld(world_delta)));
        window_state.request_paint(self.id);
    }

    fn request_zoom_at(
        &mut self,
        point: Point,
        factor: f64,
        window_state: &mut crate::WindowState,
    ) {
        if let Some(anchor_time) = self.time_at_local_point(point) {
            self.viewport_request
                .set(Some(ProfilerViewportRequest::ZoomAround {
                    anchor_time,
                    factor,
                }));
        }
        window_state.request_paint(self.id);
    }
}

impl View for ProfilerOverviewView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Profiler Overview View".into()
    }

    fn style_pass(&mut self, cx: &mut StyleCx<'_>) {
        let Some(theme) = cx.get_prop(Theme) else {
            return;
        };
        self.palette = OverviewPalette {
            background: theme.bg_overlay(),
            border: theme.border_muted(),
            textured_base: theme.text_muted().with_alpha(0.55),
            viewport_fill: theme.primary().with_alpha(0.12),
            viewport_stroke: theme.primary().with_alpha(0.72),
        };
        cx.window_state.request_paint(self.id);
    }

    fn event(&mut self, cx: &mut crate::context::EventCx) -> EventPropagation {
        if let Some(layout) = LayoutChangedListener::extract(&cx.event) {
            self.update_from_layout(layout, cx.window_state);
            return EventPropagation::Continue;
        }

        match &cx.event {
            Event::Pointer(PointerEvent::Down(event)) => {
                if event.button == Some(PointerButton::Primary) {
                    let pointer_id = event.pointer.pointer_id;
                    let point = event.state.logical_point();
                    if let Some(pointer_id) = pointer_id {
                        cx.request_pointer_capture(pointer_id);
                    }
                    self.dragging = true;
                    self.request_center_at(point, cx.window_state);
                    EventPropagation::Stop
                } else {
                    EventPropagation::Continue
                }
            }
            Event::Pointer(PointerEvent::Move(event)) => {
                self.cursor_time
                    .set(self.time_at_local_point(event.current.logical_point()));
                if self.dragging {
                    self.request_center_at(event.current.logical_point(), cx.window_state);
                    EventPropagation::Stop
                } else {
                    EventPropagation::Continue
                }
            }
            Event::Pointer(PointerEvent::Scroll(event)) => {
                let delta = event.resolve_to_points(None, None);
                if delta.x.abs() > delta.y.abs() && delta.x.abs() > f64::EPSILON {
                    self.request_pan_by(delta.x, cx.window_state);
                    EventPropagation::Stop
                } else if delta.y.abs() > f64::EPSILON {
                    let factor = (1.0 - delta.y / 400.0).max(0.01);
                    self.request_zoom_at(event.state.logical_point(), factor, cx.window_state);
                    EventPropagation::Stop
                } else {
                    EventPropagation::Continue
                }
            }
            Event::Pointer(PointerEvent::Gesture(gesture)) => {
                let PointerGesture::Pinch(delta) = gesture.gesture else {
                    return EventPropagation::Continue;
                };
                let factor = (1.0 + f64::from(delta)).max(0.01);
                self.request_zoom_at(gesture.state.logical_point(), factor, cx.window_state);
                EventPropagation::Stop
            }
            Event::Pointer(PointerEvent::Up(_)) => {
                self.dragging = false;
                EventPropagation::Continue
            }
            Event::Pointer(PointerEvent::Leave(_)) => {
                self.dragging = false;
                self.cursor_time.set(None);
                EventPropagation::Continue
            }
            _ => EventPropagation::Continue,
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        if cx.target_id != self.id.get_element_id() {
            return;
        }

        let outer = Rect::from_origin_size(Point::ZERO, self.size).to_rounded_rect(16.0);
        cx.painter
            .fill(outer, &Brush::Solid(self.palette.background))
            .draw();
        cx.painter
            .stroke(outer, &Stroke::new(1.0), &Brush::Solid(self.palette.border))
            .draw();

        let inner = self.inner_rect();
        if inner.width() <= f64::EPSILON || inner.height() <= f64::EPSILON {
            return;
        }

        cx.painter
            .fill(
                inner.to_rounded_rect(12.0),
                &Brush::Solid(self.palette.background.with_alpha(0.82)),
            )
            .draw();

        if !self.bins.is_empty() {
            let bin_width = inner.width() / self.bins.len() as f64;
            let min_bar_height = 6.0_f64.min(inner.height());
            for (idx, bin) in self.bins.iter().enumerate() {
                if bin.density <= 0.01 {
                    continue;
                }
                let x0 = inner.x0 + idx as f64 * bin_width;
                let x1 = (x0 + bin_width + 0.35).min(inner.x1);
                let height = min_bar_height + (inner.height() - min_bar_height) * bin.density;
                let y0 = inner.y1 - height;
                let fill = mix_overview_color(
                    self.palette.textured_base,
                    bin.color,
                    0.25 + bin.density * 0.75,
                )
                .with_alpha(0.92);
                cx.painter
                    .fill(
                        Rect::new(x0, y0, x1, inner.y1).to_rounded_rect(1.5),
                        &Brush::Solid(fill),
                    )
                    .draw();
            }
        }

        let (visible_start, visible_end) = self.visible_range.get();
        let x0 = inner.x0 + inner.width() * (visible_start / self.total_duration_secs);
        let x1 = inner.x0 + inner.width() * (visible_end / self.total_duration_secs);
        let viewport_rect = Rect::new(x0, inner.y0, x1.max(x0 + 1.0), inner.y1);
        cx.painter
            .fill(
                viewport_rect.to_rounded_rect(10.0),
                &Brush::Solid(self.palette.viewport_fill),
            )
            .draw();
        cx.painter
            .stroke(
                viewport_rect.to_rounded_rect(10.0),
                &Stroke::new(1.0),
                &Brush::Solid(self.palette.viewport_stroke),
            )
            .draw();
    }
}

fn info(name: impl Display, value: String) -> impl IntoView {
    info_row(
        name.to_string(),
        Label::new(value).style(|s| {
            s.font_size(12.5)
                .font_bold()
                .with_theme(|s, t| s.color(t.text()))
        }),
    )
}

fn info_row(name: String, view: impl IntoView + 'static) -> impl IntoView {
    Stack::new((
        Label::new(name)
            .style(|s| {
                s.margin_right(5.0)
                    .font_size(10.5)
                    .with_theme(|s, t| s.color(t.text_muted()))
            })
            .container()
            .style(|s| s.min_width(80.0).flex_direction(FlexDirection::RowReverse)),
        view,
    ))
    .style(|s| {
        s.padding_horiz(10.0)
            .padding_vert(8.0)
            .border_radius(12.0)
            .with_theme(|s, t| s.background(t.bg_overlay()))
    })
}

fn profile_view(profile: &Rc<Profile>) -> impl IntoView {
    let origin = profile.timeline_origin;
    let frames: Rc<[_]> = profile
        .frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| {
            frame.start.zip(origin).map(|(start, origin)| {
                Rc::new(ProfileFrameData {
                    index,
                    start: start.saturating_duration_since(origin),
                    duration: frame.duration,
                    sum: frame.sum,
                    event_count: frame.event_count,
                })
            })
        })
        .collect::<Vec<_>>()
        .into();

    let total_duration = profile_total_duration(profile).max(Duration::from_micros(1));
    let lanes: Rc<[Vec<TimelineItem>]> = Rc::from(build_timeline_lanes(profile));
    let overview_bins = build_overview_bins(&lanes, total_duration.as_secs_f64(), lanes.len());
    let interval_count = lanes.iter().map(|lane| lane.len()).sum::<usize>();
    let frame_markers: Rc<[TimelineFrameMarker]> = profile
        .frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| {
            frame
                .start
                .zip(origin)
                .map(|(start, origin)| TimelineFrameMarker {
                    index,
                    start: start.saturating_duration_since(origin),
                })
        })
        .collect::<Vec<_>>()
        .into();
    let instant_markers: Rc<[TimelineInstantMarker]> = profile
        .events
        .iter()
        .filter(|event| event.start == event.end)
        .filter_map(|event| {
            origin.map(|origin| TimelineInstantMarker {
                start: event.start.saturating_duration_since(origin),
                color: instant_marker_color(&event.name),
            })
        })
        .collect::<Vec<_>>()
        .into();
    let selected_frame = RwSignal::new(None::<Rc<ProfileFrameData>>);
    let active_frame_index = RwSignal::new(None::<usize>);
    let cursor_time = RwSignal::new(None::<f64>);
    let visible_range = RwSignal::new((0.0, total_duration.as_secs_f64()));
    let viewport_request = RwSignal::new(None::<ProfilerViewportRequest>);
    let suppress_frame_list_select = RwSignal::new(false);

    let hovered_event: RwSignal<Option<TimelineItem>> = RwSignal::new(None);

    let event_tooltip = dyn_container(
        move || hovered_event.get(),
        move |hovered_event| {
            if let Some(event) = hovered_event {
                let accent = timeline_item_color(&event);
                Stack::vertical((
                    Stack::vertical((
                        Label::new(event.label)
                            .style(|s| {
                                s.font_size(15.0)
                                    .font_bold()
                                    .text_wrap()
                                    .min_width(0.0)
                                    .max_width_full()
                                    .with_theme(|s, t| s.color(t.text()))
                            })
                            .scroll()
                            .style(|s| s.width_full().max_height(200)),
                        Label::new(event.source.to_string())
                            .style(move |s| s.font_size(11.0).color(accent)),
                    ))
                    .style(|s| s.gap(3.0).width_full()),
                    Stack::vertical((
                        info("Depth", event.depth.to_string()),
                        info("Start", format_duration_short(event.start)),
                        info("Duration", format_duration_short(event.duration)),
                    ))
                    .style(|s| s.gap(6.0).width_full()),
                ))
                .style(|s| s.width_full().gap(10.0))
                .into_any()
            } else {
                Stack::vertical((
                    Label::new("Hover an interval").style(|s| {
                        s.font_size(14.0)
                            .font_bold()
                            .with_theme(|s, t| s.color(t.text()))
                    }),
                    Label::new("Bars expose exact start and duration here.")
                        .style(|s| s.font_size(11.0).with_theme(|s, t| s.color(t.text_muted()))),
                ))
                .style(|s| s.padding(2.0).width_full().gap(4.0))
                .into_any()
            }
        },
    )
    .style(|s| {
        s.width_full()
            .padding(14.0)
            .border_radius(18.0)
            .border(1.0)
            .with_theme(|s, t| s.background(t.bg_overlay()).border_color(t.border_muted()))
    });

    let frame_views: Vec<_> = frames
        .iter()
        .map(|frame| {
            let frame = frame.clone();
            let frame_for_style = frame.clone();
            Button::new(
                Stack::vertical((
                    Stack::horizontal((
                        Label::new(format!("Frame #{}", frame.index)).style(|s| {
                            s.font_size(13.0)
                                .font_bold()
                                .with_theme(|s, t| s.color(t.text()))
                                .flex_grow(1.0)
                        }),
                        Label::new(format_duration_short(frame.sum)).style(|s| {
                            s.font_size(11.0)
                                .font_bold()
                                .padding_horiz(8.0)
                                .padding_vert(4.0)
                                .border_radius(999.0)
                                .background(Color::from_rgba8(54, 111, 196, 22))
                                .color(Color::from_rgb8(54, 111, 196))
                        }),
                    ))
                    .style(|s| s.items_center().gap(10.0)),
                    Stack::horizontal((
                        Label::new(format!("start {}", format_duration_short(frame.start))),
                        Label::new(format!("{} events", frame.event_count)),
                    ))
                    .style(|s| {
                        s.gap(10.0)
                            .font_size(10.5)
                            .with_theme(|s, t| s.color(t.text_muted()))
                    }),
                ))
                .style(move |s| {
                    let selected = active_frame_index.get() == Some(frame_for_style.index);
                    s.width_full()
                        .justify_start()
                        .padding(12.0)
                        .border_radius(16.0)
                        .border(1.0)
                        .with_theme(move |s, t| {
                            s.border_color(if selected {
                                t.def(|t| t.primary().with_alpha(0.38))
                            } else {
                                t.border_muted()
                            })
                            .background(if selected {
                                t.def(|t| t.primary().with_alpha(0.12))
                            } else {
                                t.bg_base()
                            })
                        })
                }),
            )
            .style(|s| s.width_full().justify_start())
        })
        .collect();
    let frames_clone = frames.clone();
    let frames_list = list(frame_views);
    let frame_list_selection = frames_list.selection();
    let frames_list = frames_list.on_select(move |idx| {
        if suppress_frame_list_select.get_untracked() {
            return;
        }
        let frame = idx.and_then(|idx| frames_clone.get(idx)).cloned();
        let new_index = frame.as_ref().map(|frame| frame.index);
        let current_index = selected_frame
            .get_untracked()
            .as_ref()
            .map(|frame| frame.index);
        if current_index == new_index {
            return;
        }
        active_frame_index.set(new_index);
        selected_frame.set(frame);
    });

    let frames_for_current = frames.clone();
    let current_frame_index = Memo::new(move |_| {
        if let Some(cursor_time) = cursor_time.get() {
            frame_for_cursor_time(&frames_for_current, cursor_time).map(|frame| frame.index)
        } else {
            let visible = visible_range.get();
            frame_for_visible_range(&frames_for_current, visible).map(|frame| frame.index)
        }
    });

    {
        Effect::new(move |_| {
            let frame_index = current_frame_index.get();
            if active_frame_index.get_untracked() != frame_index {
                active_frame_index.set(frame_index);
            }
            if frame_list_selection.get_untracked() != frame_index {
                suppress_frame_list_select.set(true);
                frame_list_selection.set(frame_index);
                suppress_frame_list_select.set(false);
            }
        });
    }

    let frames_list = frames_list.style(|s| {
        s.width_full().gap(8.0).class(ListItemClass, |s| {
            s.selected(|s| s.unset_background().hover(|s| s.unset_background()))
                .hover(|s| s.unset_background())
        })
    });

    let frames_panel = profiler_panel(
        Stack::vertical((
            Stack::vertical((
                Label::new("Frames").style(|s| {
                    s.font_size(16.0)
                        .font_bold()
                        .with_theme(|s, t| s.color(t.text()))
                }),
                Label::new("Select a frame to center the timeline.")
                    .style(|s| s.font_size(11.0).with_theme(|s, t| s.color(t.text_muted()))),
            ))
            .style(|s| s.gap(4.0)),
            Scroll::new(frames_list).style(|s| {
                s.flex_basis(0)
                    .min_height(0)
                    .flex_grow(1.0)
                    .padding_right(2.0)
            }),
            Stack::vertical((
                Label::new("Inspector").style(|s| {
                    s.font_size(12.0)
                        .font_bold()
                        .with_theme(|s, t| s.color(t.text()))
                }),
                event_tooltip,
            ))
            .style(|s| s.width_full().gap(10.0)),
        ))
        .style(|s| s.padding(16.0).width_full().height_full().gap(14.0)),
    )
    .style(|s| s.min_width(280.0).flex_grow(1.0));

    let timeline = if lanes.is_empty() {
        Stack::vertical((
            Label::new("No timeline events").style(|s| {
                s.font_size(18.0)
                    .font_bold()
                    .with_theme(|s, t| s.color(t.text()))
            }),
            Label::new("Start profiling and capture a frame to populate the timeline.")
                .style(|s| s.font_size(12.0).with_theme(|s, t| s.color(t.text_muted()))),
        ))
        .style(|s| {
            s.width_full()
                .min_height(0)
                .flex_basis(0)
                .flex_grow(1.0)
                .padding(24.0)
                .items_center()
                .justify_center()
                .gap(6.0)
        })
        .into_any()
    } else {
        Stack::vertical((
            ProfilerTimelineView::new(
                lanes,
                frame_markers.clone(),
                instant_markers.clone(),
                total_duration,
                hovered_event,
                cursor_time,
                selected_frame,
                active_frame_index,
                visible_range,
                viewport_request,
            )
            .style(|s| s.width_full().min_height(0.0).flex_grow(1.0)),
            Stack::vertical((
                Stack::horizontal((
                    Label::new("Overview").style(|s| {
                        s.font_size(11.0)
                            .font_bold()
                            .with_theme(|s, t| s.color(t.text()))
                    }),
                    Label::new("Drag, scroll, or pinch to navigate the timeline.")
                        .style(|s| s.font_size(10.5).with_theme(|s, t| s.color(t.text_muted()))),
                ))
                .style(|s| s.items_center().justify_between().gap(10.0)),
                ProfilerOverviewView::new(
                    overview_bins,
                    total_duration,
                    cursor_time,
                    visible_range,
                    viewport_request,
                )
                .style(|s| s.width_full().height(OVERVIEW_HEIGHT)),
            ))
            .style(|s| s.width_full().gap(8.0)),
        ))
        .style(|s| s.size_full().min_height(0.0).gap(12.0))
        .into_any()
    };

    let timeline_panel = profiler_panel(
        Stack::vertical((
            Stack::horizontal((
                Stack::vertical((
                    Label::new("Timeline").style(|s| {
                        s.font_size(18.0)
                            .font_bold()
                            .with_theme(|s, t| s.color(t.text()))
                    }),
                    Label::new("Events are packed into independent tracks by overlap.")
                        .style(|s| s.font_size(11.0).with_theme(|s, t| s.color(t.text_muted()))),
                ))
                .style(|s| s.gap(4.0).min_width(0.0).flex_grow(1.0)),
                Stack::horizontal((
                    profiler_chip("Start", "0 ms", Color::from_rgb8(41, 78, 163)),
                    profiler_chip(
                        "End",
                        format_duration_short(total_duration),
                        Color::from_rgb8(203, 92, 73),
                    ),
                ))
                .style(|s| s.gap(8.0)),
            ))
            .style(|s| s.items_start().gap(12.0)),
            Stack::horizontal((
                profiler_chip(
                    "Frames",
                    frames.len().to_string(),
                    Color::from_rgb8(29, 142, 120),
                ),
                profiler_chip(
                    "Intervals",
                    interval_count.to_string(),
                    Color::from_rgb8(54, 111, 196),
                ),
                profiler_chip(
                    "Captured Span",
                    format_duration_short(total_duration),
                    Color::from_rgb8(64, 157, 163),
                ),
            ))
            .style(|s| s.gap(8.0).flex_wrap(taffy::style::FlexWrap::Wrap)),
            timeline,
        ))
        .style(|s| s.padding(16.0).min_width(0.0).height_full().gap(14.0)),
    )
    .style(|s| s.min_width(0).flex_basis(0).flex_grow(1.0));

    Resizable::new((frames_panel, timeline_panel)).style(|s| {
        s.height_full()
            .width_full()
            .gap(14.0)
            .padding(14.0)
            .with_theme(|s, t| s.background(t.bg_base()))
    })
}

thread_local! {
    pub(crate) static PROFILE: RwSignal<Option<Rc<Profile>>> = {
        Scope::new().create_rw_signal(None)
    };
}

pub fn profiler(window_id: WindowId) -> impl IntoView {
    let profiling = RwSignal::new(false);
    let profile = PROFILE.with(|c| *c);

    let button = Stack::horizontal((
        Button::new(Label::derived(move || {
            if profiling.get() {
                "Stop Profiling"
            } else {
                "Start Profiling"
            }
        }))
        .action(move || {
            add_app_update_event(AppUpdateEvent::ProfileWindow {
                window_id,
                end_profile: if profiling.get() {
                    Some(profile.write_only())
                } else {
                    None
                },
            });
            profiling.set(!profiling.get());
        })
        .style(|s| s.margin(5.0)),
        Label::derived(move || if profiling.get() { "Profiling..." } else { "" }),
    ))
    .style(|s| s.items_center());

    let separator = ().style(move |s| {
        s.width_full()
            .min_height(1.0)
            .with_theme(|s, t| s.background(t.border()))
    });

    let lower = dyn_container(
        move || profile.get(),
        move |profile| {
            if let Some(profile) = profile {
                profile_view(&profile).into_any()
            } else {
                Label::new("No profile")
                    .style(|s| s.padding(5.0))
                    .into_any()
            }
        },
    )
    .style(|s| s.width_full().min_height(0).flex_basis(0).flex_grow(1.0));

    Stack::vertical((button, separator, lower))
        .style(|s| s.size_full())
        .on_event_cont(listener::WindowClosed, move |_, _| {
            if profiling.get() {
                add_app_update_event(AppUpdateEvent::ProfileWindow {
                    window_id,
                    end_profile: Some(profile.write_only()),
                });
            }
        })
}
