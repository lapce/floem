#![deny(missing_docs)]

//! # Decorator
//!
//! The decorator trait is the primary interface for extending the appearance and functionality of ['View']s.

use floem_reactive::{SignalUpdate, create_effect, create_updater};
use peniko::kurbo::{Point, Rect};
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, Modifiers};

use crate::{
    action::{set_window_menu, set_window_scale, set_window_title},
    animate::Animation,
    event::{Event, EventListener, EventPropagation},
    menu::Menu,
    style::{Style, StyleClass, StyleSelector},
    view::{IntoView, View},
};

/// A trait that extends the appearance and functionality of Views through styling and event handling.
pub trait Decorators: IntoView<V = Self::DV> + Sized {
    /// The type of the decorated view.
    ///
    /// Using this type allows for chaining of decorators as well as maintaining the original type of the view which allows you to call methods that were a part of the original view even after calling a decorators method.
    type DV: View;

    /// Alter the style of the view.
    ///
    /// Earlier applications of `style` have lower priority than later calls.
    /// ```rust
    /// # use floem::{peniko::color::palette, View, views::{Decorators, label, stack}};
    /// fn view() -> impl View {
    ///     label(|| "Hello".to_string())
    ///         .style(|s| s.font_size(20.0).color(palette::css::RED))
    /// }
    ///
    /// fn other() -> impl View {
    ///     stack((
    ///         view(), // will be red and size 20
    ///         // will be green and default size due to the previous style being overwritten
    ///         view().style(|s| s.color(palette::css::GREEN)),
    ///     ))
    /// }
    /// ```
    fn style(self, style: impl Fn(Style) -> Style + 'static) -> Self::DV {
        let view = self.into_view();
        let view_id = view.id();
        let state = view_id.state();

        let offset = state.borrow_mut().style.next_offset();
        let style = create_updater(
            move || style(Style::new()),
            move |style| {
                view_id.update_style(offset, style);
            },
        );
        state.borrow_mut().style.push(style);

        view
    }

    /// Add a debug name to the view that will be shown in the inspector.
    ///
    /// This can be called multiple times and each name will be shown in the inspector with the most recent name showing first.
    fn debug_name(self, name: impl Into<String>) -> Self::DV {
        let view = self.into_view();
        let view_id = view.id();
        let state = view_id.state();
        state.borrow_mut().debug_name.push(name.into());
        view
    }

    /// Conditionally add a debug name to the view that will be shown in the inspector.
    ///
    /// # Reactivity
    /// Both the `apply` and `name` functions are reactive.
    fn debug_name_if<S: Into<String>>(
        self,
        apply: impl Fn() -> bool + 'static,
        name: impl Fn() -> S + 'static,
    ) -> Self::DV {
        let view = self.into_view();
        let view_id = view.id();
        create_effect(move |_| {
            let apply = apply();
            let state = view_id.state();
            if apply {
                state.borrow_mut().debug_name.push(name().into());
            } else {
                state
                    .borrow_mut()
                    .debug_name
                    .retain_mut(|n| n != &name().into());
            }
        });

        view
    }

    /// The visual style to apply when the mouse hovers over the element
    fn dragging_style(self, style: impl Fn(Style) -> Style + 'static) -> Self::DV {
        let view = self.into_view();
        let view_id = view.id();
        create_effect(move |_| {
            let style = style(Style::new());
            view_id.update_style_selector(StyleSelector::Dragging, style);
        });
        view
    }

    /// Add a style class to the view
    fn class<C: StyleClass>(self, _class: C) -> Self::DV {
        let view = self.into_view();
        view.id().add_class(C::class_ref());
        view
    }

