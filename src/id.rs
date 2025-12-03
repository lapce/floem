#![deny(missing_docs)]
//! # `ViewId`s
//!
//! [`ViewId`]s are unique identifiers for views.
//! They're used to identify views in the view tree.

use std::{any::Any, cell::RefCell, marker::PhantomData, rc::Rc};

use peniko::kurbo::{Affine, Point, Rect, RoundedRect, Vec2};
use smallvec::SmallVec;
use taffy::{Display, Layout, NodeId, TaffyResult, TaffyTree};
use understory_responder::types::Outcome;
use winit::window::WindowId;

use crate::{
    ScreenLayout,
    animate::{AnimStateCommand, Animation},
    context::{EventCallback, ResizeCallback},
    event::EventListener,
    menu::Menu,
    style::{
        Disabled, DisplayProp, Draggable, Focusable, Hidden, PointerEvents, PointerEventsProp,
        Style, StyleClassRef,
    },
    update::{CENTRAL_DEFERRED_UPDATE_MESSAGES, CENTRAL_UPDATE_MESSAGES, UpdateMessage},
    view::{IntoView, View},
    view_state::{ChangeFlags, StackOffset, ViewState},
    view_storage::{NodeContext, VIEW_STORAGE},
    window_tracking::{is_known_root, window_id_for_root},
};
pub struct NotThreadSafe(*const ());

/// A small unique identifier, and handle, for an instance of a [View](crate::View).
///
/// Through this handle, you can access the associated view [ViewState](crate::view_state::ViewState).
/// You can also use this handle to access the children ViewId's which allows you access to their states.
///
/// This type is not thread safe and can only be used from the main thread.
#[derive(Copy, Clone, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct ViewId(slotmap::KeyData, PhantomData<NotThreadSafe>);
impl std::fmt::Debug for ViewId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let debug_name = self.debug_name();
        let mut start = f.debug_struct("ViewId");

        if !debug_name.is_empty() {
            start.field("id", &self.0).field("debug_name", &debug_name)
        } else {
            start.field("id", &self.0)
        }
        .finish()
    }
}

impl slotmap::__impl::From<slotmap::KeyData> for ViewId {
    fn from(k: slotmap::KeyData) -> Self {
        ViewId(k, PhantomData)
    }
}
unsafe impl slotmap::Key for ViewId {
    fn data(&self) -> slotmap::KeyData {
        self.0
    }
}

impl ViewId {
    /// Create a new unique `Viewid`.
    pub fn new() -> ViewId {
        VIEW_STORAGE.with_borrow_mut(|s| s.view_ids.insert(()))
    }

