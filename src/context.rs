use floem_reactive::Scope;
use floem_renderer::gpu_resources::{GpuResourceError, GpuResources};
use floem_renderer::Renderer as FloemRenderer;
use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Shape, Size, Vec2};
use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Arc,
};
use winit::window::Window;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

#[cfg(feature = "crossbeam")]
use crossbeam::channel::Receiver;
#[cfg(not(feature = "crossbeam"))]
use std::sync::mpsc::Receiver;

use taffy::prelude::NodeId;

use crate::animate::{AnimStateKind, RepeatMode};
use crate::easing::{Easing, Linear};
use crate::menu::MenuBuilder;
use crate::renderer::Renderer;
use crate::style::{DisplayProp, PointerEvents, PointerEventsProp};
use crate::view_state::IsHiddenState;
use crate::{
    action::{exec_after, show_context_menu},
    app_state::AppState,
    event::{Event, EventListener, EventPropagation},
    id::ViewId,
    inspector::CaptureState,
    style::{Style, StyleProp, ZIndex},
    view::{paint_bg, paint_border, paint_outline, View},
    view_state::ChangeFlags,
};

pub type EventCallback = dyn FnMut(&Event) -> EventPropagation;
pub type ResizeCallback = dyn Fn(Rect);
pub type MenuCallback = dyn Fn() -> MenuBuilder;

pub(crate) struct ResizeListener {
    pub(crate) rect: Rect,
    pub(crate) callback: Box<ResizeCallback>,
}

/// The listener when the view is got moved to a different position in the window
pub(crate) struct MoveListener {
    pub(crate) window_origin: Point,
    pub(crate) callback: Box<dyn Fn(Point)>,
}

pub struct DragState {
    pub(crate) id: ViewId,
    pub(crate) offset: Vec2,
    pub(crate) released_at: Option<Instant>,
    pub(crate) release_location: Option<Point>,
}

pub(crate) enum FrameUpdate {
    Style(ViewId),
    Layout(ViewId),
    Paint(ViewId),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PointerEventConsumed {
    Yes,
    No,
}

/// A bundle of helper methods to be used by `View::event` handlers
pub struct EventCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl EventCx<'_> {
    pub fn app_state_mut(&mut self) -> &mut AppState {
        self.app_state
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }

    pub fn update_active(&mut self, id: ViewId) {
        self.app_state.update_active(id);
    }

    pub fn is_active(&self, id: ViewId) -> bool {
        self.app_state.is_active(&id)
    }

    #[allow(unused)]
    pub(crate) fn update_focus(&mut self, id: ViewId, keyboard_navigation: bool) {
        self.app_state.update_focus(id, keyboard_navigation);
    }

