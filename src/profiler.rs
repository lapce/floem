use crate::app::{AppUpdateEvent, add_app_update_event};
use crate::event::{Event, EventListener, EventPropagation};
use crate::inspector::header;
use crate::view::IntoView;
use crate::views::{
    Decorators, button, clip, container, dyn_container, empty, h_stack, label, scroll, stack,
    static_label, text, v_stack, v_stack_from_iter,
};
use floem_reactive::{RwSignal, Scope, SignalGet, SignalUpdate, create_rw_signal};
use peniko::Color;
use peniko::color::palette;
use std::fmt::Display;
use std::mem;
use std::rc::Rc;
use taffy::style::FlexDirection;
use ui_events::pointer::PointerEvent;
use winit::window::WindowId;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

#[derive(Clone)]
pub struct ProfileEvent {
    pub start: Instant,
    pub end: Instant,
    pub name: &'static str,
}

#[derive(Default)]
pub struct ProfileFrame {
    pub events: Vec<ProfileEvent>,
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
}

fn info(name: impl Display, value: String) -> impl IntoView {
    info_row(name.to_string(), static_label(value))
}

fn info_row(name: String, view: impl IntoView + 'static) -> impl IntoView {
    stack((
        stack((static_label(name).style(|s| {
            s.margin_right(5.0)
                .color(palette::css::BLACK.with_alpha(0.6))
        }),))
        .style(|s| s.min_width(80.0).flex_direction(FlexDirection::RowReverse)),
        view,
    ))
    .style(|s| {
        s.padding(5.0)
            .hover(|s| s.background(Color::from_rgba8(228, 237, 216, 160)))
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
            })
        })
        .collect();
    frames.sort_by(|a, b| b.sum.cmp(&a.sum));

    let selected_frame = create_rw_signal(None);

    let zoom = create_rw_signal(1.0);

    let frames: Vec<_> = frames
        .iter()
        .enumerate()
        .map(|(i, frame)| {
            let frame = frame.clone();
            let frame_ = frame.clone();
            h_stack((
                static_label(format!("Frame #{i}")).style(|s| s.flex_grow(1.0)),
                static_label(format!("{:.4} ms", frame.sum.as_secs_f64() * 1000.0))
                    .style(|s| s.margin_right(16)),
            ))
            .on_click_stop(move |_| {
                selected_frame.set(Some(frame.clone()));
                zoom.set(1.0);
            })
            .style(move |s| {
                let selected = selected_frame
                    .get()
                    .map(|selected| Rc::ptr_eq(&selected, &frame_))
                    .unwrap_or(false);
                s.padding(5.0)
                    .apply_if(selected, |s| s.background(Color::from_rgb8(213, 208, 216)))
                    .hover(move |s| {
                        s.background(Color::from_rgba8(228, 237, 216, 160))
                            .apply_if(selected, |s| s.background(Color::from_rgb8(186, 180, 216)))
                    })
            })
        })
        .collect();

    let hovered_event: RwSignal<Option<ProfileEvent>> = create_rw_signal(None);

    let event_tooltip = dyn_container(
        move || hovered_event.get(),
        move |hovered_event| {
            if let Some(event) = hovered_event {
                let len = event
                    .end
                    .saturating_duration_since(event.start)
                    .as_secs_f64();
                v_stack((
                    info("Name", event.name.to_string()),
                    info("Time", format!("{:.4} ms", len * 1000.0)),
                ))
                .into_any()
            } else {
                text("No hovered event")
                    .style(|s| s.padding(5.0))
                    .into_any()
            }
        },
    )
    .style(|s| s.min_height(50));

    let frames = v_stack((
        header("Frames"),
        scroll(v_stack_from_iter(frames).style(|s| s.width_full())).style(|s| {
            s.background(palette::css::WHITE)
                .flex_basis(0)
                .min_height(0)
                .flex_grow(1.0)
        }),
        header("Event"),
        event_tooltip,
    ))
    .style(|s| s.max_width_pct(60.0).min_width(200.0));

    let separator = empty().style(move |s| {
        s.height_full()
            .min_width(1.0)
            .background(palette::css::BLACK.with_alpha(0.2))
    });

    let timeline = dyn_container(
        move || selected_frame.get(),
        move |selected_frame| {
            if let Some(frame) = selected_frame {
                let list = frame.events.iter().map(|event| {
                    let len = event
                        .end
                        .saturating_duration_since(event.start)
                        .as_secs_f64();
                    let left = event
                        .start
                        .saturating_duration_since(frame.start.unwrap())
                        .as_secs_f64()
                        / frame.duration.as_secs_f64();
                    let width = len / frame.duration.as_secs_f64();
                    let event_ = event.clone();
                    clip(
                        static_label(format!("{} ({:.4} ms)", event.name, len * 1000.0))
                            .style(|s| s.padding(5.0)),
                    )
                    .style(move |s| {
                        s.min_width(0)
                            .width_pct(width * 100.0)
                            .absolute()
                            .inset_left_pct(left * 100.0)
                            .border(0.3)
                            .border_color(Color::from_rgb8(129, 164, 192))
                            .background(Color::from_rgb8(209, 222, 233).with_alpha(0.6))
                            .text_clip()
                            .hover(|s| {
                                s.color(palette::css::WHITE)
                                    .background(palette::css::BLACK.with_alpha(0.6))
                            })
                    })
                    .on_event_cont(EventListener::PointerEnter, move |_| {
                        hovered_event.set(Some(event_.clone()))
                    })
                });
                scroll(
                    v_stack_from_iter(list)
                        .style(move |s| s.min_width_pct(zoom.get() * 100.0).height_full()),
                )
                .style(|s| s.height_full().min_width(0).flex_basis(0).flex_grow(1.0))
                .on_event(EventListener::PointerWheel, move |e| {
                    if let Event::Pointer(PointerEvent::Scroll { delta, .. }) = e {
                        zoom.set(zoom.get() * (1.0 - delta.y() / 400.0));
                        EventPropagation::Stop
                    } else {
                        EventPropagation::Continue
                    }
                })
                .into_any()
            } else {
                text("No selected frame")
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
            .background(palette::css::WHITE)
    });

    let timeline = v_stack((header("Timeline"), timeline))
        .style(|s| s.min_width(0).flex_basis(0).flex_grow(1.0));

    h_stack((frames, separator, timeline)).style(|s| s.height_full().width_full().max_width_full())
}

