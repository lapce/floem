#![deny(missing_docs)]

//! # Decorator
//!
//! The decorator trait is the primary interface for extending the appearance and functionality of ['View']s.

use floem_reactive::{Effect, SignalUpdate, UpdaterEffect};
use peniko::kurbo::{Point, Rect};
use std::rc::Rc;
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, Modifiers};

use crate::{
    ViewId,
    action::{set_window_scale, set_window_title},
    animate::Animation,
    event::{Event, EventListener, EventPropagation},
    platform::menu::Menu,
    style::{Style, StyleClass},
    view::{HasViewId, IntoView},
};

/// A trait that extends the appearance and functionality of Views through styling and event handling.
///
/// This trait is automatically implemented for all [`IntoView`] types via a blanket implementation.
/// The decoration behavior depends on the type's [`IntoView::Intermediate`] type:
///
/// - **[`View`] types**: Decorated directly (already have a [`ViewId`])
/// - **Primitives** (`&str`, `String`, `i32`, etc.): Wrapped in [`LazyView`](crate::LazyView)
///   which creates a [`ViewId`] eagerly but defers view construction
/// - **Tuples/Vecs**: Converted eagerly to their view type
pub trait Decorators: IntoView {
    /// Alter the style of the view.
    ///
    /// The Floem style system provides comprehensive styling capabilities including:
    ///
    /// ## Layout & Sizing
    /// - **Flexbox & Grid**: Full CSS-style layout with `flex()`, `grid()`, alignment, and gap controls
    /// - **Dimensions**: Width, height, min/max sizes with pixels, percentages, or auto sizing
    /// - **Spacing**: Padding, margins with individual side control or shorthand methods
    /// - **Positioning**: Absolute positioning with inset controls
    ///
    /// ## Visual Styling
    /// - **Colors & Brushes**: Solid colors, gradients, and custom brushes for backgrounds and text
    /// - **Borders**: Individual border styling per side with colors, widths, and radius
    /// - **Shadows**: Box shadows with blur, spread, offset, and color customization
    /// - **Typography**: Font family, size, weight, style, and line height control
    ///
    /// ## Interactive States
    /// - **Pseudo-states**: Styling for hover, focus, active, disabled, and selected states
    /// - **Dark Mode**: Automatic dark mode styling support
    /// - **Responsive Design**: Breakpoint-based styling for different screen sizes
    ///
    /// ## Advanced Features
    /// - **Animations**: Smooth transitions between style changes with easing functions
    /// - **Custom Properties**: Define and use custom style properties for specialized views
    /// - **Style Classes**: Reusable style definitions that can be applied across views
    /// - **Conditional Styling**: Apply styles based on conditions using `apply_if()` and `apply_opt()`
    /// - **Transform**: Scale, translate, and rotate transformations
    ///
    /// ## Style Application
    /// Styles are reactive and will automatically update when dependencies change.
    /// Subsequent calls to `style` will overwrite previous ones.
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
    fn style(self, style: impl Fn(Style) -> Style + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let view_id = intermediate.view_id();
        let state = view_id.state();

        let offset = state.borrow_mut().style.next_offset();
        let style = UpdaterEffect::new(
            move || style(Style::new()),
            move |style| {
                view_id.update_style(offset, style);
            },
        );
        state.borrow_mut().style.push(style);

        intermediate
    }