    /// Internal method used by Floem. This can be called from parent `View`s to propagate an event to the child `View`.
    pub(crate) fn unconditional_view_event(
        &mut self,
        view_id: ViewId,
        event: Event,
        directed: bool,
    ) -> (EventPropagation, PointerEventConsumed) {
        if view_id.style_has_hidden() {
            // we don't process events for hidden view
            return (EventPropagation::Continue, PointerEventConsumed::No);
        }
        if self.app_state.is_disabled(&view_id) && !event.allow_disabled() {
            // if the view is disabled and the event is not processed
            // for disabled views
            return (EventPropagation::Continue, PointerEventConsumed::No);
        }

        // offset the event positions if the event has positions
        // e.g. pointer events, so that the position is relative
        // to the view, taking into account of the layout location
        // of the view and the viewport of the view if it's in a scroll.
        let event = self.offset_event(view_id, event);

        let view = view_id.view();
        let view_state = view_id.state();

        let disable_default = if let Some(listener) = event.listener() {
            view_state
                .borrow()
                .disable_default_events
                .contains(&listener)
        } else {
            false
        };

        let is_pointer_none = event.is_pointer()
            && view_state.borrow().computed_style.get(PointerEventsProp)
                == Some(PointerEvents::None);

        if !disable_default
            && !is_pointer_none
            && view
                .borrow_mut()
                .event_before_children(self, &event)
                .is_processed()
        {
            if let Event::PointerDown(event) = &event {
                if self.app_state.keyboard_navigable.contains(&view_id) {
                    let rect = view_id.get_size().unwrap_or_default().to_rect();
                    let now_focused = rect.contains(event.pos);
                    if now_focused {
                        self.app_state.update_focus(view_id, false);
                    }
                }
            }
            if let Event::PointerMove(_event) = &event {
                let view_state = view_state.borrow();
                let style = view_state.combined_style.builtin();
                if let Some(cursor) = style.cursor() {
                    if self.app_state.cursor.is_none() {
                        self.app_state.cursor = Some(cursor);
                    }
                }
            }
            return (EventPropagation::Stop, PointerEventConsumed::Yes);
        }

        let mut view_pointer_event_consumed = PointerEventConsumed::No;

        if !directed {
            let children = view_id.children();
            for child in children.into_iter().rev() {
                if !self.should_send(child, &event) {
                    continue;
                }
                let (event_propagation, pointer_event_consumed) =
                    self.unconditional_view_event(child, event.clone(), false);
                if event_propagation.is_processed() {
                    return (EventPropagation::Stop, PointerEventConsumed::Yes);
                }
                if event.is_pointer() && pointer_event_consumed == PointerEventConsumed::Yes {
                    // if a child's pointer event was consumed because pointer-events: auto
                    // we don't pass the pointer event the next child
                    // also, we mark pointer_event_consumed to be yes
                    // so that it will be bublled up the parent
                    view_pointer_event_consumed = PointerEventConsumed::Yes;
                    break;
                }
            }
        }

        if !disable_default
            && !is_pointer_none
            && view
                .borrow_mut()
                .event_after_children(self, &event)
                .is_processed()
        {
            return (EventPropagation::Stop, PointerEventConsumed::Yes);
        }

        if is_pointer_none {
            // if pointer-events: none, we don't handle the pointer event
            return (EventPropagation::Continue, view_pointer_event_consumed);
        }

        // CLARIFY: should this be disabled when disable_default?
        if !disable_default {
            let popout_menu = || {
                let bottom_left = {
                    let layout = view_state.borrow().layout_rect;
                    Point::new(layout.x0, layout.y1)
                };

                let popout_menu = view_state.borrow().popout_menu.clone();
                show_context_menu(popout_menu?(), Some(bottom_left));
                Some((EventPropagation::Stop, PointerEventConsumed::Yes))
            };

            match &event {
                Event::PointerDown(pointer_event) => {
                    self.app_state.clicking.insert(view_id);
                    if pointer_event.button.is_primary() {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        let on_view = rect.contains(pointer_event.pos);

                        if on_view {
                            if self.app_state.keyboard_navigable.contains(&view_id) {
                                // if the view can be focused, we update the focus
                                self.app_state.update_focus(view_id, false);
                            }
                            if pointer_event.count == 2
                                && view_state
                                    .borrow()
                                    .event_listeners
                                    .contains_key(&EventListener::DoubleClick)
                            {
                                view_state.borrow_mut().last_pointer_down =
                                    Some(pointer_event.clone());
                            }
                            if view_state
                                .borrow()
                                .event_listeners
                                .contains_key(&EventListener::Click)
                            {
                                view_state.borrow_mut().last_pointer_down =
                                    Some(pointer_event.clone());
                            }

                            #[cfg(target_os = "macos")]
                            if let Some((ep, pec)) = popout_menu() {
                                return (ep, pec);
                            };

                            if self.app_state.draggable.contains(&view_id)
                                && self.app_state.drag_start.is_none()
                            {
                                self.app_state.drag_start = Some((view_id, pointer_event.pos));
                            }
                        }
                    } else if pointer_event.button.is_secondary() {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        let on_view = rect.contains(pointer_event.pos);

                        if on_view {
                            if self.app_state.keyboard_navigable.contains(&view_id) {
                                // if the view can be focused, we update the focus
                                self.app_state.update_focus(view_id, false);
                            }
                            if view_state
                                .borrow()
                                .event_listeners
                                .contains_key(&EventListener::SecondaryClick)
                            {
                                view_state.borrow_mut().last_pointer_down =
                                    Some(pointer_event.clone());
                            }
                        }
                    }
                }
                Event::PointerMove(pointer_event) => {
                    let rect = view_id.get_size().unwrap_or_default().to_rect();
                    if rect.contains(pointer_event.pos) {
                        if self.app_state.is_dragging() {
                            self.app_state.dragging_over.insert(view_id);
                            view_id.apply_event(&EventListener::DragOver, &event);
                        } else {
                            self.app_state.hovered.insert(view_id);
                            let view_state = view_state.borrow();
                            let style = view_state.combined_style.builtin();
                            if let Some(cursor) = style.cursor() {
                                if self.app_state.cursor.is_none() {
                                    self.app_state.cursor = Some(cursor);
                                }
                            }
                        }
                    }
                    if self.app_state.draggable.contains(&view_id) {
                        if let Some((_, drag_start)) = self
                            .app_state
                            .drag_start
                            .as_ref()
                            .filter(|(drag_id, _)| drag_id == &view_id)
                        {
                            let offset = pointer_event.pos - *drag_start;
                            if let Some(dragging) = self
                                .app_state
                                .dragging
                                .as_mut()
                                .filter(|d| d.id == view_id && d.released_at.is_none())
                            {
                                // update the mouse position if the view is dragging and not released
                                dragging.offset = drag_start.to_vec2();
                                self.app_state.request_paint(view_id);
                            } else if offset.x.abs() + offset.y.abs() > 1.0 {
                                // start dragging when moved 1 px
                                self.app_state.active = None;
                                self.app_state.dragging = Some(DragState {
                                    id: view_id,
                                    offset: drag_start.to_vec2(),
                                    released_at: None,
                                    release_location: None,
                                });
                                self.update_active(view_id);
                                self.app_state.request_paint(view_id);
                                view_id.apply_event(&EventListener::DragStart, &event);
                            }
                        }
                    }
                    if view_id
                        .apply_event(&EventListener::PointerMove, &event)
                        .is_some_and(|prop| prop.is_processed())
                    {
                        return (EventPropagation::Stop, PointerEventConsumed::Yes);
                    }
                }
                Event::PointerUp(pointer_event) => {
                    if pointer_event.button.is_primary() {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        let on_view = rect.contains(pointer_event.pos);

                        #[cfg(not(target_os = "macos"))]
                        if on_view {
                            if let Some((ep, pec)) = popout_menu() {
                                return (ep, pec);
                            };
                        }

                        // if id_path.is_none() {
                        if !directed {
                            if on_view {
                                if let Some(dragging) = self.app_state.dragging.as_mut() {
                                    let dragging_id = dragging.id;
                                    if view_id
                                        .apply_event(&EventListener::Drop, &event)
                                        .is_some_and(|prop| prop.is_processed())
                                    {
                                        // if the drop is processed, we set dragging to none so that the animation
                                        // for the dragged view back to its original position isn't played.
                                        self.app_state.dragging = None;
                                        self.app_state.request_paint(view_id);
                                        dragging_id.apply_event(&EventListener::DragEnd, &event);
                                    }
                                }
                            }
                        } else if let Some(dragging) =
                            self.app_state.dragging.as_mut().filter(|d| d.id == view_id)
                        {
                            let dragging_id = dragging.id;
                            dragging.released_at = Some(Instant::now());
                            dragging.release_location = Some(pointer_event.pos);
                            self.app_state.request_paint(view_id);
                            dragging_id.apply_event(&EventListener::DragEnd, &event);
                        }

                        let last_pointer_down = view_state.borrow_mut().last_pointer_down.take();

                        let event_listeners = view_state.borrow().event_listeners.clone();
                        if let Some(handlers) = event_listeners.get(&EventListener::DoubleClick) {
                            view_state.borrow_mut();
                            if on_view
                                && self.app_state.is_clicking(&view_id)
                                && last_pointer_down
                                    .as_ref()
                                    .map(|e| e.count == 2)
                                    .unwrap_or(false)
                                && handlers.iter().fold(false, |handled, handler| {
                                    handled | (handler.borrow_mut())(&event).is_processed()
                                })
                            {
                                return (EventPropagation::Stop, PointerEventConsumed::Yes);
                            }
                        }

                        if let Some(handlers) = event_listeners.get(&EventListener::Click) {
                            if on_view
                                && self.app_state.is_clicking(&view_id)
                                && last_pointer_down.is_some()
                                && handlers.iter().fold(false, |handled, handler| {
                                    handled | (handler.borrow_mut())(&event).is_processed()
                                })
                            {
                                return (EventPropagation::Stop, PointerEventConsumed::Yes);
                            }
                        }

                        if view_id
                            .apply_event(&EventListener::PointerUp, &event)
                            .is_some_and(|prop| prop.is_processed())
                        {
                            return (EventPropagation::Stop, PointerEventConsumed::Yes);
                        }
                    } else if pointer_event.button.is_secondary() {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        let on_view = rect.contains(pointer_event.pos);

                        let last_pointer_down = view_state.borrow_mut().last_pointer_down.take();
                        let event_listeners = view_state.borrow().event_listeners.clone();
                        if let Some(handlers) = event_listeners.get(&EventListener::SecondaryClick)
                        {
                            if on_view
                                && last_pointer_down.is_some()
                                && handlers.iter().fold(false, |handled, handler| {
                                    handled | (handler.borrow_mut())(&event).is_processed()
                                })
                            {
                                return (EventPropagation::Stop, PointerEventConsumed::Yes);
                            }
                        }

                        let viewport_event_position = {
                            let layout = view_state.borrow().layout_rect;
                            Point::new(
                                layout.x0 + pointer_event.pos.x,
                                layout.y0 + pointer_event.pos.y,
                            )
                        };
                        let context_menu = view_state.borrow().context_menu.clone();
                        if let Some(menu) = context_menu {
                            show_context_menu(menu(), Some(viewport_event_position));
                            return (EventPropagation::Stop, PointerEventConsumed::Yes);
                        }
                    }
                }
                Event::KeyDown(_) => {
                    if self.app_state.is_focused(&view_id) && event.is_keyboard_trigger() {
                        view_id.apply_event(&EventListener::Click, &event);
                    }
                }
                Event::WindowResized(_) => {
                    if view_state.borrow().has_style_selectors.has_responsive() {
                        view_id.request_style();
                    }
                }
                _ => (),
            }
        }

        if !disable_default {
            if let Some(listener) = event.listener() {
                let event_listeners = view_state.borrow().event_listeners.clone();
                if let Some(handlers) = event_listeners.get(&listener).cloned() {
                    let should_run = if let Some(pos) = event.point() {
                        let rect = view_id.get_size().unwrap_or_default().to_rect();
                        rect.contains(pos)
                    } else {
                        true
                    };
                    if should_run
                        && handlers.iter().fold(false, |handled, handler| {
                            handled | (handler.borrow_mut())(&event).is_processed()
                        })
                    {
                        return (EventPropagation::Stop, PointerEventConsumed::Yes);
                    }
                }
            }
        }

        (EventPropagation::Continue, PointerEventConsumed::Yes)
    }

