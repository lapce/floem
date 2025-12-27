use std::collections::HashMap;

use muda::MenuId;
use peniko::kurbo::{Point, Size};
use rustc_hash::FxHashSet;
use taffy::{AvailableSpace, NodeId};
use winit::cursor::CursorIcon;
use winit::window::Theme;

use crate::{
    context::{DragState, FrameUpdate},
    event::{Event, EventListener},
    id::ViewId,
    inspector::CaptureState,
    responsive::{GridBreakpoints, ScreenSizeBp},
    style::{CursorStyle, StyleSelector},
    view_storage::VIEW_STORAGE,
};

/// Encapsulates and owns the global state of the application,
pub struct WindowState {
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
    pub(crate) style_dirty: FxHashSet<ViewId>,
    pub(crate) view_style_dirty: FxHashSet<ViewId>,
    pub(crate) request_paint: bool,
    pub(crate) dragging: Option<DragState>,
    pub(crate) drag_start: Option<(ViewId, Point)>,
    pub(crate) dragging_over: FxHashSet<ViewId>,
    pub(crate) screen_size_bp: ScreenSizeBp,
    pub(crate) grid_bps: GridBreakpoints,
    pub(crate) clicking: FxHashSet<ViewId>,
    pub(crate) hovered: FxHashSet<ViewId>,
    pub(crate) focusable: FxHashSet<ViewId>,
    pub(crate) file_hovered: FxHashSet<ViewId>,
    // whether the window is in light or dark mode
    pub(crate) light_dark_theme: winit::window::Theme,
    // if `true`, then the window will not follow the os theme changes
    pub(crate) theme_overriden: bool,
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

impl WindowState {
    pub fn new(root_view_id: ViewId, os_theme: Option<Theme>) -> Self {
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
            view_style_dirty: Default::default(),
            style_dirty: Default::default(),
            dragging: None,
            drag_start: None,
            dragging_over: FxHashSet::default(),
            clicking: FxHashSet::default(),
            hovered: FxHashSet::default(),
            focusable: FxHashSet::default(),
            file_hovered: FxHashSet::default(),
            theme_overriden: false,
            light_dark_theme: os_theme.unwrap_or(Theme::Light),
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

        let cleanup_listeners = view_state.borrow().cleanup_listeners.borrow().clone();
        for action in cleanup_listeners {
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
        self.dragging_over.remove(&id);
        self.clicking.remove(&id);
        self.hovered.remove(&id);
        self.file_hovered.remove(&id);
        self.clicking.remove(&id);
        self.focusable.remove(&id);
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

    pub fn is_hovered(&self, id: &ViewId) -> bool {
        self.hovered.contains(id)
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

    pub(crate) fn build_style_traversal(&mut self, root: ViewId) -> Vec<ViewId> {
        let mut traversal =
            Vec::with_capacity(self.style_dirty.len() + self.view_style_dirty.len());
        // If capture is active, traverse all views
        if self.capture.is_some() {
            // Clear dirty flags because we're traversing everything
            self.style_dirty.clear();
            self.view_style_dirty.clear();
            let mut stack = vec![root];
            while let Some(view_id) = stack.pop() {
                traversal.push(view_id);
                let children = VIEW_STORAGE
                    .with_borrow(|s| s.children.get(view_id).cloned().unwrap_or_default());
                // Push in reverse order for left-to-right DFS
                for child in children.iter().rev() {
                    stack.push(*child);
                }
            }
            // Don't return yet, fall through to sorting
        } else {
            // Collect all dirty views
            let mut dirty_views = std::mem::take(&mut self.style_dirty);
            for view_id in &self.view_style_dirty {
                dirty_views.insert(*view_id);
            }
            if dirty_views.is_empty() {
                return Vec::new();
            }
            // Iterative DFS collecting only dirty nodes
            let mut stack = vec![root];
            while let Some(view_id) = stack.pop() {
                if dirty_views.remove(&view_id) {
                    traversal.push(view_id);
                    // Early exit if we've found all dirty nodes
                    if dirty_views.is_empty() {
                        break;
                    }
                }
                let children = VIEW_STORAGE
                    .with_borrow(|s| s.children.get(view_id).cloned().unwrap_or_default());
                // Push in reverse order for left-to-right DFS
                for child in children.iter().rev() {
                    stack.push(*child);
                }
            }
        }

        // Ensure views with custom style parents come after those parents
        // Scan backwards and bubble views up to after their custom parent if needed
        let mut i = traversal.len();
        while i > 0 {
            i -= 1;
            let view_id = traversal[i];
            if let Some(style_parent) = view_id.state().borrow().style_cx_parent {
                // Find where the custom parent is
                if let Some(parent_pos) = traversal[..i].iter().position(|&v| v == style_parent) {
                    // Move this view to right after its parent
                    let view = traversal.remove(i);
                    traversal.insert(parent_pos + 1, view);
                }
            }
        }

        traversal
    }

    pub fn is_dark_mode(&self) -> bool {
        self.light_dark_theme == Theme::Dark
    }

    pub fn is_file_hover(&self, id: &ViewId) -> bool {
        self.file_hovered.contains(id)
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

    pub fn compute_layout(&mut self) {
        if let Some(root) = self.root {
            let _ = self.root_view_id.taffy().borrow_mut().set_style(
                root,
                crate::style::Style::new().size_full().to_taffy_style(),
            );
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
