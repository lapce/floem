use floem_renderer::Renderer as FloemRenderer;
use floem_winit::window::CursorIcon;
use kurbo::{Affine, Insets, Point, Rect, RoundedRect, Shape, Size, Vec2};
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    ops::{Deref, DerefMut},
    rc::Rc,
    time::Instant,
};
use taffy::{
    prelude::{Layout, NodeId},
    style::{AvailableSpace, Display},
};

use crate::{
    action::{exec_after, show_context_menu},
    animate::AnimId,
    event::{Event, EventListener},
    id::Id,
    inspector::CaptureState,
    menu::Menu,
    responsive::{GridBreakpoints, ScreenSizeBp},
    style::{
        BuiltinStyle, CursorStyle, DisplayProp, Style, StyleClassRef, StyleProp, StyleSelector,
        ZIndex,
    },
    unit::PxPct,
    view::{paint_bg, paint_border, paint_outline, ViewData, Widget},
    view_data::ChangeFlags,
};

pub use crate::view_data::ViewState;

/// Control whether an event will continue propagating or whether it should stop.
pub enum EventPropagation {
    /// Stop event propagation and mark the event as processed
    Stop,
    /// Let event propagation continue
    Continue,
}
impl EventPropagation {
    pub fn is_continue(&self) -> bool {
        matches!(self, EventPropagation::Continue)
    }

    pub fn is_stop(&self) -> bool {
        matches!(self, EventPropagation::Stop)
    }

    pub fn is_processed(&self) -> bool {
        matches!(self, EventPropagation::Stop)
    }
}

pub type EventCallback = dyn Fn(&Event) -> EventPropagation;
pub type ResizeCallback = dyn Fn(Rect);
pub type MenuCallback = dyn Fn() -> Menu;

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
    pub(crate) id: Id,
    pub(crate) offset: Vec2,
    pub(crate) released_at: Option<std::time::Instant>,
}

pub(crate) enum FrameUpdate {
    Style(Id),
    Layout(Id),
    Paint(Id),
}

/// Encapsulates and owns the global state of the application,
/// including the `ViewState` of each view.
pub struct AppState {
    /// keyboard focus
    pub(crate) focus: Option<Id>,
    /// when a view is active, it gets mouse event even when the mouse is
    /// not on it
    pub(crate) active: Option<Id>,
    pub(crate) root: Option<NodeId>,
    pub(crate) root_size: Size,
    pub(crate) scale: f64,
    pub taffy: taffy::TaffyTree,
    pub(crate) view_states: HashMap<Id, ViewState>,
    stale_view_state: ViewState,
    pub(crate) scheduled_updates: Vec<FrameUpdate>,
    pub(crate) request_compute_layout: bool,
    pub(crate) request_paint: bool,
    pub(crate) disabled: HashSet<Id>,
    pub(crate) keyboard_navigable: HashSet<Id>,
    pub(crate) draggable: HashSet<Id>,
    pub(crate) dragging: Option<DragState>,
    pub(crate) drag_start: Option<(Id, Point)>,
    pub(crate) dragging_over: HashSet<Id>,
    pub(crate) screen_size_bp: ScreenSizeBp,
    pub(crate) grid_bps: GridBreakpoints,
    pub(crate) clicking: HashSet<Id>,
    pub(crate) hovered: HashSet<Id>,
    /// This keeps track of all views that have an animation,
    /// regardless of the status of the animation
    pub(crate) cursor: Option<CursorStyle>,
    pub(crate) last_cursor: CursorIcon,
    pub(crate) keyboard_navigation: bool,
    pub(crate) window_menu: HashMap<usize, Box<dyn Fn()>>,
    pub(crate) context_menu: HashMap<usize, Box<dyn Fn()>>,

