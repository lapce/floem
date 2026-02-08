use std::{cell::RefCell, collections::HashMap, time::Duration};

use crate::platform::menu_types::MenuId;

use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Size, Vec2};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use taffy::{AvailableSpace, NodeId};
use ui_events::pointer::{PointerId, PointerInfo};
use understory_event_state::{click::ClickState, focus::FocusState, hover::HoverState};
use winit::cursor::CursorIcon;
use winit::window::Theme;

use std::rc::Rc;

use crate::{
    BoxTree,
    action::{add_update_message, exec_after},
    context::FrameUpdate,
    event::{DragTracker, Event, WindowEvent, clear_hit_test_cache},
    inspector::CaptureState,
    layout::responsive::{GridBreakpoints, ScreenSizeBp},
    message::UpdateMessage,
    style::{
        CursorStyle, Style, StyleCache, StyleSelector, ZIndex, recalc::StyleRecalcChange,
        theme::default_theme,
    },
    view::{LayoutNodeCx, MeasureCx, VIEW_STORAGE, ViewId},
    visual_id::VisualId,
};

/// A small set of ViewIds, optimized for small collections (< 8 items).
/// Uses linear search which is faster than hashing for small N.
/// Inspired by Chromium's approach for event listener collections.
pub(crate) type ViewIdSmallSet = SmallVec<[ViewId; 8]>;

/// A small set of ViewIds, optimized for small collections (< 8 items).
/// Uses linear search which is faster than hashing for small N.
/// Inspired by Chromium's approach for event listener collections.
pub(crate) type VisualIdSmallSet = SmallVec<[VisualId; 8]>;

