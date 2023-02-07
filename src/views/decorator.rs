use leptos_reactive::create_effect;

use crate::{
    app::AppContext,
    context::EventCx,
    event::{Event, EventListner},
    style::Style,
    view::View,
};

pub trait Decorators: View + Sized {
    fn style(self, cx: AppContext, style: impl Fn() -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            AppContext::update_style(id, style);
        });
        self
    }

    fn event(self, listener: EventListner, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        AppContext::update_event_listner(id, listener, Box::new(action));
        self
    }
}

impl<V: View> Decorators for V {}