    /// This is set if we're currently capturing the window for the inspector.
    pub(crate) capture: Option<CaptureState>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let mut taffy = taffy::TaffyTree::new();
        taffy.disable_rounding();
        Self {
            root: None,
            focus: None,
            active: None,
            scale: 1.0,
            root_size: Size::ZERO,
            screen_size_bp: ScreenSizeBp::Xs,
            stale_view_state: ViewState::new(&mut taffy),
            taffy,
            view_states: HashMap::new(),
            scheduled_updates: Vec::new(),
            request_paint: false,
            request_compute_layout: false,
            disabled: HashSet::new(),
            keyboard_navigable: HashSet::new(),
            draggable: HashSet::new(),
            dragging: None,
            drag_start: None,
            dragging_over: HashSet::new(),
            clicking: HashSet::new(),
            hovered: HashSet::new(),
            cursor: None,
            last_cursor: CursorIcon::Default,
            keyboard_navigation: false,
            grid_bps: GridBreakpoints::default(),
            window_menu: HashMap::new(),
            context_menu: HashMap::new(),
            capture: None,
        }
    }

    pub fn view_state(&mut self, id: Id) -> &mut ViewState {
        if !id.has_id_path() {
            // if the id doesn't have a id path, that means it's been cleaned up,
            // so we shouldn't create a new ViewState for this Id.
            return &mut self.stale_view_state;
        }
        self.view_states
            .entry(id)
            .or_insert_with(|| ViewState::new(&mut self.taffy))
    }

    /// This removes a view from the app state.
    pub fn remove_view(&mut self, view: &mut dyn Widget) {
        view.for_each_child_mut(&mut |child| {
            self.remove_view(child);
            false
        });
        let id = view.view_data().id();
        let view_state = self.view_state(id);
        if let Some(action) = view_state.cleanup_listener.as_ref() {
            action();
        }
        let node = view_state.node;
        if let Ok(children) = self.taffy.children(node) {
            for child in children {
                let _ = self.taffy.remove(child);
            }
        }
        let _ = self.taffy.remove(node);
        id.remove_id_path();
        self.view_states.remove(&id);
        self.disabled.remove(&id);
        self.keyboard_navigable.remove(&id);
        self.draggable.remove(&id);
        self.dragging_over.remove(&id);
        self.clicking.remove(&id);
        self.hovered.remove(&id);
        self.clicking.remove(&id);
        if self.focus == Some(id) {
            self.focus = None;
        }
        if self.active == Some(id) {
            self.active = None;
        }
    }

    pub fn is_hidden(&self, id: Id) -> bool {
        self.view_states
            .get(&id)
            .map(|s| s.combined_style.get(DisplayProp) == Display::None)
            .unwrap_or(false)
    }

    /// Is this view, or any parent view, marked as hidden
    pub fn is_hidden_recursive(&self, id: Id) -> bool {
        id.id_path()
            .unwrap()
            .dispatch()
            .iter()
            .any(|id| self.is_hidden(*id))
    }

    pub(crate) fn can_focus(&self, id: Id) -> bool {
        self.keyboard_navigable.contains(&id)
            && !self.is_disabled(&id)
            && !self.is_hidden_recursive(id)
    }

    pub fn is_hovered(&self, id: &Id) -> bool {
        self.hovered.contains(id)
    }

    pub fn is_disabled(&self, id: &Id) -> bool {
        self.disabled.contains(id)
    }

    pub fn is_focused(&self, id: &Id) -> bool {
        self.focus.map(|f| &f == id).unwrap_or(false)
    }

    pub fn is_active(&self, id: &Id) -> bool {
        self.active.map(|a| &a == id).unwrap_or(false)
    }

    pub fn is_clicking(&self, id: &Id) -> bool {
        self.clicking.contains(id)
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
            .as_ref()
            .map(|d| d.released_at.is_none())
            .unwrap_or(false)
    }

    pub fn set_root_size(&mut self, size: Size) {
        self.root_size = size;
        self.compute_layout();
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compute_style(
        &mut self,
        id: Id,
        view_data: &mut ViewData,
        view_style: Option<Style>,
        view_interact_state: InteractionState,
        view_class: Option<StyleClassRef>,
        classes: &[StyleClassRef],
        context: &Style,
    ) -> bool {
        let screen_size_bp = self.screen_size_bp;
        let view_state = self.view_state(id);
        view_state.compute_style(
            view_data,
            view_style,
            view_interact_state,
            screen_size_bp,
            view_class,
            classes,
            context,
        )
    }

    pub(crate) fn get_computed_style(&mut self, id: Id) -> &Style {
        let view_state = self.view_state(id);
        &view_state.combined_style
    }

    pub fn get_builtin_style(&mut self, id: Id) -> BuiltinStyle<'_> {
        self.get_computed_style(id).builtin()
    }

    pub fn compute_layout(&mut self) {
        if let Some(root) = self.root {
            let _ = self.taffy.compute_layout(
                root,
                taffy::prelude::Size {
                    width: AvailableSpace::Definite((self.root_size.width / self.scale) as f32),
                    height: AvailableSpace::Definite((self.root_size.height / self.scale) as f32),
                },
            );
        }
    }

    /// Requests style for a view and all direct and indirect children.
    pub fn request_style_recursive(&mut self, id: Id) {
        let view = self.view_state(id);
        view.request_style_recursive = true;
        self.request_style(id);
    }

    /// Request that this the `id` view be styled, laid out and painted again.
    /// This will recursively request this for all parents.
    pub fn request_all(&mut self, id: Id) {
        self.request_changes(id, ChangeFlags::all());
        self.request_paint(id);
    }

    pub(crate) fn request_changes(&mut self, id: Id, flags: ChangeFlags) {
        let view = self.view_state(id);
        if view.requested_changes.contains(flags) {
            return;
        }
        view.requested_changes.insert(flags);
        if let Some(parent) = id.parent() {
            self.request_changes(parent, flags);
        }
    }

    /// Requests that the style pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_style(&mut self, id: Id) {
        self.scheduled_updates.push(FrameUpdate::Style(id));
    }

    /// Requests that the layout pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_layout(&mut self, id: Id) {
        self.scheduled_updates.push(FrameUpdate::Layout(id));
    }

    /// Requests that the paint pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_paint(&mut self, id: Id) {
        self.scheduled_updates.push(FrameUpdate::Paint(id));
    }

    pub fn request_style(&mut self, id: Id) {
        self.request_changes(id, ChangeFlags::STYLE)
    }

    pub fn request_layout(&mut self, id: Id) {
        self.request_changes(id, ChangeFlags::LAYOUT)
    }

    /// Requests that `compute_layout` will run for `_id` and all direct and indirect children.
    pub fn request_compute_layout_recursive(&mut self, _id: Id) {
        self.request_compute_layout = true;
    }

    // `Id` is unused currently, but could be used to calculate damage regions.
    pub fn request_paint(&mut self, _id: Id) {
        self.request_paint = true;
    }

    /// `viewport` is relative to the `id` view.
    pub(crate) fn set_viewport(&mut self, id: Id, viewport: Rect) {
        let view = self.view_state(id);
        view.viewport = Some(viewport);
    }

    /// This gets the Taffy Layout and adjusts it to be relative to the parent `Widget`.
    pub(crate) fn get_layout(&self, id: Id) -> Option<Layout> {
        let widget_parent = id
            .parent()
            .and_then(|id| self.view_states.get(&id).map(|view| view.node));

        let mut node = self.view_states.get(&id).map(|view| view.node)?;
        let mut layout = *self.taffy.layout(node).ok()?;

        loop {
            let parent = self.taffy.parent(node);

            if parent == widget_parent {
                break;
            }

            node = parent?;

            layout.location = layout.location + self.taffy.layout(node).ok()?.location;
        }

        Some(layout)
    }

    /// Returns the layout rect excluding borders, padding and position.
    /// This is relative to the view.
    pub fn get_content_rect(&mut self, id: Id) -> Rect {
        let size = self
            .get_layout(id)
            .map(|layout| layout.size)
            .unwrap_or_default();
        let rect = Size::new(size.width as f64, size.height as f64).to_rect();
        let view_state = self.view_state(id);
        let props = &view_state.layout_props;
        let pixels = |px_pct, abs| match px_pct {
            PxPct::Px(v) => v,
            PxPct::Pct(pct) => pct * abs,
        };
        rect.inset(-Insets {
            x0: props.border_left().0 + pixels(props.padding_left(), rect.width()),
            x1: props.border_right().0 + pixels(props.padding_right(), rect.width()),
            y0: props.border_top().0 + pixels(props.padding_top(), rect.height()),
            y1: props.border_bottom().0 + pixels(props.padding_bottom(), rect.height()),
        })
    }

    pub fn get_layout_rect(&mut self, id: Id) -> Rect {
        self.view_state(id).layout_rect
    }

    pub(crate) fn update_active(&mut self, id: Id) {
        if self.active.is_some() {
            // the first update_active wins, so if there's active set,
            // don't do anything.
            return;
        }
        self.active = Some(id);

        // To apply the styles of the Active selector
        if self.has_style_for_sel(id, StyleSelector::Active) {
            self.request_style(id);
        }
    }

    pub(crate) fn update_screen_size_bp(&mut self, size: Size) {
        let bp = self.grid_bps.get_width_bp(size.width);
        self.screen_size_bp = bp;
    }

    pub(crate) fn clear_focus(&mut self) {
        if let Some(old_id) = self.focus {
            // To remove the styles applied by the Focus selector
            if self.has_style_for_sel(old_id, StyleSelector::Focus)
                || self.has_style_for_sel(old_id, StyleSelector::FocusVisible)
            {
                self.request_style(old_id);
            }
        }

        self.focus = None;
    }

    pub(crate) fn update_focus(&mut self, id: Id, keyboard_navigation: bool) {
        if self.focus.is_some() {
            return;
        }

        self.focus = Some(id);
        self.keyboard_navigation = keyboard_navigation;

        if self.has_style_for_sel(id, StyleSelector::Focus)
            || self.has_style_for_sel(id, StyleSelector::FocusVisible)
        {
            self.request_style(id);
        }
    }

    pub(crate) fn has_style_for_sel(&mut self, id: Id, selector_kind: StyleSelector) -> bool {
        let view_state = self.view_state(id);

        view_state.has_style_selectors.has(selector_kind)
            || (selector_kind == StyleSelector::Dragging && view_state.dragging_style.is_some())
    }

    // TODO: animated should be a HashMap<Id, AnimId>
    // so we don't have to loop through all view states
    pub(crate) fn get_view_id_by_anim_id(&self, anim_id: AnimId) -> Id {
        *self
            .view_states
            .iter()
            .find(|(_, vs)| {
                vs.animation
                    .as_ref()
                    .map(|a| a.id() == anim_id)
                    .unwrap_or(false)
            })
            .unwrap()
            .0
    }

    pub(crate) fn update_context_menu(&mut self, menu: &mut Menu) {
        if let Some(action) = menu.item.action.take() {
            self.context_menu.insert(menu.item.id as usize, action);
        }
        for child in menu.children.iter_mut() {
            match child {
                crate::menu::MenuEntry::Separator => {}
                crate::menu::MenuEntry::Item(item) => {
                    if let Some(action) = item.action.take() {
                        self.context_menu.insert(item.id as usize, action);
                    }
                }
                crate::menu::MenuEntry::SubMenu(m) => {
                    self.update_context_menu(m);
                }
            }
        }
    }

    pub(crate) fn get_event_listeners(
        &self,
        id: Id,
        listener: &EventListener,
    ) -> Option<&Vec<Box<EventCallback>>> {
        self.view_states
            .get(&id)
            .and_then(|s| s.event_listeners.get(listener))
    }

    pub(crate) fn apply_event(
        &self,
        id: Id,
        listener: &EventListener,
        event: &crate::event::Event,
    ) -> Option<EventPropagation> {
        self.view_states
            .get(&id)
            .and_then(|s| s.apply_event(listener, event))
    }

    pub(crate) fn focus_changed(&mut self, old: Option<Id>, new: Option<Id>) {
        if let Some(id) = new {
            // To apply the styles of the Focus selector
            if self.has_style_for_sel(id, StyleSelector::Focus)
                || self.has_style_for_sel(id, StyleSelector::FocusVisible)
            {
                self.request_style_recursive(id);
            }
            self.view_states.get(&id).and_then(|state| {
                state.apply_event(&EventListener::FocusGained, &Event::FocusGained)
            });
        }

        if let Some(old_id) = old {
            // To remove the styles applied by the Focus selector
            if self.has_style_for_sel(old_id, StyleSelector::Focus)
                || self.has_style_for_sel(old_id, StyleSelector::FocusVisible)
            {
                self.request_style_recursive(old_id);
            }
            self.view_states
                .get(&old_id)
                .and_then(|state| state.apply_event(&EventListener::FocusLost, &Event::FocusLost));
        }
    }
}