thread_local! {
    pub(crate) static PROFILE: RwSignal<Option<Rc<Profile>>> = {
        Scope::new().create_rw_signal(None)
    };
}

pub fn profiler(window_id: WindowId) -> impl IntoView {
    let profiling = create_rw_signal(false);
    let profile = PROFILE.with(|c| *c);

    let button = h_stack((
        button(label(move || {
            if profiling.get() {
                "Stop Profiling"
            } else {
                "Start Profiling"
            }
        }))
        .on_click_stop(move |_| {
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
        label(move || if profiling.get() { "Profiling..." } else { "" }),
    ))
    .style(|s| s.items_center());

    let separator = empty().style(move |s| {
        s.width_full()
            .min_height(1.0)
            .background(palette::css::BLACK.with_alpha(0.2))
    });

    let lower = dyn_container(
        move || profile.get(),
        move |profile| {
            if let Some(profile) = profile {
                profile_view(&profile).into_any()
            } else {
                text("No profile").style(|s| s.padding(5.0)).into_any()
            }
        },
    )
    .style(|s| s.width_full().min_height(0).flex_basis(0).flex_grow(1.0));

    // FIXME: This needs an extra `container` or the `v_stack` ends up horizontal.
    container(v_stack((button, separator, lower)).style(|s| s.width_full().height_full()))
        .style(|s| s.width_full().height_full())
        .on_event_cont(EventListener::WindowClosed, move |_| {
            if profiling.get() {
                add_app_update_event(AppUpdateEvent::ProfileWindow {
                    window_id,
                    end_profile: Some(profile.write_only()),
                });
            }
        })
}
