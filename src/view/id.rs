#![deny(missing_docs)]
//! # `ViewId`s
//!
//! [`ViewId`]s are unique identifiers for views.
//! They're used to identify views in the view tree.

use std::{any::Any, cell::RefCell, collections::HashSet, rc::Rc};

use floem_reactive::Scope;
use peniko::kurbo::{Affine, Insets, Point, Rect, Size};
use slotmap::new_key_type;
use taffy::{Layout, NodeId, TaffyTree};
use winit::window::WindowId;

use ui_events::pointer::PointerId;

use super::stacking::{invalidate_all_overlay_caches, invalidate_stacking_cache};
use super::{ChangeFlags, IntoView, StackOffset, VIEW_STORAGE, View, ViewState};

thread_local! {
    /// Views that have scopes but couldn't find a parent scope when added.
    /// These need to be re-parented after the view tree is fully assembled.
    static PENDING_SCOPE_REPARENTS: RefCell<HashSet<ViewId>> = RefCell::new(HashSet::new());
}
use crate::{
    ScreenLayout,
    action::add_update_message,
    animate::{AnimStateCommand, Animation},
    context::{EventCallback, ResizeCallback},
    event::{EventListener, EventPropagation},
    message::{
        CENTRAL_DEFERRED_UPDATE_MESSAGES, CENTRAL_UPDATE_MESSAGES, DeferredChild, DeferredChildren,
        DeferredReactiveSetup, UpdateMessage,
    },
    platform::menu::Menu,
    style::{Draggable, Focusable, PointerEvents, Style, StyleClassRef, StyleSelector},
    unit::PxPct,
    window::tracking::{is_known_root, window_id_for_root},
};

use super::AnyView;

new_key_type! {
    /// A small unique identifier for an instance of a [View](crate::View).
    ///
    /// This id is how you can access and modify a view, including accessing children views and updating state.
   pub struct ViewId;
}

impl ViewId {
    /// Create a new unique `Viewid`.
    pub fn new() -> ViewId {
        VIEW_STORAGE.with_borrow_mut(|s| s.view_ids.insert(()))
    }

    /// Check if this ViewId is still valid (exists in VIEW_STORAGE).
    ///
    /// A ViewId becomes invalid when it has been removed from the view tree.
    /// This is useful for filtering out stale ViewIds from collections.
    pub fn is_valid(&self) -> bool {
        VIEW_STORAGE.with_borrow(|s| s.view_ids.contains_key(*self))
    }

    /// Remove this view id and all of its children from the `VIEW_STORAGE`.
    ///
    /// Note: For full cleanup including taffy nodes and cleanup listeners,
    /// use `window_state.remove_view()` or send an `UpdateMessage::RemoveViews`.
    pub fn remove(&self) {
        // Dispose children scope if this view had reactive children
        if let Some(scope) = self.take_children_scope() {
            scope.dispose();
        }
        // Dispose keyed children scopes if this view had keyed reactive children
        if let Some(keyed_children) = self.take_keyed_children() {
            for (_child_id, scope) in keyed_children {
                scope.dispose();
            }
        }
        // Get parent before removing, for stacking cache invalidation
        let parent = self.parent();
        VIEW_STORAGE.with_borrow_mut(|s| {
            // Remove the cached root, in the (unlikely) case that this view is
            // re-added to a different window
            s.root.remove(*self);
            // Remove from overlays if registered
            s.overlays.remove(*self);
            // Remove self from parent's children list
            if let Some(Some(parent)) = s.parent.get(*self)
                && let Some(children) = s.children.get_mut(*parent)
            {
                children.retain(|c| c != self);
            }
            // Clean up all SecondaryMap entries for this view to prevent
            // stale data when slots are reused. SecondaryMaps don't auto-clean
            // when the primary SlotMap key is removed.
            s.children.remove(*self);
            s.parent.remove(*self);
            s.states.remove(*self);
            s.views.remove(*self);
            // Remove from primary SlotMap last
            s.view_ids.remove(*self);
        });
        // Invalidate parent's stacking cache since its children changed
        if let Some(parent) = parent {
            invalidate_stacking_cache(parent);
        }
    }