    /// Add a debug name to the view that will be shown in the inspector.
    ///
    /// This can be called multiple times and each name will be shown in the inspector with the most recent name showing first.
    fn debug_name(self, name: impl Into<String>) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let view_id = intermediate.view_id();
        let state = view_id.state();
        state.borrow_mut().debug_name.push(name.into());
        intermediate
    }

    /// Conditionally add a debug name to the view that will be shown in the inspector.
    ///
    /// # Reactivity
    /// Both the `apply` and `name` functions are reactive.
    fn debug_name_if<S: Into<String>>(
        self,
        apply: impl Fn() -> bool + 'static,
        name: impl Fn() -> S + 'static,
    ) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let view_id = intermediate.view_id();
        Effect::new(move |_| {
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

        intermediate
    }

    /// The visual style to apply when the view is being dragged
    fn dragging_style(self, style: impl Fn(Style) -> Style + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let view_id = intermediate.view_id();
        Effect::new(move |_| {
            let style = style(Style::new());
            {
                let state = view_id.state();
                state.borrow_mut().dragging_style = Some(style);
            }
            view_id.request_style();
        });
        intermediate
    }

    /// Add a style class to the view
    fn class<C: StyleClass>(self, _class: C) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        intermediate.view_id().add_class(C::class_ref());
        intermediate
    }

    /// Conditionally add a style class to the view
    fn class_if<C: StyleClass>(
        self,
        apply: impl Fn() -> bool + 'static,
        _class: C,
    ) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let id = intermediate.view_id();
        Effect::new(move |_| {
            let apply = apply();
            if apply {
                id.add_class(C::class_ref());
            } else {
                ViewId::remove_class(&id, C::class_ref());
            }
        });
        intermediate
    }

    /// Remove a style class from the view
    fn remove_class<C: StyleClass>(self, _class: C) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        intermediate.view_id().remove_class(C::class_ref());
        intermediate
    }

    /// Allows the element to be navigated to with the keyboard. Similar to setting tabindex="0" in html.
    #[deprecated(note = "Set this property using `Style::focusable` instead")]
    fn keyboard_navigable(self) -> Self::Intermediate {
        self.style(|s| s.focusable(true))
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
    ) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let id = intermediate.view_id();
        Effect::new(move |_| {
            let (event, disable) = disable();
            if disable {
                id.disable_default_event(event);
            } else {
                id.remove_disable_default_event(event);
            }
        });
        intermediate
    }

    /// Mark the view as draggable
    #[deprecated(note = "use `Style::draggable` directly instead")]
    fn draggable(self) -> Self::Intermediate {
        self.style(move |s| s.draggable(true))
    }

    /// Mark the view as disabled
    ///
    /// # Reactivity
    /// The `disabled_fn` is reactive.
    #[deprecated(note = "use `Style::set_disabled` directly instead")]
    fn disabled(self, disabled_fn: impl Fn() -> bool + 'static) -> Self::Intermediate {
        self.style(move |s| s.set_disabled(disabled_fn()))
    }

    /// Add an event handler for the given [`EventListener`].
    fn on_event(
        self,
        listener: EventListener,
        action: impl FnMut(&Event) -> EventPropagation + 'static,
    ) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        intermediate
            .view_id()
            .add_event_listener(listener, Box::new(action));
        intermediate
    }

    /// Add an handler for pressing down a specific key.
    ///
    /// NOTE: View should have `.keyboard_navigable()` in order to receive keyboard events
    fn on_key_down(
        self,
        key: Key,
        cmp: impl Fn(Modifiers) -> bool + 'static,
        action: impl Fn(&Event) + 'static,
    ) -> Self::Intermediate {
        self.on_event(EventListener::KeyDown, move |e| {
            if let Event::Key(KeyboardEvent {
                state: KeyState::Down,
                key: event_key,
                modifiers,
                ..
            }) = e
                && *event_key == key
                && cmp(*modifiers)
            {
                action(e);
                return EventPropagation::Stop;
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
    ) -> Self::Intermediate {
        self.on_event(EventListener::KeyUp, move |e| {
            if let Event::Key(KeyboardEvent {
                state: KeyState::Up,
                key: event_key,
                modifiers,
                ..
            }) = e
                && *event_key == key
                && cmp(*modifiers)
            {
                action(e);
                return EventPropagation::Stop;
            }
            EventPropagation::Continue
        })
    }

    /// Add an event handler for the given [`EventListener`]. This event will be handled with
    /// the given handler and the event will continue propagating.
    fn on_event_cont(
        self,
        listener: EventListener,
        action: impl Fn(&Event) + 'static,
    ) -> Self::Intermediate {
        self.on_event(listener, move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for the given [`EventListener`]. This event will be handled with
    /// the given handler and the event will stop propagating.
    fn on_event_stop(
        self,
        listener: EventListener,
        action: impl Fn(&Event) + 'static,
    ) -> Self::Intermediate {
        self.on_event(listener, move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Add an event handler for [`EventListener::Click`].
    fn on_click(
        self,
        action: impl FnMut(&Event) -> EventPropagation + 'static,
    ) -> Self::Intermediate {
        self.on_event(EventListener::Click, action)
    }

    /// Add an event handler for [`EventListener::Click`]. This event will be handled with
    /// the given handler and the event will continue propagating.
    fn on_click_cont(self, action: impl Fn(&Event) + 'static) -> Self::Intermediate {
        self.on_click(move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for [`EventListener::Click`]. This event will be handled with
    /// the given handler and the event will stop propagating.
    fn on_click_stop(self, mut action: impl FnMut(&Event) + 'static) -> Self::Intermediate {
        self.on_click(move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Attach action executed on button click or Enter or Space Key.
    fn action(self, mut action: impl FnMut() + 'static) -> Self::Intermediate {
        self.on_click(move |_| {
            action();
            EventPropagation::Stop
        })
    }

    /// Add an event handler for [`EventListener::DoubleClick`]
    fn on_double_click(
        self,
        action: impl Fn(&Event) -> EventPropagation + 'static,
    ) -> Self::Intermediate {
        self.on_event(EventListener::DoubleClick, action)
    }

    /// Add an event handler for [`EventListener::DoubleClick`]. This event will be handled with
    /// the given handler and the event will continue propagating.
    fn on_double_click_cont(self, action: impl Fn(&Event) + 'static) -> Self::Intermediate {
        self.on_double_click(move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for [`EventListener::DoubleClick`]. This event will be handled with
    /// the given handler and the event will stop propagating.
    fn on_double_click_stop(self, action: impl Fn(&Event) + 'static) -> Self::Intermediate {
        self.on_double_click(move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Add an event handler for [`EventListener::SecondaryClick`]. This is most often the "Right" click.
    fn on_secondary_click(
        self,
        action: impl Fn(&Event) -> EventPropagation + 'static,
    ) -> Self::Intermediate {
        self.on_event(EventListener::SecondaryClick, action)
    }

    /// Add an event handler for [`EventListener::SecondaryClick`]. This is most often the "Right" click.
    /// This event will be handled with the given handler and the event will continue propagating.
    fn on_secondary_click_cont(self, action: impl Fn(&Event) + 'static) -> Self::Intermediate {
        self.on_secondary_click(move |e| {
            action(e);
            EventPropagation::Continue
        })
    }

    /// Add an event handler for [`EventListener::SecondaryClick`]. This is most often the "Right" click.
    /// This event will be handled with the given handler and the event will stop propagating.
    fn on_secondary_click_stop(self, action: impl Fn(&Event) + 'static) -> Self::Intermediate {
        self.on_secondary_click(move |e| {
            action(e);
            EventPropagation::Stop
        })
    }

    /// Adds an event handler for resize events for this view.
    ///
    /// # Reactivity
    /// The action will be called whenever the view is resized but will not rerun automatically in response to signal changes
    fn on_resize(self, action: impl Fn(Rect) + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let id = intermediate.view_id();
        let state = id.state();
        state.borrow_mut().add_resize_listener(Rc::new(action));
        intermediate
    }

    /// Adds an event handler for move events for this view.
    ///
    /// # Reactivity
    /// The action will be called whenever the view is moved but will not rerun automatically in response to signal changes
    fn on_move(self, action: impl Fn(Point) + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let id = intermediate.view_id();
        let state = id.state();
        state.borrow_mut().add_move_listener(Rc::new(action));
        intermediate
    }

    /// Adds an event handler for cleanup events for this view.
    ///
    /// The cleanup event occurs when the view is removed from the view tree.
    ///
    /// # Reactivity
    /// The action will be called when the view is removed from the view tree but will not rerun automatically in response to signal changes
    fn on_cleanup(self, action: impl Fn() + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let id = intermediate.view_id();
        let state = id.state();
        state.borrow_mut().add_cleanup_listener(Rc::new(action));
        intermediate
    }

    /// Add an animation to the view.
    ///
    /// You can add more than one animation to a view and all of them can be active at the same time.
    ///
    /// See the [`Animation`] struct for more information on how to create animations.
    ///
    /// # Reactivity
    /// The animation function will be updated in response to signal changes in the function. The behavior is the same as the [`Decorators::style`] method.
    fn animation(self, animation: impl Fn(Animation) -> Animation + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let view_id = intermediate.view_id();
        let state = view_id.state();

        let offset = state.borrow_mut().animations.next_offset();
        let initial_animation = UpdaterEffect::new(
            move || animation(Animation::new()),
            move |animation| {
                view_id.update_animation(offset, animation);
            },
        );
        for effect_state in &initial_animation.effect_states {
            effect_state.update(|stack| stack.push((view_id, offset)));
        }

        state.borrow_mut().animations.push(initial_animation);

        intermediate
    }

    /// Clear the focus from the window.
    ///
    /// # Reactivity
    /// The when function is reactive and will rereun in response to any signal changes in the function.
    fn clear_focus(self, when: impl Fn() + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let id = intermediate.view_id();
        Effect::new(move |_| {
            when();
            id.clear_focus();
        });
        intermediate
    }

    /// Request that this view gets the focus for the window.
    ///
    /// # Reactivity
    /// The when function is reactive and will rereun in response to any signal changes in the function.
    fn request_focus(self, when: impl Fn() + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let id = intermediate.view_id();
        Effect::new(move |_| {
            when();
            id.request_focus();
        });
        intermediate
    }

    /// Set the window scale factor.
    ///
    /// This internally calls the [`crate::action::set_window_scale`] function.
    ///
    /// # Reactivity
    /// The scale function is reactive and will rereun in response to any signal changes in the function.
    fn window_scale(self, scale_fn: impl Fn() -> f64 + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        Effect::new(move |_| {
            let window_scale = scale_fn();
            set_window_scale(window_scale);
        });
        intermediate
    }

    /// Set the window title.
    ///
    /// This internally calls the [`crate::action::set_window_title`] function.
    ///
    /// # Reactivity
    /// The title function is reactive and will rereun in response to any signal changes in the function.
    fn window_title(self, title_fn: impl Fn() -> String + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        Effect::new(move |_| {
            let window_title = title_fn();
            set_window_title(window_title);
        });
        intermediate
    }

    /// Set the system window menu
    ///
    /// This internally calls the [`crate::action::set_window_menu`] function.
    ///
    /// Platform support:
    /// - Windows: No
    /// - macOS: Yes (not currently implemented)
    /// - Linux: No
    /// - wasm32: No
    ///
    /// # Reactivity
    /// The menu function is reactive and will rereun in response to any signal changes in the function.
    #[cfg(not(target_arch = "wasm32"))]
    fn window_menu(self, menu_fn: impl Fn() -> Menu + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        Effect::new(move |_| {
            let menu = menu_fn();
            crate::action::set_window_menu(menu);
        });
        intermediate
    }

    /// Adds a secondary-click context menu to the view, which opens at the mouse position.
    ///
    /// # Reactivity
    /// The menu function is not reactive and will not rerun automatically in response to signal changes while the menu is showing and will only update the menu items each time that it is created
    fn context_menu(self, menu: impl Fn() -> Menu + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let id = intermediate.view_id();
        id.update_context_menu(menu);
        intermediate
    }

    /// Adds a primary-click context menu, which opens below the view.
    ///
    /// # Reactivity
    /// The menu function is not reactive and will not rerun automatically in response to signal changes while the menu is showing and will only update the menu items each time that it is created
    fn popout_menu(self, menu: impl Fn() -> Menu + 'static) -> Self::Intermediate {
        let intermediate = self.into_intermediate();
        let id = intermediate.view_id();
        id.update_popout_menu(menu);
        intermediate
    }
}

/// Blanket implementation for all [`IntoView`] types.
impl<T: IntoView> Decorators for T {}