    /// translate a window-positioned event to the local coordinate system of a view
    pub(crate) fn offset_event(&self, id: ViewId, event: Event) -> Event {
        let state = id.state();
        let viewport = state.borrow().viewport;
        let transform = state.borrow().transform;

        if let Some(layout) = id.get_layout() {
            event.transform(
                Affine::translate((
                    layout.location.x as f64 - viewport.map(|rect| rect.x0).unwrap_or(0.0),
                    layout.location.y as f64 - viewport.map(|rect| rect.y0).unwrap_or(0.0),
                )) * transform,
            )
        } else {
            event
        }
    }

    /// Used to determine if you should send an event to another view. This is basically a check for pointer events to see if the pointer is inside a child view and to make sure the current view isn't hidden or disabled.
    /// Usually this is used if you want to propagate an event to a child view
    pub fn should_send(&mut self, id: ViewId, event: &Event) -> bool {
        if id.style_has_hidden() || (self.app_state.is_disabled(&id) && !event.allow_disabled()) {
            return false;
        }

        let Some(point) = event.point() else {
            return true;
        };

        let layout_rect = id.layout_rect();
        let Some(layout) = id.get_layout() else {
            return false;
        };

        // Check if point is within current view's bounds
        let current_rect = layout_rect.with_origin(Point::new(
            layout.location.x as f64,
            layout.location.y as f64,
        ));

        if !current_rect.contains(point) {
            return false;
        }

        true
    }
}

