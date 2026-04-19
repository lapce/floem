use std::{cell::RefCell, collections::HashMap, time::Instant};

use crate::{
    action::exec_after_animation_frame,
    inspector::CaptureState,
    platform::menu_types::MenuId,
    style::{StyleSelectors, recalc::StyleReason},
    view::ViewStorage,
};
use crate::ElementIdExt;

use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Size, Vec2};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec};
use taffy::{AvailableSpace, NodeId};
use ui_events::pointer::{PointerId, PointerInfo};
use understory_event_state::{click::ClickState, focus::FocusState, hover::HoverState};
use understory_focus::{FocusEntry, FocusSpace};
use winit::cursor::CursorIcon;
use winit::window::Theme;

use std::rc::Rc;

use crate::{
    BoxTree, ElementId,
    action::add_update_message,
    context::FrameUpdate,
    event::{DragTracker, Event, WindowEvent, clear_hit_test_cache},
    layout::responsive::{GridBreakpoints, ScreenSizeBp},
    message::UpdateMessage,
    style::{CursorStyle, Style, StyleSelector, theme::default_theme},
    view::{LayoutNodeCx, MeasureCx, VIEW_STORAGE, ViewId},
};

/// A small map from PointerId to ViewId, optimized for the common case of 1-2 pointers.
/// Most applications only have a mouse pointer or a few touch points active at once.
/// Uses linear search which is faster than HashMap for small N due to cache locality.
pub(crate) type PointerCaptureMap = SmallVec<[(PointerId, ElementId); 2]>;

#[derive(Default)]
pub(crate) struct FocusNavCache {
    built: bool,
    meta_revision: u64,
    entries: SmallVec<[FocusEntry<ElementId>; 128]>,
}

fn build_focus_space_for_scope<'a>(
    tree: &BoxTree,
    scope_root: ElementId,
    out: &'a mut SmallVec<[FocusEntry<ElementId>; 128]>,
) -> FocusSpace<'a, ElementId> {
    out.clear();

    if !tree.is_alive(scope_root.0) {
        return FocusSpace { nodes: &[] };
    }

    // Match the upstream adapter traversal, but take focus policy from
    // Floem-owned metadata instead of raw box-tree focus flags.
    let mut stack = vec![(scope_root.0, 0u8)];
    while let Some((id, depth)) = stack.pop() {
        if !tree.is_alive(id) {
            continue;
        }

        if let (Some(flags), Some(bounds), Some(meta)) =
            (tree.flags(id), tree.world_bounds(id), tree.element_meta(id))
        {
            let focus = meta.focus;
            if flags.contains(understory_box_tree::NodeFlags::VISIBLE)
                && focus.is_keyboard_navigable()
                && focus.enabled
            {
                out.push(FocusEntry {
                    id: meta.element_id,
                    rect: bounds,
                    order: focus.order,
                    group: focus.group,
                    enabled: focus.enabled,
                    scope_depth: if focus.scope_depth == 0 {
                        depth
                    } else {
                        focus.scope_depth
                    },
                });
            }
        }

        let next_depth = depth.saturating_add(1);
        for &child in tree.children_of(id).iter().rev() {
            stack.push((child, next_depth));
        }
    }

    FocusSpace {
        nodes: out.as_slice(),
    }
}

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
    /// The OS-provided DPI scale for this window.
    ///
    /// This comes from the platform/windowing backend and changes when the window
    /// moves between monitors with different DPI settings. It does not change layout
    /// coordinates directly. Instead, it affects:
    /// - rendering: converts logical units to physical pixels for the renderer/surface
    /// - events: converts incoming physical event coordinates into the window's logical space
    /// - layout: indirectly, because `root_size` is stored in OS-logical units derived from it
    pub(crate) os_scale: f64,
    /// The user-controlled zoom factor for the window.
    ///
    /// This is an window-level scale, distinct from DPI. It affects:
    /// - layout: the root layout space is divided by this value so views lay out in zoomed logical units
    /// - rendering: combines with [`Self::os_scale`] to produce the renderer scale
    /// - events: combines with [`Self::os_scale`] so pointer/file-drag coordinates resolve into the
    ///   same logical space used by layout and the box tree
    pub(crate) user_scale: f64,
    pub(crate) scheduled_updates: Vec<FrameUpdate>,
    pub(crate) style_dirty: FxHashMap<ViewId, StyleReason>,
    pub(crate) request_paint: bool,
    pub(crate) drag_tracker: DragTracker,
    pub(crate) screen_size_bp: ScreenSizeBp,
    pub(crate) grid_bps: GridBreakpoints,
    pub(crate) click_state: ClickState<SmallVec<[ElementId; 64]>>,
    // TODO: Track hover state per pointer
    pub(crate) hover_state: HoverState<ElementId>,
    pub(crate) key_trigger_state: bool,
    pub(crate) focus_state: FocusState<ElementId>,
    pub(crate) last_focused_element: Option<ElementId>,
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

    /// Engine-owned style tree. One [`StyleNodeId`](floem_style::StyleNodeId)
    /// per styled view — kept in sync with the view hierarchy by this crate,
    /// then walked by [`StyleTree::compute_style`] during the style pass.
    ///
    /// Phase 2a (this commit): the tree's node set and parent/child edges
    /// track view lifecycle; style data and cascade invocation still live
    /// in `StyleCx`. Later phases push style/class data and flip the
    /// cascade to run here.
    pub(crate) style_tree: floem_style::StyleTree,

    /// The default theme style containing class definitions for built-in components.
    /// This is used as the root style context for all views when no parent exists.
    /// Contains styling like `.class(ListClass, |s| { s.class(ListItemClass, ...) })`.
    pub(crate) default_theme: Style,

    /// Cached inherited props from default_theme for root views.
    /// This avoids recomputing the inherited props from default_theme on every StyleCx::new().
    /// Updated when default_theme changes (on theme switch).
    pub(crate) default_theme_inherited: Style,

    /// Tracking for views that have a visual position listener
    pub(crate) listeners: FxHashMap<crate::event::listener::EventListenerKey, Vec<ViewId>>,
    pub(crate) needs_layout: bool,
    pub(crate) needs_box_tree_from_layout: bool,
    pub(crate) needs_box_tree_commit: bool,
    /// Views that need their box tree node updated (e.g., after transform or scroll changes).
    /// These are processed after layout and before commit.
    pub(crate) views_needing_box_tree_update: FxHashSet<ViewId>,
    pub(crate) focus_nav_cache: FocusNavCache,
    /// Timestamp captured once per frame, shared by all views during the style pass.
    /// Avoids per-view `Instant::now()` syscalls.
    pub(crate) frame_start: Instant,
}

