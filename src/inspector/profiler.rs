use super::{TimingKind, TimingReport, header};
use crate::app::{AppUpdateEvent, add_app_update_event};
use crate::context::VisualChanged;
use crate::event::{EventPropagation, PointerScrollEventExt, listener};
use crate::prelude::palette::css;
use crate::style::CustomStylable;
use crate::theme::StyleThemeExt;
use crate::ui_events::pointer::PointerGesture;
use crate::view::IntoView;
use crate::views::resizable::Resizable;
use crate::views::{
    Button, Clip, Container, ContainerExt, Decorators, Label, Scroll, Stack, dyn_container, list,
};
use floem_reactive::{RwSignal, Scope, SignalGet, SignalUpdate};
use peniko::Color;
use std::cell::RefCell;
use std::fmt::Display;
use std::mem;
use std::rc::Rc;
use taffy::Overflow;
use taffy::style::FlexDirection;
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
    frames: Vec<ProfileFrame>,
}

impl Profile {
    pub fn next_frame(&mut self) {
        self.frames.push(mem::take(&mut self.current));
    }
}

struct ProfileFrameData {
    start: Option<Instant>,
    duration: Duration,
    sum: Duration,
    events: Vec<ProfileEvent>,
    timing: Option<TimingReport>,
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

fn build_timeline_lanes(frame: &ProfileFrameData) -> Vec<Vec<TimelineItem>> {
    let mut items = Vec::new();

    if let Some(frame_start) = frame.start {
        items.extend(frame.events.iter().map(|event| TimelineItem {
            label: event.name.to_string(),
            source: "Profiler Event",
            start: event.start.saturating_duration_since(frame_start),
            duration: event.end.saturating_duration_since(event.start),
            depth: event.depth,
            kind: TimelineItemKind::Event,
        }));
    }

    if let Some(report) = &frame.timing {
        items.extend(report.spans.iter().map(|span| TimelineItem {
            label: span.label.to_string(),
            source: "Timing Span",
            start: span.start,
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
    let mut frames: Vec<_> = profile
        .frames
        .iter()
        .map(|frame| {
            let start = frame.events.first().map(|event| event.start);
            let end = frame.events.last().map(|event| event.end);
            let sum = frame
                .events
                .iter()
                .map(|event| event.end.saturating_duration_since(event.start))
                .sum();
            let duration = start
                .and_then(|start| end.map(|end| end.saturating_duration_since(start)))
                .unwrap_or_default();
            Rc::new(ProfileFrameData {
                start,
                duration,
                sum,
                events: frame.events.clone(),
                timing: frame.timing.clone(),
            })
        })
        .collect();
    frames.sort_by(|a, b| b.sum.cmp(&a.sum));
    let frames: Rc<[_]> = frames.into();

    let selected_frame = RwSignal::new(None);
    let frame_views: Vec<_> = frames
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, frame)| {
            let frame = frame.clone();
            Stack::horizontal((
                Label::new(format!("Frame #{i}")).style(|s| s.flex_grow(1.0)),
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
                    selected_frame.set(idx.and_then(|idx| frames_clone.get(idx)).cloned())
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

    let timeline = dyn_container(
        move || selected_frame.get(),
        move |selected_frame| {
            if let Some(frame) = selected_frame {
                let lanes = Rc::new(build_timeline_lanes(&frame));
                if lanes.is_empty() {
                    Label::new("No frame events")
                        .style(|s| s.padding(5.0))
                        .into_any()
                } else {
                    let viewport = Rc::new(RefCell::new(Viewport1D::new(0.0..1.0)));
                    let viewport_rev = RwSignal::new(0_u64);
                    let viewport_initialized = RwSignal::new(false);
                    {
                        let mut viewport = viewport.borrow_mut();
                        viewport.set_world_bounds(Some(
                            0.0..frame.duration.as_secs_f64().max(f64::MIN_POSITIVE),
                        ));
                        viewport.set_zoom_limits(1e-6, 1e12);
                    }

                    let timeline_rows = dyn_container(move || viewport_rev.get(), {
                        let lanes = lanes.clone();
                        let viewport = viewport.clone();
                        move |_| {
                            let lane_rows = lanes.iter().enumerate().map(|(lane_idx, lane)| {
                                let viewport = viewport.borrow();
                                Clip::new(Stack::from_iter(lane.iter().map(|item| {
                                    let item = item.clone();
                                    let left = viewport.world_to_view_x(item.start.as_secs_f64());
                                    let right = viewport.world_to_view_x(
                                        item.start.as_secs_f64() + item.duration.as_secs_f64(),
                                    );
                                    let width = (right - left).abs().max(1.0);
                                    let color = timeline_item_color(&item.kind);
                                    let item_ = item.clone();
                                    Clip::new(Label::new(item.label.clone()).style(move |s| {
                                        s.selectable(false)
                                            .padding_horiz(6.0)
                                            .padding_vert(2.0)
                                            .padding_left(6.0 + item.depth as f64 * 8.0)
                                            .text_clip()
                                            .min_width(0.0)
                                            .font_size(11.0)
                                    }))
                                    .style(move |s| {
                                        s.min_width(1.0)
                                            .height(18.0)
                                            .width(width)
                                            .absolute()
                                            .inset_left(left.min(right))
                                            .border(0.5)
                                            .border_radius(2.0)
                                            .text_clip()
                                            .background(color.with_alpha(0.7))
                                            .hover(move |s| s.background(color.with_alpha(0.9)))
                                            .with_theme(|s, t| s.border_color(t.border()))
                                    })
                                    .on_event_cont(listener::PointerEnter, move |_, _| {
                                        hovered_event.set(Some(item_.clone()))
                                    })
                                    .debug_name(format!("Profiler Timeline Item Lane {lane_idx}"))
                                })))
                                .style(|s| {
                                    s.height(18.0)
                                        .width_full()
                                        .border_radius(4.0)
                                        .with_theme(|s, t| s.background(t.bg_elevated()))
                                })
                                .debug_name(format!("Profiler Timeline Lane {lane_idx}"))
                            });

                            Stack::vertical_from_iter(lane_rows)
                                .style(|s| s.width_full().gap(4.0).padding(8.0))
                                .into_any()
                        }
                    })
                    .style(|s| s.width_full().min_width(0.0).flex_grow(1.0));

                    Scroll::new(timeline_rows)
                        .custom_style(|s| s.vertical_track_inset(5.).show_bars_when_idle(false))
                        .style(|s| {
                            s.height_full()
                                .min_width(0)
                                .flex_basis(0)
                                .flex_grow(1.0)
                                .overflow_x(Overflow::Clip)
                                .overflow_y(Overflow::Scroll)
                        })
                        .on_event_stop(VisualChanged::listener(), {
                            let viewport = viewport.clone();
                            move |_cx, change| {
                                let width = change.new_visual_aabb.width().max(1.0);
                                let mut viewport = viewport.borrow_mut();
                                viewport.set_view_span(0.0..width);
                                if !viewport_initialized.get_untracked() {
                                    viewport.fit_world();
                                    viewport_initialized.set(true);
                                }
                                viewport_rev.update(|rev| *rev += 1);
                            }
                        })
                        .on_event(listener::PointerWheel, {
                            let viewport = viewport.clone();
                            move |_cx, se| {
                                let delta = se.resolve_to_points(None, None);
                                let anchor_x = se.state.logical_point().x;
                                let mut viewport = viewport.borrow_mut();
                                let mut changed = false;

                                if delta.x.abs() > f64::EPSILON {
                                    viewport.pan_by_view(delta.x);
                                    changed = true;
                                }

                                if delta.y.abs() > f64::EPSILON {
                                    let factor = (1.0 - delta.y / 400.0).max(0.01);
                                    viewport.zoom_about_view_point(anchor_x, factor);
                                    changed = true;
                                }

                                if changed {
                                    viewport_rev.update(|rev| *rev += 1);
                                    EventPropagation::Stop
                                } else {
                                    EventPropagation::Continue
                                }
                            }
                        })
                        .on_event(listener::PinchGesture, {
                            let viewport = viewport.clone();
                            move |_cx, gesture| {
                                let PointerGesture::Pinch(delta) = gesture.gesture else {
                                    return EventPropagation::Continue;
                                };
                                let anchor_x = gesture.state.logical_point().x;
                                let factor = (1.0 + f64::from(delta)).max(0.01);
                                let mut viewport = viewport.borrow_mut();
                                viewport.zoom_about_view_point(anchor_x, factor);
                                viewport_rev.update(|rev| *rev += 1);
                                EventPropagation::Stop
                            }
                        })
                        .into_any()
                }
            } else {
                Label::new("No selected frame")
                    .style(|s| s.padding(5.0))
                    .into_any()
            }
        },
    )
    .style(|s| {
        s.width_full()
            .min_height(0)
            .flex_basis(0)
            .flex_grow(1.0)
            .with_theme(|s, t| s.background(t.bg_base()))
    });

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

    // FIXME: This needs an extra `container` or the `Stack::vertical` ends up horizontal.
    Container::new(Stack::vertical((button, separator, lower)).style(|s| s.size_full()))
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