    /// Register this view as an overlay.
    ///
    /// Overlays escape z-index constraints and are painted at the root level,
    /// above all other views. The root is determined at registration time.
    pub(crate) fn register_overlay(&self) {
        let root_id = self.root().unwrap_or(*self);
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.overlays.insert(*self, root_id);
        });
        // Invalidate overlay cache - use invalidate_all since root may not be finalized yet
        invalidate_all_overlay_caches();
    }

    /// Unregister this view as an overlay.
    #[allow(dead_code)] // Kept for API symmetry with register_overlay
    pub(crate) fn unregister_overlay(&self) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.overlays.remove(*self);
        });
        // Invalidate overlay cache
        invalidate_all_overlay_caches();
    }

    /// Check if this view is registered as an overlay.
    pub(crate) fn is_overlay(&self) -> bool {
        VIEW_STORAGE.with_borrow(|s| s.overlays.contains_key(*self))
    }

    /// Get access to the taffy tree
    pub fn taffy(&self) -> Rc<RefCell<TaffyTree>> {
        VIEW_STORAGE.with_borrow(|s| s.taffy.clone())
    }

    /// Create a new taffy layout node
    pub fn new_taffy_node(&self) -> NodeId {
        self.taffy()
            .borrow_mut()
            .new_leaf(taffy::style::Style::DEFAULT)
            .unwrap()
    }

    /// Set the layout properties on a taffy node
    pub fn set_taffy_style(&self, node: NodeId, style: taffy::Style) {
        let _ = self.taffy().borrow_mut().set_style(node, style);
    }

    /// Get the layout for a taffy node relative to it's parent
    pub fn taffy_layout(&self, node: NodeId) -> Option<taffy::Layout> {
        self.taffy().borrow().layout(node).cloned().ok()
    }

    /// Mark the taffy node associated with this view as dirty.
    pub fn mark_view_layout_dirty(&self) -> taffy::TaffyResult<()> {
        let node = self.taffy_node();
        self.taffy().borrow_mut().mark_dirty(node)
    }
    /// Get the taffy node associated with this Id
    pub fn taffy_node(&self) -> NodeId {
        self.state().borrow().node
    }

    /// set the transform on a view that is applied after style transforms
    pub fn set_transform(&self, transform: Affine) {
        self.state().borrow_mut().transform = transform;
        self.request_layout();
    }

    pub(crate) fn state(&self) -> Rc<RefCell<ViewState>> {
        VIEW_STORAGE.with_borrow_mut(|s| {
            if !s.view_ids.contains_key(*self) {
                // if view_ids doesn't have this view id, that means it's been cleaned up,
                // so we shouldn't create a new ViewState for this Id.
                s.stale_view_state.clone()
            } else {
                s.states
                    .entry(*self)
                    .unwrap()
                    .or_insert_with(|| {
                        Rc::new(RefCell::new(ViewState::new(
                            *self,
                            &mut s.taffy.borrow_mut(),
                        )))
                    })
                    .clone()
            }
        })
    }

    /// Get access to the View
    pub(crate) fn view(&self) -> Rc<RefCell<Box<dyn View>>> {
        VIEW_STORAGE.with_borrow(|s| {
            s.views
                .get(*self)
                .cloned()
                .unwrap_or_else(|| s.stale_view.clone())
        })
    }

    /// Add a child View to this Id's list of children
    pub fn add_child(&self, child: Box<dyn View>) {
        let child_id = child.id();
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.children.entry(*self).unwrap().or_default().push(child_id);
            s.parent.insert(child_id, Some(*self));
            s.views.insert(child_id, Rc::new(RefCell::new(child)));
        });
        // Re-parent child's scope under nearest ancestor's scope to match view hierarchy.
        // This ensures scope hierarchy matches view hierarchy for proper cleanup.
        reparent_scope_if_needed(child_id, *self);
        // Invalidate stacking cache since children changed
        invalidate_stacking_cache(*self);
    }

    /// Append multiple children to this Id's list of children.
    ///
    /// This is more efficient than calling `add_child` multiple times
    /// as it only borrows VIEW_STORAGE once.
    ///
    /// Takes a `Vec` to ensure views are fully constructed before borrowing
    /// VIEW_STORAGE, avoiding potential borrow conflicts.
    pub fn append_children(&self, children: Vec<Box<dyn View>>) {
        let child_ids: Vec<ViewId> = children.iter().map(|c| c.id()).collect();
        VIEW_STORAGE.with_borrow_mut(|s| {
            let children_list = s.children.entry(*self).unwrap().or_default();
            for child in children {
                let child_id = child.id();
                children_list.push(child_id);
                s.parent.insert(child_id, Some(*self));
                s.views.insert(child_id, Rc::new(RefCell::new(child)));
            }
        });
        // Re-parent child scopes under nearest ancestor's scope
        for child_id in child_ids {
            reparent_scope_if_needed(child_id, *self);
        }
        // Invalidate stacking cache since children changed
        invalidate_stacking_cache(*self);
    }

    /// Set the children views of this Id
    /// See also [`Self::set_children_vec`]
    pub fn set_children<const N: usize, V: IntoView>(&self, children: [V; N]) {
        let children_ids: Vec<ViewId> = VIEW_STORAGE.with_borrow_mut(|s| {
            let mut children_ids = Vec::new();
            for child in children {
                let child_view = child.into_view();
                let child_view_id = child_view.id();
                children_ids.push(child_view_id);
                s.parent.insert(child_view_id, Some(*self));
                s.views
                    .insert(child_view_id, Rc::new(RefCell::new(child_view.into_any())));
            }
            s.children.insert(*self, children_ids.clone());
            children_ids
        });
        // Re-parent child scopes under nearest ancestor's scope
        for child_id in children_ids {
            reparent_scope_if_needed(child_id, *self);
        }
        // Invalidate stacking cache since children changed
        invalidate_stacking_cache(*self);
    }

    /// Set the children views of this Id using a Vector
    /// See also [`Self::set_children`] and [`Self::set_children_iter`]
    pub fn set_children_vec(&self, children: Vec<impl IntoView>) {
        self.set_children_iter(children.into_iter().map(|c| c.into_any()));
    }

    /// Set the children views of this Id using an iterator of boxed views.
    ///
    /// This is the most efficient way to set children when you already have
    /// an iterator of `Box<dyn View>`, as it avoids intermediate allocations.
    ///
    /// See also [`Self::set_children`] and [`Self::set_children_vec`]
    pub fn set_children_iter(&self, children: impl Iterator<Item = Box<dyn View>>) {
        let children_ids: Vec<ViewId> = VIEW_STORAGE.with_borrow_mut(|s| {
            let mut children_ids = Vec::new();
            for child_view in children {
                let child_view_id = child_view.id();
                children_ids.push(child_view_id);
                s.parent.insert(child_view_id, Some(*self));
                s.views
                    .insert(child_view_id, Rc::new(RefCell::new(child_view)));
            }
            s.children.insert(*self, children_ids.clone());
            children_ids
        });
        // Re-parent child scopes under nearest ancestor's scope
        for child_id in children_ids {
            reparent_scope_if_needed(child_id, *self);
        }
        // Invalidate stacking cache since children changed
        invalidate_stacking_cache(*self);
    }

    /// Set the view that should be associated with this Id
    pub fn set_view(&self, view: Box<dyn View>) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            if s.view_ids.contains_key(*self) {
                s.views.insert(*self, Rc::new(RefCell::new(view)));
            }
        });
    }

    /// Set the Id that should be used as the parent of this Id
    pub fn set_parent(&self, parent: ViewId) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            if s.view_ids.contains_key(*self) {
                s.parent.insert(*self, Some(parent));
            }
        });
    }

    /// Set the Ids that should be used as the children of this Id
    pub fn set_children_ids(&self, children: Vec<ViewId>) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            if s.view_ids.contains_key(*self) {
                s.children.insert(*self, children);
            }
        });
        // Invalidate stacking cache since children changed
        invalidate_stacking_cache(*self);
    }

    /// Get the list of `ViewId`s that are associated with the children views of this `ViewId`
    pub fn children(&self) -> Vec<ViewId> {
        VIEW_STORAGE.with_borrow(|s| s.children.get(*self).cloned().unwrap_or_default())
    }

    /// Get access to the list of `ViewId`s that are associated with the children views of this `ViewId`
    pub fn with_children<R>(&self, mut children: impl FnMut(&[ViewId]) -> R) -> R {
        VIEW_STORAGE.with_borrow(|s| children(s.children.get(*self).map_or(&[], |v| v)))
    }

    /// Get the `ViewId` that has been set as this `ViewId`'s parent
    pub fn parent(&self) -> Option<ViewId> {
        VIEW_STORAGE.with_borrow(|s| s.parent.get(*self).cloned().flatten())
    }

    /// Get the root view of the window that the given view is in
    pub fn root(&self) -> Option<ViewId> {
        VIEW_STORAGE.with_borrow_mut(|s| {
            if let Some(root) = s.root.get(*self) {
                // The cached value will be cleared on remove() above
                return *root;
            }
            let root_view_id = s.root_view_id(*self);
            // root_view_id() always returns SOMETHING.  If the view is not yet added
            // to a window, it can be itself or its nearest ancestor, which means we
            // will store garbage permanently.
            if let Some(root) = root_view_id
                && is_known_root(&root)
            {
                s.root.insert(*self, root_view_id);
                return Some(root);
            }
            None
        })
    }

    /// Get the computed rectangle that covers the area of this View
    pub fn layout_rect(&self) -> Rect {
        self.state().borrow().layout_rect
    }

    /// Get the size of this View
    pub fn get_size(&self) -> Option<Size> {
        self.get_layout()
            .map(|l| Size::new(l.size.width as f64, l.size.height as f64))
    }

    /// Get the Size of the parent View
    pub fn parent_size(&self) -> Option<Size> {
        let parent_id = self.parent()?;
        parent_id.get_size()
    }

    /// Returns the layout rect excluding borders, padding and position.
    /// This is relative to the view.
    pub fn get_content_rect(&self) -> Rect {
        let size = self
            .get_layout()
            .map(|layout| layout.size)
            .unwrap_or_default();
        let rect = Size::new(size.width as f64, size.height as f64).to_rect();
        let view_state = self.state();
        let props = &view_state.borrow().layout_props;
        let pixels = |px_pct, abs| match px_pct {
            PxPct::Px(v) => v,
            PxPct::Pct(pct) => pct * abs,
        };
        let border = props.border();
        let padding = props.padding();
        rect.inset(-Insets {
            x0: border.left.map_or(0.0, |b| b.0.width)
                + pixels(padding.left.unwrap_or(PxPct::Px(0.0)), rect.width()),
            x1: border.right.map_or(0.0, |b| b.0.width)
                + pixels(padding.right.unwrap_or(PxPct::Px(0.0)), rect.width()),
            y0: border.top.map_or(0.0, |b| b.0.width)
                + pixels(padding.top.unwrap_or(PxPct::Px(0.0)), rect.height()),
            y1: border.bottom.map_or(0.0, |b| b.0.width)
                + pixels(padding.bottom.unwrap_or(PxPct::Px(0.0)), rect.height()),
        })
    }
    /// This gets the Taffy Layout and adjusts it to be relative to the parent `View`.
    pub fn get_layout(&self) -> Option<Layout> {
        let widget_parent = self.parent().map(|id| id.state().borrow().node);

        let taffy = self.taffy();
        let mut node = self.state().borrow().node;
        let mut layout = *taffy.borrow().layout(node).ok()?;

        loop {
            let parent = taffy.borrow().parent(node);

            if parent == widget_parent {
                break;
            }

            node = parent?;

            layout.location = layout.location + taffy.borrow().layout(node).ok()?.location;
        }

        Some(layout)
    }

    /// Get the taffy layout of this id relative to a parent/ancestor ID
    pub fn get_layout_relative_to(&self, relative_to: ViewId) -> Option<Layout> {
        let taffy = self.taffy();
        let target_node = relative_to.state().borrow().node;
        let mut node = self.state().borrow().node;
        let mut layout = *taffy.borrow().layout(node).ok()?;

        loop {
            let parent = taffy.borrow().parent(node);
            if parent == Some(target_node) {
                break;
            }

            // If we've reached the root without finding the target, return None
            node = parent?;
            layout.location = layout.location + taffy.borrow().layout(node).ok()?.location;
        }

        Some(layout)
    }

    /// Get the taffy layout of this id relative to the root
    pub fn get_layout_relative_to_root(&self) -> Option<Layout> {
        let taffy = self.taffy();
        let node = self.state().borrow().node;
        let layout = *taffy.borrow().layout(node).ok()?;

        Some(layout)
    }

    /// Returns the CSS transform applied to this view.
    ///
    /// This returns the view's local transform (not including parent transforms).
    /// The transform includes translate, rotate, and scale operations.
    pub fn get_transform(&self) -> peniko::kurbo::Affine {
        self.state().borrow().transform
    }

    /// Returns the view's visual position in window coordinates.
    ///
    /// This is derived from `visual_transform`, which is the single source
    /// of truth for a view's position. For views without CSS scale/rotate transforms,
    /// this equals the layout position plus CSS translate. For views with scale/rotate,
    /// this includes the effect of center-based transforms.
    pub fn get_visual_origin(&self) -> peniko::kurbo::Point {
        self.state().borrow().visual_origin()
    }

    /// Returns the view's window origin (layout position after CSS translate).
    ///
    /// This is the position used for child layout and does NOT include scale/rotate effects.
    /// For the position including all CSS transforms (scale, rotate), use `get_visual_origin()`.
    ///
    /// The difference between `window_origin` and `visual_origin`:
    /// - `window_origin`: Position from layout + CSS translate (used for child positioning)
    /// - `visual_origin`: Position from visual_transform (includes center-based scale/rotate)
    ///
    /// For views without scale/rotate transforms, these values are identical.
    pub fn get_window_origin(&self) -> peniko::kurbo::Point {
        self.state().borrow().window_origin()
    }

    /// Returns the layout rect in window coordinates.
    ///
    /// This is the bounding rect that encompasses this view and its children,
    /// positioned at the window origin. Useful for hit testing and paint bounds.
    pub fn get_layout_rect(&self) -> peniko::kurbo::Rect {
        self.state().borrow().layout_rect
    }

    /// Returns the complete local-to-window coordinate transform.
    ///
    /// This transform converts coordinates from this view's local space to window
    /// coordinates. It combines:
    /// - The view's position in the window
    /// - Any CSS transforms (scale, rotate)
    ///
    /// To convert a local point to window coordinates: `visual_transform * point`
    /// To convert a window point to local coordinates: `visual_transform.inverse() * point`
    ///
    /// This is the transform used by event dispatch to convert pointer coordinates.
    pub fn get_visual_transform(&self) -> peniko::kurbo::Affine {
        self.state().borrow().visual_transform
    }

    /// Returns true if this view is hidden.
    pub fn is_hidden(&self) -> bool {
        self.state().borrow().visibility.is_hidden()
    }

    /// if the view has pointer events none
    pub fn pointer_events_none(&self) -> bool {
        let state = self.state();
        let state = state.borrow();
        state
            .computed_style
            .builtin()
            .pointer_events()
            .map(|p| p == PointerEvents::None)
            .unwrap_or(false)
    }

    /// Returns true if the view is disabled
    ///
    /// This is done by checking if the style for this view has `Disabled` set to true.
    pub fn is_disabled(&self) -> bool {
        self.state().borrow_mut().style_interaction_cx.disabled
    }

    /// Returns true if the view is selected
    ///
    /// This is done by checking if the parent has set this view as selected
    /// via `parent_set_selected()`.
    pub fn is_selected(&self) -> bool {
        self.state().borrow().parent_set_style_interaction.selected
    }

    /// Check if this id can be focused.
    ///
    /// This is done by checking if the style for this view has `Focusable` set to true.
    pub fn can_focus(&self) -> bool {
        self.state().borrow().computed_style.get(Focusable)
    }

    /// Check if this id can be dragged.
    ///
    /// This is done by checking if the style for this view has `Draggable` set to true.
    pub fn can_drag(&self) -> bool {
        self.state().borrow().computed_style.get(Draggable)
    }

    /// Request that this the `id` view be styled, laid out and painted again.
    /// This will recursively request this for all parents.
    pub fn request_all(&self) {
        add_update_message(UpdateMessage::RequestStyle(*self));
        add_update_message(UpdateMessage::RequestViewStyle(*self));
        self.request_layout();
        self.add_update_message(UpdateMessage::RequestPaint);
    }

    /// Request that this view have it's layout pass run
    pub fn request_layout(&self) {
        self.request_changes(ChangeFlags::LAYOUT)
    }

    /// Get the window id of the window containing this view, if there is one.
    pub fn window_id(&self) -> Option<WindowId> {
        self.root().and_then(window_id_for_root)
    }

    /// Request that this view have it's paint pass run
    pub fn request_paint(&self) {
        self.add_update_message(UpdateMessage::RequestPaint);
    }

    /// request that this node be styled again
    /// This will recursively request style for all parents.
    pub fn request_style(&self) {
        self.add_update_message(UpdateMessage::RequestStyle(*self));
    }

    /// Use this when you want the `view_style` method from the `View` trait to be rerun.
    pub fn request_view_style(&self) {
        self.add_update_message(UpdateMessage::RequestViewStyle(*self));
    }

    pub(crate) fn request_changes(&self, flags: ChangeFlags) {
        let state = self.state();
        if state.borrow().requested_changes.contains(flags) {
            return;
        }
        state.borrow_mut().requested_changes.insert(flags);
        if let Some(parent) = self.parent() {
            parent.request_changes(flags);
        }
    }

    /// Requests style for this view and all direct and indirect children.
    pub fn request_style_recursive(&self) {
        let state = self.state();
        state.borrow_mut().request_style_recursive = true;
        self.request_style();
    }

    /// Requests style for this view and descendants that have the specified selector.
    ///
    /// This is more efficient than `request_style_recursive` when only views with
    /// certain selectors (like `:focus`, `:active`) need to be updated.
    /// Views without the selector in their `has_style_selectors` are skipped.
    ///
    /// # Arguments
    /// * `selector` - The selector type to check for (e.g., `StyleSelector::Focus`)
    pub fn request_style_for_selector_recursive(&self, selector: StyleSelector) {
        // Always request style for self (the root of the recursive call)
        self.request_style();

        // Recursively check children
        fn request_for_descendants(id: ViewId, selector: StyleSelector) {
            for child in id.children() {
                let needs_update = {
                    let state = child.state();
                    let state = state.borrow();
                    state.has_style_selectors.has(selector)
                };

                if needs_update {
                    child.request_style();
                }

                // Always recurse to find nested views with the selector
                request_for_descendants(child, selector);
            }
        }

        request_for_descendants(*self, selector);
    }

    /// Request that this view gain the window focus
    pub fn request_focus(&self) {
        self.add_update_message(UpdateMessage::Focus(*self));
    }

    /// Clear the focus from this window
    pub fn clear_focus(&self) {
        self.add_update_message(UpdateMessage::ClearFocus(*self));
    }

    /// Set the system context menu that should be shown when this view is right-clicked
    pub fn update_context_menu(&self, menu: impl Fn() -> Menu + 'static) {
        self.state().borrow_mut().context_menu = Some(Rc::new(menu));
    }

    /// Set the system popout menu that should be shown when this view is clicked
    ///
    /// Adds a primary-click context menu, which opens below the view.
    pub fn update_popout_menu(&self, menu: impl Fn() -> Menu + 'static) {
        self.state().borrow_mut().popout_menu = Some(Rc::new(menu));
    }

    /// Request that this view receive the active state (mark that this element is currently being interacted with)
    ///
    /// When an View has Active, it will receive events such as mouse events, even if the mouse is not directly over this view.
    /// This is usefor for views such as Sliders, where the mouse event should be sent to the slider view as long as the mouse is pressed down,
    /// even if the mouse moves out of the view, or even out of the Window.
    pub fn request_active(&self) {
        self.add_update_message(UpdateMessage::Active(*self));
    }

    /// Request that the active state be removed from this View
    pub fn clear_active(&self) {
        self.add_update_message(UpdateMessage::ClearActive(*self));
    }

    // =========================================================================
    // Pointer Capture API (W3C Pointer Events inspired)
    // =========================================================================

    /// Set pointer capture for this view.
    ///
    /// When a view has pointer capture for a pointer, all subsequent pointer events
    /// for that pointer are dispatched directly to this view, regardless of where
    /// the pointer moves. This is useful for:
    /// - Drag operations that should continue even when the pointer leaves the view
    /// - Sliders and scrollbars that need to track pointer movement globally
    /// - Any interaction that requires reliable pointer tracking
    ///
    /// The capture will be applied on the next pointer event for this pointer ID.
    /// When capture is set:
    /// - `GotPointerCapture` event is fired to this view
    /// - All subsequent pointer events for this pointer are routed here
    /// - When released, `LostPointerCapture` event is fired
    ///
    /// Capture is automatically released on `PointerUp` for the captured pointer.
    ///
    /// # Example
    /// ```ignore
    /// fn event_before_children(&mut self, cx: &mut EventCx, event: &Event) -> EventPropagation {
    ///     if let Event::Pointer(PointerEvent::Down(e)) = event {
    ///         if let Some(pointer_id) = e.pointer.pointer_id {
    ///             self.id().set_pointer_capture(pointer_id);
    ///         }
    ///     }
    ///     EventPropagation::Continue
    /// }
    /// ```
    pub fn set_pointer_capture(&self, pointer_id: PointerId) {
        self.add_update_message(UpdateMessage::SetPointerCapture {
            view_id: *self,
            pointer_id,
        });
    }

    /// Release pointer capture from this view.
    ///
    /// If this view has capture for the specified pointer, the capture will be
    /// released on the next pointer event. A `LostPointerCapture` event will be
    /// fired when the release takes effect.
    ///
    /// Note: This only releases capture if this view currently has it.
    /// It's safe to call even if this view doesn't have capture.
    pub fn release_pointer_capture(&self, pointer_id: PointerId) {
        self.add_update_message(UpdateMessage::ReleasePointerCapture {
            view_id: *self,
            pointer_id,
        });
    }

    /// Send a message to the application to open the Inspector for this Window
    pub fn inspect(&self) {
        self.add_update_message(UpdateMessage::Inspect);
    }

    /// Scrolls the view and all direct and indirect children to bring the view to be
    /// visible. The optional rectangle can be used to add an additional offset and intersection.
    pub fn scroll_to(&self, rect: Option<Rect>) {
        self.add_update_message(UpdateMessage::ScrollTo { id: *self, rect });
    }

    pub(crate) fn transition_anim_complete(&self) {
        self.add_update_message(UpdateMessage::ViewTransitionAnimComplete(*self));
    }

    pub(crate) fn update_animation(&self, offset: StackOffset<Animation>, animation: Animation) {
        let state = self.state();
        state.borrow_mut().animations.set(offset, animation);
        self.request_style();
    }

    pub(crate) fn update_animation_state(
        &self,
        offset: StackOffset<Animation>,
        command: AnimStateCommand,
    ) {
        let view_state = self.state();
        view_state
            .borrow_mut()
            .animations
            .update(offset, move |anim| anim.transition(command));
        self.request_style();
    }

    /// Send a state update to the `update` method of the associated View
    pub fn update_state(&self, state: impl Any) {
        self.add_update_message(UpdateMessage::State {
            id: *self,
            state: Box::new(state),
        });
    }

    /// `viewport` is relative to the `id` view.
    pub(crate) fn set_viewport(&self, viewport: Rect) {
        let state = self.state();
        state.borrow_mut().viewport = Some(viewport);
    }

    /// Add an callback on an action for a given `EventListener`
    pub fn add_event_listener(&self, listener: EventListener, action: Box<EventCallback>) {
        let state = self.state();
        state.borrow_mut().add_event_listener(listener, action);
    }

    /// Set a callback that should be run when the size of the view changes
    pub fn add_resize_listener(&self, action: Rc<ResizeCallback>) {
        let state = self.state();
        state.borrow_mut().add_resize_listener(action);
    }

    /// Set a callback that should be run when the position of the view changes
    pub fn add_move_listener(&self, action: Rc<dyn Fn(Point)>) {
        let state = self.state();
        state.borrow_mut().add_move_listener(action);
    }

    /// Set a callback that should be run when the view is removed from the view tree
    pub fn add_cleanup_listener(&self, action: Rc<dyn Fn()>) {
        let state = self.state();
        state.borrow_mut().add_cleanup_listener(action);
    }

    /// Get the combined style that is associated with this View.
    ///
    /// This will have all of the style properties set in it that are relevant to this view, including all properties from relevant classes.
    ///
    /// ## Warning
    /// The view styles do not store property transition states, only markers of which properties _should_ be transitioned over time on change.
    ///
    /// If you have a property that could be transitioned over time, make sure to use a [prop extractor](crate::prop_extractor) that is updated in a style method of the View to extract the property.
    pub fn get_combined_style(&self) -> Style {
        self.state().borrow().combined_style.clone()
    }

    /// Add a class to the list of style classes that are associated with this `ViewId`
    pub fn add_class(&self, class: StyleClassRef) {
        let state = self.state();
        state.borrow_mut().classes.push(class);
        self.request_style_recursive();
    }

    /// Remove a class from the list of style classes that are associated with this `ViewId`
    pub fn remove_class(&self, class: StyleClassRef) {
        let state = self.state();
        state.borrow_mut().classes.retain_mut(|c| *c != class);
        self.request_style_recursive();
    }

    pub(crate) fn update_style(&self, offset: StackOffset<Style>, style: Style) {
        let state = VIEW_STORAGE.with_borrow(|s| s.states.get(*self).cloned());
        if let Some(state) = state {
            let old_any_inherited = state.borrow().style().any_inherited();
            state.borrow_mut().style.set(offset, style);
            // Immediately set the STYLE flag so code can detect
            // that a style change is pending before process_update runs
            self.request_changes(ChangeFlags::STYLE);
            if state.borrow().style().any_inherited() || old_any_inherited {
                self.request_style_recursive();
            } else {
                self.request_style();
            }
        }
    }

    pub(crate) fn apply_event(
        &self,
        listener: &EventListener,
        event: &crate::event::Event,
    ) -> Option<EventPropagation> {
        let mut handled = false;
        let event_listeners = self.state().borrow().event_listeners.clone();
        if let Some(handlers) = event_listeners.get(listener) {
            for handler in handlers {
                handled |= (handler.borrow_mut())(event).is_processed();
            }
        } else {
            return None;
        }
        if handled {
            Some(EventPropagation::Stop)
        } else {
            Some(EventPropagation::Continue)
        }
    }

    /// Disables the default view behavior for the specified event.
    ///
    /// Children will still see the event, but the view event function will not be called nor the event listeners on the view
    pub fn disable_default_event(&self, event: EventListener) {
        self.state()
            .borrow_mut()
            .disable_default_events
            .insert(event);
    }

    /// Re-enables the default view behavior for a previously disabled event.
    pub fn remove_disable_default_event(&self, event: EventListener) {
        self.state()
            .borrow_mut()
            .disable_default_events
            .remove(&event);
    }

    /// Alter the visibility of the current window the view represented by this ID
    /// is in.
    pub fn window_visible(&self, visible: bool) {
        self.add_update_message(UpdateMessage::WindowVisible(visible));
    }

    /// Request removal of views during the update phase.
    ///
    /// This schedules the views to be removed with proper cleanup
    /// (cleanup listeners, taffy nodes, recursive children removal).
    /// Used by `keyed_children` for efficient keyed diffing.
    pub fn request_remove_views(&self, view_ids: Vec<ViewId>) {
        if !view_ids.is_empty() {
            self.add_update_message(UpdateMessage::RemoveViews(view_ids));
        }
    }

    /// Queue a child to be added during the next update cycle.
    ///
    /// The child will be constructed when the message is processed. The scope
    /// is resolved at build time by looking up the parent's context scope in
    /// the view hierarchy, enabling proper context propagation.
    pub fn add_child_deferred(&self, child_fn: impl FnOnce() -> AnyView + 'static) {
        self.add_update_message(UpdateMessage::AddChild {
            parent_id: *self,
            child: DeferredChild::new(child_fn),
        });
    }

    /// Queue multiple children to be added during the next update cycle.
    ///
    /// The children will be constructed when the message is processed. The scope
    /// is resolved at build time by looking up the parent's context scope in
    /// the view hierarchy, enabling proper context propagation.
    pub fn add_children_deferred(&self, children_fn: impl FnOnce() -> Vec<AnyView> + 'static) {
        self.add_update_message(UpdateMessage::AddChildren {
            parent_id: *self,
            children: DeferredChildren::new(children_fn),
        });
    }

    /// Queue a reactive children setup to run during the next update cycle.
    ///
    /// The setup function will be called inside the view's scope (resolved via `find_scope()`)
    /// when the message is processed. This enables lazy setup of reactive children
    /// (derived_children, derived_child, keyed_children) inside the correct scope for context access.
    pub fn setup_reactive_children_deferred(&self, setup: impl FnOnce() + 'static) {
        self.add_update_message(UpdateMessage::SetupReactiveChildren {
            setup: DeferredReactiveSetup::new(*self, setup),
        });
    }

    fn add_update_message(&self, msg: UpdateMessage) {
        let _ = CENTRAL_UPDATE_MESSAGES.try_with(|msgs| {
            let mut msgs = msgs.borrow_mut();
            msgs.push((*self, msg));
        });
    }

    /// Send a state update that will be placed in deferred messages
    // TODO: what is the difference?
    pub fn update_state_deferred(&self, state: impl Any) {
        CENTRAL_DEFERRED_UPDATE_MESSAGES.with_borrow_mut(|msgs| {
            msgs.push((*self, Box::new(state)));
        });
    }

    /// Get a layout in screen-coordinates for this view, if possible.
    pub fn screen_layout(&self) -> Option<ScreenLayout> {
        crate::layout::try_create_screen_layout(self)
    }

    /// Set the custom style parent to make it so that a view will pull it's style context from a different parent.
    /// This is useful for overlays that are children of the window root but should pull their style cx from the creating view
    pub fn set_style_parent(&self, parent_id: ViewId) {
        self.state().borrow_mut().style_cx_parent = Some(parent_id);
    }

    /// Clear the custom style parent
    pub fn clear_style_parent(&self) {
        self.state().borrow_mut().style_cx_parent = None;
    }

    /// Set the children scope for reactive children.
    ///
    /// This stores the scope used by `ParentView::derived_children` so that
    /// when children are updated, the old scope can be properly disposed.
    pub fn set_children_scope(&self, scope: Scope) {
        self.state().borrow_mut().children_scope = Some(scope);
    }

    /// Take and dispose the children scope, returning the old scope if it existed.
    ///
    /// This is called when reactive children are updated to clean up the old scope.
    pub fn take_children_scope(&self) -> Option<Scope> {
        self.state().borrow_mut().children_scope.take()
    }

    /// Set the keyed children state for reactive keyed children.
    ///
    /// This stores the children and their scopes used by `ParentView::keyed_children`.
    /// Each child has its own scope that gets disposed when the child is removed.
    pub fn set_keyed_children(&self, children: Vec<(ViewId, Scope)>) {
        self.state().borrow_mut().keyed_children = Some(children);
    }

    /// Take the keyed children state, returning it if it existed.
    ///
    /// This is called when keyed children are updated to apply diffs.
    pub fn take_keyed_children(&self) -> Option<Vec<(ViewId, Scope)>> {
        self.state().borrow_mut().keyed_children.take()
    }

    /// Set the scope for this view.
    ///
    /// Views that provide context to children (like Combobox, Dialog, etc.) should
    /// call this in their `into_view()` to store their scope. This scope is then
    /// used when processing deferred children so they have access to the context.
    ///
    /// The scope hierarchy is kept in sync with the view hierarchy, so when
    /// a parent scope is disposed, all child scopes are also disposed.
    pub fn set_scope(&self, scope: Scope) {
        self.state().borrow_mut().scope = Some(scope);
    }

    /// Get the scope for this view, if one was set.
    pub fn scope(&self) -> Option<Scope> {
        self.state().borrow().scope
    }

    /// Find the nearest ancestor (including self) that has a scope.
    ///
    /// This walks up the view tree to find the first view with a scope,
    /// which should be used when building deferred children.
    pub fn find_scope(&self) -> Option<Scope> {
        // Check self first
        if let Some(scope) = self.scope() {
            return Some(scope);
        }
        // Walk up ancestors
        let mut current = self.parent();
        while let Some(parent_id) = current {
            if let Some(scope) = parent_id.scope() {
                return Some(scope);
            }
            current = parent_id.parent();
        }
        None
    }
}