/// A small map from PointerId to ViewId, optimized for the common case of 1-2 pointers.
/// Most applications only have a mouse pointer or a few touch points active at once.
/// Uses linear search which is faster than HashMap for small N due to cache locality.
pub(crate) type PointerCaptureMap = SmallVec<[(PointerId, VisualId); 2]>;

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
    pub(crate) click_state: ClickState<Rc<[VisualId]>>,
    pub(crate) hover_state: HoverState<VisualId>,
    pub(crate) focus_state: FocusState<VisualId>,
    pub(crate) focusable: FxHashSet<ViewId>,
    pub(crate) file_hover_state: HoverState<VisualId>,
    pub(crate) visual_id_cursors: FxHashMap<VisualId, CursorStyle>,
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

    /// Cached inherited props from default_theme for root views.
    /// This avoids recomputing the inherited props from default_theme on every StyleCx::new().
    /// Updated when default_theme changes (on theme switch).
    pub(crate) default_theme_inherited: Rc<Style>,

    /// Tracking for views that have a layout listener
    pub(crate) has_layout_listener: FxHashSet<ViewId>,
    /// Tracking for views that have a visual position listener
    pub(crate) has_visual_changed_listener: FxHashSet<ViewId>,
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
            focus_state: FocusState::new(),
            click_state: ClickState::new(),
            hover_state: HoverState::new(),
            file_hover_state: HoverState::new(),
            visual_id_cursors: FxHashMap::default(),
            focusable: FxHashSet::default(),
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
            style_cache: StyleCache::new(),
            pending_child_change: FxHashMap::default(),
            pending_global_recalc: StyleRecalcChange::new(
                crate::style::Propagate::RecalcDescendants,
            ),
            default_theme: Rc::new(theme),
            default_theme_inherited: Rc::new(inherited),
            needs_layout: true,
            needs_box_tree_from_layout: true,
            needs_box_tree_commit: true,
            has_layout_listener: FxHashSet::default(),
            has_visual_changed_listener: FxHashSet::default(),
            views_needing_box_tree_update: FxHashSet::default(),
        }
    }

    /// Extract inherited props from a theme style for root view initialization.
    fn extract_inherited_props(theme: &Style) -> Style {
        let mut inherited_style = Style::new();
        if theme.any_inherited() {
            let inherited_props = theme.map.iter().filter(|(k, _)| k.inherited());
            inherited_style.apply_iter(inherited_props);
        }
        inherited_style
    }

    /// Update the default theme when the OS theme changes.
    pub(crate) fn update_default_theme(&mut self, theme: Theme) {
        let new_theme = default_theme(theme);
        let inherited = Self::extract_inherited_props(&new_theme);
        self.default_theme = Rc::new(new_theme);
        self.default_theme_inherited = Rc::new(inherited);
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
        let this_visual_id = id.get_visual_id();
        box_tree.borrow_mut().reparent(this_visual_id.0, None);
        id.remove();
        self.focusable.remove(&id);
        self.fixed_elements.remove(&id);

        // Clean up pointer capture state for removed view
        self.pointer_capture_target
            .retain(|(_, v)| *v != this_visual_id);
        self.pending_pointer_capture_target
            .retain(|(_, v)| *v != this_visual_id);
    }

    pub fn is_hovered(&self, id: impl Into<VisualId>) -> bool {
        let id = id.into();
        self.hover_state.current_path().contains(&id)
    }

    pub fn is_file_hover(&self, id: impl Into<VisualId>) -> bool {
        let id = id.into();
        self.file_hover_state.current_path().contains(&id)
    }

    pub fn is_focused(&self, id: impl Into<VisualId>) -> bool {
        self.focus_state
            .current_path()
            .last()
            .map(|f| *f == id.into())
            .unwrap_or(false)
    }

    pub fn is_clicking(&self, id: impl Into<VisualId>) -> bool {
        let id = id.into();
        self.click_state.presses().any(|p| p.target.contains(&id))
    }

    /// Check if a view has pointer capture for any pointer.
    pub fn has_capture(&self, id: impl Into<VisualId>) -> bool {
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
        target: impl Into<VisualId>,
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
        target: impl Into<VisualId>,
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
        target: impl Into<VisualId>,
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
        target: impl Into<VisualId>,
    ) -> bool {
        let target = target.into();
        self.pending_pointer_capture_target
            .iter()
            .any(|(id, v)| *id == pointer_id && *v == target)
    }

    /// Get the pending capture target for a pointer.
    #[inline]
    pub(crate) fn get_pending_capture_target(&self, pointer_id: PointerId) -> Option<VisualId> {
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
    pub(crate) fn get_pointer_capture_target(&self, pointer_id: PointerId) -> Option<VisualId> {
        self.pointer_capture_target
            .iter()
            .find(|(id, _)| *id == pointer_id)
            .map(|(_, v)| *v)
    }

    /// Check if any pointer has active capture to the given view.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn has_any_capture(&self, target: impl Into<VisualId>) -> bool {
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
        self.fixed_elements.insert(id);
    }

    /// Unregister a view from fixed positioning.
    /// Called when a view's style sets IsFixed to false.
    pub fn unregister_fixed_element(&mut self, id: ViewId) {
        self.fixed_elements.remove(&id);
    }

    pub fn compute_layout(&mut self) {
        let mut measure_context = MeasureCx::default();
        let _ = self.root_view_id.taffy().borrow_mut().set_style(
            self.root_layout_node,
            crate::style::Style::new().size_full().to_taffy_style(),
        );

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
    // The box tree update system has three separate operations:
    //
    // 1. update_box_tree_from_layout() - Full tree walk after layout
    //    - Called automatically after layout completes
    //    - Updates all box tree nodes from layout tree and view state
    //
    // 2. update_box_tree_for_view(view_id) - Single view update (non-recursive)
    //    - Used when a specific view changes (transform, scroll offset, etc.)
    //    - More efficient than full tree walk
    //    - Children inherit changes through box tree's hierarchical system
    //
    // 3. commit_box_tree() - Commit and handle damage
    //    - Called after any update operation
    //    - Computes world transforms and damage regions
    //    - Updates hover state if pointer is in damaged area
    //
    // The commit happens separately from updates so multiple updates can be
    // batched before committing. The `needs_box_tree_commit` flag tracks
    // whether a commit is needed.

    /// Update the box tree from the layout tree by walking the entire tree.
    ///
    /// This walks the layout tree recursively and updates all box tree nodes with:
    /// - Local bounds from layout
    /// - Local transforms from view state
    /// - Scroll offsets
    /// - Clip rectangles
    ///
    /// This should be called after layout completes to sync all box tree properties.
    /// The commit will happen separately when `commit_box_tree()` is called.
    pub fn update_box_tree_from_layout(&mut self) {
        let box_tree = self.box_tree.clone();
        let layout_tree = self.layout_tree.clone();
        compute_absolute_transforms_and_boxes(
            layout_tree,
            box_tree,
            self.root_layout_node,
            Vec2::ZERO, // parent_scroll - root has no parent scroll
            Vec2::ZERO, // parent_scroll_ctx - root has no accumulated scroll
        );
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
                                    (p.child_translation, p.scroll_ctx)
                                })
                                .unwrap_or((Vec2::ZERO, Vec2::ZERO))
                        } else {
                            (Vec2::ZERO, Vec2::ZERO)
                        };

                    let props = compute_view_box_properties(
                        view_id,
                        layout,
                        parent_scroll,
                        parent_scroll_ctx,
                    );

                    // Update box tree
                    let mut box_tree = self.box_tree.borrow_mut();
                    box_tree.set_local_bounds(props.visual_id.0, props.local_rect);
                    box_tree.set_local_clip(props.visual_id.0, props.clip);
                    box_tree.set_local_transform(props.visual_id.0, props.local_transform);
                    box_tree.set_local_z_index(props.visual_id.0, Some(props.z_index));
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

    /// Commit the box tree changes and handle damage regions.
    /// This should be called after updating the box tree (either from layout or for specific views).
    pub fn commit_box_tree(&mut self) {
        if let Some(dragging) = &mut self.drag_tracker.active_drag
            && let Some(dragging_preview) = dragging.dragging_preview.clone()
        {
            let local_bounds = self
                .box_tree
                .borrow()
                .local_bounds(dragging_preview.visual_id.0)
                .unwrap_or_default();

            // Get current world transform and update natural position (detects layout changes)
            let current_transform = self
                .box_tree
                .borrow()
                .compute_world_transform(dragging_preview.visual_id.0)
                .unwrap_or(Affine::IDENTITY);

            let natural_position = dragging.update_and_get_natural_position(current_transform);

            // Calculate the drag point offset (where user grabbed within the element)
            let drag_point_offset = Point::new(
                local_bounds.width() * (dragging_preview.drag_point_pct.0.0 / 100.0),
                local_bounds.height() * (dragging_preview.drag_point_pct.1.0 / 100.0),
            );

            // Calculate and apply position
            let new_point =
                dragging.calculate_position(natural_position, drag_point_offset);
            dragging.record_applied_translation(new_point);

            self.box_tree
                .borrow_mut()
                .set_world_translation(dragging_preview.visual_id.0, new_point);

            // Schedule next animation frame if needed
            if dragging.should_schedule_animation_frame() {
                let timer = exec_after(Duration::from_millis(8), move |_| {
                    add_update_message(UpdateMessage::RequestBoxTreeCommit);
                });
                dragging.animation_timer = Some(timer);
            }
        }

        // Clean up completed animations
        if let Some(dragging) = &self.drag_tracker.active_drag {
            if dragging.released_at.is_some() && dragging.is_animation_complete() {
                self.views_needing_box_tree_update
                    .insert(dragging.visual_id.view_id());
                self.drag_tracker.active_drag = None;
            }
        }

        let damage = self.box_tree.borrow_mut().commit();
        let pointer = self.last_pointer;
        for damage_rect in &damage.dirty_rects {
            if damage_rect.contains(pointer.0) {
                clear_hit_test_cache();
                let root_visual_id = self.root_view_id.get_visual_id();
                crate::event::GlobalEventCx::new(self, root_visual_id).update_hover_from_point(
                    pointer.0,
                    pointer.1,
                    &Event::Window(WindowEvent::ChangeUnderCursor),
                );
            }
        }
        self.needs_box_tree_commit = false;
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
        id: impl Into<VisualId>,
        cursor: CursorStyle,
    ) -> Option<CursorStyle> {
        self.needs_cursor_resolution = true;
        self.visual_id_cursors.insert(id.into(), cursor)
    }

    /// returns the previously set cursor if there was one
    pub fn clear_cursor(&mut self, id: impl Into<VisualId>) -> Option<CursorStyle> {
        self.needs_cursor_resolution = true;
        self.visual_id_cursors.remove(&id.into())
    }
}

