use glazier::kurbo::{Point, Rect};
use leptos_reactive::create_effect;

use crate::{
    animate::Animation,
    app_handle::{AppContext, StyleSelector},
    event::{Event, EventListner},
    responsive::ScreenSize,
    style::Style,
    view::View,
};

pub trait Decorators: View + Sized {
    fn style(self, style: impl Fn() -> Style + 'static) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            id.update_style(style);
        });
        self
    }

    fn base_style(self, style: impl Fn() -> Style + 'static) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            id.update_base_style(style);
        });
        self
    }

    /// The visual style to apply when the mouse hovers over the element
    fn hover_style(self, style: impl Fn() -> Style + 'static) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            id.update_style_selector(style, StyleSelector::Hover);
        });
        self
    }

    fn focus_style(self, style: impl Fn() -> Style + 'static) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            id.update_style_selector(style, StyleSelector::Focus);
        });
        self
    }

    /// Similar to the `:focus-visible` css selector, this style only activates when tab navigation is used.
    fn focus_visible_style(self, style: impl Fn() -> Style + 'static) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            id.update_style_selector(style, StyleSelector::FocusVisible);
        });
        self
    }

    /// Allows the element to be navigated to with the keyboard. Similar to setting tabindex="0" in html.
    fn keyboard_navigatable(self) -> Self {
        let id = self.id();
        id.keyboard_navigatable();
        self
    }

    fn active_style(self, style: impl Fn() -> Style + 'static) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            id.update_style_selector(style, StyleSelector::Active);
        });
        self
    }

    fn disabled_style(self, style: impl Fn() -> Style + 'static) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            id.update_style_selector(style, StyleSelector::Disabled);
        });
        self
    }

    fn responsive_style(self, size: ScreenSize, style: impl Fn() -> Style + 'static) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();
        create_effect(cx.scope, move |_| {
            let style = style();
            id.update_responsive_style(style, size);
        });
        self
    }

    fn disabled(self, disabled_fn: impl Fn() -> bool + 'static) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();

        create_effect(cx.scope, move |_| {
            let is_disabled = disabled_fn();
            id.update_disabled(is_disabled);
        });

        self
    }

    fn on_event(self, listener: EventListner, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        id.update_event_listner(listener, Box::new(action));
        self
    }

    fn on_click(self, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        id.update_event_listner(EventListner::Click, Box::new(action));
        self
    }

    fn on_double_click(self, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        id.update_event_listner(EventListner::DoubleClick, Box::new(action));
        self
    }

    fn on_resize(self, action: impl Fn(Point, Rect) + 'static) -> Self {
        let id = self.id();
        id.update_resize_listner(Box::new(action));
        self
    }

    fn animation(self, anim: Animation) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();
        create_effect(cx.scope, move |_| {
            id.update_animation(anim.clone());
        });
        self
    }

    fn window_scale(self, scale_fn: impl Fn() -> f64 + 'static) -> Self {
        let cx = AppContext::get_current();
        let id = self.id();

        create_effect(cx.scope, move |_| {
            let window_scale = scale_fn();
            id.update_window_scale(window_scale);
        });
        self
    }
}

impl<V: View> Decorators for V {}