impl WindowState {
    pub fn new(root_view_id: ViewId, os_theme: Option<Theme>, os_scale: f64) -> Self {
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
            os_scale,
            user_scale: 1.0,
            root_size: Size::ZERO,
            fixed_elements: FxHashSet::default(),
            screen_size_bp: ScreenSizeBp::Xs,
            scheduled_updates: vec![FrameUpdate::Paint(root_view_id)],
            request_paint: true,
            style_dirty: Default::default(),
            drag_tracker: DragTracker::new(),
            focus_state: FocusState::new(),
            last_focused_element: None,
            key_trigger_state: false,
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
                Point::new(-1., -1.),
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
            style_tree: floem_style::StyleTree::new(),
            default_theme: theme,
            default_theme_inherited: inherited,
            needs_layout: true,
            needs_box_tree_from_layout: true,
            needs_box_tree_commit: true,
            listeners: FxHashMap::default(),
            views_needing_box_tree_update: FxHashSet::default(),
            focus_nav_cache: FocusNavCache::default(),
            frame_start: Instant::now(),
        }
    }

    #[inline]
    pub(crate) fn invalidate_focus_nav_cache(&mut self) {
        self.focus_nav_cache.built = false;
        self.focus_nav_cache.entries.clear();
    }

    /// Returns the effective window scale used when converting between logical
    /// layout units and physical pixels.
    ///
    /// This is the scale that rendering and input coordinate conversion should use.
    /// It combines:
    /// - [`Self::os_scale`]: platform DPI scaling
    /// - [`Self::user_scale`]: application zoom
    ///
    /// Layout should usually not use this directly. Layout operates in logical
    /// coordinates and already accounts for user zoom by dividing the root layout
    /// space by [`Self::user_scale`].
    #[inline]
    pub fn effective_scale(&self) -> f64 {
        self.os_scale * self.user_scale
    }

    #[inline]
    fn focus_nav_cache_is_stale(&self) -> bool {
        if !self.focus_nav_cache.built {
            return true;
        }

        self.focus_nav_cache.meta_revision != crate::focus_nav_meta_revision()
    }

    pub(crate) fn keyboard_focus_space(&mut self) -> understory_focus::FocusSpace<'_, ElementId> {
        if self.focus_nav_cache_is_stale() {
            self.focus_nav_cache.entries.clear();
            let box_tree = self.box_tree.borrow();
            let root = self.root_view_id.get_element_id();
            build_focus_space_for_scope(&box_tree, root, &mut self.focus_nav_cache.entries);

            self.focus_nav_cache.meta_revision = crate::focus_nav_meta_revision();
            self.focus_nav_cache.built = true;
        }

        understory_focus::FocusSpace {
            nodes: &self.focus_nav_cache.entries,
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
        self.default_theme = new_theme;
        self.default_theme_inherited = inherited;
        self.style_tree.clear_cache();
    }

    /// This removes a view from the app state.
    pub fn remove_view(&mut self, id: ViewId) {
        self.invalidate_focus_nav_cache();
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
        self.style_dirty.remove(&id);
        self.views_needing_box_tree_update.remove(&id);

        // Release the companion StyleTree node. `remove_view` recurses
        // into children first, so their nodes are already gone by the
        // time we reach this point and no orphan descendants are left
        // behind.
        if let Some(style_node) = view_state.borrow().style_node {
            self.style_tree.remove_node(style_node);
        }

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
        self.focus_state.current_path().last() == Some(&id.into())
    }

    pub fn is_focus_within(&self, id: impl Into<ElementId>) -> bool {
        self.focus_state.current_path().contains(&id.into())
    }

    #[deprecated(note = "use `ViewId::is_active` instead")]
    pub fn is_clicking(&self, id: impl Into<ElementId>) -> bool {
        self.is_active(id)
    }

    pub fn is_active(&self, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.pointer_capture_target.iter().any(|t| t.1 == id)
            || self.click_state.presses().any(|p| p.target.contains(&id))
            || (self.key_trigger_state && self.focus_state.current_path().contains(&id))
    }

    /// Check if a view has pointer capture for any pointer.
    pub fn has_capture(&self, id: impl Into<ElementId>) -> bool {
        self.has_any_capture(id)
    }

    pub(crate) fn build_style_traversal(
        &mut self,
        root: ViewId,
    ) -> SmallVec<[(ViewId, StyleReason); 16]> {
        let mut traversal: SmallVec<[(ViewId, StyleReason); 16]> =
            SmallVec::with_capacity(self.style_dirty.len());

        if self.capture.is_some() {
            // Capture mode always does a full traversal and should consume all dirty entries.
            self.style_dirty.clear();

            // Full traversal when capture active
            let mut stack: SmallVec<[ViewId; 8]> = smallvec![root];
            // stack.push(root);
            while let Some(view_id) = stack.pop() {
                traversal.push((view_id, StyleReason::full_recalc()));

                let children = VIEW_STORAGE
                    .with_borrow(|s| s.children.get(view_id).cloned().unwrap_or_default());

                for child in children.iter().rev() {
                    stack.push(*child);
                }
            }
        } else {
            // Number of dirty views we still need to collect
            let mut remaining = self.style_dirty.len();

            if remaining == 0 {
                return SmallVec::new();
            }

            // DFS collecting dirty views in tree order
            let mut stack: SmallVec<[ViewId; 8]> = smallvec![root];

            while let Some(view_id) = stack.pop() {
                if let Some(reason) = self.style_dirty.remove(&view_id) {
                    traversal.push((view_id, reason));
                    remaining -= 1;

                    // Early exit once all dirty views found
                    if remaining == 0 {
                        break;
                    }
                }

                let children = VIEW_STORAGE
                    .with_borrow(|s| s.children.get(view_id).cloned().unwrap_or_default());

                for child in children.iter().rev() {
                    stack.push(*child);
                }
            }

            if remaining > 0 {
                // Some dirty views were not reachable from this window root.
                // Keeping them would cause style/update loops to spin forever
                // because traversal can no longer make progress on them.
                self.style_dirty.clear();
            }
        }

        // Fix ordering for custom style parents
        let mut i = traversal.len();
        while i > 0 {
            i -= 1;

            let view_id = traversal[i].0;

            if let Some(style_parent) = view_id.state().borrow().style_cx_parent
                && let Some(parent_pos) =
                    traversal[..i].iter().position(|(v, _)| *v == style_parent)
            {
                let view = traversal.remove(i);
                traversal.insert(parent_pos + 1, view);
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
    pub(crate) fn has_any_capture(&self, target: impl Into<ElementId>) -> bool {
        let target = target.into();
        self.pointer_capture_target
            .iter()
            .any(|(_, v)| *v == target)
    }

    /// Check if the pending capture map contains an entry for the given pointer.
    #[inline]
    #[expect(dead_code)]
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
            self.box_tree
                .borrow_mut()
                .set_world_position(id.get_element_id().0, None);
            self.needs_box_tree_commit = true;
            self.needs_layout = true;
        }
    }

    fn apply_fixed_element_styles(&self) {
        let root_size = self.root_size / self.user_scale;
        let fixed_views: SmallVec<[ViewId; 32]> = self.fixed_elements.iter().copied().collect();
        VIEW_STORAGE.with_borrow(|s| {
            for view_id in fixed_views {
                if let Some(state) = s.states.get(view_id) {
                    let state_borrow = state.borrow();
                    if !state_borrow.style_storage.combined_style.builtin().is_fixed() {
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
                    width: AvailableSpace::Definite(
                        (self.root_size.width / self.user_scale) as f32,
                    ),
                    height: AvailableSpace::Definite(
                        (self.root_size.height / self.user_scale) as f32,
                    ),
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
        self.invalidate_focus_nav_cache();
        let box_tree = self.box_tree.clone();
        let layout_tree = self.layout_tree.clone();
        VIEW_STORAGE.with_borrow(|s| {
            compute_absolute_transforms_and_boxes(
                s,
                layout_tree,
                box_tree,
                self.root_layout_node,
                Vec2::ZERO, // parent_scroll - root has no parent scroll
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
        self.invalidate_focus_nav_cache();
        VIEW_STORAGE.with_borrow(|s| {
            let state = s.states.get(view_id);

            if let Some(state) = state {
                let layout_node = state.borrow().layout_id;
                let layout = self.layout_tree.borrow().layout(layout_node).ok().copied();

                if let Some(layout) = layout {
                    // Get parent's scroll offset and scroll_ctx
                    let parent_scroll =
                        if let Some(parent_id) = s.parent.get(view_id).and_then(|p| *p) {
                            s.states
                                .get(parent_id)
                                .map(|p| {
                                    let p = p.borrow();
                                    p.child_translation
                                })
                                .unwrap_or(Vec2::ZERO)
                        } else {
                            Vec2::ZERO
                        };

                    let props = compute_view_box_properties(s, view_id, layout, parent_scroll);

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
    /// 1. Apply drag-preview world position override (if an active drag preview exists)
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
        self.invalidate_focus_nav_cache();
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
                .borrow_mut()
                .world_transform(dragging_preview.element_id.0)
                .unwrap_or(Affine::IDENTITY);

            let natural_position = dragging.update_and_get_natural_position(current_transform);

            // Calculate the drag point offset (where user grabbed within the element)
            let drag_point_offset = Point::new(
                local_bounds.width() * (dragging_preview.drag_point_pct.0.0 / 100.0),
                local_bounds.height() * (dragging_preview.drag_point_pct.1.0 / 100.0),
            );

            // Calculate and apply position
            let new_point = dragging.calculate_position(natural_position, drag_point_offset);
            dragging.record_applied_position(new_point);

            self.box_tree
                .borrow_mut()
                .set_world_position(dragging_preview.element_id.0, Some(new_point));

            // Schedule next animation frame if needed
            if dragging.should_schedule_animation_frame() {
                let timer = exec_after_animation_frame(move |_| {
                    add_update_message(UpdateMessage::RequestBoxTreeCommit);
                });
                dragging.animation_timer = Some(timer);
            }
        }

        // Clean up completed animations
        if let Some(dragging) = &self.drag_tracker.active_drag
            && dragging.released_at.is_some()
            && dragging.is_animation_complete()
        {
            if let Some(dragging_preview) = &dragging.dragging_preview {
                self.box_tree
                    .borrow_mut()
                    .set_world_position(dragging_preview.element_id.0, None);
            }
            self.views_needing_box_tree_update
                .insert(dragging.element_id.owning_id());
            self.drag_tracker.active_drag = None;
        }

        let mut dirty_rects = Vec::new();

        // Understory now exposes only last-committed world transforms while dirty.
        // Commit once to refresh layout-derived parent world transforms before applying
        // overlay/fixed corrections that depend on them, then commit the corrected state.
        dirty_rects.extend(self.box_tree.borrow_mut().commit().dirty_rects);

        self.apply_overlay_parent_transforms();
        self.apply_fixed_positioning_transforms();

        dirty_rects.extend(self.box_tree.borrow_mut().commit().dirty_rects);

        let pointer = self.last_pointer;
        for damage_rect in &dirty_rects {
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
        if !dirty_rects.is_empty() {
            self.invalidate_focus_nav_cache();
            self.request_paint = true;
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
                        let font_size_cx = overlay_state_borrow.style_storage.layout_props.font_size_cx();
                        let style_transform_props =
                            overlay_state_borrow.style_storage.view_transform_props.clone();
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

                        let style_transform = style_transform_props.affine(size, &font_size_cx);
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
                .world_transform(logical_parent_element_id.0)
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
    /// position override in the box tree.
    ///
    /// Running this after local-transform updates keeps the adjustment stable for both full
    /// layout sync and incremental box-tree updates.
    fn apply_fixed_positioning_transforms(&self) {
        let root_size = self.root_size / self.user_scale;
        let positions: SmallVec<[(ElementId, Point); 32]> = VIEW_STORAGE.with_borrow(|s| {
            self.fixed_elements
                .iter()
                .filter_map(|&view_id| {
                    let state = s.states.get(view_id)?;
                    let state_borrow = state.borrow();
                    let element_id = state_borrow.element_id;
                    let local_bounds = self
                        .box_tree
                        .borrow()
                        .local_bounds(element_id.0)
                        .unwrap_or_default();

                    let mut pos = Point::new(0.0, 0.0);
                    let font_size_cx = state_borrow.style_storage.layout_props.font_size_cx();
                    let layout_props = &state_borrow.style_storage.layout_props;

                    if let (Some(left), Some(_)) = (
                        layout_props
                            .inset_left()
                            .resolve(root_size.width, &font_size_cx),
                        layout_props
                            .inset_right()
                            .resolve(root_size.width, &font_size_cx),
                    ) {
                        pos.x = left;
                    } else if let Some(left) = layout_props
                        .inset_left()
                        .resolve(root_size.width, &font_size_cx)
                    {
                        pos.x = left;
                    } else if let Some(right) = layout_props
                        .inset_right()
                        .resolve(root_size.width, &font_size_cx)
                    {
                        pos.x = root_size.width - right - local_bounds.width();
                    }

                    if let (Some(top), Some(_)) = (
                        layout_props
                            .inset_top()
                            .resolve(root_size.height, &font_size_cx),
                        layout_props
                            .inset_bottom()
                            .resolve(root_size.height, &font_size_cx),
                    ) {
                        pos.y = top;
                    } else if let Some(top) = layout_props
                        .inset_top()
                        .resolve(root_size.height, &font_size_cx)
                    {
                        pos.y = top;
                    } else if let Some(bottom) = layout_props
                        .inset_bottom()
                        .resolve(root_size.height, &font_size_cx)
                    {
                        pos.y = root_size.height - bottom - local_bounds.height();
                    }

                    Some((element_id, pos))
                })
                .collect()
        });

        let mut tree = self.box_tree.borrow_mut();
        for (element_id, pos) in positions {
            tree.set_world_position(element_id.0, Some(pos));
        }
    }

    /// Requests that the style pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_style(&mut self, id: ViewId, reason: StyleReason) {
        self.schedule_style_with_target(id.get_element_id(), reason);
    }

    /// Ensure `view_id` has a companion [`floem_style::StyleNodeId`] in
    /// [`Self::style_tree`] and that its parent edge matches the current
    /// style-cx parent. Allocates the node on first call.
    ///
    /// The tree parent follows floem's `style_cx_parent` override when
    /// set (e.g. list items re-parent their row under the list) so
    /// inherited-prop / class-context propagation matches the old
    /// cascade. Structural `:nth-child` position is computed separately
    /// via [`Self::structural_position_for`] and pushed to the tree.
    ///
    /// Relies on top-down style traversal: when a child calls this, the
    /// style-parent's style-node has already been allocated in the same
    /// or an earlier pass, so the parent edge can be wired immediately.
    pub(crate) fn ensure_style_node(&mut self, view_id: ViewId) -> floem_style::StyleNodeId {
        let element_id = view_id.state().borrow().element_id;
        let existing = view_id.state().borrow().style_node;
        let node = match existing {
            Some(id) if self.style_tree.contains(id) => id,
            _ => {
                let id = self.style_tree.new_node(element_id);
                view_id.state().borrow_mut().style_node = Some(id);
                id
            }
        };

        let style_parent_view = view_id
            .state()
            .borrow()
            .style_cx_parent
            .or_else(|| view_id.parent());
        let parent_node = style_parent_view
            .and_then(|p| p.state().borrow().style_node)
            .filter(|p| self.style_tree.contains(*p));
        let current_parent = self.style_tree.get(node).and_then(|n| n.parent());
        if current_parent != parent_node {
            self.style_tree.set_parent(node, parent_node);
        }
        node
    }

    /// Compute the structural position (1-based `:nth-child` index and
    /// sibling count) for `view_id` relative to its style-cx parent. When
    /// a view (e.g. a row inside a list item) has a custom `style_cx_parent`,
    /// we walk up the DOM tree to find the ancestor that's a direct child
    /// of the style parent and use that ancestor's position. This matches
    /// the behavior `StyleCx::get_interact_state` had before the tree
    /// cascade took over.
    pub(crate) fn structural_position_for(&self, view_id: ViewId) -> (Option<usize>, usize) {
        let style_parent = view_id
            .state()
            .borrow()
            .style_cx_parent
            .or_else(|| view_id.parent());
        let Some(parent) = style_parent else {
            return (None, 0);
        };
        let indexed_child = parent.with_children(|siblings| {
            if siblings.contains(&view_id) {
                Some(view_id)
            } else {
                let mut cursor = view_id.parent();
                while let Some(ancestor) = cursor {
                    if ancestor.parent() == Some(parent) {
                        return Some(ancestor);
                    }
                    cursor = ancestor.parent();
                }
                None
            }
        });
        parent.with_children(|siblings| {
            (
                indexed_child
                    .and_then(|id| siblings.iter().position(|child| *child == id))
                    .map(|i| i + 1),
                siblings.len(),
            )
        })
    }

    /// Drive one cascade over the subtree rooted at `root_view`, given
    /// the set of views the traversal wants to re-style.
    ///
    /// Three passes:
    ///   1. Mirror each view's `view_style` (when the reason requests),
    ///      merged direct style, classes, parent-set interaction, and
    ///      structural position into the [`floem_style::StyleTree`].
    ///   2. Call [`floem_style::StyleTree::compute_style`]. `self` briefly
    ///      hands out its tree by move so it can pass itself as the
    ///      `&mut dyn StyleSink` sink (trait methods don't touch
    ///      `style_tree`, so the split is safe).
    ///   3. Copy cascade outputs back into each view's `style_storage` so
    ///      downstream per-view work (animations, taffy push, prop
    ///      extractors) continues reading from `ViewState` unchanged.
    pub(crate) fn run_style_cascade(
        &mut self,
        root_view: ViewId,
        traversal: &[(ViewId, StyleReason)],
    ) {
        // Pass 1: sync host state into the tree.
        for (view_id, traversal_reason) in traversal {
            let style_node = self.ensure_style_node(*view_id);

            if traversal_reason
                .flags
                .contains(floem_style::recalc::StyleReasonFlags::VIEW_STYLE)
                && let Some(view_style) = view_id.view().borrow().view_style()
            {
                let state = view_id.state();
                let mut vs = state.borrow_mut();
                let offset = vs.view_style_offset;
                vs.style.set(offset, view_style);
            }

            let view_class = view_id.view().borrow().view_class();
            let state = view_id.state();
            let direct = state.borrow_mut().style();
            let mut all_classes: SmallVec<[floem_style::StyleClassRef; 4]> =
                state.borrow().classes.clone();
            if let Some(vc) = view_class {
                all_classes.push(vc);
            }
            let parent_set_interaction = state.borrow().style_storage.parent_set_style_interaction;
            let structural = self.structural_position_for(*view_id);

            self.style_tree.set_direct_style(style_node, direct);
            self.style_tree.set_classes(style_node, &all_classes);
            self.style_tree
                .set_parent_interaction(style_node, parent_set_interaction);
            self.style_tree
                .set_structural_position_override(style_node, Some(structural));
        }

        // Pass 2: engine cascade.
        let root_style_node = root_view.state().borrow().style_node;
        if let Some(root_style_node) = root_style_node {
            let mut tree = std::mem::take(&mut self.style_tree);
            tree.compute_style(root_style_node, self);
            // Engine-originated next-frame schedule (animations mid-flight,
            // transitions still interpolating). Route each into floem's
            // per-frame update queue.
            let engine_scheduled: Vec<_> = tree.take_scheduled().collect();
            self.style_tree = tree;
            for (element_id, reason) in engine_scheduled {
                self.schedule_style_with_target(element_id, reason);
            }
        }

        // Pass 3: copy outputs back into per-view `style_storage`.
        for (view_id, _) in traversal {
            let Some(style_node) = view_id.state().borrow().style_node else {
                continue;
            };
            let Some(combined) = self.style_tree.combined_style(style_node).cloned() else {
                continue;
            };
            let inherited_cx = self
                .style_tree
                .inherited_context(style_node)
                .cloned()
                .unwrap_or_default();
            let class_cx = self
                .style_tree
                .class_context(style_node)
                .cloned()
                .unwrap_or_default();
            let post_interact = self
                .style_tree
                .style_interaction_cx(style_node)
                .unwrap_or_default();
            let computed = self
                .style_tree
                .computed_style(style_node)
                .cloned()
                .unwrap_or_default();
            let has_selectors = self.style_tree.has_style_selectors(style_node);

            let new_cursor = computed.builtin().cursor();
            let state = view_id.state();
            let mut vs = state.borrow_mut();
            let cursor_changed = vs.style_storage.style_cursor != new_cursor;
            vs.style_storage.combined_style = combined;
            vs.style_storage.computed_style = computed;
            vs.style_storage.style_cx = inherited_cx;
            vs.style_storage.class_cx = class_cx;
            vs.style_storage.style_interaction_cx = post_interact;
            vs.style_storage.has_style_selectors = has_selectors;
            if cursor_changed {
                vs.style_storage.style_cursor = new_cursor;
                drop(vs);
                self.needs_cursor_resolution = true;
            }
        }
    }

    /// Requests that the style pass will run for a specific element target on the next frame.
    ///
    /// Use this when a style update should be scoped to a sub-element owned by a view,
    /// rather than always targeting the owning view element.
    pub fn schedule_style_with_target(&mut self, element_id: ElementId, reason: StyleReason) {
        self.scheduled_updates
            .push(FrameUpdate::Style(element_id, reason));
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
    pub fn request_paint(&mut self, _id: impl Into<ElementId>) {
        self.request_paint = true;
    }

    pub(crate) fn update_screen_size_bp(&mut self, size: Size) {
        let bp = self.grid_bps.get_width_bp(size.width);
        if bp != self.screen_size_bp {
            self.screen_size_bp = bp;
            self.style_tree.clear_cache();
            self.root_view_id.request_style(StyleReason::with_selectors(
                StyleSelectors::empty().responsive(),
            ));
        }
    }

    pub(crate) fn has_style_for_sel(&self, id: ViewId, selector_kind: StyleSelector) -> bool {
        let view_state = id.state();
        let view_state = view_state.borrow();

        view_state
            .style_storage.has_style_selectors
            .is_some_and(|s| s.has(selector_kind))
    }

    pub(crate) fn mark_descendants_with_selector_dirty(
        &mut self,
        ancestor: ViewId,
        selector: StyleSelector,
    ) {
        let Some(node) = ancestor.state().borrow().style_node else {
            return;
        };
        let dirtied = self
            .style_tree
            .mark_descendants_with_selector_dirty(node, selector);
        for (element_id, reason) in dirtied {
            self.mark_style_dirty_with(element_id, reason);
        }
    }

    pub(crate) fn mark_descendants_with_responsive_selector_dirty(&mut self, ancestor: ViewId) {
        let Some(node) = ancestor.state().borrow().style_node else {
            return;
        };
        let dirtied = self
            .style_tree
            .mark_descendants_with_responsive_selector_dirty(node);
        for (element_id, reason) in dirtied {
            self.mark_style_dirty_with(element_id, reason);
        }
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

// ─────────────────────────────────────────────────────────────────────
// Style dirty map
// ─────────────────────────────────────────────────────────────────────
impl WindowState {
    /// Resolve an ElementId to the dirty map entry key (always a ViewId) and
    /// wrap the reason in a TARGET if the element is a sub-element rather than
    /// the view's primary element.
    fn resolve_dirty_reason(
        &mut self,
        element_id: ElementId,
        reason: StyleReason,
    ) -> (ViewId, StyleReason) {
        let view_id = element_id.owning_id();
        if element_id.is_view() {
            (view_id, reason)
        } else {
            (view_id, StyleReason::with_target(element_id, reason))
        }
    }

    pub fn mark_style_dirty_with(&mut self, element_id: ElementId, reason: StyleReason) {
        use std::collections::hash_map::Entry;
        let (view_id, reason) = self.resolve_dirty_reason(element_id, reason);
        match self.style_dirty.entry(view_id) {
            Entry::Occupied(mut e) => e.get_mut().merge(reason),
            Entry::Vacant(e) => {
                e.insert(reason);
            }
        }
    }

    pub fn mark_style_dirty(&mut self, element_id: ElementId) {
        self.mark_style_dirty_with(element_id, StyleReason::full_recalc());
    }

    pub fn mark_style_dirty_animation(&mut self, element_id: ElementId) {
        self.mark_style_dirty_with(element_id, StyleReason::animation());
    }

    pub fn mark_style_dirty_transition(&mut self, element_id: ElementId) {
        self.mark_style_dirty_with(element_id, StyleReason::transition());
    }

    pub fn mark_style_dirty_selector(&mut self, element_id: ElementId, selector: StyleSelector) {
        self.mark_style_dirty_with(
            element_id,
            StyleReason::with_selectors(StyleSelectors::empty().set_selector(selector, true)),
        );
    }
}

#[derive(Debug, Clone)]
struct ViewBoxProperties {
    element_id: ElementId,
    local_rect: Rect,
    local_transform: Affine,
    scroll_offset: Vec2,
    clip: Option<RoundedRect>,
}

// New helper function to compute view's box tree properties
fn compute_view_box_properties(
    s: &ViewStorage,
    view_id: ViewId,
    layout: taffy::Layout,
    parent_scroll: Vec2,
) -> ViewBoxProperties {
    let size = Size::new(layout.size.width as f64, layout.size.height as f64);
    let local_rect = Rect::from_origin_size(Point::ZERO, size);
    let local_pos = Point::new(layout.location.x as f64, layout.location.y as f64);

    let state = s.states.get(view_id).unwrap();
    let state_borrow = state.borrow();

    let font_size_cx = state_borrow.style_storage.layout_props.font_size_cx();
    let style_transform = state_borrow
        .style_storage.view_transform_props
        .affine(size, &font_size_cx);
    let view_local_transform = style_transform * state_borrow.transform;
    let scroll_offset = state_borrow.child_translation;
    let clip = state_borrow
        .style_storage.view_transform_props
        .clip_rect(local_rect, &font_size_cx);
    let element_id = state_borrow.element_id;

    drop(state_borrow);

    // Compute local transform
    let parent_transform_for_children = Affine::translate(-parent_scroll);
    let local_transform = parent_transform_for_children
        * Affine::translate(local_pos.to_vec2())
        * view_local_transform;

    ViewBoxProperties {
        element_id,
        local_rect,
        local_transform,
        scroll_offset,
        clip,
    }
}

fn compute_absolute_transforms_and_boxes(
    s: &ViewStorage,
    layout_tree: Rc<RefCell<taffy::TaffyTree<LayoutNodeCx>>>,
    box_tree: Rc<RefCell<BoxTree>>,
    node: NodeId,
    parent_scroll: Vec2,
) {
    let taffy = layout_tree.borrow();
    let layout = *taffy.layout(node).unwrap();
    let children = taffy.children(node).ok().map(|c| c.to_vec());
    drop(taffy);

    if let Some(&view_id) = s.taffy_to_view.get(&node) {
        let props = compute_view_box_properties(s, view_id, layout, parent_scroll);

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
                );
            }
        }
    }
}
