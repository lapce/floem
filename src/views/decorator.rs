use floem_reactive::create_effect;
use kurbo::{Point, Rect};

use crate::{
    action::{set_window_menu, set_window_title, update_window_scale},
    animate::Animation,
    event::{Event, EventListener},
    menu::Menu,
    responsive::ScreenSize,
    style::{Style, StyleSelector},
    view::View,
};

pub trait Decorators: View + Sized {
    /// Alter the style of the view.  
    ///
    /// -----
    ///
    /// Note: repeated applications of `style` overwrite previous styles.  
    /// ```rust
    /// # use floem::{peniko::Color, view::View, views::{Decorators, label, stack}};
    /// fn view() -> impl View {
    ///     label(|| "Hello".to_string())
    ///         .style(|s| s.font_size(20.0).color(Color::RED))
    /// }
    ///
    /// fn other() -> impl View {
    ///     stack((
    ///         view(), // will be red and size 20
    ///         // will be green and default size due to the previous style being overwritten
    ///         view().style(|s| s.color(Color::GREEN)),
    ///     ))
    /// }
    /// ```
    /// If you are returning from a function that produces a view, you may want
    /// to use `base_style` for the returned [`View`] instead.  
    fn style(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let style = style(Style::BASE);
            id.update_style(style);
        });
        self
    }

    /// Alter the base style of the view.  
    /// This is applied before `style`, and so serves as a good place to set defaults.  
    /// ```rust
    /// # use floem::{peniko::Color, view::View, views::{Decorators, label, stack}};
    /// fn view() -> impl View {
    ///    label(|| "Hello".to_string())
    ///       .base_style(|s| s.font_size(20.0).color(Color::RED))
    /// }
    ///
    /// fn other() -> impl View {
    ///     stack((
    ///         view(), // will be red and size 20
    ///         // will be green and size 20
    ///         view().style(|s| s.color(Color::GREEN)),
    ///     ))
    /// }
    /// ```
    fn base_style(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let style = style(Style::BASE);
            id.update_base_style(style);
        });
        self
    }

    /// The visual style to apply when the mouse hovers over the element
    fn hover_style(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let style = style(Style::BASE);
            id.update_style_selector(style, StyleSelector::Hover);
        });
        self
    }

    /// The visual style to apply when the mouse hovers over the element
    fn dragging_style(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let style = style(Style::BASE);
            id.update_style_selector(style, StyleSelector::Dragging);
        });
        self
    }

    fn focus_style(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let style = style(Style::BASE);
            id.update_style_selector(style, StyleSelector::Focus);
        });
        self
    }

    /// Similar to the `:focus-visible` css selector, this style only activates when tab navigation is used.
    fn focus_visible_style(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let style = style(Style::BASE);
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

    fn draggable(self) -> Self {
        let id = self.id();
        id.draggable();
        self
    }

    fn active_style(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let style = style(Style::BASE);
            id.update_style_selector(style, StyleSelector::Active);
        });
        self
    }

    fn disabled_style(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let style = style(Style::BASE);
            id.update_style_selector(style, StyleSelector::Disabled);
        });
        self
    }

    fn responsive_style(self, size: ScreenSize, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let style = style(Style::BASE);
            id.update_responsive_style(style, size);
        });
        self
    }

    fn disabled(self, disabled_fn: impl Fn() -> bool + 'static) -> Self {
        let id = self.id();

        create_effect(move |_| {
            let is_disabled = disabled_fn();
            id.update_disabled(is_disabled);
        });

        self
    }

    fn on_event(self, listener: EventListener, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        id.update_event_listener(listener, Box::new(action));
        self
    }

    fn on_click(self, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        id.update_event_listener(EventListener::Click, Box::new(action));
        self
    }

    fn on_double_click(self, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        id.update_event_listener(EventListener::DoubleClick, Box::new(action));
        self
    }

    fn on_secondary_click(self, action: impl Fn(&Event) -> bool + 'static) -> Self {
        let id = self.id();
        id.update_event_listener(EventListener::SecondaryClick, Box::new(action));
        self
    }

    fn on_resize(self, action: impl Fn(Rect) + 'static) -> Self {
        let id = self.id();
        id.update_resize_listener(Box::new(action));
        self
    }

    fn on_move(self, action: impl Fn(Point) + 'static) -> Self {
        let id = self.id();
        id.update_move_listener(Box::new(action));
        self
    }

    fn on_cleanup(self, action: impl Fn() + 'static) -> Self {
        let id = self.id();
        id.update_cleanup_listener(Box::new(action));
        self
    }

    fn animation(self, anim: Animation) -> Self {
        let id = self.id();
        create_effect(move |_| {
            id.update_animation(anim.clone());
        });
        self
    }

    fn window_scale(self, scale_fn: impl Fn() -> f64 + 'static) -> Self {
        create_effect(move |_| {
            let window_scale = scale_fn();
            update_window_scale(window_scale);
        });
        self
    }

    fn window_title(self, title_fn: impl Fn() -> String + 'static) -> Self {
        create_effect(move |_| {
            let window_title = title_fn();
            set_window_title(window_title);
        });
        self
    }

    fn window_menu(self, menu_fn: impl Fn() -> Menu + 'static) -> Self {
        create_effect(move |_| {
            let menu = menu_fn();
            set_window_menu(menu);
        });
        self
    }

    /// Adds a secondary-click context menu to the view, which opens at the mouse position.
    fn context_menu(self, menu: impl Fn() -> Menu + 'static) -> Self {
        let id = self.id();
        id.update_context_menu(Box::new(menu));
        self
    }

    /// Adds a primary-click context menu, which opens below the view.
    fn popout_menu(self, menu: impl Fn() -> Menu + 'static) -> Self {
        let id = self.id();
        id.update_popout_menu(Box::new(menu));
        self
    }
}

impl<V: View> Decorators for V {}
