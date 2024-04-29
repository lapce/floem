use std::{any::Any, cell::RefCell, rc::Rc};

use kurbo::{Insets, Point, Rect, Size};
use slotmap::{new_key_type, SecondaryMap, SlotMap};
use taffy::{Display, Layout};

use crate::{
    animate::Animation,
    context::{EventCallback, MenuCallback, ResizeCallback, ViewState},
    event::EventListener,
    style::{DisplayProp, Style, StyleClassRef, StyleSelector},
    unit::PxPct,
    update::{UpdateMessage, CENTRAL_DEFERRED_UPDATE_MESSAGES, CENTRAL_UPDATE_MESSAGES},
    view::View,
    view_data::{ChangeFlags, StackOffset},
};

thread_local! {
    pub(crate) static VIEW_STORAGE: RefCell<ViewStorage> = Default::default();
}

new_key_type! {
   pub struct ViewId;
}

pub struct ViewStorage {
    pub(crate) taffy: taffy::TaffyTree,
    view_ids: SlotMap<ViewId, ()>,
    views: SecondaryMap<ViewId, Rc<RefCell<Box<dyn View>>>>,
    children: SecondaryMap<ViewId, Vec<ViewId>>,
    // the parent of a View
    parent: SecondaryMap<ViewId, Option<ViewId>>,
    pub(crate) states: SecondaryMap<ViewId, Rc<RefCell<ViewState>>>,
    stale_view_state: Rc<RefCell<ViewState>>,
}

impl Default for ViewStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewStorage {
    pub fn new() -> Self {
        let mut taffy = taffy::TaffyTree::default();
        let state_view_state = ViewState::new(&mut taffy);

        Self {
            taffy,
            view_ids: Default::default(),
            views: Default::default(),
            children: Default::default(),
            parent: Default::default(),
            states: Default::default(),
            stale_view_state: Rc::new(RefCell::new(state_view_state)),
        }
    }
}

impl ViewId {
    pub fn new() -> ViewId {
        VIEW_STORAGE.with_borrow_mut(|s| s.view_ids.insert(()))
    }

    pub fn state(&self) -> Rc<RefCell<ViewState>> {
        VIEW_STORAGE.with_borrow_mut(|s| {
            if !s.view_ids.contains_key(*self) {
                // if view_ids doens't have this view id, that means it's been cleaned up,
                // so we shouldn't create a new ViewState for this Id.
                s.stale_view_state.clone()
            } else {
                s.states
                    .entry(*self)
                    .unwrap()
                    .or_insert_with(|| Rc::new(RefCell::new(ViewState::new(&mut s.taffy))))
                    .clone()
            }
        })
    }

    pub fn view(&self) -> Rc<RefCell<Box<dyn View>>> {
        VIEW_STORAGE.with_borrow_mut(|s| s.views.get(*self).unwrap().clone())
    }

