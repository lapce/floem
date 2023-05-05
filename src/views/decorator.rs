use glazier::kurbo::{Point, Rect};
use leptos_reactive::create_effect;

use crate::{
    app_handle::{AppContext, StyleSelector},
    event::{Event, EventListner},
    responsive::ScreenSize,
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

    /// The visual style to apply when the mouse hovers over the element
    fn hover_style(self, cx: AppContext, style: impl Fn() -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            AppContext::update_style_selector(id, style, StyleSelector::Hover);
        });
        self
    }

    fn focus_style(self, cx: AppContext, style: impl Fn() -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            AppContext::update_style_selector(id, style, StyleSelector::Focus);
        });
        self
    }

    fn active_style(self, cx: AppContext, style: impl Fn() -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            AppContext::update_style_selector(id, style, StyleSelector::Active);
        });
        self
    }

    fn disabled_style(self, cx: AppContext, style: impl Fn() -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            AppContext::update_style_selector(id, style, StyleSelector::Disabled);
        });
        self
    }

    fn responsive_style(
        self,
        cx: AppContext,
        size: ScreenSize,
        style: impl Fn() -> Style + 'static,
    ) -> Self {
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            AppContext::update_responsive_style(id, style, size);
        });
        self
    }

    fn disabled(self, cx: AppContext, disabled_fn: impl Fn() -> bool + 'static) -> Self {
        let id = self.id();

        create_effect(cx.scope, move |_| {
            let is_disabled = disabled_fn();
            AppContext::update_disabled(id, is_disabled);
        });

        self
    }

    fn on_event(self, listener: EventListner, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        AppContext::update_event_listner(id, listener, Box::new(action));
        self
    }

    fn on_click(self, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        AppContext::update_event_listner(id, EventListner::Click, Box::new(action));
        self
    }

    fn on_double_click(self, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        AppContext::update_event_listner(id, EventListner::DoubleClick, Box::new(action));
        self
    }

    fn on_resize(self, action: impl Fn(Point, Rect) + 'static) -> Self {
        let id = self.id();
        AppContext::update_resize_listner(id, Box::new(action));
        self
    }
}

impl<V: View> Decorators for V {}
