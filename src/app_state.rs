use std::collections::{HashMap, HashSet};

use muda::MenuId;
use peniko::kurbo::{Point, Size};
use taffy::{AvailableSpace, NodeId};
use winit::cursor::CursorIcon;
use winit::window::Theme;

use crate::{
    context::{DragState, FrameUpdate, InteractionState},
    event::{Event, EventListener},
    id::ViewId,
    inspector::CaptureState,
    responsive::{GridBreakpoints, ScreenSizeBp},
    style::{CursorStyle, Style, StyleClassRef, StyleSelector},
    view_storage::VIEW_STORAGE,
};

/// Encapsulates and owns the global state of the application,
pub struct AppState {
    /// keyboard focus
    pub(crate) focus: Option<ViewId>,
    pub(crate) prev_focus: Option<ViewId>,
    /// when a view is active, it gets mouse event even when the mouse is
    /// not on it
    pub(crate) active: Option<ViewId>,
    pub(crate) root_view_id: ViewId,
    pub(crate) root: Option<NodeId>,
    pub(crate) root_size: Size,
    pub(crate) scale: f64,
    pub(crate) scheduled_updates: Vec<FrameUpdate>,
    pub(crate) request_compute_layout: bool,
    pub(crate) request_paint: bool,
    // the bool indicates if this item is the root of the disabled item
    pub(crate) disabled: HashSet<(ViewId, bool)>,
    pub(crate) keyboard_navigable: HashSet<ViewId>,
    pub(crate) draggable: HashSet<ViewId>,
    pub(crate) dragging: Option<DragState>,
    pub(crate) drag_start: Option<(ViewId, Point)>,
    pub(crate) dragging_over: HashSet<ViewId>,
    pub(crate) screen_size_bp: ScreenSizeBp,
    pub(crate) grid_bps: GridBreakpoints,
    pub(crate) clicking: HashSet<ViewId>,
    pub(crate) hovered: HashSet<ViewId>,
    pub(crate) os_theme: Option<winit::window::Theme>,
    /// This keeps track of all views that have an animation,
    /// regardless of the status of the animation
    pub(crate) cursor: Option<CursorStyle>,
    pub(crate) last_cursor: CursorIcon,
    pub(crate) last_cursor_location: Point,
    pub(crate) keyboard_navigation: bool,
    pub(crate) context_menu: HashMap<MenuId, Box<dyn Fn()>>,

    /// This is set if we're currently capturing the window for the inspector.
    pub(crate) capture: Option<CaptureState>,
}

impl AppState {
    pub fn new(root_view_id: ViewId) -> Self {
        Self {
            root: None,
            root_view_id,
            focus: None,
            prev_focus: None,
            active: None,
            scale: 1.0,
            root_size: Size::ZERO,
            screen_size_bp: ScreenSizeBp::Xs,
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
            os_theme: None,
            cursor: None,
            last_cursor: CursorIcon::Default,
            last_cursor_location: Default::default(),
            keyboard_navigation: false,
            grid_bps: GridBreakpoints::default(),
            context_menu: HashMap::new(),
            capture: None,
        }
    }

    /// This removes a view from the app state.
    pub fn remove_view(&mut self, id: ViewId) {
        let exists = VIEW_STORAGE.with_borrow(|s| s.view_ids.contains_key(id));
        if !exists {
            return;
        }

        let children = id.children();
        for child in children {
            self.remove_view(child);
        }
        let view_state = id.state();

        let cleanup_listener = view_state.borrow().cleanup_listener.clone();
        if let Some(action) = cleanup_listener {
            action();
        }

        let node = view_state.borrow().node;
        let taffy = id.taffy();
        let mut taffy = taffy.borrow_mut();

        let children = taffy.children(node);
        if let Ok(children) = children {
            for child in children {
                let _ = taffy.remove(child);
            }
        }
        let _ = taffy.remove(node);
        id.remove();
        self.disabled.remove(&(id, true));
        self.disabled.remove(&(id, false));
        self.keyboard_navigable.remove(&id);
        self.draggable.remove(&id);
        self.dragging_over.remove(&id);
        self.clicking.remove(&id);
        self.hovered.remove(&id);
        self.clicking.remove(&id);
        if self.focus == Some(id) {
            self.focus = None;
        }
        if self.prev_focus == Some(id) {
            self.prev_focus = None;
        }

        if self.active == Some(id) {
            self.active = None;
        }
    }

    pub(crate) fn can_focus(&self, id: ViewId) -> bool {
        self.keyboard_navigable.contains(&id) && !self.is_disabled(&id) && !id.is_hidden_recursive()
    }

    pub fn is_hovered(&self, id: &ViewId) -> bool {
        self.hovered.contains(id)
    }

