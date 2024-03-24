//! # Decorator
//!
//! The decorator trait is the primary interface for extending the appereance and functionality of ['View']s.

use floem_reactive::{create_effect, create_updater};
use floem_winit::keyboard::{Key, ModifiersState};
use kurbo::{Point, Rect};

use crate::{
    action::{set_window_menu, set_window_title, update_window_scale},
    animate::Animation,
    event::{Event, EventListener},
    menu::Menu,
    style::{Style, StyleClass, StyleSelector},
    view::View,
    EventPropagation,
};

/// A trait that extends the appearance and functionality of Views through styling and event handling.
pub trait Decorators: View + Sized {
    /// Alter the style of the view.
    ///
    /// Earlier applications of `style` have lower priority than later calls.
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
    fn style(mut self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        let offset = self.view_data_mut().style.next_offset();
        let style = create_updater(
            move || style(Style::new()),
            move |style| id.update_style(style, offset),
        );
        self.view_data_mut().style.push(style);
        self
    }

    fn debug_name(mut self, name: impl Into<String>) -> Self {
        self.view_data_mut().debug_name.push(name.into());
        self
    }

    /// The visual style to apply when the mouse hovers over the element
    fn dragging_style(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let style = style(Style::new());
            id.update_style_selector(style, StyleSelector::Dragging);
        });
        self
    }

    fn class<C: StyleClass>(self, _class: C) -> Self {
        self.id().add_class(C::class_ref());
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

    fn disabled(self, disabled_fn: impl Fn() -> bool + 'static) -> Self {
        let id = self.id();

        create_effect(move |_| {
            let is_disabled = disabled_fn();
            id.update_disabled(is_disabled);
        });

        self
    }

    /// Add an event handler for the given [EventListener].
    fn on_event(
        self,
        listener: EventListener,
        action: impl Fn(&Event) -> EventPropagation + 'static,
    ) -> Self {
        let id = self.id();
        id.update_event_listener(listener, Box::new(action));
        self
    }

    /// Add an handler for pressing down a specific key.
    ///
    /// NOTE: View should have `.keyboard_navigable()` in order to receive keyboard events
    fn on_key_down(
        mut self,
        key: Key,
        modifiers: ModifiersState,
        action: impl Fn(&Event) + 'static,
    ) -> Self {
        self.view_data_mut().event_handlers.push(Box::new(move |e| {
            if let Event::KeyDown(ke) = e {
                if ke.key.logical_key == key && ke.modifiers == modifiers {
                    action(e);
                    return EventPropagation::Stop;
                }
            }
            EventPropagation::Continue
        }));
        self
    }

    /// Add an handler for a specific key being released.
    ///
    /// NOTE: View should have `.keyboard_navigable()` in order to receive keyboard events
    fn on_key_up(
        mut self,
        key: Key,
        modifiers: ModifiersState,
        action: impl Fn(&Event) + 'static,
    ) -> Self {
        self.view_data_mut().event_handlers.push(Box::new(move |e| {
            if let Event::KeyUp(ke) = e {
                if ke.key.logical_key == key && ke.modifiers == modifiers {
                    action(e);
                    return EventPropagation::Stop;
                }
            }
            EventPropagation::Continue
        }));
        self
    }

    /// Add an event handler for the given [EventListener]. This event will be handled with
    /// the given handler and the event will continue propagating.
    fn on_event_cont(self, listener: EventListener, action: impl Fn(&Event) + 'static) -> Self {
        self.on_event(listener, move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for the given [EventListener]. This event will be handled with
    /// the given handler and the event will stop propagating.
    fn on_event_stop(self, listener: EventListener, action: impl Fn(&Event) + 'static) -> Self {
        self.on_event(listener, move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Add an event handler for [EventListener::Click].
    fn on_click(self, action: impl Fn(&Event) -> EventPropagation + 'static) -> Self {
        let id = self.id();
        id.update_event_listener(EventListener::Click, Box::new(action));
        self
    }

    /// Add an event handler for [EventListener::Click]. This event will be handled with
    /// the given handler and the event will continue propagating.
    fn on_click_cont(self, action: impl Fn(&Event) + 'static) -> Self {
        self.on_click(move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for [EventListener::Click]. This event will be handled with
    /// the given handler and the event will stop propagating.
    fn on_click_stop(self, action: impl Fn(&Event) + 'static) -> Self {
        self.on_click(move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Add an event handler for [EventListener::DoubleClick]
    fn on_double_click(self, action: impl Fn(&Event) -> EventPropagation + 'static) -> Self {
        let id = self.id();
        id.update_event_listener(EventListener::DoubleClick, Box::new(action));
        self
    }

    /// Add an event handler for [EventListener::DoubleClick]. This event will be handled with
    /// the given handler and the event will continue propagating.
    fn on_double_click_cont(self, action: impl Fn(&Event) + 'static) -> Self {
        self.on_double_click(move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for [EventListener::DoubleClick]. This event will be handled with
    /// the given handler and the event will stop propagating.
    fn on_double_click_stop(self, action: impl Fn(&Event) + 'static) -> Self {
        self.on_double_click(move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Add an event handler for [EventListener::SecondaryClick]. This is most often the "Right" click.
    fn on_secondary_click(self, action: impl Fn(&Event) -> EventPropagation + 'static) -> Self {
        let id = self.id();
        id.update_event_listener(EventListener::SecondaryClick, Box::new(action));
        self
    }

    /// Add an event handler for [EventListener::SecondaryClick]. This is most often the "Right" click.
    /// This event will be handled with the given handler and the event will continue propagating.
    fn on_secondary_click_cont(self, action: impl Fn(&Event) + 'static) -> Self {
        self.on_secondary_click(move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for [EventListener::SecondaryClick]. This is most often the "Right" click.
    /// This event will be handled with the given handler and the event will stop propagating.
    fn on_secondary_click_stop(self, action: impl Fn(&Event) + 'static) -> Self {
        self.on_secondary_click(move |e| {
            action(e);
            EventPropagation::Stop
        })
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

    fn clear_focus(self, when: impl Fn() + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            when();
            id.clear_focus();
        });
        self
    }

    fn request_focus(self, when: impl Fn() + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            when();
            id.request_focus();
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
