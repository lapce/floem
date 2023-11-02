use crate::app::{add_app_update_event, AppUpdateEvent};
use crate::context::{AppState, LayoutCx};
use crate::event::{Event, EventListener};
use crate::id::Id;
use crate::new_window;
use crate::style::{Style, StyleMapValue, TextOverflow};
use crate::view::View;
use crate::views::{
    dyn_container, empty, img_dynamic, list, scroll, stack, text, Decorators, Label,
};
use crate::window::WindowConfig;
use floem_reactive::{create_rw_signal, RwSignal, Scope};
use image::DynamicImage;
use kurbo::{Point, Rect, Size};
use peniko::Color;
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Display;
use std::rc::Rc;
use std::time::{Duration, Instant};
use taffy::style::{AlignItems, FlexDirection};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

#[derive(Clone, Debug)]
pub struct CapturedView {
    id: Id,
    name: String,
    layout: Rect,
    clipped: Rect,
    children: Vec<Rc<CapturedView>>,
}

impl CapturedView {
    pub fn capture(view: &dyn View, app_state: &mut AppState, clip: Rect) -> Self {
        let layout = app_state.get_layout_rect(view.id());
        let clipped = layout.intersect(clip);
        Self {
            id: view.id(),
            name: view.debug_name().to_string(),
            layout,
            clipped,
            children: view
                .children()
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
}

pub struct Capture {
    pub root: CapturedView,
    pub start: Instant,
    pub post_layout: Instant,
    pub end: Instant,
    pub taffy_duration: Duration,
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
    pub(crate) fn capture_style(id: Id, cx: &mut LayoutCx) {
        if cx.app_state_mut().capture.is_some() {
            let direct = cx.style.direct.clone();
            let mut current = (*cx.style.current).clone();
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

pub fn captured_view(
    view: &CapturedView,
    depth: usize,
    selected: RwSignal<Option<Id>>,
    highlighted: RwSignal<Option<Id>>,
) -> Box<dyn View> {
    let offset = depth as f64 * 14.0;
    let name = text(view.name.clone()).style(|s| s.text_overflow(TextOverflow::Ellipsis));
    let height = 20.0;
    let id = view.id;

    if view.children.is_empty() {
        return Box::new(
            name.style(move |s| {
                s.width_full()
                    .text_overflow(TextOverflow::Ellipsis)
                    .padding_left(20.0 + offset)
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
            .on_click(move |_| {
                selected.set(Some(id));
                true
            })
            .on_event(EventListener::PointerEnter, move |_| {
                highlighted.set(Some(id));
                false
            }),
        );
    }

    let expanded = create_rw_signal(true);

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
            .on_click(move |_| {
                expanded.set(!expanded.get());
                true
            }),
        name,
    ))
    .style(move |s| {
        s.width_full()
            .padding_left(3.0)
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
    .on_click(move |_| {
        selected.set(Some(id));
        true
    })
    .on_event(EventListener::PointerEnter, move |_| {
        highlighted.set(Some(id));
        false
    });

    let children = view.children.clone();

    let line = empty().style(move |s| {
        s.absolute()
            .height_full()
            .width(1.0)
            .margin_left(9.0 + offset)
            .background(Color::BLACK.with_alpha_factor(0.1))
    });

    let list = dyn_container(
        move || expanded.get(),
        move |expanded| {
            if expanded {
                let children = children.clone();
                Box::new(
                    list(
                        move || children.clone().into_iter().enumerate(),
                        |(i, _)| *i,
                        move |(_, child)| captured_view(&child, depth + 1, selected, highlighted),
                    )
                    .style(|s| s.flex_col().width_full()),
                )
            } else {
                Box::new(empty())
            }
        },
    );

    let list = stack((line, list)).style(|s| s.flex_col().width_full());

    Box::new(stack((row, list)).style(|s| s.flex_col().width_full()))
}

fn header(label: impl Display) -> Label {
    text(label).style(|s| {
        s.padding(5.0)
            .background(Color::WHITE_SMOKE)
            .width_full()
            .height(27.0)
            .border_bottom(1.0)
            .border_color(Color::LIGHT_GRAY)
    })
}

fn info(s: String) -> Label {
    text(s).style(|s| s.padding(5.0))
}

fn stats(capture: &Capture) -> impl View {
    let layout_time = capture.post_layout.saturating_duration_since(capture.start);
    let paint_time = capture.end.saturating_duration_since(capture.post_layout);
    let layout_time = info(format!(
        "Layout time: {:.4} ms",
        layout_time.as_secs_f64() * 1000.0
    ));
    let taffy_time = info(format!(
        "Taffy time: {:.4} ms",
        capture.taffy_duration.as_secs_f64() * 1000.0
    ));
    let paint_time = info(format!(
        "Paint time: {:.4} ms",
        paint_time.as_secs_f64() * 1000.0
    ));
    let w = info(format!("Window Width: {}", capture.window_size.width));
    let h = info(format!("Window Height: {}", capture.window_size.height));
    stack((layout_time, taffy_time, paint_time, w, h)).style(|s| s.flex_col())
}

fn selected_view(capture: &Rc<Capture>, selected: RwSignal<Option<Id>>) -> impl View {
    let capture = capture.clone();
    dyn_container(
        move || selected.get(),
        move |current| {
            if let Some(view) = current.and_then(|id| capture.root.find(id)) {
                let name = info(format!("Type: {}", view.name));
                let count = info(format!("Child Count: {}", view.children.len()));
                let beyond = |view: f64, window| {
                    if view > window {
                        format!(" ({} after window edge)", view - window)
                    } else if view < 0.0 {
                        format!(" ({} before window edge)", -view)
                    } else {
                        String::new()
                    }
                };
                let x = info(format!(
                    "X: {}{}",
                    view.layout.x0,
                    beyond(view.layout.x0, capture.window_size.width)
                ));
                let y = info(format!(
                    "Y: {}{}",
                    view.layout.y0,
                    beyond(view.layout.y0, capture.window_size.height)
                ));
                let w = info(format!(
                    "Width: {}{}",
                    view.layout.width(),
                    beyond(view.layout.x1, capture.window_size.width)
                ));
                let h = info(format!(
                    "Height: {}{}",
                    view.layout.height(),
                    beyond(view.layout.y1, capture.window_size.height)
                ));
                let clear = text("Clear selection")
                    .style(|s| {
                        s.background(Color::WHITE_SMOKE)
                            .border(1.0)
                            .padding(5.0)
                            .margin(5.0)
                            .border_color(Color::BLACK.with_alpha_factor(0.4))
                            .border_radius(4.0)
                            .hover(move |s| {
                                s.border_color(Color::BLACK.with_alpha_factor(0.2))
                                    .background(Color::GRAY.with_alpha_factor(0.6))
                            })
                    })
                    .on_click(move |_| {
                        selected.set(None);
                        true
                    });
                let clear = stack((clear,));

                let style_header = header("View Style");

                let mut styles = capture
                    .state
                    .styles
                    .get(&view.id)
                    .cloned()
                    .unwrap_or_default()
                    .map
                    .into_iter()
                    .map(|(p, v)| ((p, format!("{p:?}")), v))
                    .collect::<Vec<_>>();
                styles.sort_unstable_by(|a, b| a.0 .1.cmp(&b.0 .1));

                let style_list = list(
                    move || styles.clone(),
                    |(i, _)| i.0,
                    move |((p, name), v)| {
                        let v: Box<dyn View> = match v {
                            StyleMapValue::Val(v) => {
                                let v = &*v;
                                (p.info.debug_view)(v)
                                    .unwrap_or_else(|| Box::new(text((p.info.debug_any)(v))))
                            }
                            StyleMapValue::Unset => Box::new(text("Unset".to_owned())),
                        };
                        stack((
                            stack((text(name.strip_prefix("floem::style::").unwrap_or(&name))
                                .style(|s| {
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
                                .hover(|s| s.background(Color::rgba8(228, 237, 216, 160)))
                        })
                    },
                )
                .style(|s| s.flex_col().width_full());

                Box::new(
                    stack((name, count, x, y, w, h, clear, style_header, style_list))
                        .style(|s| s.flex_col().width_full()),
                )
            } else {
                Box::new(info("No selection".to_string()))
            }
        },
    )
}

fn capture_view(capture: &Rc<Capture>) -> impl View {
    let selected = create_rw_signal(None);
    let highlighted = create_rw_signal(None);

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
                    highlighted.set(Some(view.id));
                    return false;
                }
            }
            if highlighted.get().is_some() {
                highlighted.set(None);
            }
            false
        })
        .on_click(move |e| {
            if let Event::PointerUp(e) = e {
                if let Some(view) = capture__.root.find_by_pos(e.pos) {
                    selected.set(Some(view.id));
                    return true;
                }
            }
            if selected.get().is_some() {
                selected.set(None);
            }
            true
        })
        .on_event(EventListener::PointerLeave, move |_| {
            highlighted.set(None);
            false
        });

    let capture_ = capture.clone();
    let selected_overlay = empty().style(move |s| {
        if let Some(view) = selected.get().and_then(|id| capture_.root.find(id)) {
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
        if let Some(view) = highlighted.get().and_then(|id| capture_.root.find(id)) {
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
        stack((
            header("Selected View"),
            selected_view(capture, selected),
            header("Stats"),
            stats(capture),
        ))
        .style(|s| s.flex_col().width_full()),
    );

    let left = stack((
        header("Captured Window"),
        scroll(image).style(|s| s.max_height_pct(60.0)),
        left_scroll,
    ))
    .style(|s| s.flex_col().height_full().max_width_pct(60.0));

    let tree = stack((
        header("View Tree"),
        scroll(captured_view(&capture.root, 0, selected, highlighted))
            .style(|s| s.width_full().height_full())
            .on_event(EventListener::PointerLeave, move |_| {
                highlighted.set(None);
                false
            })
            .on_click(move |_| {
                selected.set(None);
                true
            }),
    ))
    .style(|s| s.flex_col().width_full().height_full());

    let seperator = empty().style(move |s| {
        s.height_full()
            .min_width(1.0)
            .background(Color::BLACK.with_alpha_factor(0.2))
    });

    stack((left, seperator, tree)).style(|s| s.flex_row().width_full().height_full())
}

fn inspector_view(capture: &Option<Rc<Capture>>) -> impl View {
    let view: Box<dyn View> = if let Some(capture) = capture {
        Box::new(capture_view(capture))
    } else {
        Box::new(text("No capture"))
    };

    stack((view,))
        .window_title(|| "Floem Inspector".to_owned())
        .on_event(EventListener::WindowClosed, |_| {
            RUNNING.set(false);
            false
        })
        .style(|s| {
            s.font_size(12.0)
                .width_full()
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
                let view = dyn_container(
                    move || capture.get(),
                    |capture| Box::new(inspector_view(&capture)),
                );
                let id = view.id();
                view.style(|s| s.width_full().height_full()).on_event(
                    EventListener::KeyUp,
                    move |e| {
                        if let Event::KeyUp(e) = e {
                            if e.key.logical_key == Key::Named(NamedKey::F11)
                                && e.modifiers.shift_key()
                            {
                                id.inspect();
                                return true;
                            }
                        }
                        false
                    },
                )
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