    /// Conditionally add a style class to the view
    fn class_if<C: StyleClass>(self, apply: impl Fn() -> bool + 'static, _class: C) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        create_effect(move |_| {
            let apply = apply();
            if apply {
                id.add_class(C::class_ref());
            } else {
                id.remove_class(C::class_ref());
            }
        });
        view
    }

    /// Remove a style class from the view
    fn remove_class<C: StyleClass>(self, _class: C) -> Self::DV {
        let view = self.into_view();
        view.id().remove_class(C::class_ref());
        view
    }

    /// Allows the element to be navigated to with the keyboard. Similar to setting tabindex="0" in html.
    fn keyboard_navigable(self) -> Self::DV {
        let view = self.into_view();
        view.id().keyboard_navigable();
        view
    }

    /// Dynamically controls whether the default view behavior for an event should be disabled.
    /// When disable is true, children will still see the event, but the view event function will not be called nor
    /// the event listeners on the view.
    ///
    /// # Reactivity
    /// This function is reactive and will re-run the disable function automatically in response to changes in signals
    fn disable_default_event(
        self,
        disable: impl Fn() -> (EventListener, bool) + 'static,
    ) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        create_effect(move |_| {
            let (event, disable) = disable();
            if disable {
                id.disable_default_event(event);
            } else {
                id.remove_disable_default_event(event);
            }
        });
        view
    }

    /// Mark the view as draggable
    fn draggable(self) -> Self::DV {
        let view = self.into_view();
        view.id().draggable();
        view
    }

    /// Mark the view as disabled
    ///
    /// # Reactivity
    /// The `disabled_fn` is reactive.
    fn disabled(self, disabled_fn: impl Fn() -> bool + 'static) -> Self::DV {
        let view = self.into_view();
        let id = view.id();

        create_effect(move |_| {
            let is_disabled = disabled_fn();
            id.update_disabled(is_disabled);
        });

        view
    }

    /// Add an event handler for the given [`EventListener`].
    fn on_event(
        self,
        listener: EventListener,
        action: impl FnMut(&Event) -> EventPropagation + 'static,
    ) -> Self::DV {
        let view = self.into_view();
        view.id().add_event_listener(listener, Box::new(action));
        view
    }

    /// Add an handler for pressing down a specific key.
    ///
    /// NOTE: View should have `.keyboard_navigable()` in order to receive keyboard events
    fn on_key_down(
        self,
        key: Key,
        cmp: impl Fn(Modifiers) -> bool + 'static,
        action: impl Fn(&Event) + 'static,
    ) -> Self::DV {
        self.on_event(EventListener::KeyDown, move |e| {
            if let Event::Key(KeyboardEvent {
                state: KeyState::Down,
                key: event_key,
                modifiers,
                ..
            }) = e
            {
                if *event_key == key && cmp(*modifiers) {
                    action(e);
                    return EventPropagation::Stop;
                }
            }
            EventPropagation::Continue
        })
    }

    /// Add an handler for a specific key being released.
    ///
    /// NOTE: View should have `.keyboard_navigable()` in order to receive keyboard events
    fn on_key_up(
        self,
        key: Key,
        cmp: impl Fn(Modifiers) -> bool + 'static,
        action: impl Fn(&Event) + 'static,
    ) -> Self::DV {
        self.on_event(EventListener::KeyUp, move |e| {
            if let Event::Key(KeyboardEvent {
                state: KeyState::Up,
                key: event_key,
                modifiers,
                ..
            }) = e
            {
                if *event_key == key && cmp(*modifiers) {
                    action(e);
                    return EventPropagation::Stop;
                }
            }
            EventPropagation::Continue
        })
    }

    /// Add an event handler for the given [`EventListener`]. This event will be handled with
    /// the given handler and the event will continue propagating.
    fn on_event_cont(self, listener: EventListener, action: impl Fn(&Event) + 'static) -> Self::DV {
        self.on_event(listener, move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for the given [`EventListener`]. This event will be handled with
    /// the given handler and the event will stop propagating.
    fn on_event_stop(self, listener: EventListener, action: impl Fn(&Event) + 'static) -> Self::DV {
        self.on_event(listener, move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Add an event handler for [`EventListener::Click`].
    fn on_click(self, action: impl FnMut(&Event) -> EventPropagation + 'static) -> Self::DV {
        self.on_event(EventListener::Click, action)
    }

    /// Add an event handler for [`EventListener::Click`]. This event will be handled with
    /// the given handler and the event will continue propagating.
    fn on_click_cont(self, action: impl Fn(&Event) + 'static) -> Self::DV {
        self.on_click(move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for [`EventListener::Click`]. This event will be handled with
    /// the given handler and the event will stop propagating.
    fn on_click_stop(self, mut action: impl FnMut(&Event) + 'static) -> Self::DV {
        self.on_click(move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Add an event handler for [`EventListener::DoubleClick`]
    fn on_double_click(self, action: impl Fn(&Event) -> EventPropagation + 'static) -> Self::DV {
        self.on_event(EventListener::DoubleClick, action)
    }

    /// Add an event handler for [`EventListener::DoubleClick`]. This event will be handled with
    /// the given handler and the event will continue propagating.
    fn on_double_click_cont(self, action: impl Fn(&Event) + 'static) -> Self::DV {
        self.on_double_click(move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for [`EventListener::DoubleClick`]. This event will be handled with
    /// the given handler and the event will stop propagating.
    fn on_double_click_stop(self, action: impl Fn(&Event) + 'static) -> Self::DV {
        self.on_double_click(move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Add an event handler for [`EventListener::SecondaryClick`]. This is most often the "Right" click.
    fn on_secondary_click(self, action: impl Fn(&Event) -> EventPropagation + 'static) -> Self::DV {
        self.on_event(EventListener::SecondaryClick, action)
    }

    /// Add an event handler for [`EventListener::SecondaryClick`]. This is most often the "Right" click.
    /// This event will be handled with the given handler and the event will continue propagating.
    fn on_secondary_click_cont(self, action: impl Fn(&Event) + 'static) -> Self::DV {
        self.on_secondary_click(move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for [`EventListener::SecondaryClick`]. This is most often the "Right" click.
    /// This event will be handled with the given handler and the event will stop propagating.
    fn on_secondary_click_stop(self, action: impl Fn(&Event) + 'static) -> Self::DV {
        self.on_secondary_click(move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Set the event handler for resize events for this view.
    ///
    /// There can only be one resize event handler for a view.
    ///
    /// # Reactivity
    /// The action will be called whenever the view is resized but will not rerun automatically in response to signal changes
    fn on_resize(self, action: impl Fn(Rect) + 'static) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        let state = id.state();
        state.borrow_mut().update_resize_listener(Box::new(action));
        view
    }

    /// Set the event handler for move events for this view.
    ///
    /// There can only be one move event handler for a view.
    ///
    /// # Reactivity
    /// The action will be called whenever the view is moved but will not rerun automatically in response to signal changes
    fn on_move(self, action: impl Fn(Point) + 'static) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        let state = id.state();
        state.borrow_mut().update_move_listener(Box::new(action));
        view
    }

    /// Set the event handler for cleanup events for this view.
    ///
    /// The cleanup event is called when the view is removed from the view tree.
    ///
    /// There can only be one cleanup event handler for a view.
    ///
    /// # Reactivity
    /// The action will be called when the view is removed from the view tree but will not rerun automatically in response to signal changes
    fn on_cleanup(self, action: impl Fn() + 'static) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        let state = id.state();
        state.borrow_mut().update_cleanup_listener(action);
        view
    }

    /// Add an animation to the view.
    ///
    /// You can add more than one animation to a view and all of them can be active at the same time.
    ///
    /// See the [`Animation`] struct for more information on how to create animations.
    ///
    /// # Reactivity
    /// The animation function will be updated in response to signal changes in the function. The behavior is the same as the [`Decorators::style`] method.
    fn animation(self, animation: impl Fn(Animation) -> Animation + 'static) -> Self::DV {
        let view = self.into_view();
        let view_id = view.id();
        let state = view_id.state();

        let offset = state.borrow_mut().animations.next_offset();
        let initial_animation = create_updater(
            move || animation(Animation::new()),
            move |animation| {
                view_id.update_animation(offset, animation);
            },
        );
        for effect_state in &initial_animation.effect_states {
            effect_state.update(|stack| stack.push((view_id, offset)));
        }

        state.borrow_mut().animations.push(initial_animation);

        view
    }

    /// Clear the focus from the window.
    ///
    /// # Reactivity
    /// The when function is reactive and will rereun in response to any signal changes in the function.
    fn clear_focus(self, when: impl Fn() + 'static) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        create_effect(move |_| {
            when();
            id.clear_focus();
        });
        view
    }

    /// Request that this view gets the focus for the window.
    ///
    /// # Reactivity
    /// The when function is reactive and will rereun in response to any signal changes in the function.
    fn request_focus(self, when: impl Fn() + 'static) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        create_effect(move |_| {
            when();
            id.request_focus();
        });
        view
    }

    /// Set the window scale factor.
    ///
    /// This internally calls the [`crate::action::set_window_scale`] function.
    ///
    /// # Reactivity
    /// The scale function is reactive and will rereun in response to any signal changes in the function.
    fn window_scale(self, scale_fn: impl Fn() -> f64 + 'static) -> Self {
        create_effect(move |_| {
            let window_scale = scale_fn();
            set_window_scale(window_scale);
        });
        self
    }

    /// Set the window title.
    ///
    /// This internally calls the [`crate::action::set_window_title`] function.
    ///
    /// # Reactivity
    /// The title function is reactive and will rereun in response to any signal changes in the function.
    fn window_title(self, title_fn: impl Fn() -> String + 'static) -> Self {
        create_effect(move |_| {
            let window_title = title_fn();
            set_window_title(window_title);
        });
        self
    }

    /// Set the system window menu
    ///
    /// This internally calls the [`crate::action::set_window_menu`] function.
    ///
    /// Platform support:
    /// - Windows: No
    /// - macOS: Yes (not currently implemented)
    /// - Linux: No
    ///
    /// # Reactivity
    /// The menu function is reactive and will rereun in response to any signal changes in the function.
    fn window_menu(self, menu_fn: impl Fn() -> Menu + 'static) -> Self {
        create_effect(move |_| {
            let menu = menu_fn();
            set_window_menu(menu);
        });
        self
    }

    /// Adds a secondary-click context menu to the view, which opens at the mouse position.
    ///
    /// # Reactivity
    /// The menu function is not reactive and will not rerun automatically in response to signal changes while the menu is showing and will only update the menu items each time that it is created
    fn context_menu(self, menu: impl Fn() -> Menu + 'static) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        id.update_context_menu(menu);
        view
    }

    /// Adds a primary-click context menu, which opens below the view.
    ///
    /// # Reactivity
    /// The menu function is not reactive and will not rerun automatically in response to signal changes while the menu is showing and will only update the menu items each time that it is created
    fn popout_menu(self, menu: impl Fn() -> Menu + 'static) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        id.update_popout_menu(menu);
        view
    }
}

impl<VW: View, IV: IntoView<V = VW>> Decorators for IV {
    type DV = VW;
}