#[derive(Default)]
pub struct InteractionState {
    pub(crate) is_hovered: bool,
    pub(crate) is_selected: bool,
    pub(crate) is_disabled: bool,
    pub(crate) is_focused: bool,
    pub(crate) is_clicking: bool,
    pub(crate) is_dark_mode: bool,
    pub(crate) using_keyboard_navigation: bool,
}

pub struct StyleCx<'a> {
    pub(crate) app_state: &'a mut AppState,
    pub(crate) current_view: ViewId,
    pub(crate) current: Rc<Style>,
    pub(crate) direct: Style,
    saved: Vec<Rc<Style>>,
    pub(crate) now: Instant,
    saved_disabled: Vec<bool>,
    saved_selected: Vec<bool>,
    disabled: bool,
    selected: bool,
}

impl<'a> StyleCx<'a> {
    pub(crate) fn new(app_state: &'a mut AppState, root: ViewId) -> Self {
        Self {
            app_state,
            current_view: root,
            current: Default::default(),
            direct: Default::default(),
            saved: Default::default(),
            now: Instant::now(),
            saved_disabled: Default::default(),
            saved_selected: Default::default(),
            disabled: false,
            selected: false,
        }
    }

    /// Marks the current context as selected.
    pub fn selected(&mut self) {
        self.selected = true;
    }

    fn get_interact_state(&self, id: &ViewId) -> InteractionState {
        InteractionState {
            is_selected: self.selected,
            is_hovered: self.app_state.is_hovered(id),
            is_disabled: self.app_state.is_disabled(id),
            is_focused: self.app_state.is_focused(id),
            is_clicking: self.app_state.is_clicking(id),
            is_dark_mode: self.app_state.is_dark_mode(),
            using_keyboard_navigation: self.app_state.keyboard_navigation,
        }
    }

    /// Internal method used by Floem to compute the styles for the view.
    pub fn style_view(&mut self, view_id: ViewId) {
        self.save();
        let view = view_id.view();
        let view_state = view_id.state();
        {
            let mut view_state = view_state.borrow_mut();
            if !view_state.requested_changes.contains(ChangeFlags::STYLE) {
                self.restore();
                return;
            }
            view_state.requested_changes.remove(ChangeFlags::STYLE);
        }

        let view_style = view.borrow().view_style();
        let view_class = view.borrow().view_class();
        {
            let mut view_state = view_state.borrow_mut();

            // Propagate style requests to children if needed.
            if view_state.request_style_recursive {
                view_state.request_style_recursive = false;
                let children = view_id.children();
                for child in children {
                    let view_state = child.state();
                    let mut state = view_state.borrow_mut();
                    state.request_style_recursive = true;
                    state.requested_changes.insert(ChangeFlags::STYLE);
                }
            }
        }

        let mut view_interact_state = self.get_interact_state(&view_id);
        view_interact_state.is_disabled |= self.disabled;
        self.disabled = view_interact_state.is_disabled;
        let mut new_frame = self.app_state.compute_style(
            view_id,
            view_style,
            view_interact_state,
            view_class,
            &self.current,
        );

        let style = view_state.borrow().combined_style.clone();
        self.direct = style;
        Style::apply_only_inherited(&mut self.current, &self.direct);
        let mut computed_style = (*self.current).clone();
        computed_style.apply_mut(self.direct.clone());
        CaptureState::capture_style(view_id, self, computed_style.clone());
        view_state.borrow_mut().computed_style = computed_style;

        // This is used by the `request_transition` and `style` methods below.
        self.current_view = view_id;

        {
            let mut view_state = view_state.borrow_mut();
            // Extract the relevant layout properties so the content rect can be calculated
            // when painting.
            view_state.layout_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut new_frame,
            );

            view_state.view_style_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut new_frame,
            );
            if new_frame {
                self.app_state.schedule_style(view_id);
            }
        }
        // If there's any changes to the Taffy style, request layout.
        let layout_style = view_state.borrow().layout_props.to_style();
        let taffy_style = self.direct.clone().apply(layout_style).to_taffy_style();
        if taffy_style != view_state.borrow().taffy_style {
            view_state.borrow_mut().taffy_style = taffy_style;
            view_id.request_layout();
        }

        view.borrow_mut().style_pass(self);

        let mut is_hidden_state = view_state.borrow().is_hidden_state;
        let computed_display = view_state.borrow().combined_style.get(DisplayProp);
        is_hidden_state.transition(
            computed_display,
            || {
                let count = animations_on_remove(view_id, Scope::current());
                view_state.borrow_mut().num_waiting_animations = count;
                count > 0
            },
            || {
                animations_on_create(view_id);
            },
            || {
                stop_reset_remove_animations(view_id);
            },
            || view_state.borrow().num_waiting_animations,
        );
        // if request_layout {
        //     view_id.request_layout();
        // }

        view_state.borrow_mut().is_hidden_state = is_hidden_state;
        let modified = view_state
            .borrow()
            .combined_style
            .clone()
            .apply_opt(is_hidden_state.get_display(), Style::display);

        view_state.borrow_mut().combined_style = modified;

        let mut transform = Affine::IDENTITY;

        let transform_x = match view_state.borrow().layout_props.translate_x() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => pct / 100.,
        };
        let transform_y = match view_state.borrow().layout_props.translate_y() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => pct / 100.,
        };
        transform *= Affine::translate(Vec2 {
            x: transform_x,
            y: transform_y,
        });

        let scale_x = view_state.borrow().layout_props.scale_x().0 / 100.;
        let scale_y = view_state.borrow().layout_props.scale_y().0 / 100.;
        let size = view_id.layout_rect();
        let center_x = size.width() / 2.;
        let center_y = size.height() / 2.;
        transform *= Affine::translate(Vec2 {
            x: center_x,
            y: center_y,
        });
        transform *= Affine::scale_non_uniform(scale_x, scale_y);
        let rotation = view_state.borrow().layout_props.rotation().0;
        transform *= Affine::rotate(rotation);
        transform *= Affine::translate(Vec2 {
            x: -center_x,
            y: -center_y,
        });

        view_state.borrow_mut().transform = transform;

        self.restore();
    }

    pub fn now(&self) -> Instant {
        self.now
    }

    pub fn save(&mut self) {
        self.saved.push(self.current.clone());
        self.saved_disabled.push(self.disabled);
        self.saved_selected.push(self.selected);
    }

    pub fn restore(&mut self) {
        self.current = self.saved.pop().unwrap_or_default();
        self.disabled = self.saved_disabled.pop().unwrap_or_default();
        self.selected = self.saved_selected.pop().unwrap_or_default();
    }

    pub fn get_prop<P: StyleProp>(&self, _prop: P) -> Option<P::Type> {
        self.direct
            .get_prop::<P>()
            .or_else(|| self.current.get_prop::<P>())
    }

    pub fn style(&self) -> Style {
        (*self.current).clone().apply(self.direct.clone())
    }

    pub fn direct_style(&self) -> &Style {
        &self.direct
    }

    pub fn indirect_style(&self) -> &Style {
        &self.current
    }

    pub fn request_transition(&mut self) {
        let id = self.current_view;
        self.app_state_mut().schedule_style(id);
    }

    pub fn app_state_mut(&mut self) -> &mut AppState {
        self.app_state
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }
}

