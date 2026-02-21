use std::{cell::RefCell, collections::HashMap};

use crate::{
    action::exec_after_animation_frame, event::listener, platform::menu_types::MenuId,
    view::ViewStorage,
};

use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Size, Vec2};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use taffy::{AvailableSpace, NodeId};
use ui_events::pointer::{PointerId, PointerInfo};
use understory_event_state::{click::ClickState, hover::HoverState};
use winit::cursor::CursorIcon;
use winit::window::Theme;

use std::rc::Rc;

use crate::{
    BoxTree,
    action::add_update_message,
    context::FrameUpdate,
    element_id::ElementId,
    event::{DragTracker, Event, WindowEvent, clear_hit_test_cache},
    inspector::CaptureState,
    layout::responsive::{GridBreakpoints, ScreenSizeBp},
    message::UpdateMessage,
    style::{CursorStyle, Style, StyleSelector, recalc::StyleRecalcChange, theme::default_theme},
    view::{LayoutNodeCx, MeasureCx, VIEW_STORAGE, ViewId},
};

/// A small set of ViewIds, optimized for small collections (< 8 items).
/// Uses linear search which is faster than hashing for small N.
/// Inspired by Chromium's approach for event listener collections.
pub(crate) type ViewIdSmallSet = SmallVec<[ViewId; 8]>;

/// A small set of ViewIds, optimized for small collections (< 8 items).
/// Uses linear search which is faster than hashing for small N.
/// Inspired by Chromium's approach for event listener collections.
pub(crate) type VisualIdSmallSet = SmallVec<[ElementId; 8]>;

/// A small map from PointerId to ViewId, optimized for the common case of 1-2 pointers.
/// Most applications only have a mouse pointer or a few touch points active at once.
/// Uses linear search which is faster than HashMap for small N due to cache locality.
pub(crate) type PointerCaptureMap = SmallVec<[(PointerId, ElementId); 2]>;

/// Encapsulates and owns the global state of the application,
pub struct WindowState {
    pub(crate) layout_tree: Rc<RefCell<taffy::TaffyTree<LayoutNodeCx>>>,
    pub(crate) box_tree: Rc<RefCell<BoxTree>>,

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
    pub(crate) root_layout_node: NodeId,
    pub(crate) root_size: Size,
    /// Set of ViewIds that have IsFixed style. When root_size changes,
    /// we request layout on these views directly instead of traversing the tree.
    pub(crate) fixed_elements: FxHashSet<ViewId>,
    pub(crate) scale: f64,
    pub(crate) scheduled_updates: Vec<FrameUpdate>,
    pub(crate) style_dirty: FxHashSet<ViewId>,
    pub(crate) view_style_dirty: FxHashSet<ViewId>,
    pub(crate) request_paint: bool,
    pub(crate) drag_tracker: DragTracker,
    pub(crate) screen_size_bp: ScreenSizeBp,
    pub(crate) grid_bps: GridBreakpoints,
    pub(crate) click_state: ClickState<Rc<[ElementId]>>,
    // TODO: Track hover state per pointer
    pub(crate) hover_state: HoverState<ElementId>,
    pub(crate) focus_state: Option<ElementId>,
    pub(crate) file_drag_paths: Option<Rc<[std::path::PathBuf]>>,
    pub(crate) element_id_cursors: FxHashMap<ElementId, CursorStyle>,
    // whether the window is in light or dark mode
    pub(crate) light_dark_theme: winit::window::Theme,
    // if `true`, then the window will not follow the os theme changes
    pub(crate) theme_overriden: bool,
    /// This keeps track of all views that have an animation,
    /// regardless of the status of the animation
    pub(crate) cursor: Option<CursorStyle>,
    pub(crate) needs_cursor_resolution: bool,
    pub(crate) last_cursor_icon: CursorIcon,
    pub(crate) last_pointer: (Point, PointerInfo),
    pub(crate) keyboard_navigation: bool,
    pub(crate) context_menu: HashMap<MenuId, Box<dyn Fn()>>,

    /// This is set if we're currently capturing the window for the inspector.
    pub(crate) capture: Option<CaptureState>,

    /// Cache for style resolution results.
    /// Views with identical styles and interaction states can share resolved styles.
    // no need for style cache
    // pub(crate) style_cache: StyleCache,

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
    pub(crate) default_theme: Style,

    /// Cached inherited props from default_theme for root views.
    /// This avoids recomputing the inherited props from default_theme on every StyleCx::new().
    /// Updated when default_theme changes (on theme switch).
    pub(crate) default_theme_inherited: Style,