    pub fn set_children(&self, children: Vec<Box<dyn View>>) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            let mut children_ids = Vec::new();
            for child in children {
                let child_view_id = child.id();
                children_ids.push(child_view_id);
                s.parent.insert(child_view_id, Some(*self));
                s.views.insert(child_view_id, Rc::new(RefCell::new(child)));
            }
            s.children.insert(*self, children_ids);
        });
    }

    pub fn set_view(&self, view: Box<dyn View>) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            if s.view_ids.contains_key(*self) {
                s.views.insert(*self, Rc::new(RefCell::new(view)));
            }
        });
    }

    pub fn set_parent(&self, parent: ViewId) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            if s.view_ids.contains_key(*self) {
                s.parent.insert(*self, Some(parent));
            }
        });
    }

    pub fn set_children_ids(&self, children: Vec<ViewId>) {
        VIEW_STORAGE.with_borrow_mut(|s| {
            if s.view_ids.contains_key(*self) {
                s.children.insert(*self, children);
            }
        });
    }

    pub fn children(&self) -> Vec<ViewId> {
        VIEW_STORAGE.with_borrow(|s| s.children.get(*self).cloned().unwrap_or_default())
    }

    pub fn parent(&self) -> Option<ViewId> {
        VIEW_STORAGE.with_borrow(|s| s.parent.get(*self).cloned().flatten())
    }

    pub fn root(&self) -> Option<ViewId> {
        VIEW_STORAGE.with_borrow(|s| {
            let mut parent = s.parent.get(*self).cloned().flatten();
            while let Some(id) = parent {
                if let Some(id) = s.parent.get(id).cloned().flatten() {
                    parent = Some(id)
                } else {
                    return parent;
                }
            }
            parent
        })
    }

    pub fn layout_rect(&self) -> Rect {
        self.state().borrow().layout_rect
    }

    pub(crate) fn get_size(&self) -> Option<Size> {
        self.get_layout()
            .map(|l| Size::new(l.size.width as f64, l.size.height as f64))
    }

    pub fn parent_size(&self) -> Option<Size> {
        let parent_id = self.parent()?;
        parent_id.get_size()
    }

    /// Returns the layout rect excluding borders, padding and position.
    /// This is relative to the view.
    pub fn get_content_rect(&mut self) -> Rect {
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
        rect.inset(-Insets {
            x0: props.border_left().0 + pixels(props.padding_left(), rect.width()),
            x1: props.border_right().0 + pixels(props.padding_right(), rect.width()),
            y0: props.border_top().0 + pixels(props.padding_top(), rect.height()),
            y1: props.border_bottom().0 + pixels(props.padding_bottom(), rect.height()),
        })
    }

    /// This gets the Taffy Layout and adjusts it to be relative to the parent `View`.
    pub(crate) fn get_layout(&self) -> Option<Layout> {
        let widget_parent = self.parent().map(|id| id.state().borrow().node);

        let mut node = self.state().borrow().node;
        VIEW_STORAGE.with_borrow_mut(|s| {
            let mut layout = *s.taffy.layout(node).ok()?;

            loop {
                let parent = s.taffy.parent(node);

                if parent == widget_parent {
                    break;
                }

                node = parent?;

                layout.location = layout.location + s.taffy.layout(node).ok()?.location;
            }

            Some(layout)
        })
    }

    pub fn is_hidden(&self) -> bool {
        let state = self.state();
        state.borrow().combined_style.get(DisplayProp) == Display::None
    }

    /// Is this view, or any parent view, marked as hidden
    pub fn is_hidden_recursive(&self) -> bool {
        if self.is_hidden() {
            return true;
        }

        let mut parent = self.parent();
        while let Some(id) = parent {
            if id.is_hidden() {
                return true;
            }
            parent = id.parent();
        }

        false
    }

    /// Request that this the `id` view be styled, laid out and painted again.
    /// This will recursively request this for all parents.
    pub fn request_all(&self) {
        self.request_changes(ChangeFlags::all());
    }

    pub fn request_layout(&self) {
        self.request_changes(ChangeFlags::LAYOUT)
    }

    pub fn request_paint(&self) {
        self.add_update_message(UpdateMessage::RequestPaint);
    }

    /// request that this node be styled again
    /// This will recursively request style for all parents.
    pub(crate) fn request_style(&self) {
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
    pub(crate) fn request_style_recursive(&self) {
        let state = self.state();
        state.borrow_mut().request_style_recursive = true;
        self.request_style();
    }

    pub fn request_focus(&self) {
        self.add_update_message(UpdateMessage::Focus(*self));
    }

    pub fn clear_focus(&self) {
        self.add_update_message(UpdateMessage::ClearFocus(*self));
    }

    pub fn update_context_menu(&self, menu: Box<MenuCallback>) {
        self.state().borrow_mut().context_menu = Some(menu);
    }

    pub fn update_popout_menu(&self, menu: Box<MenuCallback>) {
        self.state().borrow_mut().popout_menu = Some(menu);
    }

    pub fn request_active(&self) {
        self.add_update_message(UpdateMessage::Active(*self));
    }

    pub fn inspect(&self) {
        self.add_update_message(UpdateMessage::Inspect);
    }

    pub fn scroll_to(&self, rect: Option<Rect>) {
        self.add_update_message(UpdateMessage::ScrollTo { id: *self, rect });
    }

    pub fn update_animation(&self, animation: Animation) {
        self.add_update_message(UpdateMessage::Animation {
            id: *self,
            animation,
        });
    }

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

    pub fn add_event_listener(&self, listener: EventListener, action: Box<EventCallback>) {
        let state = self.state();
        state.borrow_mut().add_event_listener(listener, action);
    }

    pub fn update_resize_listener(&self, action: Box<ResizeCallback>) {
        let state = self.state();
        state.borrow_mut().update_resize_listener(action);
    }

    pub fn update_move_listener(&self, action: Box<dyn Fn(Point)>) {
        let state = self.state();
        state.borrow_mut().update_move_listener(action);
    }

    pub fn update_cleanup_listener(&self, action: Box<dyn Fn()>) {
        let state = self.state();
        state.borrow_mut().update_cleanup_listener(action);
    }

    pub(crate) fn add_class(&self, class: StyleClassRef) {
        let state = self.state();
        state.borrow_mut().classes.push(class);
        self.request_style_recursive();
    }

    pub(crate) fn update_style_selector(&self, selector: StyleSelector, style: Style) {
        if let StyleSelector::Dragging = selector {
            let state = self.state();
            state.borrow_mut().dragging_style = Some(style);
        }
        self.request_style();
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

    pub fn update_disabled(&self, is_disabled: bool) {
        self.add_update_message(UpdateMessage::Disabled {
            id: *self,
            is_disabled,
        });
    }

    pub fn keyboard_navigatable(&self) {
        self.add_update_message(UpdateMessage::KeyboardNavigable { id: *self });
    }

    pub fn draggable(&self) {
        self.add_update_message(UpdateMessage::Draggable { id: *self });
    }

    fn add_update_message(&self, msg: UpdateMessage) {
        CENTRAL_UPDATE_MESSAGES.with(|msgs| {
            msgs.borrow_mut().push((*self, msg));
        });
    }

    pub fn update_state_deferred(&self, state: impl Any) {
        CENTRAL_DEFERRED_UPDATE_MESSAGES.with(|msgs| {
            msgs.borrow_mut().push((*self, Box::new(state)));
        });
    }
}