pub struct ComputeLayoutCx<'a> {
    pub(crate) app_state: &'a mut AppState,
    pub(crate) viewport: Rect,
    pub(crate) window_origin: Point,
    pub(crate) saved_viewports: Vec<Rect>,
    pub(crate) saved_window_origins: Vec<Point>,
}

impl<'a> ComputeLayoutCx<'a> {
    pub(crate) fn new(app_state: &'a mut AppState, viewport: Rect) -> Self {
        Self {
            app_state,
            viewport,
            window_origin: Point::ZERO,
            saved_viewports: Vec::new(),
            saved_window_origins: Vec::new(),
        }
    }

    pub fn window_origin(&self) -> Point {
        self.window_origin
    }

    pub fn app_state_mut(&mut self) -> &mut AppState {
        self.app_state
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }

    pub fn save(&mut self) {
        self.saved_viewports.push(self.viewport);
        self.saved_window_origins.push(self.window_origin);
    }

    pub fn restore(&mut self) {
        self.viewport = self.saved_viewports.pop().unwrap_or_default();
        self.window_origin = self.saved_window_origins.pop().unwrap_or_default();
    }

    pub fn current_viewport(&self) -> Rect {
        self.viewport
    }

    /// Internal method used by Floem. This method derives its calculations based on the [Taffy Node](taffy::tree::NodeId) returned by the `View::layout` method.
    ///
    /// It's responsible for:
    /// - calculating and setting the view's origin (local coordinates and window coordinates)
    /// - calculating and setting the view's viewport
    /// - invoking any attached `context::ResizeListener`s
    ///
    /// Returns the bounding rect that encompasses this view and its children
    pub fn compute_view_layout(&mut self, id: ViewId) -> Option<Rect> {
        let view_state = id.state();

        if view_state.borrow().is_hidden_state == IsHiddenState::Hidden {
            view_state.borrow_mut().layout_rect = Rect::ZERO;
            return None;
        }

        self.save();

        let layout = id.get_layout().unwrap_or_default();
        let origin = Point::new(layout.location.x as f64, layout.location.y as f64);
        let this_viewport = view_state.borrow().viewport;
        let this_viewport_origin = this_viewport.unwrap_or_default().origin().to_vec2();
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);
        let parent_viewport = self.viewport.with_origin(
            Point::new(
                self.viewport.x0 - layout.location.x as f64,
                self.viewport.y0 - layout.location.y as f64,
            ) + this_viewport_origin,
        );
        self.viewport = parent_viewport.intersect(size.to_rect());
        if let Some(this_viewport) = this_viewport {
            self.viewport = self.viewport.intersect(this_viewport);
        }

        let window_origin = origin + self.window_origin.to_vec2() - this_viewport_origin;
        self.window_origin = window_origin;
        {
            view_state.borrow_mut().window_origin = window_origin;
        }

        let resize_listener = view_state.borrow().resize_listener.clone();
        if let Some(resize) = resize_listener.as_ref() {
            let mut resize = resize.borrow_mut();
            let new_rect = size.to_rect().with_origin(origin);
            if new_rect != resize.rect {
                resize.rect = new_rect;
                (*resize.callback)(new_rect);
            }
        }