/// Re-parent a child view's scope under the nearest ancestor's scope.
///
/// This ensures that the Scope hierarchy matches the View hierarchy, which is
/// important for proper cleanup - when a parent scope is disposed, all child
/// scopes (and their signals/effects) are also disposed.
///
/// This handles the case where views are constructed eagerly (children created
/// before parents) - the scopes may have been created in the wrong order, so
/// we fix up the hierarchy when the view tree is assembled.
///
/// If the parent scope can't be found yet (because the view tree isn't fully
/// assembled), the child is added to a pending list and will be re-parented
/// later via `process_pending_scope_reparents`.
fn reparent_scope_if_needed(child_id: ViewId, parent_id: ViewId) {
    // Get child's scope (if it has one)
    let child_scope = child_id.scope();
    if let Some(child_scope) = child_scope {
        // Find the nearest ancestor with a scope
        if let Some(parent_scope) = parent_id.find_scope() {
            // Guard: Don't create a cycle if same scope is on both views
            if child_scope != parent_scope {
                // Re-parent child's scope under parent's scope
                child_scope.set_parent(parent_scope);
            }
        } else {
            // Parent scope not found yet - the view tree might not be fully assembled.
            // Add to pending list for later processing.
            PENDING_SCOPE_REPARENTS.with_borrow_mut(|pending| {
                pending.insert(child_id);
            });
        }
    }
}