struct ViewBoxProperties {
    visual_id: VisualId,
    local_rect: Rect,
    local_transform: Affine,
    scroll_offset: Vec2,
    scroll_ctx: Vec2,
    clip: Option<RoundedRect>,
    z_index: i32,
}

// New helper function to compute view's box tree properties
fn compute_view_box_properties(
    view_id: ViewId,
    layout: taffy::Layout,
    parent_scroll: Vec2,
    parent_scroll_ctx: Vec2,
) -> ViewBoxProperties {
    let size = Size::new(layout.size.width as f64, layout.size.height as f64);
    let local_rect = Rect::from_origin_size(Point::ZERO, size);
    let local_pos = Point::new(layout.location.x as f64, layout.location.y as f64);

    VIEW_STORAGE.with_borrow(|s| {
        let state = s.states.get(view_id).unwrap();
        let state_borrow = state.borrow();

        let style_transform = state_borrow.view_transform_props.affine(size);
        let view_local_transform = style_transform * state_borrow.transform;
        let scroll_offset = state_borrow.child_translation;
        let clip = state_borrow.box_tree_props.clip_rect(local_rect);
        let visual_id = state_borrow.visual_id;
        let z_index = state_borrow.combined_style.get(ZIndex).unwrap_or(0);

        drop(state_borrow);

        // Compute scroll context
        let scroll_ctx = if parent_scroll != Vec2::ZERO {
            parent_scroll_ctx + parent_scroll
        } else {
            parent_scroll_ctx
        };

        // Compute local transform
        let parent_transform_for_children = Affine::translate(-parent_scroll);
        let local_transform = view_local_transform
            * parent_transform_for_children
            * Affine::translate(local_pos.to_vec2());

        // Compute layout window origin (position in window coordinates after scrolling)
        let layout_window_origin =
            Point::new(local_pos.x - scroll_ctx.x, local_pos.y - scroll_ctx.y);

        // Update state
        let mut state_mut = state.borrow_mut();
        state_mut.scroll_ctx = scroll_ctx;
        state_mut.layout_window_origin = layout_window_origin;

        ViewBoxProperties {
            visual_id,
            local_rect,
            local_transform,
            scroll_offset,
            scroll_ctx,
            clip,
            z_index,
        }
    })
}