/// A bundle of helper methods to be used by `View::event` handlers
pub struct EventCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> EventCx<'a> {
    /// request that this node be styled, laid out and painted again
    /// This will recursively request this for all parents.
    pub fn request_all(&mut self, id: Id) {
        self.app_state.request_style(id);
        self.app_state.request_layout(id);
    }

    /// request that this node be styled again
    /// This will recursively request style for all parents.
    pub fn request_style(&mut self, id: Id) {
        self.app_state.request_style(id);
    }

    /// request that this node be laid out again
    /// This will recursively request layout for all parents and set the `ChangeFlag::LAYOUT` at root
    pub fn request_layout(&mut self, id: Id) {
        self.app_state.request_layout(id);
    }

    /// request that this node be painted again
    pub fn request_paint(&mut self, id: Id) {
        self.app_state.request_paint(id);
    }

    pub fn app_state_mut(&mut self) -> &mut AppState {
        self.app_state
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }

    pub fn update_active(&mut self, id: Id) {
        self.app_state.update_active(id);
    }

    pub fn is_active(&self, id: Id) -> bool {
        self.app_state.is_active(&id)
    }

    #[allow(unused)]
    pub(crate) fn update_focus(&mut self, id: Id, keyboard_navigation: bool) {
        self.app_state.update_focus(id, keyboard_navigation);
    }

    pub fn get_computed_style(&self, id: Id) -> Option<&Style> {
        self.app_state
            .view_states
            .get(&id)
            .map(|s| &s.combined_style)
    }

    pub fn get_layout(&self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
    }

    /// Internal method used by Floem. This can be called from parent `View`s to propagate an event to the child `View`.
    pub fn view_event(
        &mut self,
        view: &mut dyn Widget,
        id_path: Option<&[Id]>,
        event: Event,
    ) -> EventPropagation {
        if self.should_send(view.view_data().id(), &event) {
            self.unconditional_view_event(view, id_path, event)
        } else {
            EventPropagation::Continue
        }
    }

    /// Internal method used by Floem. This can be called from parent `View`s to propagate an event to the child `View`.
    pub(crate) fn unconditional_view_event(
        &mut self,
        view: &mut dyn Widget,
        id_path: Option<&[Id]>,
        event: Event,
    ) -> EventPropagation {
        let id = view.view_data().id();
        if self.app_state.is_hidden(id) {
            // we don't process events for hidden view
            return EventPropagation::Continue;
        }
        if self.app_state.is_disabled(&id) && !event.allow_disabled() {
            // if the view is disabled and the event is not processed
            // for disabled views
            return EventPropagation::Continue;
        }

        // offset the event positions if the event has positions
        // e.g. pointer events, so that the position is relative
        // to the view, taking into account of the layout location
        // of the view and the viewport of the view if it's in a scroll.
        let event = self.offset_event(id, event);

        // if there's id_path, it's an event only for a view.
        if let Some(id_path) = id_path {
            if id_path.is_empty() {
                // this happens when the parent is the destination,
                // but the parent just passed the event on,
                // so it's not really for this view and we stop
                // the event propagation.
                return EventPropagation::Continue;
            }

            let id = id_path[0];
            let id_path = &id_path[1..];

            if id != view.view_data().id() {
                // This shouldn't happen
                return EventPropagation::Continue;
            }

            // we're the parent of the event destination, so pass it on to the child
            if !id_path.is_empty() {
                if let Some(child) = view.child_mut(id_path[0]) {
                    return self.unconditional_view_event(child, Some(id_path), event.clone());
                } else {
                    // we don't have the child, stop the event propagation
                    return EventPropagation::Continue;
                }
            }
        }

        // if the event was dispatched to an id_path, the event is supposed to be only
        // handled by this view only, so we pass an empty id_path
        // and the event propagation would be stopped at this view
        if view
            .event(
                self,
                if id_path.is_some() { Some(&[]) } else { None },
                event.clone(),
            )
            .is_processed()
        {
            return EventPropagation::Stop;
        }

        let mut is_down_and_has_click = false;

        match &event {
            Event::PointerDown(event) => {
                self.app_state.clicking.insert(id);
                if event.button.is_primary() {
                    let rect = self.get_size(id).unwrap_or_default().to_rect();
                    let now_focused = rect.contains(event.pos);

                    if now_focused {
                        if self.app_state.keyboard_navigable.contains(&id) {
                            // if the view can be focused, we update the focus
                            self.app_state.update_focus(id, false);
                        }
                        if event.count == 2
                            && self.has_event_listener(id, EventListener::DoubleClick)
                        {
                            let view_state = self.app_state.view_state(id);
                            view_state.last_pointer_down = Some(event.clone());
                        }
                        if self.has_event_listener(id, EventListener::Click) {
                            let view_state = self.app_state.view_state(id);
                            view_state.last_pointer_down = Some(event.clone());
                            is_down_and_has_click = true;
                        }

                        let bottom_left = {
                            let layout = self.app_state.view_state(id).layout_rect;
                            Point::new(layout.x0, layout.y1)
                        };
                        if let Some(menu) = &self.app_state.view_state(id).popout_menu {
                            show_context_menu(menu(), Some(bottom_left));
                            return EventPropagation::Stop;
                        }
                        if self.app_state.draggable.contains(&id)
                            && self.app_state.drag_start.is_none()
                        {
                            self.app_state.drag_start = Some((id, event.pos));
                        }
                    }
                } else if event.button.is_secondary() {
                    let rect = self.get_size(id).unwrap_or_default().to_rect();
                    let now_focused = rect.contains(event.pos);

                    if now_focused {
                        if self.app_state.keyboard_navigable.contains(&id) {
                            // if the view can be focused, we update the focus
                            self.app_state.update_focus(id, false);
                        }
                        if self.has_event_listener(id, EventListener::SecondaryClick) {
                            let view_state = self.app_state.view_state(id);
                            view_state.last_pointer_down = Some(event.clone());
                        }
                    }
                }
            }
            Event::PointerMove(pointer_event) => {
                let rect = self.get_size(id).unwrap_or_default().to_rect();
                if rect.contains(pointer_event.pos) {
                    if self.app_state.is_dragging() {
                        self.app_state.dragging_over.insert(id);
                        self.apply_event(id, &EventListener::DragOver, &event);
                    } else {
                        self.app_state.hovered.insert(id);
                        let style = self.app_state.get_builtin_style(id);
                        if let Some(cursor) = style.cursor() {
                            if self.app_state.cursor.is_none() {
                                self.app_state.cursor = Some(cursor);
                            }
                        }
                    }
                }
                if self.app_state.draggable.contains(&id) {
                    if let Some((_, drag_start)) = self
                        .app_state
                        .drag_start
                        .as_ref()
                        .filter(|(drag_id, _)| drag_id == &id)
                    {
                        let vec2 = pointer_event.pos - *drag_start;

                        if let Some(dragging) = self
                            .app_state
                            .dragging
                            .as_mut()
                            .filter(|d| d.id == id && d.released_at.is_none())
                        {
                            // update the dragging offset if the view is dragging and not released
                            dragging.offset = vec2;
                            id.request_paint();
                        } else if vec2.x.abs() + vec2.y.abs() > 1.0 {
                            // start dragging when moved 1 px
                            self.app_state.active = None;
                            self.update_active(id);
                            self.app_state.dragging = Some(DragState {
                                id,
                                offset: vec2,
                                released_at: None,
                            });
                            id.request_paint();
                            self.apply_event(id, &EventListener::DragStart, &event);
                        }
                    }
                }
                if self
                    .apply_event(id, &EventListener::PointerMove, &event)
                    .is_some_and(|prop| prop.is_processed())
                {
                    return EventPropagation::Stop;
                }
            }
            Event::PointerUp(pointer_event) => {
                if pointer_event.button.is_primary() {
                    let rect = self.get_size(id).unwrap_or_default().to_rect();
                    let on_view = rect.contains(pointer_event.pos);

                    if id_path.is_none() {
                        if on_view {
                            if let Some(dragging) = self.app_state.dragging.as_mut() {
                                let dragging_id = dragging.id;
                                if self
                                    .apply_event(id, &EventListener::Drop, &event)
                                    .is_some_and(|prop| prop.is_processed())
                                {
                                    // if the drop is processed, we set dragging to none so that the animation
                                    // for the dragged view back to its original position isn't played.
                                    self.app_state.dragging = None;
                                    id.request_paint();
                                    self.apply_event(dragging_id, &EventListener::DragEnd, &event);
                                }
                            }
                        }
                    } else if let Some(dragging) =
                        self.app_state.dragging.as_mut().filter(|d| d.id == id)
                    {
                        let dragging_id = dragging.id;
                        dragging.released_at = Some(std::time::Instant::now());
                        id.request_paint();
                        self.apply_event(dragging_id, &EventListener::DragEnd, &event);
                    }

                    let last_pointer_down = self.app_state.view_state(id).last_pointer_down.take();
                    if let Some(handlers) =
                        self.get_event_listeners(id, &EventListener::DoubleClick)
                    {
                        if on_view
                            && self.app_state.is_clicking(&id)
                            && last_pointer_down
                                .as_ref()
                                .map(|e| e.count == 2)
                                .unwrap_or(false)
                            && handlers.iter().fold(false, |handled, handler| {
                                handled | handler(&event).is_processed()
                            })
                        {
                            return EventPropagation::Stop;
                        }
                    }
                    if let Some(handlers) = self.get_event_listeners(id, &EventListener::Click) {
                        if on_view
                            && self.app_state.is_clicking(&id)
                            && last_pointer_down.is_some()
                            && handlers.iter().fold(false, |handled, handler| {
                                handled | handler(&event).is_processed()
                            })
                        {
                            return EventPropagation::Stop;
                        }
                    }

                    if self
                        .apply_event(id, &EventListener::PointerUp, &event)
                        .is_some_and(|prop| prop.is_processed())
                    {
                        return EventPropagation::Stop;
                    }
                } else if pointer_event.button.is_secondary() {
                    let rect = self.get_size(id).unwrap_or_default().to_rect();
                    let on_view = rect.contains(pointer_event.pos);

                    let last_pointer_down = self.app_state.view_state(id).last_pointer_down.take();
                    if let Some(handlers) =
                        self.get_event_listeners(id, &EventListener::SecondaryClick)
                    {
                        if on_view
                            && last_pointer_down.is_some()
                            && handlers.iter().fold(false, |handled, handler| {
                                handled | handler(&event).is_processed()
                            })
                        {
                            return EventPropagation::Stop;
                        }
                    }

                    let viewport_event_position = {
                        let layout = self.app_state.view_state(id).layout_rect;
                        Point::new(
                            layout.x0 + pointer_event.pos.x,
                            layout.y0 + pointer_event.pos.y,
                        )
                    };
                    if let Some(menu) = &self.app_state.view_state(id).context_menu {
                        show_context_menu(menu(), Some(viewport_event_position));
                        return EventPropagation::Stop;
                    }
                }
            }
            Event::KeyDown(_) => {
                if self.app_state.is_focused(&id) && event.is_keyboard_trigger() {
                    self.apply_event(id, &EventListener::Click, &event);
                }
            }
            Event::WindowResized(_) => {
                if let Some(view_state) = self.app_state.view_states.get(&id) {
                    if view_state.has_style_selectors.has_responsive() {
                        self.app_state.request_style(id);
                    }
                }
            }
            _ => (),
        }

        for handler in &view.view_data().event_handlers {
            if (handler)(&event).is_processed() {
                return EventPropagation::Stop;
            }
        }

        if let Some(listener) = event.listener() {
            if let Some(handlers) = self.get_event_listeners(id, &listener) {
                let should_run = if let Some(pos) = event.point() {
                    let rect = self.get_size(id).unwrap_or_default().to_rect();
                    rect.contains(pos)
                } else {
                    true
                };
                if should_run
                    && handlers.iter().fold(false, |handled, handler| {
                        handled | handler(&event).is_processed()
                    })
                {
                    return EventPropagation::Stop;
                }
            }
        }

        if is_down_and_has_click {
            return EventPropagation::Stop;
        }

        EventPropagation::Continue
    }

    pub(crate) fn get_size(&self, id: Id) -> Option<Size> {
        self.app_state
            .get_layout(id)
            .map(|l| Size::new(l.size.width as f64, l.size.height as f64))
    }

    pub(crate) fn has_event_listener(&self, id: Id, listener: EventListener) -> bool {
        self.app_state
            .view_states
            .get(&id)
            .map(|s| s.event_listeners.contains_key(&listener))
            .unwrap_or(false)
    }

    pub(crate) fn get_event_listeners(
        &self,
        id: Id,
        listener: &EventListener,
    ) -> Option<&Vec<Box<EventCallback>>> {
        self.app_state.get_event_listeners(id, listener)
    }

    pub(crate) fn apply_event(
        &self,
        id: Id,
        listener: &EventListener,
        event: &Event,
    ) -> Option<EventPropagation> {
        self.app_state.apply_event(id, listener, event)
    }

    /// translate a window-positioned event to the local coordinate system of a view
    pub(crate) fn offset_event(&self, id: Id, event: Event) -> Event {
        let viewport = self
            .app_state
            .view_states
            .get(&id)
            .and_then(|view| view.viewport);

        if let Some(layout) = self.get_layout(id) {
            event.offset((
                layout.location.x as f64 - viewport.map(|rect| rect.x0).unwrap_or(0.0),
                layout.location.y as f64 - viewport.map(|rect| rect.y0).unwrap_or(0.0),
            ))
        } else {
            event
        }
    }

    /// Used to determine if you should send an event to another view. This is basically a check for pointer events to see if the pointer is inside a child view and to make sure the current view isn't hidden or disabled.
    /// Usually this is used if you want to propagate an event to a child view
    pub fn should_send(&mut self, id: Id, event: &Event) -> bool {
        if self.app_state.is_hidden(id)
            || (self.app_state.is_disabled(&id) && !event.allow_disabled())
        {
            return false;
        }
        if let Some(point) = event.point() {
            let layout_rect = self.app_state.get_layout_rect(id);
            if let Some(layout) = self.get_layout(id) {
                if layout_rect
                    .with_origin(Point::new(
                        layout.location.x as f64,
                        layout.location.y as f64,
                    ))
                    .contains(point)
                {
                    return true;
                }
            }
            false
        } else {
            true
        }
    }
}

