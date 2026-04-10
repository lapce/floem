use super::{TimingKind, TimingReport, header};
use crate::app::{AppUpdateEvent, add_app_update_event};
use crate::context::{LayoutChanged, LayoutChangedListener, PaintCx, StyleCx};
use crate::event::{Event, EventPropagation, PointerScrollEventExt, listener};
use crate::prelude::EventListenerTrait;
use crate::prelude::palette::css;
use crate::style::theme::Theme;
use crate::text::{Attrs, AttrsList, TextLayout};
use crate::theme::StyleThemeExt;
use crate::ui_events::pointer::PointerGesture;
use crate::view::{IntoView, View, ViewId};
use crate::views::resizable::Resizable;
use crate::views::{Button, ContainerExt, Decorators, Label, Scroll, Stack, dyn_container, list};
use crate::{ElementId, box_tree::ElementMeta};
use floem_reactive::{RwSignal, Scope, SignalGet, SignalTracker, SignalUpdate};
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

fn timeline_item_color(kind: &TimelineItemKind) -> Color {
    match kind {
        TimelineItemKind::Event => css::STEEL_BLUE,
        TimelineItemKind::Timing(TimingKind::Total) => css::SLATE_BLUE,
        TimelineItemKind::Timing(TimingKind::Update) => css::STEEL_BLUE,
        TimelineItemKind::Timing(TimingKind::Style) => css::SEA_GREEN,
        TimelineItemKind::Timing(TimingKind::Layout) => css::GOLDENROD,
        TimelineItemKind::Timing(TimingKind::BoxTree) => css::SANDY_BROWN,
        TimelineItemKind::Timing(TimingKind::Paint) => css::CORAL,
        TimelineItemKind::Timing(TimingKind::Present) => css::MEDIUM_ORCHID,
        TimelineItemKind::Timing(TimingKind::Renderer) => css::DEEP_SKY_BLUE,
    }
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

    items.extend(profile.events.iter().map(|event| TimelineItem {
        label: event.name.to_string(),
        source: "Profiler Event",
        start: event.start.saturating_duration_since(origin),
        duration: event.end.saturating_duration_since(event.start),
        depth: event.depth,
        kind: TimelineItemKind::Event,
    }));

    for frame in &profile.frames {
        let Some(report) = &frame.timing else {
            continue;
        };
        let Some(anchor) = report.anchor else {
            continue;
        };
        items.extend(report.spans.iter().map(|span| TimelineItem {
            label: span.label.to_string(),
            source: "Timing Span",
            start: anchor.saturating_duration_since(origin) + span.start,
            duration: span.duration,
            depth: span.depth,
            kind: TimelineItemKind::Timing(span.kind),
        }));
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

#[derive(Clone, Copy)]
struct TimelinePalette {
    lane_bg: Color,
    border: Color,
    text: Color,
}

struct TimelineElement {
    element_id: ElementId,
    lane_idx: usize,
    item: TimelineItem,
    label_layout: TextLayout,
}

struct ProfilerTimelineView {
    id: ViewId,
    hovered_event: RwSignal<Option<TimelineItem>>,
    selected_frame: RwSignal<Option<Rc<ProfileFrameData>>>,
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
    last_selected_frame: Option<usize>,
    palette: TimelinePalette,
    tracker: Option<SignalTracker>,
}

impl ProfilerTimelineView {
    fn new(
        lanes: Rc<[Vec<TimelineItem>]>,
        frame_markers: Rc<[TimelineFrameMarker]>,
        total_duration: Duration,
        hovered_event: RwSignal<Option<TimelineItem>>,
        selected_frame: RwSignal<Option<Rc<ProfileFrameData>>>,
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
            selected_frame,
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
            last_selected_frame: None,
            palette: TimelinePalette {
                lane_bg: css::LIGHT_GRAY.with_alpha(0.3),
                border: css::GRAY,
                text: css::BLACK,
            },
            tracker: None,
        };
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

    fn sync_viewport_size(&mut self, window_state: &mut crate::WindowState) {
        self.viewport.set_view_span(0.0..self.size.width.max(1.0));
        if !self.viewport_initialized {
            self.viewport.fit_world();
            self.viewport_initialized = true;
        }
        self.sync_content_transform(window_state);
    }

    fn total_duration_secs(&self) -> f64 {
        self.viewport
            .world_bounds()
            .map(|bounds| (bounds.end - bounds.start).abs())
            .unwrap_or(1.0)
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
        self.size = layout.new_box.size();
        self.sync_viewport_size(window_state);
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

    fn style_pass(&mut self, cx: &mut StyleCx<'_>) {
        let Some(theme) = cx.get_prop(Theme) else {
            return;
        };
        let palette = TimelinePalette {
            lane_bg: theme.bg_elevated(),
            border: theme.border(),
            text: theme.text(),
        };
        let palette_changed = palette.lane_bg != self.palette.lane_bg
            || palette.border != self.palette.border
            || palette.text != self.palette.text;
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
            Event::Pointer(ui_events::pointer::PointerEvent::Enter(_)) => {
                let hovered = self
                    .element_indices
                    .contains_key(&cx.target)
                    .then_some(cx.target);
                self.set_hovered_element(hovered, cx.window_state);
                if hovered.is_some() {
                    EventPropagation::Stop
                } else {
                    EventPropagation::Continue
                }
            }
            Event::Pointer(ui_events::pointer::PointerEvent::Scroll(event)) => {
                let delta = event.resolve_to_points(None, None);
                let anchor_x = event.state.logical_point().x;
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
            Event::Pointer(ui_events::pointer::PointerEvent::Gesture(gesture)) => {
                let PointerGesture::Pinch(delta) = gesture.gesture else {
                    return EventPropagation::Continue;
                };
                let factor = (1.0 + f64::from(delta)).max(0.01);
                let anchor_x = gesture.state.logical_point().x;
                self.viewport.zoom_about_view_point(anchor_x, factor);
                self.invalidate_timeline_paint(cx.window_state);
                self.refresh_viewport(cx.window_state);
                EventPropagation::Stop
            }
            _ => EventPropagation::Continue,
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        if self.tracker.is_none() {
            let id = self.id;
            self.tracker = Some(SignalTracker::new(move || {
                id.request_paint();
            }));
        }

        let selected_frame = self
            .tracker
            .as_ref()
            .unwrap()
            .track(|| self.selected_frame.get());
        if self.sync_selected_frame(selected_frame) {
            self.invalidate_timeline_paint(cx.window_state);
            self.refresh_viewport(cx.window_state);
        }

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
            let selected_index = self.last_selected_frame;
            for marker in self.frame_markers.iter() {
                let x = self.viewport.world_to_view_x(marker.start.as_secs_f64());
                if x < 0.0 || x > self.size.width {
                    continue;
                }
                let is_selected = selected_index == Some(marker.index);
                let color = if is_selected {
                    css::TOMATO
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
            return;
        }

        if let Some(&element_idx) = self.element_indices.get(&cx.target_id) {
            let element = &self.elements[element_idx];
            let rect = cx.layout_rect_local.to_rounded_rect(2.0);
            let hovered = self.hovered_element == Some(element.element_id);
            let color = timeline_item_color(&element.item.kind);
            cx.painter
                .fill(
                    rect,
                    &Brush::Solid(color.with_alpha(if hovered { 0.9 } else { 0.7 })),
                )
                .draw();
            let stroke_scale = transform_max_scale(cx.world_transform).max(f64::MIN_POSITIVE);
            cx.painter
                .stroke(
                    rect,
                    &Stroke::new(0.5 / stroke_scale),
                    &Brush::Solid(self.palette.border),
                )
                .draw();
        }
    }

    fn post_paint(&mut self, cx: &mut PaintCx) {
        if cx.target_id != self.id.get_element_id() {
            return;
        }

        let inverse = cx.world_transform.inverse();
        let box_tree = self.id.box_tree();
        let box_tree = box_tree.borrow();

        for element in &self.elements {
            let Some(world_rect) = box_tree.world_bounds(element.element_id.0) else {
                continue;
            };
            let rect = inverse.transform_rect_bbox(world_rect);
            if rect.width() < 10.0 || rect.x1 <= 0.0 || rect.x0 >= self.size.width {
                continue;
            }
            let text_origin = Point::new(
                rect.x0 + 6.0,
                rect.y0 + (TIMELINE_LANE_HEIGHT - element.label_layout.size().height) * 0.5,
            );
            cx.painter.with_fill_clip(rect, |p| {
                element
                    .label_layout
                    .draw_with_painter(p.as_dyn(), text_origin, cx.font_embolden);
            });
        }
    }
}

fn info(name: impl Display, value: String) -> impl IntoView {
    info_row(name.to_string(), Label::new(value))
}

fn info_row(name: String, view: impl IntoView + 'static) -> impl IntoView {
    Stack::new((
        Label::new(name)
            .style(|s| {
                s.margin_right(5.0)
                    .with_theme(|s, t| s.color(t.text_muted()))
            })
            .container()
            .style(|s| s.min_width(80.0).flex_direction(FlexDirection::RowReverse)),
        view,
    ))
    .style(|s| {
        s.padding(5.0)
            .with_theme(|s, t| s.hover(|s| s.background(t.bg_elevated())))
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
    let selected_frame = RwSignal::new(None::<Rc<ProfileFrameData>>);
    let frame_views: Vec<_> = frames
        .iter()
        .map(|frame| {
            let frame = frame.clone();
            Stack::horizontal((
                Label::new(format!("Frame #{}", frame.index)).style(|s| s.flex_grow(1.0)),
                Label::new(format!("{:.4} ms", frame.start.as_secs_f64() * 1000.0))
                    .style(|s| s.margin_right(16)),
                Label::new(format!("{} ev", frame.event_count)).style(|s| s.margin_right(16)),
                Label::new(format!("{:.4} ms", frame.sum.as_secs_f64() * 1000.0))
                    .style(|s| s.margin_right(16)),
            ))
            .style(move |s| s.selectable(false).padding(5.0))
        })
        .collect();

    let hovered_event: RwSignal<Option<TimelineItem>> = RwSignal::new(None);

    let event_tooltip = dyn_container(
        move || hovered_event.get(),
        move |hovered_event| {
            if let Some(event) = hovered_event {
                let len = event.duration.as_secs_f64();
                Stack::vertical((
                    info("Name", event.label).style(|s| s.width_full()),
                    info("Source", event.source.to_string()).style(|s| s.width_full()),
                    info("Depth", event.depth.to_string()).style(|s| s.width_full()),
                    info(
                        "Start",
                        format!("{:.4} ms", event.start.as_secs_f64() * 1000.0),
                    )
                    .style(|s| s.width_full()),
                    info("Time", format!("{:.4} ms", len * 1000.0)).style(|s| s.width_full()),
                ))
                .style(|s| s.width_full())
                .into_any()
            } else {
                Label::new("No hovered event")
                    .style(|s| s.padding(5.0).width_full())
                    .into_any()
            }
        },
    )
    .style(|s| s.width_full());
    let frames_clone = frames.clone();

    let frames = Stack::vertical((
        header("Frames"),
        Scroll::new(
            list(frame_views)
                .on_select(move |idx| {
                    let frame = idx.and_then(|idx| frames_clone.get(idx)).cloned();
                    selected_frame.set(frame);
                })
                .style(|s| s.width_full()),
        )
        .style(|s| {
            s.flex_basis(0)
                .min_height(0)
                .flex_grow(1.0)
                .with_theme(|s, t| s.background(t.bg_base()))
        }),
        header("Event").style(|s| {
            s.border_top(1)
                .with_theme(|s, t| s.border_top_color(t.border()).color(t.primary()))
        }),
        event_tooltip,
    ))
    .style(|s| s.min_width(230.0).flex_grow(1.));

    let timeline = if lanes.is_empty() {
        Label::new("No timeline events")
            .style(|s| {
                s.width_full()
                    .min_height(0)
                    .flex_basis(0)
                    .flex_grow(1.0)
                    .padding(5.0)
            })
            .into_any()
    } else {
        ProfilerTimelineView::new(
            lanes,
            frame_markers,
            total_duration,
            hovered_event,
            selected_frame,
        )
        .style(|s| s.size_full())
        .into_any()
    };

    let timeline = Stack::vertical((header("Timeline"), timeline))
        .style(|s| s.min_width(0).flex_basis(0).flex_grow(1.0));

    Resizable::new((frames, timeline)).style(|s| s.height_full().width_full().max_width_full())
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
