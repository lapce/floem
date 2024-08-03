use peniko::kurbo::Point;
use std::cell::RefCell;
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::style::{Style, StyleClass as _};
use crate::views::Decorators;
use crate::{
    action::{add_overlay, exec_after, remove_overlay, TimerToken},
    context::{EventCx, UpdateCx},
    event::{Event, EventPropagation},
    id::ViewId,
    prop, prop_extractor, style_class,
    view::{default_compute_layout, IntoView, View},
};

style_class!(pub TooltipClass);
style_class!(pub TooltipContainerClass);

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
    overlay: Rc<RefCell<Option<ViewId>>>,
    tip: Rc<dyn Fn() -> Box<dyn View>>,
    style: TooltipStyle,
    tip_style: Style,
    scale: f64,
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
    let overlay = Rc::new(RefCell::new(None));
    Tooltip {
        id,
        tip: Rc::new(move || tip().into_any()),
        hover: None,
        overlay: overlay.clone(),
        style: Default::default(),
        tip_style: Default::default(),
        scale: 1.0,
        window_origin: None,
    }
    .class(TooltipContainerClass)
    .on_cleanup(move || {
        if let Some(overlay_id) = overlay.borrow_mut().take() {
            remove_overlay(overlay_id);
        }
    })
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

                    let tip_style = self.tip_style.clone();
                    let overlay_id = add_overlay(
                        window_origin
                            + self.hover.unwrap().0.to_vec2()
                            + (10. / self.scale, 10. / self.scale),
                        move |_| tip().style(move |_| tip_style.clone()),
                    );
                    // overlay_id.request_all();
                    *self.overlay.borrow_mut() = Some(overlay_id);
                }
            }
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        self.style.read(cx);
        self.scale = cx.app_state.scale;

        self.tip_style =
            Style::new().apply_classes_from_context(&[TooltipClass::class_ref()], &cx.current);

        for child in self.id.children() {
            cx.style_view(child);
        }
    }

    fn event_before_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        match &event {
            Event::PointerMove(e) => {
                if self.overlay.borrow().is_none() && cx.app_state.dragging.is_none() {
                    let id = self.id();
                    let token =
                        exec_after(Duration::from_secs_f64(self.style.delay()), move |token| {
                            id.update_state(token);
                        });
                    self.hover = Some((e.pos, token));
                }
            }
            Event::PointerLeave
            | Event::PointerDown(_)
            | Event::PointerUp(_)
            | Event::PointerWheel(_)
            | Event::KeyUp(_)
            | Event::KeyDown(_) => {
                self.hover = None;
                if let Some(id) = self.overlay.borrow_mut().take() {
                    remove_overlay(id);
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

pub trait TooltipTrait {
    fn tooltip<V: IntoView + 'static>(self, tip: impl Fn() -> V + 'static) -> Tooltip;
}

impl<T: View + 'static> TooltipTrait for T {
    fn tooltip<V: IntoView + 'static>(self, tip: impl Fn() -> V + 'static) -> Tooltip {
        tooltip(self, tip)
    }
}