#[derive(Default)]
pub struct InteractionState {
    pub(crate) is_hovered: bool,
    pub(crate) is_selected: bool,
    pub(crate) is_disabled: bool,
    pub(crate) is_focused: bool,
    pub(crate) is_clicking: bool,
    pub(crate) using_keyboard_navigation: bool,
}

pub struct StyleCx<'a> {
    pub(crate) app_state: &'a mut AppState,
    pub(crate) current_view: Id,
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
    pub(crate) fn new(app_state: &'a mut AppState, root: Id) -> Self {
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

    fn get_interact_state(&self, id: &Id) -> InteractionState {
        InteractionState {
            is_selected: self.selected,
            is_hovered: self.app_state.is_hovered(id),
            is_disabled: self.app_state.is_disabled(id),
            is_focused: self.app_state.is_focused(id),
            is_clicking: self.app_state.is_clicking(id),
            using_keyboard_navigation: self.app_state.keyboard_navigation,
        }
    }

    /// Internal method used by Floem to compute the styles for the view.
    pub fn style_view(&mut self, view: &mut dyn Widget) {
        self.save();
        let id = view.view_data().id();
        let view_state = self.app_state_mut().view_state(id);
        if !view_state.requested_changes.contains(ChangeFlags::STYLE) {
            return;
        }
        view_state.requested_changes.remove(ChangeFlags::STYLE);

        let view_style = view.view_style();
        let view_class = view.view_class();
        let classes = &view_state.classes.clone()[..];

        // Propagate style requests to children if needed.
        if view_state.request_style_recursive {
            view_state.request_style_recursive = false;
            view.for_each_child(&mut |child| {
                let state = self.app_state_mut().view_state(child.view_data().id());
                state.request_style_recursive = true;
                state.requested_changes.insert(ChangeFlags::STYLE);
                false
            });
        }

        let mut view_interact_state = self.get_interact_state(&id);
        view_interact_state.is_disabled |= self.disabled;
        self.disabled = view_interact_state.is_disabled;
        let mut new_frame = self.app_state.compute_style(
            id,
            view.view_data_mut(),
            view_style,
            view_interact_state,
            view_class,
            classes,
            &self.current,
        );

        let style = self.app_state_mut().get_computed_style(id).clone();
        self.direct = style;
        Style::apply_only_inherited(&mut self.current, &self.direct);
        CaptureState::capture_style(id, self);

        // If there's any changes to the Taffy style, request layout.
        let taffy_style = self.direct.to_taffy_style();
        let view_state = self.app_state_mut().view_state(id);
        if taffy_style != view_state.taffy_style {
            view_state.taffy_style = taffy_style;
            self.app_state_mut().request_layout(id);
        }

        // This is used by the `request_transition` and `style` methods below.
        self.current_view = id;

        let view_state = self.app_state.view_state(id);

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
            self.app_state.schedule_style(id);
        }

        view.style(self);

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

    pub fn get_layout(&self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
    }

    pub fn layout(&self, node: NodeId) -> Option<Layout> {
        self.app_state.taffy.layout(node).ok().copied()
    }

    pub(crate) fn get_resize_listener(&mut self, id: Id) -> Option<&mut ResizeListener> {
        self.app_state
            .view_states
            .get_mut(&id)
            .and_then(|s| s.resize_listener.as_mut())
    }

    pub(crate) fn get_move_listener(&mut self, id: Id) -> Option<&mut MoveListener> {
        self.app_state
            .view_states
            .get_mut(&id)
            .and_then(|s| s.move_listener.as_mut())
    }

    /// Internal method used by Floem. This method derives its calculations based on the [Taffy Node](taffy::tree::NodeId) returned by the `View::layout` method.
    ///
    /// It's responsible for:
    /// - calculating and setting the view's origin (local coordinates and window coordinates)
    /// - calculating and setting the view's viewport
    /// - invoking any attached context::ResizeListeners
    ///
    /// Returns the bounding rect that encompasses this view and its children
    pub fn compute_view_layout(&mut self, view: &mut dyn Widget) -> Option<Rect> {
        let id = view.view_data().id();
        if self.app_state().is_hidden(id) {
            self.app_state_mut().view_state(id).layout_rect = Rect::ZERO;
            return None;
        }

        self.save();

        let layout = self.app_state().get_layout(id).unwrap();
        let origin = Point::new(layout.location.x as f64, layout.location.y as f64);
        let this_viewport = self
            .app_state()
            .view_states
            .get(&id)
            .and_then(|view| view.viewport);
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

        if let Some(resize) = self.get_resize_listener(id) {
            let new_rect = size.to_rect().with_origin(origin);
            if new_rect != resize.rect {
                resize.rect = new_rect;
                (*resize.callback)(new_rect);
            }
        }

        if let Some(listener) = self.get_move_listener(id) {
            if window_origin != listener.window_origin {
                listener.window_origin = window_origin;
                (*listener.callback)(window_origin);
            }
        }

        let child_layout_rect = view.compute_layout(self);

        let layout_rect = size.to_rect().with_origin(self.window_origin);
        let layout_rect = if let Some(child_layout_rect) = child_layout_rect {
            layout_rect.union(child_layout_rect)
        } else {
            layout_rect
        };
        self.app_state_mut().view_state(id).layout_rect = layout_rect;

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

    pub fn get_computed_style(&mut self, id: Id) -> &Style {
        self.app_state.get_computed_style(id)
    }

    pub fn set_style(&mut self, node: NodeId, style: taffy::style::Style) {
        let _ = self.app_state.taffy.set_style(node, style);
    }

    pub fn new_node(&mut self) -> NodeId {
        self.app_state
            .taffy
            .new_leaf(taffy::style::Style::DEFAULT)
            .unwrap()
    }

    /// Responsible for invoking the recalculation of style and thus the layout and
    /// creating or updating the layout of child nodes within the closure.
    ///
    /// You should ensure that all children are laid out within the closure and/or whatever
    /// other work you need to do to ensure that the layout for the returned nodes is correct.
    pub fn layout_node(
        &mut self,
        id: Id,
        has_children: bool,
        mut children: impl FnMut(&mut LayoutCx) -> Vec<NodeId>,
    ) -> NodeId {
        let view_state = self.app_state.view_state(id);
        let node = view_state.node;
        if !view_state.requested_changes.contains(ChangeFlags::LAYOUT) {
            return node;
        }
        view_state.requested_changes.remove(ChangeFlags::LAYOUT);
        let style = view_state.combined_style.to_taffy_style();
        let _ = self.app_state.taffy.set_style(node, style);

        if has_children {
            let nodes = children(self);
            let _ = self.app_state.taffy.set_children(node, &nodes);
        }

        node
    }

    /// Internal method used by Floem to invoke the user-defined `View::layout` method.
    pub fn layout_view(&mut self, view: &mut dyn Widget) -> NodeId {
        view.layout(self)
    }
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
}

impl<'a> PaintCx<'a> {
    pub fn save(&mut self) {
        self.saved_transforms.push(self.transform);
        self.saved_clips.push(self.clip);
        self.saved_z_indexes.push(self.z_index);
    }

    pub fn restore(&mut self) {
        self.transform = self.saved_transforms.pop().unwrap_or_default();
        self.clip = self.saved_clips.pop().unwrap_or_default();
        self.z_index = self.saved_z_indexes.pop().unwrap_or_default();
        self.paint_state.renderer.transform(self.transform);
        if let Some(z_index) = self.z_index {
            self.paint_state.renderer.set_z_index(z_index);
        } else {
            self.paint_state.renderer.set_z_index(0);
        }
        if let Some(rect) = self.clip {
            self.paint_state.renderer.clip(&rect);
        } else {
            self.paint_state.renderer.clear_clip();
        }
    }

    /// The entry point for painting a view. You shouldn't need to implement this yourself. Instead, implement [`Widget::paint`].
    /// It handles the internal work before and after painting [`Widget::paint`] implementations.
    /// It is responsible for
    /// - managing hidden status
    /// - clipping
    /// - painting computed styles like background color, border, font-styles, and z-index and handling painting requirements of drag and drop
    pub fn paint_view(&mut self, view: &mut dyn Widget) {
        let id = view.view_data().id();
        if self.app_state.is_hidden(id) {
            return;
        }

        self.save();
        let size = self.transform(id);
        let is_empty = self
            .clip
            .map(|rect| rect.rect().intersect(size.to_rect()).is_empty())
            .unwrap_or(false);
        if !is_empty {
            let style = self.app_state.get_computed_style(id).clone();
            let view_style_props = self.app_state.view_state(id).view_style_props.clone();

            if let Some(z_index) = style.get(ZIndex) {
                self.set_z_index(z_index);
            }

            paint_bg(self, &style, &view_style_props, size);

            view.paint(self);
            paint_border(self, &view_style_props, size);
            paint_outline(self, &view_style_props, size)
        }

        let mut drag_set_to_none = false;
        if let Some(dragging) = self.app_state.dragging.as_ref() {
            if dragging.id == id {
                let dragging_offset = dragging.offset;
                let mut offset_scale = None;
                if let Some(released_at) = dragging.released_at {
                    const LIMIT: f64 = 300.0;
                    let elapsed = released_at.elapsed().as_millis() as f64;
                    if elapsed < LIMIT {
                        offset_scale = Some(1.0 - elapsed / LIMIT);
                        exec_after(std::time::Duration::from_millis(8), move |_| {
                            id.request_paint();
                        });
                    } else {
                        drag_set_to_none = true;
                    }
                } else {
                    offset_scale = Some(1.0);
                }

                if let Some(offset_scale) = offset_scale {
                    let offset = dragging_offset * offset_scale;
                    self.save();

                    let mut new = self.transform.as_coeffs();
                    new[4] += offset.x;
                    new[5] += offset.y;
                    self.transform = Affine::new(new);
                    self.paint_state.renderer.transform(self.transform);
                    self.set_z_index(1000);
                    self.clear_clip();

                    let style = self.app_state.get_computed_style(id).clone();
                    let view_state = self.app_state.view_state(id);
                    let mut view_style_props = view_state.view_style_props.clone();
                    let style = if let Some(dragging_style) = view_state.dragging_style.clone() {
                        let style = style.apply(dragging_style);
                        let mut _new_frame = false;
                        view_style_props.read_explicit(
                            &style,
                            &style,
                            &Instant::now(),
                            &mut _new_frame,
                        );
                        style
                    } else {
                        style
                    };
                    paint_bg(self, &style, &view_style_props, size);

                    view.paint(self);
                    paint_border(self, &view_style_props, size);
                    paint_outline(self, &view_style_props, size);

                    self.restore();
                }
            }
        }
        if drag_set_to_none {
            self.app_state.dragging = None;
        }

        self.restore();
    }

    pub fn layout(&self, node: NodeId) -> Option<Layout> {
        self.app_state.taffy.layout(node).ok().copied()
    }

    pub fn get_layout(&mut self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
    }

    /// Returns the layout rect excluding borders, padding and position.
    /// This is relative to the view.
    pub fn get_content_rect(&mut self, id: Id) -> Rect {
        self.app_state.get_content_rect(id)
    }

    pub fn get_computed_style(&mut self, id: Id) -> &Style {
        self.app_state.get_computed_style(id)
    }

    pub(crate) fn get_builtin_style(&mut self, id: Id) -> BuiltinStyle<'_> {
        self.app_state.get_builtin_style(id)
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
            self.paint_state.renderer.clip(&rect);
            rect.to_rounded_rect(0.0)
        } else {
            self.paint_state.renderer.clip(&shape);
            rect
        };
        self.clip = Some(rect);
    }

    /// Remove clipping so the entire window can be rendered to.
    pub fn clear_clip(&mut self) {
        self.clip = None;
        self.paint_state.renderer.clear_clip();
    }

    pub fn offset(&mut self, offset: (f64, f64)) {
        let mut new = self.transform.as_coeffs();
        new[4] += offset.0;
        new[5] += offset.1;
        self.transform = Affine::new(new);
        self.paint_state.renderer.transform(self.transform);
        if let Some(rect) = self.clip.as_mut() {
            let raidus = rect.radii();
            *rect = rect
                .rect()
                .with_origin(rect.origin() - Vec2::new(offset.0, offset.1))
                .to_rounded_rect(raidus);
        }
    }

    pub fn transform(&mut self, id: Id) -> Size {
        if let Some(layout) = self.get_layout(id) {
            let offset = layout.location;
            let mut new = self.transform.as_coeffs();
            new[4] += offset.x as f64;
            new[5] += offset.y as f64;
            self.transform = Affine::new(new);
            self.paint_state.renderer.transform(self.transform);

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
        self.paint_state.renderer.set_z_index(z_index);
    }

    pub fn is_focused(&self, id: Id) -> bool {
        self.app_state.is_focused(&id)
    }
}