    /// Tracking for views that have a visual position listener
    pub(crate) listeners: FxHashMap<listener::EventListenerKey, Vec<ViewId>>,
    pub(crate) needs_layout: bool,
    pub(crate) needs_box_tree_from_layout: bool,
    pub(crate) needs_box_tree_commit: bool,
    /// Views that need their box tree node updated (e.g., after transform or scroll changes).
    /// These are processed after layout and before commit.
    pub(crate) views_needing_box_tree_update: FxHashSet<ViewId>,
}

impl WindowState {
    pub fn new(root_view_id: ViewId, os_theme: Option<Theme>) -> Self {
        let theme = default_theme(os_theme.unwrap_or(Theme::Light));
        let inherited = Self::extract_inherited_props(&theme);
        let box_tree = VIEW_STORAGE.with_borrow_mut(|s| s.box_tree(root_view_id));
        let layout_tree = VIEW_STORAGE.with_borrow_mut(|s| s.taffy.clone());
        let root_layout_node = root_view_id.taffy_node();

        Self {
            root_layout_node,
            root_view_id,
            layout_tree,
            box_tree,
            pointer_capture_target: PointerCaptureMap::new(),
            pending_pointer_capture_target: PointerCaptureMap::new(),
            scale: 1.0,
            root_size: Size::ZERO,
            fixed_elements: FxHashSet::default(),
            screen_size_bp: ScreenSizeBp::Xs,
            scheduled_updates: Vec::new(),
            request_paint: false,
            view_style_dirty: Default::default(),
            style_dirty: Default::default(),
            drag_tracker: DragTracker::new(),
            focus_state: None,
            click_state: ClickState::new(),
            hover_state: HoverState::new(),
            file_drag_paths: None,
            element_id_cursors: FxHashMap::default(),
            theme_overriden: false,
            light_dark_theme: os_theme.unwrap_or(Theme::Light),
            cursor: None,
            needs_cursor_resolution: false,
            last_cursor_icon: CursorIcon::Default,
            last_pointer: (
                Point::ZERO,
                PointerInfo {
                    pointer_id: None,
                    persistent_device_id: None,
                    pointer_type: ui_events::pointer::PointerType::Unknown,
                },
            ),
            keyboard_navigation: false,
            grid_bps: GridBreakpoints::default(),
            context_menu: HashMap::new(),
            capture: None,
            pending_child_change: FxHashMap::default(),
            pending_global_recalc: StyleRecalcChange::new(
                crate::style::Propagate::RecalcDescendants,
            ),
            default_theme: theme,
            default_theme_inherited: inherited,
            needs_layout: true,
            needs_box_tree_from_layout: true,
            needs_box_tree_commit: true,
            listeners: FxHashMap::default(),
            views_needing_box_tree_update: FxHashSet::default(),
        }
    }

    /// Extract inherited props from a theme style for root view initialization.
    fn extract_inherited_props(theme: &Style) -> Style {
        let mut inherited_style = Style::new();
        if theme.any_inherited() {
            let inherited_props = theme.map.iter().filter(|(k, _)| k.inherited());
            inherited_style.apply_iter(inherited_props, None);
        }
        inherited_style
    }

    /// Update the default theme when the OS theme changes.
    pub(crate) fn update_default_theme(&mut self, theme: Theme) {
        let new_theme = default_theme(theme);
        let inherited = Self::extract_inherited_props(&new_theme);
        self.default_theme = new_theme;
        self.default_theme_inherited = inherited;
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

        let node = view_state.borrow().layout_id;
        let taffy = id.taffy();
        let mut taffy = taffy.borrow_mut();

        let children = taffy.children(node);
        if let Ok(children) = children {
            for child in children {
                let _ = taffy.remove(child);
            }
        }
        let _ = taffy.remove(node);

        let box_tree = id.box_tree();
        // Remove from box tree first
        let this_element_id = id.get_element_id();
        box_tree.borrow_mut().remove(this_element_id.0);
        self.needs_box_tree_commit = true;
        id.remove();
        self.fixed_elements.remove(&id);
        let keys = view_state.borrow().registered_listener_keys.clone();
        for key in keys {
            if let Some(ids) = self.listeners.get_mut(&key) {
                ids.retain(|&v| v != id);
            }
        }
        self.views_needing_box_tree_update.remove(&id);

        // Clean up pointer capture state for removed view
        self.pointer_capture_target
            .retain(|(_, v)| *v != this_element_id);
        self.pending_pointer_capture_target
            .retain(|(_, v)| *v != this_element_id);
    }

