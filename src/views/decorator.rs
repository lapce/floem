use leptos_reactive::create_effect;

use crate::{
    app::AppContext,
    event::{Event, EventListner},
    style::Style,
    view::View,
};

pub trait Decorators: View + Sized {
    fn style(self, cx: AppContext, style: impl Fn() -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            AppContext::add_style(id, style);
        });
        self
    }

    fn event(
        self,
        cx: AppContext,
        listener: EventListner,
        action: impl Fn(Event) + 'static,
    ) -> Self {
        let id = self.id();
        AppContext::add_event_listner(id, listener, action);
        self
    }
}

impl<V: View> Decorators for V {}
