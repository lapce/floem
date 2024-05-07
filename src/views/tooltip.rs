use peniko::kurbo::Point;
use std::{rc::Rc, time::Duration};

use crate::{
    action::{add_overlay, exec_after, remove_overlay, TimerToken},
    context::{EventCx, UpdateCx},
    event::{Event, EventPropagation},
    id::ViewId,
    prop, prop_extractor, style_class,
    view::{default_compute_layout, IntoView, View},
};

use super::{container, Decorators};

style_class!(pub TooltipClass);

prop!(pub Delay: f64 {} = 0.6);

prop_extractor! {
    TooltipStyle {
        delay: Delay,
    }
}

/// A view that displays a tooltip for its child.
pub struct Tooltip {
    id: ViewId,
    hover: Option<(Point, TimerToken)>,
    overlay: Option<ViewId>,
    tip: Rc<dyn Fn() -> Box<dyn View>>,
    style: TooltipStyle,
    window_origin: Option<Point>,
}

/// A view that displays a tooltip for its child.
pub fn tooltip<V: IntoView + 'static, T: IntoView + 'static>(
    child: V,
    tip: impl Fn() -> T + 'static,
) -> Tooltip {
    let id = ViewId::new();
    let child = child.into_view();
    id.set_children(vec![child]);
    Tooltip {
        id,
        tip: Rc::new(move || container(tip()).class(TooltipClass).into_any()),
        hover: None,
        overlay: None,
        style: Default::default(),
        window_origin: None,
    }
}

impl View for Tooltip {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(token) = state.downcast::<TimerToken>() {
            if let Some(window_origin) = self.window_origin {
                if self.hover.map(|(_, t)| t) == Some(*token) {
                    let tip = self.tip.clone();
                    self.overlay = Some(add_overlay(
                        window_origin + self.hover.unwrap().0.to_vec2() + (10., 10.),
                        move |_| tip(),
                    ));
                }
            }
        }
    }

    fn event_before_children(&mut self, _cx: &mut EventCx, event: &Event) -> EventPropagation {
        match &event {
            Event::PointerMove(e) => {
                if self.overlay.is_none() {
                    let id = self.id();
                    let token =
                        exec_after(Duration::from_secs_f64(self.style.delay()), move |token| {
                            id.update_state(token);
                        });
                    self.hover = Some((e.pos, token));
                }
            }
            Event::PointerLeave => {
                self.hover = None;
                if let Some(id) = self.overlay {
                    remove_overlay(id);
                    self.overlay = None;
                }
            }
            _ => {}
        }
        EventPropagation::Continue
    }

    fn compute_layout(
        &mut self,
        cx: &mut crate::context::ComputeLayoutCx,
    ) -> Option<peniko::kurbo::Rect> {
        self.window_origin = Some(cx.window_origin);
        default_compute_layout(self.id, cx)
    }
}

impl Drop for Tooltip {
    fn drop(&mut self) {
        if let Some(id) = self.overlay {
            remove_overlay(id)
        }
    }
}