    /// Remove this view id and all of it's children from the `VIEW_STORAGE`
    pub fn remove(&self) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            // Remove the cached root, in the (unlikely) case that this view is
            // re-added to a different window
            s.root.remove(*self);
            if let Some(Some(parent)) = s.parent.get(*self) {
                if let Some(children) = s.children.get_mut(*parent) {
                    children.retain(|c| c != self);
                }
            }
            s.view_ids.remove(*self);
        });
    }

    /// Get access to the taffy tree
    pub fn taffy(&self) -> Rc<RefCell<TaffyTree<NodeContext>>> {
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

    /// Get the layout for a taffy node
    pub fn taffy_layout(&self, node: NodeId) -> Option<taffy::Layout> {
        self.taffy().borrow().layout(node).cloned().ok()
    }

    /// Mark the taffy node associated with this view as dirty.
    pub fn mark_view_layout_dirty(&self) -> TaffyResult<()> {
        let node = self.taffy_node();
        self.taffy().borrow_mut().mark_dirty(node)
    }

    /// Get the taffy node associated with this Id
    pub fn taffy_node(&self) -> NodeId {
        self.state().borrow().node
    }

    /// set the clip rectange in local coordinates in the box tree
    pub fn set_box_tree_clip(&self, clip: Option<RoundedRect>) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            let node_id = s.state(*self).borrow().box_node;
            s.box_tree.borrow_mut().set_local_clip(node_id, clip)
        })
    }

    /// set the clip rectange in local coordinates in the box tree
    pub fn set_box_tree_clip_behavior(&self, behavior: understory_box_tree::ClipBehavior) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            let node_id = s.state(*self).borrow().box_node;
            s.box_tree.borrow_mut().set_clip_behavior(node_id, behavior)
        })
    }

    /// set the transform on a view that is applied after style transforms
    pub fn set_transform(&self, transform: Affine) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            s.state(*self).borrow_mut().transform = transform;
        });
        self.request_layout();
    }

    pub(crate) fn state(&self) -> Rc<RefCell<ViewState>> {
        VIEW_STORAGE.with_borrow_mut(|s| s.state(*self))
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
        VIEW_STORAGE.with_borrow_mut(|s| {
            let child_id = child.id();
            s.children.entry(*self).unwrap().or_default().push(child_id);
            s.parent.insert(child_id, Some(*self));
            s.views.insert(child_id, Rc::new(RefCell::new(child)));
            let child_taffy_node = s.state(child_id).borrow().node;
            let this_taffy_node = s.state(*self).borrow().node;
            let _ = s
                .taffy
                .borrow_mut()
                .set_children(this_taffy_node, &[child_taffy_node]);
            let child_box_node = s.state(child_id).borrow().box_node;
            let this_box_node = s.state(*self).borrow().box_node;
            s.box_tree
                .borrow_mut()
                .reparent(child_box_node, Some(this_box_node));
        });
    }

    /// Set the children views of this Id using a Vector
    pub fn set_children<V: IntoView>(&self, children: impl Into<Vec<V>>) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            let children = children.into();

            let this_box_node = s.state(*self).borrow().box_node;
            let mut children_ids = Vec::with_capacity(children.len());
            let mut children_nodes = Vec::with_capacity(children.len());
            for child in children {
                let child_view = child.into_view();
                let child_view_id = child_view.id();
                let child_taffy_node = s.state(child_view_id).borrow().node;
                children_nodes.push(child_taffy_node);
                children_ids.push(child_view_id);
                s.parent.insert(child_view_id, Some(*self));
                let child_box_node = s.state(child_view_id).borrow().box_node;
                s.box_tree
                    .borrow_mut()
                    .reparent(child_box_node, Some(this_box_node));
                s.views
                    .insert(child_view_id, Rc::new(RefCell::new(child_view.into_any())));
            }
            s.children.insert(*self, children_ids);
            let this_taffy_node = s.state(*self).borrow().node;
            let _ = s
                .taffy
                .borrow_mut()
                .set_children(this_taffy_node, &children_nodes);
        });
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
                let parent_box_node = s.state(parent).borrow().box_node;
                let this_box_node = s.state(*self).borrow().box_node;
                s.box_tree
                    .borrow_mut()
                    .reparent(this_box_node, Some(parent_box_node));
            }
        });
    }

    /// Set the Ids that should be used as the children of this Id
    pub fn set_children_ids(&self, children: Vec<ViewId>) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            if !s.view_ids.contains_key(*self) {
                return;
            }

            let this_taffy_node = s.state(*self).borrow().node;
            let this_box_node = s.state(*self).borrow().box_node;

            let taffy_children: Vec<_> = children
                .iter()
                .map(|child| s.state(*child).borrow().node)
                .collect();

            for child in &children {
                s.parent.insert(*child, Some(*self));
                let child_box_node = s.state(*child).borrow().box_node;
                s.box_tree
                    .borrow_mut()
                    .reparent(child_box_node, Some(this_box_node));
            }

            let _ = s
                .taffy
                .borrow_mut()
                .set_children(this_taffy_node, &taffy_children);
            s.children.insert(*self, children);
        });
    }

    /// Get the list of `ViewId`s that are associated with the children views of this `ViewId`
    pub fn children(&self) -> Vec<ViewId> {
        VIEW_STORAGE.with_borrow(|s| s.children.get(*self).cloned().unwrap_or_default())
    }

    /// Get access to the list of `ViewId`s that are associated with the children views of this `ViewId`
    pub fn with_children<R>(&self, children: impl Fn(&[ViewId]) -> R) -> R {
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
            if let Some(root) = root_view_id {
                if is_known_root(&root) {
                    s.root.insert(*self, root_view_id);
                    return Some(root);
                }
            }
            None
        })
    }

    /// Get the chain of debug names that have been applied to this view.
    pub fn debug_name(&self) -> String {
        let state_names = self
            .state()
            .try_borrow()
            .ok()
            .map(|state| state.debug_name.iter().rev().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let view_name = self
            .view()
            .try_borrow()
            .ok()
            .map(|view| View::debug_name(view.as_ref()).to_string())
            .unwrap_or_else(|| "<borrow failed>".to_string());

        state_names
            .into_iter()
            .chain(std::iter::once(view_name))
            .collect::<Vec<_>>()
            .join(" - ")
    }

    /// Returns the layout rect relative to the parent view.
    ///
    /// The position is relative to the parent view's origin. This is the raw layout
    /// as computed by Taffy, useful for measuring and positioning views within their
    /// parent's coordinate space.
    pub fn layout_rect(&self) -> Rect {
        self.layout()
            .map(|l| Rect {
                x0: f64::from(l.location.x),
                y0: f64::from(l.location.y),
                x1: f64::from(l.location.x + l.size.width),
                y1: f64::from(l.location.y + l.size.height),
            })
            .unwrap_or_default()
    }

    /// Returns the content rect relative to the parent view.
    ///
    /// The content rect excludes borders and padding, representing the area where
    /// child content is positioned. The position is relative to the parent view's
    /// origin, useful for parent-driven layout calculations.
    pub fn content_rect(&self) -> Rect {
        self.layout()
            .map(|r| Rect {
                x0: f64::from(r.content_box_x()),
                y0: f64::from(r.content_box_y()),
                x1: f64::from(r.content_box_x() + r.content_box_width()),
                y1: f64::from(r.content_box_y() + r.content_box_height()),
            })
            .unwrap_or_default()
    }

    /// Returns the view rect relative to the parent view.
    ///
    /// This provides the full visual bounds of the view. The position is relative
    /// to the parent view's origin.
    pub fn view_rect(&self) -> Rect {
        self.layout()
            .map(|r| Rect {
                x0: f64::from(r.location.x),
                y0: f64::from(r.location.y),
                x1: f64::from(r.location.x + r.size.width),
                y1: f64::from(r.location.y + r.size.height),
            })
            .unwrap_or_default()
    }

    /// Returns the layout rect in the view's local coordinate space.
    ///
    /// This is the correct rect to use for hit testing against events that have been
    /// transformed through `window_event_to_view()`. Here's why:
    ///
    /// When an event is transformed from window space to view-local space via
    /// `window_event_to_view()`, it applies the inverse of the view's accumulated transform.
    /// This accumulated transform includes:
    /// 1. All ancestor translations (from layout positions)
    /// 2. All ancestor transforms (affine transformations)
    /// 3. All ancestor scroll offsets
    /// 4. This view's own layout position
    /// 5. This view's own transform
    /// 6. This view's own scroll offset (if it's a scroll view)
    ///
    /// After applying the inverse, the event point is in this view's "natural" coordinate
    /// space - the space where (0, 0) is at the view's top-left corner and the view extends
    /// to (width, height) at its bottom-right corner. This is exactly the coordinate space
    /// that `layout_rect_local()` describes.
    ///
    /// In summary: transformed events → use layout_rect_local() for hit testing.
    pub fn layout_rect_local(&self) -> Rect {
        self.layout()
            .map(|l| Rect {
                x0: 0.0,
                y0: 0.0,
                x1: f64::from(l.size.width),
                y1: f64::from(l.size.height),
            })
            .unwrap_or_default()
    }

    /// Returns the content rect in the view's local coordinate space.
    ///
    /// The content rect excludes borders and padding, representing the area where
    /// child content should be positioned. This is in the view's local coordinate
    /// space, with an offset that accounts for borders and padding.
    ///
    /// Like `layout_rect_local()`, this is in the same coordinate space as events
    /// transformed via `window_event_to_view()`.
    pub fn content_rect_local(&self) -> Rect {
        self.layout()
            .map(|r| {
                let x0 = f64::from(r.border.left + r.padding.left);
                let y0 = f64::from(r.border.top + r.padding.top);
                let x1 = x0 + f64::from(r.content_box_width());
                let y1 = y0 + f64::from(r.content_box_height());
                Rect { x0, y0, x1, y1 }
            })
            .unwrap_or_default()
    }

    /// Returns the Taffy layout for this view.
    ///
    /// The layout includes the view's size and position relative to its parent view.
    /// This is the layout information from Taffy without any adjustments for
    /// borders, padding, or other styling properties.
    ///
    /// # Returns
    /// - `Some(Layout)` containing size and position information
    /// - `None` if layout information is unavailable
    pub fn layout(&self) -> Option<Layout> {
        let taffy = self.taffy();
        let node = self.state().borrow().node;
        taffy.borrow().layout(node).ok().copied()
    }

    /// Returns true if the computed style for this view is marked as hidden by setting in this view, or any parent, `Hidden` to true. For hiding views, you should prefer to set `Hidden` to true rather than using `Display::None` as checking for `Hidden` is cheaper, more correct, and used for optimizations in Floem
    pub fn is_hidden(&self) -> bool {
        let state = self.state();
        let state = state.borrow();
        state.computed_style.get(Hidden) || state.computed_style.get(DisplayProp) == Display::None
    }

    /// if the view has pointer events none
    pub fn pointer_events_none(&self) -> bool {
        let state = self.state();
        let state = state.borrow();
        state
            .computed_style
            .get(PointerEventsProp)
            .map(|p| p == PointerEvents::None)
            .unwrap_or(false)
    }

    /// Returns true if the view is disabled
    ///
    /// This is done by checking if the style for this view has `Disabled` set to true.
    pub fn is_disabled(&self) -> bool {
        let state = self.state();
        let state = state.borrow();
        state.computed_style.get(Disabled)
    }

    /// Returns true if the view is selected
    ///
    /// This is done by checking if the style for this view has `Selected` set to true.
    pub fn is_selected(&self) -> bool {
        let state = self.state();
        let state = state.borrow();
        state.computed_style.get(Disabled)
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
        self.request_changes(ChangeFlags::all());
    }

    /// Request that this view have it's layout pass run
    pub fn request_layout(&self) {
        self.add_update_message(UpdateMessage::RequestLayout)
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
        self.request_changes(ChangeFlags::STYLE)
    }

    /// Use this when you want the `view_style` method from the `View` trait to be rerun.
    pub fn request_view_style(&self) {
        self.request_changes(ChangeFlags::VIEW_STYLE)
    }

    /// use this if your view wants to run the layout function after any window layout change
    pub fn needs_post_layout(&self) {
        self.add_update_message(UpdateMessage::NeedsPostLayout(*self));
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

    /// Set a scroll offset that will affect children.
    /// If you have a view that visually affects how far children should scroll, set it here.
    pub fn set_scroll_offset(&self, scroll_offset: Vec2) {
        let state = self.state();
        let mut state = state.borrow_mut();
        if state.scroll_offset != scroll_offset {
            state.scroll_offset = scroll_offset;
            self.request_layout();
        }
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
        let state = self.state();
        let old_any_inherited = state.borrow().style().any_inherited();
        state.borrow_mut().style.set(offset, style);
        if state.borrow().style().any_inherited() || old_any_inherited {
            self.request_style_recursive();
        } else {
            self.request_style();
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
        crate::screen_layout::try_create_screen_layout(self)
    }

    /// get the understory box node associated with this View
    pub fn box_node(&self) -> understory_box_tree::NodeId {
        VIEW_STORAGE.with_borrow_mut(|s| s.state(*self).borrow().box_node)
    }

    /// get the world transform from the box tree for this view.
    /// Returns none if the node is dirty
    pub fn world_transform(&self) -> Option<Affine> {
        let node_id = self.box_node();
        VIEW_STORAGE.with_borrow(|s| {
            let box_tree = s.box_tree.borrow();
            box_tree.world_transform(node_id)
        })
    }

    /// gets the world bounds, including clips for this view
    pub fn world_bounds(&self) -> Option<Rect> {
        let node_id = self.box_node();
        VIEW_STORAGE.with_borrow(|s| {
            let box_tree = s.box_tree.borrow();
            box_tree.world_bounds(node_id)
        })
    }
}
