use std::collections::HashMap;

use muda::MenuId;
use peniko::kurbo::{Point, Size, Vec2};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use taffy::{AvailableSpace, NodeId};
use ui_events::pointer::PointerId;
use winit::cursor::CursorIcon;
use winit::window::Theme;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use std::rc::Rc;

use crate::{
    context::FrameUpdate,
    event::{Event, EventListener, clear_hit_test_cache},
    inspector::CaptureState,
    layout::responsive::{GridBreakpoints, ScreenSizeBp},
    style::{CursorStyle, Style, StyleCache, StyleSelector, recalc::StyleRecalcChange, theme::default_theme},
    view::VIEW_STORAGE,
    view::ViewId,
};

/// A small set of ViewIds, optimized for small collections (< 8 items).
/// Uses linear search which is faster than hashing for small N.
/// Inspired by Chromium's approach for event listener collections.
pub(crate) type ViewIdSmallSet = SmallVec<[ViewId; 8]>;

/// A small map from PointerId to ViewId, optimized for the common case of 1-2 pointers.
/// Most applications only have a mouse pointer or a few touch points active at once.
/// Uses linear search which is faster than HashMap for small N due to cache locality.
pub(crate) type PointerCaptureMap = SmallVec<[(PointerId, ViewId); 2]>;

/// Tracks the state of a view being dragged.
pub struct DragState {
    pub(crate) id: ViewId,
    pub(crate) offset: Vec2,
    pub(crate) released_at: Option<Instant>,
    pub(crate) release_location: Option<Point>,
}

/// Encapsulates and owns the global state of the application,
pub struct WindowState {
    /// keyboard focus
    pub(crate) focus: Option<ViewId>,
    pub(crate) prev_focus: Option<ViewId>,
    /// when a view is active, it gets mouse event even when the mouse is
    /// not on it
    pub(crate) active: Option<ViewId>,

    /// Per-pointer capture tracking inspired by Chromium's PointerEventManager.
    /// Maps pointer IDs to the view that has captured that pointer.
    /// Events for captured pointers are routed directly to the capture target.
    /// Uses SmallVec for O(1) stack allocation in the common 1-2 pointer case.
    pub(crate) pointer_capture_target: PointerCaptureMap,
    /// Pending pointer captures to be applied on the next event cycle.
    /// This two-phase approach (pending → active) ensures proper event ordering:
    /// lostpointercapture fires before gotpointercapture.
    /// Uses SmallVec for O(1) stack allocation in the common 1-2 pointer case.
    pub(crate) pending_pointer_capture_target: PointerCaptureMap,

    pub(crate) root_view_id: ViewId,
    pub(crate) root: Option<NodeId>,
    pub(crate) root_size: Size,
    /// Set of ViewIds that have IsFixed style. When root_size changes,
    /// we request layout on these views directly instead of traversing the tree.
    pub(crate) fixed_elements: FxHashSet<ViewId>,
    pub(crate) scale: f64,
    pub(crate) scheduled_updates: Vec<FrameUpdate>,
    pub(crate) request_compute_layout: bool,
    pub(crate) style_dirty: FxHashSet<ViewId>,
    pub(crate) view_style_dirty: FxHashSet<ViewId>,
    pub(crate) request_paint: bool,
    pub(crate) dragging: Option<DragState>,
    pub(crate) drag_start: Option<(ViewId, Point)>,
    pub(crate) dragging_over: ViewIdSmallSet,
    pub(crate) screen_size_bp: ScreenSizeBp,
    pub(crate) grid_bps: GridBreakpoints,
    pub(crate) clicking: FxHashSet<ViewId>,
    pub(crate) hovered: ViewIdSmallSet,
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

    /// Cache for style resolution results.
    /// Views with identical styles and interaction states can share resolved styles.
    pub(crate) style_cache: StyleCache,

    /// Pending child changes for graduated style propagation.
    /// Maps view IDs to the change that should be propagated to their children.
    /// This is populated during style_view and read by views that manually
    /// process children in their style_pass.
    pub(crate) pending_child_change: FxHashMap<ViewId, StyleRecalcChange>,

    /// Pending global style recalc change.
    /// Set when global state changes (dark mode, screen size) that require
    /// propagating changes through the entire tree.
    pub(crate) pending_global_recalc: StyleRecalcChange,

    /// The default theme style containing class definitions for built-in components.
    /// This is used as the root style context for all views when no parent exists.
    /// Contains styling like `.class(ListClass, |s| { s.class(ListItemClass, ...) })`.
    pub(crate) default_theme: Rc<Style>,
}