    pub fn is_disabled(&self, id: &ViewId) -> bool {
        self.disabled.contains(&(*id, true)) || self.disabled.contains(&(*id, false))
    }

    pub fn is_focused(&self, id: &ViewId) -> bool {
        self.focus.map(|f| &f == id).unwrap_or(false)
    }

    pub fn is_active(&self, id: &ViewId) -> bool {
        self.active.map(|a| &a == id).unwrap_or(false)
    }

    pub fn is_clicking(&self, id: &ViewId) -> bool {
        self.clicking.contains(id)
    }

    pub fn is_dark_mode(&self) -> bool {
        self.os_theme == Some(Theme::Dark)
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
        view_id: ViewId,
        view_style: Option<Style>,
        view_interact_state: InteractionState,
        view_class: Option<StyleClassRef>,
        context: &Style,
    ) -> bool {
        let screen_size_bp = self.screen_size_bp;
        let view_state = view_id.state();
        let request_new_frame = view_state.borrow_mut().compute_style(
            view_style,
            view_interact_state,
            screen_size_bp,
            view_class,
            context,
        );
        request_new_frame
    }

    pub fn compute_layout(&mut self) {
        if let Some(root) = self.root {
            let _ = self.root_view_id.taffy().borrow_mut().compute_layout(
                root,
                taffy::prelude::Size {
                    width: AvailableSpace::Definite((self.root_size.width / self.scale) as f32),
                    height: AvailableSpace::Definite((self.root_size.height / self.scale) as f32),
                },
            );
        }
    }

    /// Requests that the style pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_style(&mut self, id: ViewId) {
        self.scheduled_updates.push(FrameUpdate::Style(id));
    }

    /// Requests that the layout pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_layout(&mut self, id: ViewId) {
        self.scheduled_updates.push(FrameUpdate::Layout(id));
    }

    /// Requests that the paint pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_paint(&mut self, id: ViewId) {
        self.scheduled_updates.push(FrameUpdate::Paint(id));
    }

    /// Requests that `compute_layout` will run for `_id` and all direct and indirect children.
    pub fn request_compute_layout_recursive(&mut self, _id: ViewId) {
        self.request_compute_layout = true;
    }

    // `Id` is unused currently, but could be used to calculate damage regions.
    pub fn request_paint(&mut self, _id: ViewId) {
        self.request_paint = true;
    }

    pub(crate) fn update_active(&mut self, id: ViewId) {
        if self.active.is_some() {
            // the first update_active wins, so if there's active set,
            // don't do anything.
            return;
        }
        self.active = Some(id);

        // To apply the styles of the Active selector
        if self.has_style_for_sel(id, StyleSelector::Active) {
            id.request_style();
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
                old_id.request_style();
            }
        }

        if self.focus.is_some() {
            self.prev_focus = self.focus;
        }
        self.focus = None;
    }

    pub(crate) fn update_focus(&mut self, id: ViewId, keyboard_navigation: bool) {
        if self.focus.is_some() {
            return;
        }

        self.focus = Some(id);
        self.keyboard_navigation = keyboard_navigation;

        if self.has_style_for_sel(id, StyleSelector::Focus)
            || self.has_style_for_sel(id, StyleSelector::FocusVisible)
        {
            id.request_style();
        }
    }

    pub(crate) fn has_style_for_sel(&mut self, id: ViewId, selector_kind: StyleSelector) -> bool {
        let view_state = id.state();
        let view_state = view_state.borrow();

        view_state.has_style_selectors.has(selector_kind)
            || (selector_kind == StyleSelector::Dragging && view_state.dragging_style.is_some())
    }

    pub(crate) fn update_context_menu(
        &mut self,
        actions: HashMap<MenuId, Box<dyn Fn() + 'static>>,
    ) {
        self.context_menu = actions;
    }

    pub(crate) fn focus_changed(&mut self, old: Option<ViewId>, new: Option<ViewId>) {
        if let Some(old_id) = old {
            // To remove the styles applied by the Focus selector
            if self.has_style_for_sel(old_id, StyleSelector::Focus)
                || self.has_style_for_sel(old_id, StyleSelector::FocusVisible)
            {
                old_id.request_style_recursive();
            }
            old_id.apply_event(&EventListener::FocusLost, &Event::FocusLost);
        }

        if let Some(id) = new {
            // To apply the styles of the Focus selector
            if self.has_style_for_sel(id, StyleSelector::Focus)
                || self.has_style_for_sel(id, StyleSelector::FocusVisible)
            {
                id.request_style_recursive();
            }
            id.apply_event(&EventListener::FocusGained, &Event::FocusGained);
            id.scroll_to(None);
        }
    }
}