/// Process any views that had scope re-parenting deferred.
///
/// This should be called after the view tree is fully assembled (e.g., after
/// processing all update messages). It attempts to re-parent scopes that
/// couldn't find a parent scope when they were first added.
pub fn process_pending_scope_reparents() {
    // Fast path: skip if nothing pending (common case)
    let has_pending = PENDING_SCOPE_REPARENTS.with_borrow(|pending| !pending.is_empty());
    if !has_pending {
        return;
    }

    PENDING_SCOPE_REPARENTS.with_borrow_mut(|pending| {
        pending.retain(|child_id| {
            // First check if this ViewId is still valid (not from a disposed view/window)
            // This is important for parallel test isolation
            if !child_id.is_valid() {
                return false; // Remove stale ViewId from pending
            }

            // Check if view still exists and has a scope
            let child_scope = child_id.scope();
            if let Some(child_scope) = child_scope {
                // Try to find a parent scope by walking up from the parent
                if let Some(parent_id) = child_id.parent()
                    && let Some(parent_scope) = parent_id.find_scope()
                {
                    // Guard: Don't create a cycle if same scope is on both views
                    if child_scope != parent_scope {
                        child_scope.set_parent(parent_scope);
                    }
                    return false; // Successfully handled, remove from pending
                }
                true // Still pending, keep in the set
            } else {
                false // No scope, remove from pending
            }
        });
    });
}