impl WindowState {
    pub fn new(root_view_id: ViewId, os_theme: Option<Theme>) -> Self {
        Self {
            root: None,
            root_view_id,
            focus: None,
            prev_focus: None,
            active: None,
            pointer_capture_target: PointerCaptureMap::new(),
            pending_pointer_capture_target: PointerCaptureMap::new(),
            scale: 1.0,
            root_size: Size::ZERO,
            fixed_elements: FxHashSet::default(),
            screen_size_bp: ScreenSizeBp::Xs,
            scheduled_updates: Vec::new(),
            request_paint: false,
            request_compute_layout: false,
            view_style_dirty: Default::default(),
            style_dirty: Default::default(),
            dragging: None,
            drag_start: None,
            dragging_over: ViewIdSmallSet::new(),
            clicking: FxHashSet::default(),
            hovered: ViewIdSmallSet::new(),
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
            style_cache: StyleCache::new(),
            pending_child_change: FxHashMap::default(),
            pending_global_recalc: StyleRecalcChange::NONE,
            default_theme: Rc::new(default_theme(os_theme.unwrap_or(Theme::Light))),
        }
    }

    /// Update the default theme when the OS theme changes.
    pub(crate) fn update_default_theme(&mut self, theme: Theme) {
        self.default_theme = Rc::new(default_theme(theme));
    }

    /// Mark that dark mode changed, requiring style recalc with appropriate flags.
    pub(crate) fn mark_dark_mode_changed(&mut self) {
        use crate::style::recalc::{Propagate, RecalcFlags};
        self.pending_global_recalc = self.pending_global_recalc.combine(
            &StyleRecalcChange::new(Propagate::RecalcDescendants)
                .with_flags(RecalcFlags::DARK_MODE_CHANGED),
        );
    }

    /// Mark that screen size breakpoint changed, requiring style recalc.
    pub(crate) fn mark_responsive_changed(&mut self) {
        use crate::style::recalc::{Propagate, RecalcFlags};
        self.pending_global_recalc = self.pending_global_recalc.combine(
            &StyleRecalcChange::new(Propagate::RecalcDescendants)
                .with_flags(RecalcFlags::RESPONSIVE_CHANGED),
        );
    }

    /// Take the pending global recalc change and reset it.
    pub(crate) fn take_global_recalc(&mut self) -> StyleRecalcChange {
        std::mem::take(&mut self.pending_global_recalc)
    }