        let move_listener = view_state.borrow().move_listener.clone();
        if let Some(listener) = move_listener {
            let mut listener = listener.borrow_mut();
            if window_origin != listener.window_origin {
                listener.window_origin = window_origin;
                (*listener.callback)(window_origin);
            }
        }

        let view = id.view();
        let child_layout_rect = view.borrow_mut().compute_layout(self);

        let layout_rect = size.to_rect().with_origin(self.window_origin);
        let layout_rect = if let Some(child_layout_rect) = child_layout_rect {
            layout_rect.union(child_layout_rect)
        } else {
            layout_rect
        };

        view_state.borrow_mut().layout_rect = layout_rect;

        self.restore();

        Some(layout_rect)
    }
}

/// Holds current layout state for given position in the tree.
/// You'll use this in the `View::layout` implementation to call `layout_node` on children and to access any font
pub struct LayoutCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> LayoutCx<'a> {
    pub(crate) fn new(app_state: &'a mut AppState) -> Self {
        Self { app_state }
    }

    pub fn app_state_mut(&mut self) -> &mut AppState {
        self.app_state
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }

    /// Responsible for invoking the recalculation of style and thus the layout and
    /// creating or updating the layout of child nodes within the closure.
    ///
    /// You should ensure that all children are laid out within the closure and/or whatever
    /// other work you need to do to ensure that the layout for the returned nodes is correct.
    pub fn layout_node(
        &mut self,
        id: ViewId,
        has_children: bool,
        mut children: impl FnMut(&mut LayoutCx) -> Vec<NodeId>,
    ) -> NodeId {
        let view_state = id.state();
        let node = view_state.borrow().node;
        if !view_state
            .borrow()
            .requested_changes
            .contains(ChangeFlags::LAYOUT)
        {
            return node;
        }
        view_state
            .borrow_mut()
            .requested_changes
            .remove(ChangeFlags::LAYOUT);
        let layout_style = view_state.borrow().layout_props.to_style();
        let animate_out_display = view_state.borrow().is_hidden_state.get_display();
        let style = view_state
            .borrow()
            .combined_style
            .clone()
            .apply(layout_style)
            .apply_opt(animate_out_display, Style::display)
            .to_taffy_style();
        let _ = id.taffy().borrow_mut().set_style(node, style);

        if has_children {
            let nodes = children(self);
            let _ = id.taffy().borrow_mut().set_children(node, &nodes);
        }

        node
    }

    /// Internal method used by Floem to invoke the user-defined `View::layout` method.
    pub fn layout_view(&mut self, view: &mut dyn View) -> NodeId {
        view.layout(self)
    }
}

std::thread_local! {
    /// Holds the ID of a View being painted very briefly if it is being rendered as
    /// a moving drag image.  Since that is a relatively unusual thing to need, it
    /// makes more sense to use a thread local for it and avoid cluttering the fields
    /// and memory footprint of PaintCx or PaintState or ViewId with a field for it.
    /// This is ephemerally set before paint calls that are painting the view in a
    /// location other than its natural one for purposes of drag and drop.
    static CURRENT_DRAG_PAINTING_ID : std::cell::Cell<Option<ViewId>> = const { std::cell::Cell::new(None) };
}

pub struct PaintCx<'a> {
    pub(crate) app_state: &'a mut AppState,
    pub(crate) paint_state: &'a mut PaintState,
    pub(crate) transform: Affine,
    pub(crate) clip: Option<RoundedRect>,
    pub(crate) z_index: Option<i32>,
    pub(crate) saved_transforms: Vec<Affine>,
    pub(crate) saved_clips: Vec<Option<RoundedRect>>,
    pub(crate) saved_z_indexes: Vec<Option<i32>>,
    pub gpu_resources: Option<GpuResources>,
    pub window: Option<Arc<dyn Window>>,
}