    pub fn is_hovered(&self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.file_drag_paths.is_none() && self.hover_state.current_path().contains(&id)
    }

    pub fn is_file_hover(&self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.file_drag_paths.is_some() && self.hover_state.current_path().contains(&id)
    }

    pub fn is_focused(&self, id: impl Into<ElementId>) -> bool {
        self.focus_state.map(|f| f == id.into()).unwrap_or(false)
    }

    #[deprecated(note = "use `ViewId::is_active` instead")]
    pub fn is_clicking(&self, id: impl Into<ElementId>) -> bool {
        self.is_active(id)
    }

    pub fn is_active(&self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.click_state.presses().any(|p| p.target.contains(&id))
    }

    /// Check if a view has pointer capture for any pointer.
    pub fn has_capture(&self, id: impl Into<ElementId>) -> bool {
        self.has_any_capture(id)
    }

    pub(crate) fn build_style_traversal(&mut self, root: ViewId) -> Vec<ViewId> {
        let mut traversal =
            Vec::with_capacity(self.style_dirty.len() + self.view_style_dirty.len());
        // If capture is active, traverse all views
        if self.capture.is_some() {
            // Clear dirty flags because we're traversing everything
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
            let mut dirty_views = self.style_dirty.clone();
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

    pub fn is_dragging(&self) -> bool {
        self.drag_tracker.is_dragging()
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
    pub(crate) fn set_pointer_capture(
        &mut self,
        pointer_id: PointerId,
        target: impl Into<ElementId>,
    ) -> bool {
        // Update existing entry or push new one
        if let Some(entry) = self
            .pending_pointer_capture_target
            .iter_mut()
            .find(|(id, _)| *id == pointer_id)
        {
            entry.1 = target.into();
        } else {
            self.pending_pointer_capture_target
                .push((pointer_id, target.into()));
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
        target: impl Into<ElementId>,
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
    pub(crate) fn set_active_capture(
        &mut self,
        pointer_id: PointerId,
        target: impl Into<ElementId>,
    ) {
        if let Some(entry) = self
            .pointer_capture_target
            .iter_mut()
            .find(|(id, _)| *id == pointer_id)
        {
            entry.1 = target.into();
        } else {
            self.pointer_capture_target
                .push((pointer_id, target.into()));
        }
    }

    /// Check if a view has pointer capture (pending or active).
    ///
    /// Following Chromium's behavior, this checks the pending map since
    /// that represents the "intent" of the capture state.
    #[inline]
    pub(crate) fn has_pointer_capture(
        &self,
        pointer_id: PointerId,
        target: impl Into<ElementId>,
    ) -> bool {
        let target = target.into();
        self.pending_pointer_capture_target
            .iter()
            .any(|(id, v)| *id == pointer_id && *v == target)
    }

    /// Get the pending capture target for a pointer.
    #[inline]
    pub(crate) fn get_pending_capture_target(&self, pointer_id: PointerId) -> Option<ElementId> {
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
    pub(crate) fn get_pointer_capture_target(&self, pointer_id: PointerId) -> Option<ElementId> {
        self.pointer_capture_target
            .iter()
            .find(|(id, _)| *id == pointer_id)
            .map(|(_, v)| *v)
    }

    /// Check if any pointer has active capture to the given view.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn has_any_capture(&self, target: impl Into<ElementId>) -> bool {
        let target = target.into();
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
    }

    /// Register a view as having fixed positioning.
    /// Called when a view's style sets IsFixed to true.
    pub fn register_fixed_element(&mut self, id: ViewId) {
        if self.fixed_elements.insert(id) {
            self.needs_layout = true;
        }
    }

    /// Unregister a view from fixed positioning.
    /// Called when a view's style sets IsFixed to false.
    pub fn unregister_fixed_element(&mut self, id: ViewId) {
        if self.fixed_elements.remove(&id) {
            self.needs_layout = true;
        }
    }

    fn apply_fixed_element_styles(&self) {
        let root_size = self.root_size / self.scale;
        let fixed_views: SmallVec<[ViewId; 32]> = self.fixed_elements.iter().copied().collect();
        VIEW_STORAGE.with_borrow(|s| {
            for view_id in fixed_views {
                if let Some(state) = s.states.get(view_id) {
                    let state_borrow = state.borrow();
                    if !state_borrow.combined_style.builtin().is_fixed() {
                        continue;
                    }
                    let layout_node = state_borrow.layout_id;
                    drop(state_borrow);
                    let mut taffy = self.layout_tree.borrow_mut();
                    if let Ok(existing) = taffy.style(layout_node) {
                        let mut style = existing.clone();
                        self.apply_fixed_sizing(&mut style, root_size);
                        if style != *existing {
                            let _ = taffy.set_style(layout_node, style);
                        }
                    }
                }
            }
        });
    }

    fn apply_fixed_sizing(&self, style: &mut taffy::Style, root_size: Size) {
        fn definite_length(val: &taffy::style::LengthPercentageAuto) -> Option<f32> {
            let raw = val.into_raw();
            if raw.tag() == taffy::CompactLength::LENGTH_TAG {
                Some(raw.value())
            } else {
                None
            }
        }

        let left = definite_length(&style.inset.left);
        let right = definite_length(&style.inset.right);
        let top = definite_length(&style.inset.top);
        let bottom = definite_length(&style.inset.bottom);

        // Width
        if let (Some(l), Some(r)) = (left, right) {
            let computed = (root_size.width as f32 - l - r).max(0.0);
            style.size.width = taffy::style::Dimension::length(computed);
        } else if style.size.width == taffy::style::Dimension::percent(1.0) {
            style.size.width = taffy::style::Dimension::length(root_size.width as f32);
        }

        // Height
        if let (Some(t), Some(b)) = (top, bottom) {
            let computed = (root_size.height as f32 - t - b).max(0.0);
            style.size.height = taffy::style::Dimension::length(computed);
        } else if style.size.height == taffy::style::Dimension::percent(1.0) {
            style.size.height = taffy::style::Dimension::length(root_size.height as f32);
        }
    }

    pub fn compute_layout(&mut self) {
        let mut measure_context = MeasureCx::default();
        let _ = self.root_view_id.taffy().borrow_mut().set_style(
            self.root_layout_node,
            crate::style::Style::new().size_full().to_taffy_style(),
        );

        self.apply_fixed_element_styles();

        let _ = self
            .root_view_id
            .taffy()
            .borrow_mut()
            .compute_layout_with_measure(
                self.root_layout_node,
                taffy::prelude::Size {
                    width: AvailableSpace::Definite((self.root_size.width / self.scale) as f32),
                    height: AvailableSpace::Definite((self.root_size.height / self.scale) as f32),
                },
                |known_dimensions, available_space, node_id, node_context, style| match node_context
                {
                    Some(LayoutNodeCx::Custom {
                        measure,
                        finalize: _,
                    }) => measure(
                        known_dimensions,
                        available_space,
                        node_id,
                        style,
                        &mut measure_context,
                    ),
                    None => taffy::Size::ZERO,
                },
            );

        self.needs_layout = false;

        // Finalize nodes that requested it
        let taffy = self.root_view_id.taffy();
        let taffy = taffy.borrow();
        for node_id in measure_context.needs_finalization {
            if let Ok(layout) = taffy.layout(node_id)
                && let Some(LayoutNodeCx::Custom {
                    finalize: Some(f), ..
                }) = taffy.get_node_context(node_id)
            {
                f(node_id, layout);
            }
        }

        self.needs_box_tree_commit = true;
    }

    // =========================================================================
    // Box Tree Update System
    // =========================================================================
    //
    // The box tree update system is intentionally split into two phases:
    // 1) local-space sync (cheap to batch)
    // 2) commit-time world-space resolution (global/cross-parent corrections)
    //
    // 1. update_box_tree_from_layout() - Full tree walk after layout
    //    - Called automatically after layout completes
    //    - Writes local bounds/clip/local transforms for every view
    //    - Uses logical layout parent semantics (taffy/view tree)
    //
    // 2. update_box_tree_for_view(view_id) - Single view update (non-recursive)
    //    - Used when a specific view changes (transform, scroll offset, etc.)
    //    - More efficient than full tree walk
    //    - Also writes only local-space properties
    //
    // 3. commit_box_tree() - Commit and handle damage
    //    - Must run after any local update operation
    //    - Applies commit-time corrections that require global context:
    //      - overlay logical-parent world offset remap
    //      - fixed-position viewport placement
    //    - Computes world transforms and damage regions
    //    - Updates hover state if pointer is in damaged area
    //
    // Keeping commit separate lets us coalesce many local updates into one global
    // world-space resolve. The `needs_box_tree_commit` flag tracks whether commit
    // is still required.

    /// Update the box tree from the layout tree by walking the entire tree.
    ///
    /// This pass performs a full local-space sync from layout/view state into the box tree.
    /// It recursively updates every view's:
    /// - local bounds (from layout size)
    /// - local clip (from style transform props)
    /// - local transform (layout position + parent scroll + view/style transforms)
    ///
    /// Important: this does **not** finalize world-space behavior for features that
    /// intentionally diverge from logical parentage (e.g. overlays reparented to root)
    /// or viewport anchoring (fixed-position). Those are resolved in `commit_box_tree()`
    /// via `apply_overlay_parent_transforms()` and `apply_fixed_positioning_transforms()`.
    ///
    /// Call this after layout completes; then run `commit_box_tree()` to finalize world
    /// transforms, damage, and hit-testing consistency.
    pub fn update_box_tree_from_layout(&mut self) {
        let box_tree = self.box_tree.clone();
        let layout_tree = self.layout_tree.clone();
        VIEW_STORAGE.with_borrow(|s| {
            compute_absolute_transforms_and_boxes(
                s,
                layout_tree,
                box_tree,
                self.root_layout_node,
                Vec2::ZERO, // parent_scroll - root has no parent scroll
                Vec2::ZERO, // parent_scroll_ctx - root has no accumulated scroll
            );
        });
        // Clear pending individual updates since the full tree walk handled everything
        self.views_needing_box_tree_update.clear();
        self.needs_box_tree_from_layout = false;
        self.needs_box_tree_commit = true;
    }

    /// Update the box tree for a specific view only (non-recursive).
    /// This is efficient for updating a single view's transform, scroll offset, or clip
    /// without walking the layout tree. Children's transforms are not recalculated here;
    /// they'll be handled by the box tree's hierarchical transform system during commit.
    pub fn update_box_tree_for_view(&mut self, view_id: ViewId) {
        VIEW_STORAGE.with_borrow(|s| {
            let state = s.states.get(view_id);

            if let Some(state) = state {
                let layout_node = state.borrow().layout_id;
                let layout = self.layout_tree.borrow().layout(layout_node).ok().copied();

                if let Some(layout) = layout {
                    // Get parent's scroll offset and scroll_ctx
                    let (parent_scroll, parent_scroll_ctx) =
                        if let Some(parent_id) = s.parent.get(view_id).and_then(|p| *p) {
                            s.states
                                .get(parent_id)
                                .map(|p| {
                                    let p = p.borrow();
                                    (p.child_translation, p.scroll_cx)
                                })
                                .unwrap_or((Vec2::ZERO, Vec2::ZERO))
                        } else {
                            (Vec2::ZERO, Vec2::ZERO)
                        };

                    let props = compute_view_box_properties(
                        s,
                        view_id,
                        layout,
                        parent_scroll,
                        parent_scroll_ctx,
                    );

                    // Update box tree
                    let mut box_tree = self.box_tree.borrow_mut();
                    box_tree.set_local_bounds(props.element_id.0, props.local_rect);
                    box_tree.set_local_clip(props.element_id.0, props.clip);
                    box_tree.set_local_transform(props.element_id.0, props.local_transform);
                }
            }
        });
        self.needs_box_tree_commit = true;
    }

    /// Process all pending individual box tree updates.
    /// This should be called after layout and before commit.
    pub fn process_pending_box_tree_updates(&mut self) {
        let views = std::mem::take(&mut self.views_needing_box_tree_update);
        for view_id in views {
            self.update_box_tree_for_view(view_id);
        }
    }

    /// Finalize box-tree state for this frame and produce damage.
    ///
    /// `commit_box_tree` is the world-space resolution boundary of the update pipeline.
    /// After local-space updates (`update_box_tree_from_layout` and/or
    /// `update_box_tree_for_view`), this method applies global adjustments and then
    /// commits the underlying box tree so transforms, clipping, and spatial indexing are
    /// consistent for paint + hit testing.
    ///
    /// Order of operations:
    /// 1. Apply drag-preview world translation (if an active drag preview exists)
    /// 2. Apply overlay parent-space remap (`apply_overlay_parent_transforms`)
    /// 3. Apply fixed-position viewport placement (`apply_fixed_positioning_transforms`)
    /// 4. Commit the box tree, producing dirty/damage regions
    /// 5. If pointer lies in damaged region, clear hit-test cache and refresh under-cursor routing
    ///
    /// Why this happens at commit time:
    /// - Overlay and fixed positioning depend on global/world context, not only local layout.
    /// - Separating local updates from commit allows batching many updates into one world
    ///   resolve + one damage computation.
    ///
    /// Call this after any box-tree local update before relying on paint order, hit-test
    /// correctness, or damage-driven cursor/hover updates.
    pub fn commit_box_tree(&mut self) {
        if let Some(dragging) = &mut self.drag_tracker.active_drag
            && let Some(dragging_preview) = dragging.dragging_preview.clone()
        {
            let local_bounds = self
                .box_tree
                .borrow()
                .local_bounds(dragging_preview.element_id.0)
                .unwrap_or_default();

            // Get current world transform and update natural position (detects layout changes)
            let current_transform = self
                .box_tree
                .borrow()
                .compute_world_transform(dragging_preview.element_id.0)
                .unwrap_or(Affine::IDENTITY);

            let natural_position = dragging.update_and_get_natural_position(current_transform);

            // Calculate the drag point offset (where user grabbed within the element)
            let drag_point_offset = Point::new(
                local_bounds.width() * (dragging_preview.drag_point_pct.0.0 / 100.0),
                local_bounds.height() * (dragging_preview.drag_point_pct.1.0 / 100.0),
            );

            // Calculate and apply position
            let new_point = dragging.calculate_position(natural_position, drag_point_offset);
            dragging.record_applied_translation(new_point);

            self.box_tree
                .borrow_mut()
                .set_world_translation(dragging_preview.element_id.0, new_point);

            // Schedule next animation frame if needed
            if dragging.should_schedule_animation_frame() {
                let timer = exec_after_animation_frame(move |_| {
                    add_update_message(UpdateMessage::RequestBoxTreeCommit);
                });
                dragging.animation_timer = Some(timer);
            }
        }

        // Clean up completed animations
        if let Some(dragging) = &self.drag_tracker.active_drag {
            if dragging.released_at.is_some() && dragging.is_animation_complete() {
                self.views_needing_box_tree_update
                    .insert(dragging.element_id.owning_id());
                self.drag_tracker.active_drag = None;
            }
        }

        self.apply_overlay_parent_transforms();
        self.apply_fixed_positioning_transforms();

        let damage = self.box_tree.borrow_mut().commit();
        let pointer = self.last_pointer;
        for damage_rect in &damage.dirty_rects {
            if damage_rect.contains(pointer.0) {
                clear_hit_test_cache();
                let root_element_id = self.root_view_id.get_element_id();
                crate::event::GlobalEventCx::new(
                    self,
                    root_element_id,
                    Event::Window(WindowEvent::ChangeUnderCursor),
                )
                .route_window_event();
            }
        }
        self.needs_box_tree_commit = false;
    }

    /// Reconcile overlay local transforms after box-tree reparenting.
    ///
    /// Overlays are kept in the logical view/layout tree under their declarative parent,
    /// but in the box tree they are reparented under the window root so they can stack
    /// above regular content. That creates a parent-space mismatch:
    /// - layout computes overlay `local_transform` relative to the logical parent
    /// - box tree interprets `local_transform` relative to the root
    ///
    /// This method remaps each overlay transform into root-child space at commit time:
    /// 1. Build the overlay's base local transform from layout + style/scroll (logical-parent space)
    /// 2. Compute the logical parent's world transform from the box tree
    /// 3. Set overlay local transform to `parent_world * base_local_transform`
    ///
    /// Running this in `commit_box_tree` keeps behavior correct for both full-tree updates
    /// (`update_box_tree_from_layout`) and targeted updates (`update_box_tree_for_view`).
    fn apply_overlay_parent_transforms(&self) {
        let overlay_data: SmallVec<[(ElementId, ElementId, Affine); 16]> = VIEW_STORAGE
            .with_borrow(|s| {
                s.overlays
                    .iter()
                    .filter_map(|(overlay_id, &overlay_root)| {
                        if overlay_root != self.root_view_id {
                            return None;
                        }
                        Some(overlay_id)
                    })
                    .filter_map(|overlay_id| {
                        let overlay_state = s.states.get(overlay_id)?;
                        let overlay_state_borrow = overlay_state.borrow();
                        let overlay_element_id = overlay_state_borrow.element_id;
                        let overlay_layout_id = overlay_state_borrow.layout_id;
                        let overlay_transform = overlay_state_borrow.transform;
                        let style_transform_props =
                            overlay_state_borrow.view_transform_props.clone();
                        drop(overlay_state_borrow);

                        let logical_parent_id = s.parent.get(overlay_id).and_then(|p| *p)?;
                        let parent_state = s.states.get(logical_parent_id)?;
                        let parent_state_borrow = parent_state.borrow();
                        let logical_parent_element_id = parent_state_borrow.element_id;
                        let parent_scroll = parent_state_borrow.child_translation;
                        drop(parent_state_borrow);

                        let layout = self
                            .layout_tree
                            .borrow()
                            .layout(overlay_layout_id)
                            .ok()
                            .copied()?;
                        let size = Size::new(layout.size.width as f64, layout.size.height as f64);
                        let local_pos =
                            Point::new(layout.location.x as f64, layout.location.y as f64);

                        let style_transform = style_transform_props.affine(size);
                        let view_local_transform = style_transform * overlay_transform;
                        let base_local_transform = Affine::translate(-parent_scroll)
                            * Affine::translate(local_pos.to_vec2())
                            * view_local_transform;

                        Some((
                            overlay_element_id,
                            logical_parent_element_id,
                            base_local_transform,
                        ))
                    })
                    .collect()
            });

        let mut tree = self.box_tree.borrow_mut();
        for (overlay_element_id, logical_parent_element_id, base_local_transform) in overlay_data {
            let parent_world = tree
                .compute_world_transform(logical_parent_element_id.0)
                .unwrap_or(Affine::IDENTITY);
            tree.set_local_transform(overlay_element_id.0, parent_world * base_local_transform);
        }
    }

    /// Apply viewport-relative placement for fixed-position elements.
    ///
    /// Layout computes fixed elements in normal tree flow, but CSS-like `position: fixed`
    /// semantics require final placement in window (viewport) coordinates, independent of
    /// ancestor scrolling/transforms. We resolve that at commit time by computing each fixed
    /// element's target viewport position from inset properties and writing it as a world
    /// translation in the box tree.
    ///
    /// Running this after local-transform updates keeps the adjustment stable for both full
    /// layout sync and incremental box-tree updates.
    fn apply_fixed_positioning_transforms(&self) {
        let root_size = self.root_size / self.scale;
        let positions: SmallVec<[(ElementId, Point); 32]> = VIEW_STORAGE.with_borrow(|s| {
            self.fixed_elements
                .iter()
                .filter_map(|&view_id| {
                    let state = s.states.get(view_id)?;
                    let state_borrow = state.borrow();
                    let builtin = state_borrow.combined_style.builtin();
                    let element_id = state_borrow.element_id;
                    let local_bounds = self
                        .box_tree
                        .borrow()
                        .local_bounds(element_id.0)
                        .unwrap_or_default();

                    let mut pos = Point::new(0.0, 0.0);

                    if let (Some(left), Some(_)) = (
                        builtin.inset_left().resolve(root_size.width),
                        builtin.inset_right().resolve(root_size.width),
                    ) {
                        pos.x = left;
                    } else if let Some(left) = builtin.inset_left().resolve(root_size.width) {
                        pos.x = left;
                    } else if let Some(right) = builtin.inset_right().resolve(root_size.width) {
                        pos.x = root_size.width - right - local_bounds.width();
                    }

                    if let (Some(top), Some(_)) = (
                        builtin.inset_top().resolve(root_size.height),
                        builtin.inset_bottom().resolve(root_size.height),
                    ) {
                        pos.y = top;
                    } else if let Some(top) = builtin.inset_top().resolve(root_size.height) {
                        pos.y = top;
                    } else if let Some(bottom) = builtin.inset_bottom().resolve(root_size.height) {
                        pos.y = root_size.height - bottom - local_bounds.height();
                    }

                    Some((element_id, pos))
                })
                .collect()
        });

        let mut tree = self.box_tree.borrow_mut();
        for (element_id, pos) in positions {
            tree.set_world_translation(element_id.0, pos);
        }
    }

    /// Requests that the style pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_style(&mut self, id: ViewId) {
        self.scheduled_updates.push(FrameUpdate::Style(id));
    }

    /// Requests that the layout pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_layout(&mut self) {
        self.scheduled_updates.push(FrameUpdate::Layout);
    }

    /// Requests that the box tree be commited pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_box_tree_commit(&mut self) {
        self.scheduled_updates.push(FrameUpdate::BoxTreeCommit);
    }

    /// Requests that the paint pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_paint(&mut self, id: ViewId) {
        self.scheduled_updates.push(FrameUpdate::Paint(id));
    }

    // `Id` is unused currently, but could be used to calculate damage regions.
    pub fn request_paint(&mut self, _id: ViewId) {
        self.request_paint = true;
    }

    pub(crate) fn update_screen_size_bp(&mut self, size: Size) {
        let bp = self.grid_bps.get_width_bp(size.width);
        if bp != self.screen_size_bp {
            self.screen_size_bp = bp;
            self.mark_responsive_changed();
        }
    }

    pub(crate) fn has_style_for_sel(&self, id: ViewId, selector_kind: StyleSelector) -> bool {
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

    /// returns the previously set cursor if there was one
    pub fn set_cursor(
        &mut self,
        id: impl Into<ElementId>,
        cursor: CursorStyle,
    ) -> Option<CursorStyle> {
        self.needs_cursor_resolution = true;
        self.element_id_cursors.insert(id.into(), cursor)
    }

    /// returns the previously set cursor if there was one
    pub fn clear_cursor(&mut self, id: impl Into<ElementId>) -> Option<CursorStyle> {
        self.needs_cursor_resolution = true;
        self.element_id_cursors.remove(&id.into())
    }
}

struct ViewBoxProperties {
    element_id: ElementId,
    local_rect: Rect,
    local_transform: Affine,
    scroll_offset: Vec2,
    scroll_ctx: Vec2,
    clip: Option<RoundedRect>,
}

// New helper function to compute view's box tree properties
fn compute_view_box_properties(
    s: &ViewStorage,
    view_id: ViewId,
    layout: taffy::Layout,
    parent_scroll: Vec2,
    parent_scroll_ctx: Vec2,
) -> ViewBoxProperties {
    let size = Size::new(layout.size.width as f64, layout.size.height as f64);
    let local_rect = Rect::from_origin_size(Point::ZERO, size);
    let local_pos = Point::new(layout.location.x as f64, layout.location.y as f64);

    let state = s.states.get(view_id).unwrap();
    let state_borrow = state.borrow();

    let style_transform = state_borrow.view_transform_props.affine(size);
    let view_local_transform = style_transform * state_borrow.transform;
    let scroll_offset = state_borrow.child_translation;
    let clip = state_borrow.view_transform_props.clip_rect(local_rect);
    let element_id = state_borrow.element_id;

    drop(state_borrow);

    // Compute scroll context
    let scroll_ctx = if parent_scroll != Vec2::ZERO {
        parent_scroll_ctx + parent_scroll
    } else {
        parent_scroll_ctx
    };

    // Compute local transform
    let parent_transform_for_children = Affine::translate(-parent_scroll);
    let local_transform = parent_transform_for_children
        * Affine::translate(local_pos.to_vec2())
        * view_local_transform;

    // Compute layout window origin (position in window coordinates after scrolling)
    let layout_window_origin = Point::new(local_pos.x - scroll_ctx.x, local_pos.y - scroll_ctx.y);

    // Update state
    let mut state_mut = state.borrow_mut();
    state_mut.scroll_cx = scroll_ctx;
    state_mut.layout_window_origin = layout_window_origin;

    ViewBoxProperties {
        element_id,
        local_rect,
        local_transform,
        scroll_offset,
        scroll_ctx,
        clip,
    }
}

fn compute_absolute_transforms_and_boxes(
    s: &ViewStorage,
    layout_tree: Rc<RefCell<taffy::TaffyTree<LayoutNodeCx>>>,
    box_tree: Rc<RefCell<BoxTree>>,
    node: NodeId,
    parent_scroll: Vec2,
    parent_scroll_ctx: Vec2,
) {
    let taffy = layout_tree.borrow();
    let layout = *taffy.layout(node).unwrap();
    let children = taffy.children(node).ok().map(|c| c.to_vec());
    drop(taffy);

    if let Some(&view_id) = s.taffy_to_view.get(&node) {
        let props =
            compute_view_box_properties(s, view_id, layout, parent_scroll, parent_scroll_ctx);

        // Update box tree
        {
            let mut box_tree = box_tree.borrow_mut();
            box_tree.set_local_bounds(props.element_id.0, props.local_rect);
            box_tree.set_local_clip(props.element_id.0, props.clip);
            box_tree.set_local_transform(props.element_id.0, props.local_transform);
        }

        // Recurse with this view's scroll offset
        if let Some(children) = children {
            for &child in &children {
                compute_absolute_transforms_and_boxes(
                    s,
                    layout_tree.clone(),
                    box_tree.clone(),
                    child,
                    props.scroll_offset,
                    props.scroll_ctx,
                );
            }
        }
    } else {
        // No view for this layout node, just recurse with parent's values
        if let Some(children) = children {
            for &child in &children {
                compute_absolute_transforms_and_boxes(
                    s,
                    layout_tree.clone(),
                    box_tree.clone(),
                    child,
                    parent_scroll,
                    parent_scroll_ctx,
                );
            }
        }
    }
}
