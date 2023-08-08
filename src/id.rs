//! # Ids
//!
//! [Id](id::Id)s are unique identifiers for views.
//! They're used to identify views in the view tree.
//!
//! ## Ids and Id paths
//!
//! These ids are assigned via the [ViewContext](context::ViewContext) and are unique across the entire application.
//!

use std::{any::Any, cell::RefCell, collections::HashMap, num::NonZeroU64, time::Duration};

use glazier::{
    kurbo::{Point, Vec2},
    FileDialogOptions, FileInfo,
};

use crate::{
    animate::Animation,
    app_handle::{StyleSelector, UpdateMessage, DEFERRED_UPDATE_MESSAGES, UPDATE_MESSAGES},
    context::{EventCallback, MenuCallback, ResizeCallback},
    event::EventListener,
    menu::Menu,
    responsive::ScreenSize,
    style::Style,
};

thread_local! {
    pub(crate) static ID_PATHS: RefCell<HashMap<Id,IdPath>> = Default::default();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash)]
/// A stable identifier for an element.
pub struct Id(NonZeroU64);

#[derive(Clone, Default)]
pub struct IdPath(pub(crate) Vec<Id>);

impl Id {
    /// Allocate a new, unique `Id`.
    pub fn next() -> Id {
        use glazier::Counter;
        static WIDGET_ID_COUNTER: Counter = Counter::new();
        Id(WIDGET_ID_COUNTER.next_nonzero())
    }

    #[allow(unused)]
    pub fn to_raw(self) -> u64 {
        self.0.into()
    }

    pub fn to_nonzero_raw(self) -> NonZeroU64 {
        self.0
    }

    pub fn new(&self) -> Id {
        let mut id_path =
            ID_PATHS.with(|id_paths| id_paths.borrow().get(self).cloned().unwrap_or_default());
        let new_id = Self::next();
        id_path.0.push(new_id);
        ID_PATHS.with(|id_paths| {
            id_paths.borrow_mut().insert(new_id, id_path);
        });
        new_id
    }

    pub fn parent(&self) -> Option<Id> {
        ID_PATHS.with(|id_paths| {
            id_paths.borrow().get(self).and_then(|id_path| {
                let id_path = &id_path.0;
                let len = id_path.len();
                if len >= 2 {
                    Some(id_path[len - 2])
                } else {
                    None
                }
            })
        })
    }

    pub fn id_path(&self) -> Option<IdPath> {
        ID_PATHS.with(|id_paths| id_paths.borrow().get(self).cloned())
    }

    pub fn has_id_path(&self) -> bool {
        ID_PATHS.with(|id_paths| id_paths.borrow().contains_key(self))
    }

    pub fn remove_id_path(&self) {
        ID_PATHS.with(|id_paths| id_paths.borrow_mut().remove(self));
    }

    pub fn root_id(&self) -> Option<Id> {
        ID_PATHS.with(|id_paths| {
            id_paths
                .borrow()
                .get(self)
                .and_then(|path| path.0.first().copied())
        })
    }

    pub fn request_focus(&self) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::Focus(*self));
            });
        }
    }

    pub fn request_active(&self) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::Active(*self));
            });
        }
    }

    pub fn update_disabled(&self, is_disabled: bool) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::Disabled {
                    id: *self,
                    is_disabled,
                })
            });
        }
    }

    pub fn update_window_scale(&self, window_scale: f64) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::WindowScale(window_scale))
            });
        }
    }

    pub fn request_paint(&self) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::RequestPaint);
            });
        }
    }

    pub fn request_layout(&self) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::RequestLayout { id: *self });
            });
        }
    }

    pub fn update_state(&self, state: impl Any, deferred: bool) {
        if let Some(root) = self.root_id() {
            if !deferred {
                UPDATE_MESSAGES.with(|msgs| {
                    let mut msgs = msgs.borrow_mut();
                    let msgs = msgs.entry(root).or_default();
                    msgs.push(UpdateMessage::State {
                        id: *self,
                        state: Box::new(state),
                    })
                });
            } else {
                DEFERRED_UPDATE_MESSAGES.with(|msgs| {
                    let mut msgs = msgs.borrow_mut();
                    let msgs = msgs.entry(root).or_default();
                    msgs.push((*self, Box::new(state)));
                });
            }
        }
    }

    pub fn update_base_style(&self, style: Style) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::BaseStyle { id: *self, style });
            });
        }
    }

    pub fn update_style(&self, style: Style) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::Style { id: *self, style });
            });
        }
    }

    pub fn update_style_selector(&self, style: Style, selector: StyleSelector) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::StyleSelector {
                    id: *self,
                    style,
                    selector,
                })
            });
        }
    }

    pub fn keyboard_navigatable(&self) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::KeyboardNavigable { id: *self })
            })
        }
    }

    pub fn draggable(&self) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::Draggable { id: *self })
            })
        }
    }

    pub fn update_responsive_style(&self, style: Style, size: ScreenSize) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::ResponsiveStyle {
                    id: *self,
                    style,
                    size,
                })
            });
        }
    }

    pub fn set_handle_titlebar(&self, val: bool) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::HandleTitleBar(val))
            });
        }
    }

    pub fn set_window_delta(&self, delta: Vec2) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::SetWindowDelta(delta))
            });
        }
    }

    pub fn update_event_listener(&self, listener: EventListener, action: Box<EventCallback>) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::EventListener {
                    id: *self,
                    listener,
                    action,
                })
            });
        }
    }
    pub fn update_resize_listener(&self, action: Box<ResizeCallback>) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::ResizeListener { id: *self, action })
            });
        }
    }

    pub fn update_cleanup_listener(&self, action: Box<dyn Fn()>) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::CleanupListener { id: *self, action })
            });
        }
    }

    pub fn update_animation(&self, animation: Animation) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::Animation {
                    id: *self,
                    animation,
                })
            });
        }
    }

    pub fn exec_after(&self, deadline: Duration, action: impl FnOnce() + 'static) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::RequestTimer {
                    deadline,
                    action: Box::new(action),
                })
            });
        }
    }

    pub fn open_file(
        &self,
        options: FileDialogOptions,
        file_info_action: impl Fn(Option<FileInfo>) + 'static,
    ) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::OpenFile {
                    options,
                    file_info_action: Box::new(file_info_action),
                })
            });
        }
    }

    pub fn save_as(
        &self,
        options: FileDialogOptions,
        file_info_action: impl Fn(Option<FileInfo>) + 'static,
    ) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::SaveAs {
                    options,
                    file_info_action: Box::new(file_info_action),
                })
            });
        }
    }

    pub fn update_context_menu(&self, menu: Box<MenuCallback>) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::ContextMenu { id: *self, menu })
            });
        }
    }

    pub fn update_popout_menu(&self, menu: Box<MenuCallback>) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::PopoutMenu { id: *self, menu })
            });
        }
    }

    pub fn show_context_menu(&self, menu: Menu, pos: Point) {
        if let Some(root) = self.root_id() {
            UPDATE_MESSAGES.with(|msgs| {
                let mut msgs = msgs.borrow_mut();
                let msgs = msgs.entry(root).or_default();
                msgs.push(UpdateMessage::ShowContextMenu { menu, pos })
            });
        }
    }
}