impl PaintCx<'_> {
    pub fn save(&mut self) {
        self.saved_transforms.push(self.transform);
        self.saved_clips.push(self.clip);
        self.saved_z_indexes.push(self.z_index);
    }

    pub fn restore(&mut self) {
        self.transform = self.saved_transforms.pop().unwrap_or_default();
        self.clip = self.saved_clips.pop().unwrap_or_default();
        self.z_index = self.saved_z_indexes.pop().unwrap_or_default();
        self.paint_state
            .renderer_mut()
            .set_transform(self.transform);
        if let Some(z_index) = self.z_index {
            self.paint_state.renderer_mut().set_z_index(z_index);
        } else {
            self.paint_state.renderer_mut().set_z_index(0);
        }
        if let Some(rect) = self.clip {
            self.paint_state.renderer_mut().clip(&rect);
        } else {
            self.paint_state.renderer_mut().clear_clip();
        }
    }

    /// Allows a `View` to determine if it is being called in order to
    /// paint a *draggable* image of itself during a drag (likely
    /// `draggable()` was called on the `View` or `ViewId`) as opposed
    /// to a normal paint in order to alter the way it renders itself.
    pub fn is_drag_paint(&self, id: ViewId) -> bool {
        // This could be an associated function, but it is likely
        // a Good Thing to restrict access to cases when the caller actually
        // has a PaintCx, and that doesn't make it a breaking change to
        // use instance methods in the future.
        if let Some(dragging) = CURRENT_DRAG_PAINTING_ID.get() {
            return dragging == id;
        }
        false
    }

    /// paint the children of this view
    pub fn paint_children(&mut self, id: ViewId) {
        let children = id.children();
        for child in children {
            self.paint_view(child);
        }
    }

    /// The entry point for painting a view. You shouldn't need to implement this yourself. Instead, implement [`View::paint`].
    /// It handles the internal work before and after painting [`View::paint`] implementations.
    /// It is responsible for
    /// - managing hidden status
    /// - clipping
    /// - painting computed styles like background color, border, font-styles, and z-index and handling painting requirements of drag and drop
    pub fn paint_view(&mut self, id: ViewId) {
        if id.style_has_hidden() {
            return;
        }
        let view = id.view();
        let view_state = id.state();

        self.save();
        let size = self.transform(id);
        let is_empty = self
            .clip
            .map(|rect| rect.rect().intersect(size.to_rect()).is_zero_area())
            .unwrap_or(false);
        if !is_empty {
            let style = view_state.borrow().combined_style.clone();
            let view_style_props = view_state.borrow().view_style_props.clone();
            let layout_props = view_state.borrow().layout_props.clone();

            if let Some(z_index) = style.get(ZIndex) {
                self.set_z_index(z_index);
            }

            paint_bg(self, &view_style_props, size);

            view.borrow_mut().paint(self);
            paint_border(self, &layout_props, &view_style_props, size);
            paint_outline(self, &view_style_props, size)
        }
        let mut drag_set_to_none = false;

        if let Some(dragging) = self.app_state.dragging.as_ref() {
            if dragging.id == id {
                let transform = if let Some((released_at, release_location)) =
                    dragging.released_at.zip(dragging.release_location)
                {
                    let easing = Linear;
                    const ANIMATION_DURATION_MS: f64 = 300.0;
                    let elapsed = released_at.elapsed().as_millis() as f64;
                    let progress = elapsed / ANIMATION_DURATION_MS;

                    if !(easing.finished(progress)) {
                        let offset_scale = 1.0 - easing.eval(progress);
                        let release_offset = release_location.to_vec2() - dragging.offset;

                        // Schedule next animation frame
                        exec_after(Duration::from_millis(8), move |_| {
                            id.request_paint();
                        });

                        Some(self.transform * Affine::translate(release_offset * offset_scale))
                    } else {
                        drag_set_to_none = true;
                        None
                    }
                } else {
                    // Handle active dragging
                    let translation =
                        self.app_state.last_cursor_location.to_vec2() - dragging.offset;
                    Some(self.transform.with_translation(translation))
                };

                if let Some(transform) = transform {
                    self.save();
                    self.transform = transform;
                    self.paint_state
                        .renderer_mut()
                        .set_transform(self.transform);
                    self.set_z_index(1000);
                    self.clear_clip();

                    // Apply styles
                    let style = view_state.borrow().combined_style.clone();
                    let mut view_style_props = view_state.borrow().view_style_props.clone();

                    if let Some(dragging_style) = view_state.borrow().dragging_style.clone() {
                        let style = style.apply(dragging_style);
                        let mut _new_frame = false;
                        view_style_props.read_explicit(
                            &style,
                            &style,
                            &Instant::now(),
                            &mut _new_frame,
                        );
                    }

                    // Paint with drag styling
                    let layout_props = view_state.borrow().layout_props.clone();

                    // Important: If any method early exit points are added in this
                    // code block, they MUST call CURRENT_DRAG_PAINTING_ID.take() before
                    // returning.

                    CURRENT_DRAG_PAINTING_ID.set(Some(id));

                    paint_bg(self, &view_style_props, size);
                    view.borrow_mut().paint(self);
                    paint_border(self, &layout_props, &view_style_props, size);
                    paint_outline(self, &view_style_props, size);

                    self.restore();

                    CURRENT_DRAG_PAINTING_ID.take();
                }
            }
        }

        if drag_set_to_none {
            self.app_state.dragging = None;
        }
        self.restore();
    }

    /// Clip the drawing area to the given shape.
    pub fn clip(&mut self, shape: &impl Shape) {
        let rect = if let Some(rect) = shape.as_rect() {
            rect.to_rounded_rect(0.0)
        } else if let Some(rect) = shape.as_rounded_rect() {
            rect
        } else {
            let rect = shape.bounding_box();
            rect.to_rounded_rect(0.0)
        };

        let rect = if let Some(existing) = self.clip {
            let rect = existing.rect().intersect(rect.rect());
            self.paint_state.renderer_mut().clip(&rect);
            rect.to_rounded_rect(0.0)
        } else {
            self.paint_state.renderer_mut().clip(&shape);
            rect
        };
        self.clip = Some(rect);
    }

    /// Remove clipping so the entire window can be rendered to.
    pub fn clear_clip(&mut self) {
        self.clip = None;
        self.paint_state.renderer_mut().clear_clip();
    }

    pub fn offset(&mut self, offset: (f64, f64)) {
        let mut new = self.transform.as_coeffs();
        new[4] += offset.0;
        new[5] += offset.1;
        self.transform = Affine::new(new);
        self.paint_state
            .renderer_mut()
            .set_transform(self.transform);
        if let Some(rect) = self.clip.as_mut() {
            let raidus = rect.radii();
            *rect = rect
                .rect()
                .with_origin(rect.origin() - Vec2::new(offset.0, offset.1))
                .to_rounded_rect(raidus);
        }
    }

    pub fn transform(&mut self, id: ViewId) -> Size {
        if let Some(layout) = id.get_layout() {
            let offset = layout.location;
            self.transform *= Affine::translate(Vec2 {
                x: offset.x as f64,
                y: offset.y as f64,
            });
            self.transform *= id.state().borrow().transform;

            self.paint_state
                .renderer_mut()
                .set_transform(self.transform);

            if let Some(rect) = self.clip.as_mut() {
                let raidus = rect.radii();
                *rect = rect
                    .rect()
                    .with_origin(rect.origin() - Vec2::new(offset.x as f64, offset.y as f64))
                    .to_rounded_rect(raidus);
            }

            Size::new(layout.size.width as f64, layout.size.height as f64)
        } else {
            Size::ZERO
        }
    }

    pub(crate) fn set_z_index(&mut self, z_index: i32) {
        self.z_index = Some(z_index);
        self.paint_state.renderer_mut().set_z_index(z_index);
    }

    pub fn is_focused(&self, id: ViewId) -> bool {
        self.app_state.is_focused(&id)
    }
}

