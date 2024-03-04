use kurbo::Point;
use std::{rc::Rc, time::Duration};

use crate::{
    action::{add_overlay, exec_after, remove_overlay, TimerToken},
    context::{EventCx, UpdateCx},
    event::Event,
    id::Id,
    prop, prop_extractor,
    view::{default_compute_layout, default_event, View, ViewData, Widget},
    EventPropagation,
};

prop!(pub Delay: f64 {} = 0.6);

prop_extractor! {
    TooltipStyle {
        delay: Delay,
    }
}

/// A view that displays a tooltip for its child.
pub struct Tooltip {
    data: ViewData,
    hover: Option<(Point, TimerToken)>,
    overlay: Option<Id>,
    child: Box<dyn Widget>,
    tip: Rc<dyn Fn() -> Box<dyn Widget>>,
    style: TooltipStyle,
    window_origin: Option<Point>,
}

/// A view that displays a tooltip for its child.
pub fn tooltip<V: View + 'static, T: Widget + 'static>(
    child: V,
    tip: impl Fn() -> T + 'static,
) -> Tooltip {
    Tooltip {
        data: ViewData::new(Id::next()),
        child: child.build(),
        tip: Rc::new(move || Box::new(tip())),
        hover: None,
        overlay: None,
        style: Default::default(),
        window_origin: None,
    }
}

impl View for Tooltip {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl Widget for Tooltip {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn Widget) -> bool) {
        for_each(&self.child);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool) {
        for_each(&mut self.child);
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool,
    ) {
        for_each(&mut self.child);
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Tooltip".into()
    }

    fn update(&mut self, _cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(token) = state.downcast::<TimerToken>() {
            if let Some(window_origin) = self.window_origin {
                if self.hover.map(|(_, t)| t) == Some(*token) {
                    let tip = self.tip.clone();
                    self.overlay = Some(add_overlay(
                        window_origin + self.hover.unwrap().0.to_vec2(),
                        move |_| tip(),
                    ));
                }
            }
        }
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        id_path: Option<&[Id]>,
        event: Event,
    ) -> EventPropagation {
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

        default_event(self, cx, id_path, event)
    }

    fn compute_layout(&mut self, cx: &mut crate::context::ComputeLayoutCx) -> Option<kurbo::Rect> {
        self.window_origin = Some(cx.window_origin);
        default_compute_layout(self, cx)
    }
}

impl Drop for Tooltip {
    fn drop(&mut self) {
        if let Some(id) = self.overlay {
            remove_overlay(id)
        }
    }
}
