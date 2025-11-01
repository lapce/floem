#![deny(missing_docs)]
//! # `ViewId`s
//!
//! [`ViewId`]s are unique identifiers for views.
//! They're used to identify views in the view tree.

use std::{any::Any, cell::RefCell, rc::Rc};

use peniko::kurbo::{Insets, Point, Rect, Size};
use slotmap::new_key_type;
use taffy::{Display, Layout, NodeId, TaffyTree};
use winit::window::WindowId;

use crate::{
    ScreenLayout,
    animate::{AnimStateCommand, Animation},
    context::{EventCallback, ResizeCallback},
    event::{EventListener, EventPropagation},
    menu::Menu,
    style::{Disabled, DisplayProp, Draggable, Focusable, Hidden, Style, StyleClassRef},
    unit::PxPct,
    update::{CENTRAL_DEFERRED_UPDATE_MESSAGES, CENTRAL_UPDATE_MESSAGES, UpdateMessage},
    view::{IntoView, View},
    view_state::{ChangeFlags, StackOffset, ViewState},
    view_storage::VIEW_STORAGE,
    window_tracking::{is_known_root, window_id_for_root},
};

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

    /// Get the taffy node associated with this Id
    pub fn taffy_node(&self) -> NodeId {
        self.state().borrow().node
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
                        Rc::new(RefCell::new(ViewState::new(&mut s.taffy.borrow_mut())))
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
        VIEW_STORAGE.with_borrow_mut(|s| {
            let child_id = child.id();
            s.children.entry(*self).unwrap().or_default().push(child_id);
            s.parent.insert(child_id, Some(*self));
            s.views.insert(child_id, Rc::new(RefCell::new(child)));
        });
    }

    /// Set the children views of this Id
    /// See also [`Self::set_children_vec`]
    pub fn set_children<const N: usize, V: IntoView>(&self, children: [V; N]) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            let mut children_ids = Vec::new();
            for child in children {
                let child_view = child.into_view();
                let child_view_id = child_view.id();
                children_ids.push(child_view_id);
                s.parent.insert(child_view_id, Some(*self));
                s.views
                    .insert(child_view_id, Rc::new(RefCell::new(child_view.into_any())));
            }
            s.children.insert(*self, children_ids);
        });
    }

    /// Set the children views of this Id using a Vector
    /// See also [`Self::set_children`]
    pub fn set_children_vec(&self, children: Vec<impl IntoView>) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            let mut children_ids = Vec::new();
            for child in children {
                let child_view = child.into_view();
                let child_view_id = child_view.id();
                children_ids.push(child_view_id);
                s.parent.insert(child_view_id, Some(*self));
                s.views
                    .insert(child_view_id, Rc::new(RefCell::new(child_view.into_any())));
            }
            s.children.insert(*self, children_ids);
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

    /// Returns true if the computed style for this view is marked as hidden by setting in this view, or any parent, `Hidden` to true. For hiding views, you should prefer to set `Hidden` to true rather than using `Display::None` as checking for `Hidden` is cheaper, more correct, and used for optimizations in Floem
    pub fn is_hidden(&self) -> bool {
        let state = self.state();
        let state = state.borrow();
        state.computed_style.get(Hidden) || state.computed_style.get(DisplayProp) == Display::None
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
        self.request_changes(ChangeFlags::STYLE)
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
        let state = self.state();
        let old_any_inherited = state.borrow().style().any_inherited();
        state.borrow_mut().style.set(offset, style);
        if state.borrow().style().any_inherited() || old_any_inherited {
            self.request_style_recursive();
        } else {
            self.request_style();
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
}