impl ViewId {
    /// Set the selected state for child views during styling.
    /// This should be used by parent views to propagate selected state to their children.
    /// Only requests a style update if the state actually changes.
    pub fn parent_set_selected(&self) {
        let changed = {
            let state = self.state();
            let mut state = state.borrow_mut();
            if !state.parent_set_style_interaction.selected {
                state.parent_set_style_interaction.selected = true;
                true
            } else {
                false
            }
        };
        if changed {
            self.request_style();
        }
    }

    /// Clear the selected state for child views during styling.
    /// This should be used by parent views to clear selected state propagation to their children.
    /// Only requests a style update if the state actually changes.
    pub fn parent_clear_selected(&self) {
        let changed = {
            let state = self.state();
            let mut state = state.borrow_mut();
            if state.parent_set_style_interaction.selected {
                state.parent_set_style_interaction.selected = false;
                true
            } else {
                false
            }
        };
        if changed {
            self.request_style();
        }
    }

    /// Set the disabled state for child views during styling.
    /// This should be used by parent views to propagate disabled state to their children.
    /// Only requests a style update if the state actually changes.
    pub fn parent_set_disabled(&self) {
        let changed = {
            let state = self.state();
            let mut state = state.borrow_mut();
            if !state.parent_set_style_interaction.disabled {
                state.parent_set_style_interaction.disabled = true;
                true
            } else {
                false
            }
        };
        if changed {
            self.request_style();
        }
    }