// TODO: should this be private?
pub struct PaintState {
    pub(crate) renderer: crate::renderer::Renderer,
}

impl PaintState {
    pub fn new<W>(window: &W, scale: f64, size: Size) -> Self
    where
        W: raw_window_handle::HasRawDisplayHandle + raw_window_handle::HasRawWindowHandle,
    {
        Self {
            renderer: crate::renderer::Renderer::new(window, scale, size),
        }
    }

    pub(crate) fn resize(&mut self, scale: f64, size: Size) {
        self.renderer.resize(scale, size);
    }

    pub(crate) fn set_scale(&mut self, scale: f64) {
        self.renderer.set_scale(scale);
    }
}

pub struct UpdateCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> UpdateCx<'a> {
    /// request that this node be styled, laid out and painted again
    /// This will recursively request this for all parents.
    pub fn request_all(&mut self, id: Id) {
        self.app_state.request_all(id);
    }

    /// request that this node be styled again
    /// This will recursively request style for all parents.
    pub fn request_style(&mut self, id: Id) {
        self.app_state.request_style(id);
    }

    /// request that this node be laid out again
    /// This will recursively request layout for all parents and set the `ChangeFlag::LAYOUT` at root
    pub fn request_layout(&mut self, id: Id) {
        self.app_state.request_layout(id);
    }

    pub fn app_state_mut(&mut self) -> &mut AppState {
        self.app_state
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }

    /// Used internally by Floem to send an update to the correct view based on the `Id` path.
    /// It will invoke only once `update` when the correct view is located.
    pub fn update_view(&mut self, view: &mut dyn Widget, id_path: &[Id], state: Box<dyn Any>) {
        let id = id_path[0];
        let id_path = &id_path[1..];
        if id == view.view_data().id() {
            if id_path.is_empty() {
                view.update(self, state);
            } else if let Some(child) = view.child_mut(id_path[0]) {
                self.update_view(child, id_path, state);
            }
        }
    }
}

impl Deref for PaintCx<'_> {
    type Target = crate::renderer::Renderer;

    fn deref(&self) -> &Self::Target {
        &self.paint_state.renderer
    }
}

impl DerefMut for PaintCx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.paint_state.renderer
    }
}