    /// This removes a view from the app state.
    pub fn remove_view(&mut self, id: ViewId) {
        let exists = VIEW_STORAGE.with_borrow(|s| s.view_ids.contains_key(id));
        if !exists {
            return;
        }
        // Invalidate hit test cache since view tree is changing
        clear_hit_test_cache();

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
        self.dragging_over.retain(|x| *x != id);
        self.clicking.remove(&id);
        self.hovered.retain(|x| *x != id);
        self.file_hovered.remove(&id);
        self.clicking.remove(&id);
        self.focusable.remove(&id);
        self.fixed_elements.remove(&id);
        if self.focus == Some(id) {
            self.focus = None;
        }
        if self.prev_focus == Some(id) {
            self.prev_focus = None;
        }

        if self.active == Some(id) {
            self.active = None;
        }

        // Clean up pointer capture state for removed view
        self.pointer_capture_target.retain(|(_, v)| *v != id);
        self.pending_pointer_capture_target
            .retain(|(_, v)| *v != id);
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
            for view_id in std::mem::take(&mut self.view_style_dirty) {
                dirty_views.insert(view_id);
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

    // =========================================================================
    // Pointer Capture API (inspired by Chromium's PointerEventManager)
    // =========================================================================

    /// Set pointer capture for a view.
    ///
    /// Following the W3C Pointer Events spec and Chromium's implementation:
    /// - Capture is queued in `pending_pointer_capture_target`
    /// - Applied on the next pointer event via `process_pending_pointer_capture`
    /// - Returns true if the capture was queued successfully
    ///
    /// Note: Unlike the web API, this doesn't validate that the pointer is active
    /// (has button pressed). The caller should ensure this constraint if needed.
    #[inline]
    pub(crate) fn set_pointer_capture(&mut self, pointer_id: PointerId, target: ViewId) -> bool {
        // Update existing entry or push new one
        if let Some(entry) = self
            .pending_pointer_capture_target
            .iter_mut()
            .find(|(id, _)| *id == pointer_id)
        {
            entry.1 = target;
        } else {
            self.pending_pointer_capture_target
                .push((pointer_id, target));
        }
        true
    }

    /// Release pointer capture for a specific view.
    ///
    /// Returns true if the view had capture and it was released.
    #[inline]
    pub(crate) fn release_pointer_capture(
        &mut self,
        pointer_id: PointerId,
        target: ViewId,
    ) -> bool {
        if self.has_pointer_capture(pointer_id, target) {
            self.remove_pending_capture(pointer_id);
            true
        } else {
            false
        }
    }

    /// Release pointer capture unconditionally.
    #[inline]
    pub(crate) fn release_pointer_capture_unconditional(&mut self, pointer_id: PointerId) {
        self.remove_pending_capture(pointer_id);
    }

    /// Remove a pointer from the pending capture map.
    #[inline]
    fn remove_pending_capture(&mut self, pointer_id: PointerId) {
        if let Some(pos) = self
            .pending_pointer_capture_target
            .iter()
            .position(|(id, _)| *id == pointer_id)
        {
            self.pending_pointer_capture_target.swap_remove(pos);
        }
    }

    /// Remove a pointer from the active capture map.
    #[inline]
    pub(crate) fn remove_active_capture(&mut self, pointer_id: PointerId) {
        if let Some(pos) = self
            .pointer_capture_target
            .iter()
            .position(|(id, _)| *id == pointer_id)
        {
            self.pointer_capture_target.swap_remove(pos);
        }
    }

    /// Set the active capture target for a pointer.
    #[inline]
    pub(crate) fn set_active_capture(&mut self, pointer_id: PointerId, target: ViewId) {
        if let Some(entry) = self
            .pointer_capture_target
            .iter_mut()
            .find(|(id, _)| *id == pointer_id)
        {
            entry.1 = target;
        } else {
            self.pointer_capture_target.push((pointer_id, target));
        }
    }

    /// Check if a view has pointer capture (pending or active).
    ///
    /// Following Chromium's behavior, this checks the pending map since
    /// that represents the "intent" of the capture state.
    #[inline]
    pub(crate) fn has_pointer_capture(&self, pointer_id: PointerId, target: ViewId) -> bool {
        self.pending_pointer_capture_target
            .iter()
            .any(|(id, v)| *id == pointer_id && *v == target)
    }

    /// Get the pending capture target for a pointer.
    #[inline]
    pub(crate) fn get_pending_capture_target(&self, pointer_id: PointerId) -> Option<ViewId> {
        self.pending_pointer_capture_target
            .iter()
            .find(|(id, _)| *id == pointer_id)
            .map(|(_, v)| *v)
    }

    /// Get the effective target for a pointer event, considering capture.
    ///
    /// If the pointer has an active capture, returns the capture target.
    /// Otherwise returns None, indicating normal hit-testing should be used.
    #[inline]
    pub(crate) fn get_pointer_capture_target(&self, pointer_id: PointerId) -> Option<ViewId> {
        self.pointer_capture_target
            .iter()
            .find(|(id, _)| *id == pointer_id)
            .map(|(_, v)| *v)
    }

    /// Check if any pointer has active capture to the given view.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn has_any_capture(&self, target: ViewId) -> bool {
        self.pointer_capture_target
            .iter()
            .any(|(_, v)| *v == target)
    }

    /// Check if the pending capture map contains an entry for the given pointer.
    #[inline]
    pub(crate) fn has_pending_capture(&self, pointer_id: PointerId) -> bool {
        self.pending_pointer_capture_target
            .iter()
            .any(|(id, _)| *id == pointer_id)
    }

    pub fn set_root_size(&mut self, size: Size) {
        if self.root_size != size {
            // Request layout on all fixed elements since their size depends on root_size
            for &id in &self.fixed_elements {
                id.request_layout();
            }
        }
        self.root_size = size;
        self.compute_layout();
    }

    /// Register a view as having fixed positioning.
    /// Called when a view's style sets IsFixed to true.
    pub fn register_fixed_element(&mut self, id: ViewId) {
        self.fixed_elements.insert(id);
    }

    /// Unregister a view from fixed positioning.
    /// Called when a view's style sets IsFixed to false.
    pub fn unregister_fixed_element(&mut self, id: ViewId) {
        self.fixed_elements.remove(&id);
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
        if bp != self.screen_size_bp {
            self.screen_size_bp = bp;
            self.mark_responsive_changed();
        }
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
            // Use selector-aware method to only update views that have focus styles
            if self.has_style_for_sel(old_id, StyleSelector::Focus) {
                old_id.request_style_for_selector_recursive(StyleSelector::Focus);
            }
            if self.has_style_for_sel(old_id, StyleSelector::FocusVisible) {
                old_id.request_style_for_selector_recursive(StyleSelector::FocusVisible);
            }
            old_id.apply_event(&EventListener::FocusLost, &Event::FocusLost);
        }

        if let Some(id) = new {
            // To apply the styles of the Focus selector
            // Use selector-aware method to only update views that have focus styles
            if self.has_style_for_sel(id, StyleSelector::Focus) {
                id.request_style_for_selector_recursive(StyleSelector::Focus);
            }
            if self.has_style_for_sel(id, StyleSelector::FocusVisible) {
                id.request_style_for_selector_recursive(StyleSelector::FocusVisible);
            }
            id.apply_event(&EventListener::FocusGained, &Event::FocusGained);
            id.scroll_to(None);
        }
    }
}
