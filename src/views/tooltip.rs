#![deny(missing_docs)]
use peniko::kurbo::Point;
use std::cell::RefCell;
use std::rc::Rc;
use ui_events::pointer::PointerEvent;

use crate::{
    action::{TimerToken, add_overlay, exec_after, remove_overlay},
    context::{EventCx, UpdateCx},
    event::{Event, EventPropagation, Phase},
    platform::Duration,
    prop, prop_extractor, style_class,
    view::{IntoView, View, ViewId},
    views::Decorators,
};

style_class!(
    /// A class for the tooltip views.
    pub TooltipClass
);
style_class!(
    /// A class for the tooltip container view.
    pub TooltipContainerClass
);

prop!(pub Delay: Duration {} = Duration::from_millis(600));

prop_extractor! {
    TooltipStyle {
        delay: Delay,
    }
}

/// A view that displays a tooltip for its child.
pub struct Tooltip {
    id: ViewId,
    /// Holds the hover point and time token needed to
    /// evaluate if - or when and where - display tooltip.
    hover_point: Option<(Point, TimerToken)>,
    /// Tooltip overlay view id.
    overlay: Rc<RefCell<Option<ViewId>>>,
    /// Provided by user function that dislays tooltip content.
    tip: Rc<dyn Fn() -> Box<dyn View>>,
    /// A tooltip specific styles (currently its just a delay).
    style: TooltipStyle,
}

/// A view that displays a tooltip for its child.
pub fn tooltip<V: IntoView + 'static, T: IntoView + 'static>(
    child: V,
    tip: impl Fn() -> T + 'static,
) -> Tooltip {
    let id = ViewId::new();
    let child = child.into_view();
    id.set_children([child]);
    let overlay = Rc::new(RefCell::new(None));
    Tooltip {
        id,
        tip: Rc::new(move || tip().into_any()),
        hover_point: None,
        overlay: overlay.clone(),
        style: Default::default(),
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
        if let Ok(token) = state.downcast::<TimerToken>()
            && self.hover_point.map(|(_, t)| t) == Some(*token)
        {
            let point =
                self.id.get_visual_origin() + self.hover_point.unwrap().0.to_vec2() + (0., 10.);
            let overlay_id = add_overlay(
                (self.tip)()
                    .class(TooltipClass)
                    .style(move |s| s.inset_left(point.x).inset_top(point.y)),
            );
            overlay_id.set_style_parent(self.id);
            *self.overlay.borrow_mut() = Some(overlay_id);
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let mut transitioning = false;
        self.style.read(cx, &mut transitioning);
        if self.overlay.borrow().is_some() && self.id.is_hidden() {
            let id = self.overlay.take().unwrap();
            self.hover_point = None;
            remove_overlay(id);
        }
        if transitioning {
            cx.request_transition();
        }
    }

    fn event_capture(&mut self, cx: &mut EventCx) -> EventPropagation {
        self.handle_event(cx)
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if cx.phase != Phase::Target {
            return EventPropagation::Continue;
        }
        self.handle_event(cx)
    }
}

impl Tooltip {
    fn handle_event(&mut self, cx: &mut EventCx) -> EventPropagation {
        match &cx.event {
            Event::Pointer(PointerEvent::Move(pu)) if self.overlay.borrow().is_none() => {
                let id = self.id();
                let token = exec_after(self.style.delay(), move |token| {
                    id.update_state(token);
                });
                self.hover_point = Some((pu.current.logical_point(), token));
            }
            Event::Pointer(_) | Event::Key(_) => {
                self.hover_point = None;
                if let Some(id) = self.overlay.borrow_mut().take() {
                    remove_overlay(id);
                }
            }
            _ => {}
        }
        EventPropagation::Continue
    }
}

/// Adds a [tooltip] function to a type that implements [`IntoView`].
pub trait TooltipExt {
    /// Adds a tooltip to the view.
    ///
    /// ### Examples
    /// ```rust
    /// # use floem::views::TooltipExt;
    /// # use floem::views::{text, Decorators};
    /// # use floem::prelude::{RwSignal, SignalGet};
    /// // Simple usage:
    /// let simple = text("A text with tooltip")
    ///     .tooltip(|| "This is a tooltip.");
    /// // More complex usage:
    /// let mut click_counter = RwSignal::new(0);
    /// let complex = text("A text with a tooltip that changes on click")
    ///     .on_click_stop(move|_| click_counter += 1)
    ///     .tooltip(move || format!("Clicked {} times on the label", click_counter.get()));
    /// ```
    /// ### Reactivity
    /// This function is not reactive, but it is computing its result on every tooltip trigger.
    /// It is possible then to have different tooltip output, but the output it will **not** change
    /// once while displaying a hover.
    fn tooltip<V: IntoView + 'static>(self, tip: impl Fn() -> V + 'static) -> Tooltip;
}

impl<T: IntoView + 'static> TooltipExt for T {
    fn tooltip<V: IntoView + 'static>(self, tip: impl Fn() -> V + 'static) -> Tooltip {
        tooltip(self, tip)
    }
}