// TODO: should this be private?
pub enum PaintState {
    /// The renderer is not yet initialized. This state is used to wait for the GPU resources to be acquired.
    PendingGpuResources {
        window: Arc<dyn Window>,
        rx: Receiver<Result<(GpuResources, wgpu::Surface<'static>), GpuResourceError>>,
        font_embolden: f32,
        /// This field holds an instance of `Renderer::Uninitialized` until the GPU resources are acquired,
        /// which will be returned in `PaintState::renderer` and `PaintState::renderer_mut`.
        /// All calls to renderer methods will be no-ops until the renderer is initialized.
        ///
        /// Previously, `PaintState::renderer` and `PaintState::renderer_mut` would panic if called when the renderer was uninitialized.
        /// However, this turned out to be hard to handle properly and led to panics, especially since the rest of the application code can't control when the renderer is initialized.
        renderer: crate::renderer::Renderer,
    },
    /// The renderer is initialized and ready to paint.
    Initialized { renderer: crate::renderer::Renderer },
}

impl PaintState {
    pub fn new_pending(
        window: Arc<dyn Window>,
        rx: Receiver<Result<(GpuResources, wgpu::Surface<'static>), GpuResourceError>>,
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        Self::PendingGpuResources {
            window,
            rx,
            font_embolden,
            renderer: Renderer::Uninitialized { scale, size },
        }
    }

    pub fn new(
        window: Arc<dyn Window>,
        surface: wgpu::Surface<'static>,
        gpu_resources: GpuResources,
        scale: f64,
        size: Size,
        font_embolden: f32,
    ) -> Self {
        let renderer = crate::renderer::Renderer::new(
            window.clone(),
            gpu_resources,
            surface,
            scale,
            size,
            font_embolden,
        );
        Self::Initialized { renderer }
    }

    pub(crate) fn renderer(&self) -> &crate::renderer::Renderer {
        match self {
            PaintState::PendingGpuResources { renderer, .. } => renderer,
            PaintState::Initialized { renderer } => renderer,
        }
    }

    pub(crate) fn renderer_mut(&mut self) -> &mut crate::renderer::Renderer {
        match self {
            PaintState::PendingGpuResources { renderer, .. } => renderer,
            PaintState::Initialized { renderer } => renderer,
        }
    }

    pub(crate) fn resize(&mut self, scale: f64, size: Size) {
        self.renderer_mut().resize(scale, size);
    }

    pub(crate) fn set_scale(&mut self, scale: f64) {
        self.renderer_mut().set_scale(scale);
    }
}

pub struct UpdateCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl UpdateCx<'_> {
    pub fn app_state_mut(&mut self) -> &mut AppState {
        self.app_state
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }
}

impl Deref for PaintCx<'_> {
    type Target = crate::renderer::Renderer;

    fn deref(&self) -> &Self::Target {
        self.paint_state.renderer()
    }
}

impl DerefMut for PaintCx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.paint_state.renderer_mut()
    }
}

fn animations_on_remove(id: ViewId, scope: Scope) -> u16 {
    let mut wait_for = 0;
    let state = id.state();
    let mut state = state.borrow_mut();
    state.num_waiting_animations = 0;
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_remove && !matches!(anim.repeat_mode, RepeatMode::LoopForever) {
            anim.reverse_mut();
            request_style = true;
            wait_for += 1;
            let trigger = anim.on_visual_complete;
            scope.create_updater(
                move || trigger.track(),
                move |_| {
                    id.transition_anim_complete();
                },
            );
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children()
        .into_iter()
        .fold(wait_for, |acc, id| acc + animations_on_remove(id, scope))
}
fn stop_reset_remove_animations(id: ViewId) {
    let state = id.state();
    let mut state = state.borrow_mut();
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_remove
            && anim.state_kind() == AnimStateKind::PassInProgress
            && !matches!(anim.repeat_mode, RepeatMode::LoopForever)
        {
            anim.start_mut();
            request_style = true;
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children()
        .into_iter()
        .for_each(stop_reset_remove_animations)
}

fn animations_on_create(id: ViewId) {
    let state = id.state();
    let mut state = state.borrow_mut();
    state.num_waiting_animations = 0;
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_create && !matches!(anim.repeat_mode, RepeatMode::LoopForever) {
            anim.start_mut();
            request_style = true;
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children().into_iter().for_each(animations_on_create);
}
