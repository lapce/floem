#![deny(missing_docs)]
use peniko::kurbo::Point;
use std::cell::RefCell;
use std::rc::Rc;
use ui_events::pointer::PointerEvent;

use crate::platform::Duration;

use crate::style::{Style, StyleClass};
use crate::views::Decorators;
use crate::{
    action::{TimerToken, add_overlay, exec_after, remove_overlay},
    context::{EventCx, UpdateCx},
    event::{Event, EventPropagation},
    prop, prop_extractor, style_class,
    view::ViewId,
    view::{IntoView, View, default_compute_layout},
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
    hover: Option<(Point, TimerToken)>,
    /// Tooltip overlay view id.
    overlay: Rc<RefCell<Option<ViewId>>>,
    /// Provided by user function that dislays tooltip content.
    tip: Rc<dyn Fn() -> Box<dyn View>>,
    /// A tooltip specific styles (currently its just a delay).
    style: TooltipStyle,
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
    id.set_children([child]);
    let overlay = Rc::new(RefCell::new(None));
    Tooltip {
        id,
        tip: Rc::new(move || tip().into_any()),
        hover: None,
        overlay: overlay.clone(),
        style: Default::default(),
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
        if let Ok(token) = state.downcast::<TimerToken>()
            && let Some(window_origin) = self.window_origin
            && self.hover.map(|(_, t)| t) == Some(*token)
        {
            let tip = self.tip.clone();

            let point = window_origin
                + self.hover.unwrap().0.to_vec2()
                + (10. / self.scale, 10. / self.scale);
            let overlay_id = add_overlay(
                ToolTipOverlay::new(tip()).style(move |s| s.inset_left(point.x).inset_top(point.y)),
            );
            overlay_id.set_style_parent(self.id);
            *self.overlay.borrow_mut() = Some(overlay_id);
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        self.style.read(cx);
        self.scale = cx.window_state.scale;
    }

    fn event_before_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
        match &event {
            Event::Pointer(PointerEvent::Move(pu)) => {
                if self.overlay.borrow().is_none() && cx.window_state.dragging.is_none() {
                    let id = self.id();
                    let token = exec_after(self.style.delay(), move |token| {
                        id.update_state(token);
                    });
                    self.hover = Some((pu.current.logical_point(), token));
                }
            }
            Event::Pointer(_) | Event::Key(_) => {
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

struct ToolTipOverlay {
    id: ViewId,
    offset: Point,
}

impl ToolTipOverlay {
    fn new<V: IntoView + 'static>(child: V) -> Self {
        let id = ViewId::new();
        let child = child.into_view();
        id.set_children([child]);

        Self {
            id,
            offset: Point::ZERO,
        }
    }
}

impl View for ToolTipOverlay {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<Style> {
        Some(
            Style::new()
                .translate_x(self.offset.x)
                .translate_y(self.offset.y),
        )
    }

    fn view_class(&self) -> Option<crate::style::StyleClassRef> {
        Some(TooltipClass::class_ref())
    }

    fn compute_layout(
        &mut self,
        cx: &mut crate::context::ComputeLayoutCx,
    ) -> Option<peniko::kurbo::Rect> {
        if let (Some(parent_size), Some(layout)) = (self.id.parent_size(), self.id.get_layout()) {
            use crate::kurbo::Size;

            let bottom_right = taffy::Size::from(layout.location) + layout.size;
            let bottom_right = Size::new(bottom_right.width as _, bottom_right.height as _);

            let new_offset = (parent_size - bottom_right).min(Size::ZERO);
            let new_offset = Point::new(new_offset.width, new_offset.height);

            if self.offset != new_offset {
                self.offset = new_offset;
                self.id().request_style();
            }
        }

        default_compute_layout(self.id, cx)
    }
}
