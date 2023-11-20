use kurbo::Point;
use std::time::Duration;
use taffy::style::Display;

use crate::{
    action::{exec_after, TimerToken},
    context::{EventCx, PaintCx, StyleCx},
    event::Event,
    id::Id,
    prop, prop_extracter,
    style::DisplayProp,
    view::{default_event, View, ViewData},
    EventPropagation,
};

prop!(pub Delay: f64 {} = 0.6);

prop_extracter! {
    TooltipStyle {
        delay: Delay,
    }
}

/// A view that displays a tooltip for its child.
pub struct Tooltip {
    data: ViewData,
    hover: Option<(Point, TimerToken)>,
    visible: bool,
    child: Box<dyn View>,
    tip: Box<dyn View>,
    style: TooltipStyle,
}

/// A view that displays a tooltip for its child.
pub fn tooltip<V: View + 'static, T: View + 'static>(child: V, tip: T) -> Tooltip {
    Tooltip {
        data: ViewData::new(Id::next()),
        child: Box::new(child),
        tip: Box::new(tip),
        hover: None,
        visible: false,
        style: Default::default(),
    }
}

impl View for Tooltip {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        for_each(&self.child);
        for_each(&self.tip);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        for_each(&mut self.child);
        for_each(&mut self.tip);
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn View) -> bool,
    ) {
        for_each(&mut self.tip);
        for_each(&mut self.child);
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Tooltip".into()
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(token) = state.downcast::<TimerToken>() {
            if self.hover.map(|(_, t)| t) == Some(*token) {
                self.visible = true;
                cx.request_style(self.tip.id());
                cx.request_layout(self.tip.id());
            }
        }
    }

    fn style(&mut self, cx: &mut StyleCx<'_>) {
        self.style.read(cx);

        cx.style_view(&mut self.child);
        cx.style_view(&mut self.tip);

        let tip_view = cx.app_state_mut().view_state(self.tip.id());
        tip_view.combined_style = tip_view
            .combined_style
            .clone()
            .set(
                DisplayProp,
                if self.visible {
                    Display::Flex
                } else {
                    Display::None
                },
            )
            .absolute()
            .inset_left(self.hover.map(|(p, _)| p.x).unwrap_or(0.0))
            .inset_top(self.hover.map(|(p, _)| p.y).unwrap_or(0.0))
            .z_index(100);
    }

    fn event(
        &mut self,
        cx: &mut EventCx,
        id_path: Option<&[Id]>,
        event: Event,
    ) -> EventPropagation {
        match &event {
            Event::PointerMove(e) => {
                if !self.visible {
                    let id = self.id();
                    let token =
                        exec_after(Duration::from_secs_f64(self.style.delay()), move |token| {
                            id.update_state(token, false);
                        });
                    self.hover = Some((e.pos, token));
                }
            }
            Event::PointerLeave => {
                self.hover = None;
                if self.visible {
                    self.visible = false;
                    cx.request_style(self.tip.id());
                    cx.request_layout(self.tip.id());
                }
            }
            _ => {}
        }

        default_event(self, cx, id_path, event)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        cx.paint_view(&mut self.child);

        if self.visible {
            // Remove clipping for the tooltip.
            cx.save();
            cx.clear_clip();
            cx.paint_view(&mut self.tip);
            cx.restore();
        }
    }
}