    /// Clear the disabled state for child views during styling.
    /// This should be used by parent views to clear disabled state propagation to their children.
    /// Only requests a style update if the state actually changes.
    pub fn parent_clear_disabled(&self) {
        let changed = {
            let state = self.state();
            let mut state = state.borrow_mut();
            if state.parent_set_style_interaction.disabled {
                state.parent_set_style_interaction.disabled = false;
                true
            } else {
                false
            }
        };
        if changed {
            self.request_style();
        }
    }

    /// Hide this view from layout. Sets the visibility state directly.
    /// Skips the normal transition animation logic.
    pub fn set_hidden(&self) {
        use crate::view::state::VisibilityPhase;
        let changed = {
            let state = self.state();
            let mut state = state.borrow_mut();
            if !state.visibility.force_hidden {
                state.visibility.force_hidden = true;
                state.visibility.phase = VisibilityPhase::Hidden;
                true
            } else {
                false
            }
        };
        if changed {
            self.request_layout();
        }
    }

    /// Show this view in layout. Clears the force-hidden state.
    pub fn set_visible(&self) {
        use crate::view::state::VisibilityPhase;
        let changed = {
            let state = self.state();
            let mut state = state.borrow_mut();
            if state.visibility.force_hidden {
                state.visibility.force_hidden = false;
                // Reset phase to Initial so the normal transition logic can run
                state.visibility.phase = VisibilityPhase::Initial;
                true
            } else {
                false
            }
        };
        if changed {
            // Request both style (for transition) and layout (for size recalc)
            self.request_style();
            self.request_layout();
        }
    }
}