fn compute_absolute_transforms_and_boxes(
    layout_tree: Rc<RefCell<taffy::TaffyTree<LayoutNodeCx>>>,
    box_tree: Rc<RefCell<BoxTree>>,
    node: NodeId,
    parent_scroll: Vec2,
    parent_scroll_ctx: Vec2,
) {
    VIEW_STORAGE.with_borrow(|s| {
        let taffy = layout_tree.borrow();
        let layout = *taffy.layout(node).unwrap();
        let children = taffy.children(node).ok().map(|c| c.to_vec());
        drop(taffy);

        if let Some(&view_id) = s.taffy_to_view.get(&node) {
            let props =
                compute_view_box_properties(view_id, layout, parent_scroll, parent_scroll_ctx);

            // Update box tree
            {
                let mut box_tree = box_tree.borrow_mut();
                box_tree.set_local_bounds(props.visual_id.0, props.local_rect);
                box_tree.set_local_clip(props.visual_id.0, props.clip);
                box_tree.set_local_transform(props.visual_id.0, props.local_transform);
                box_tree.set_local_z_index(props.visual_id.0, Some(props.z_index));
            }

            // Recurse with this view's scroll offset
            if let Some(children) = children {
                for &child in &children {
                    compute_absolute_transforms_and_boxes(
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
                        layout_tree.clone(),
                        box_tree.clone(),
                        child,
                        parent_scroll,
                        parent_scroll_ctx,
                    );
                }
            }
        }
    })
}
