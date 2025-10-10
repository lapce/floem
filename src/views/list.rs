use super::{Decorators, v_stack_from_iter};
use crate::context::StyleCx;
use crate::event::EventPropagation;
use crate::id::ViewId;
use crate::reactive::create_effect;
use crate::style::Style;
use crate::style_class;
use crate::view::IntoView;
use crate::{
    event::{Event, EventListener},
    view::View,
};
use floem_reactive::{RwSignal, SignalGet, SignalTrack, SignalUpdate, create_rw_signal};
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, NamedKey};

style_class!(pub ListClass);
style_class!(pub ListItemClass);

enum ListUpdate {
    SelectionChanged,
    Accept,
}

pub(crate) struct Item {
    pub(crate) id: ViewId,
    pub(crate) index: usize,
    pub(crate) selection: RwSignal<Option<usize>>,
    pub(crate) child: ViewId,
}

/// A list of views that support the selection of items. See [`list`].
pub struct List {
    id: ViewId,
    selection: RwSignal<Option<usize>>,
    onaccept: Option<Box<dyn Fn(Option<usize>)>>,
    child: ViewId,
}

impl List {
    /// Returns the index of the current selection (if any).
    pub fn selection(&self) -> RwSignal<Option<usize>> {
        self.selection
    }

    /// Adds a callback to the [List] that is updated when the current selected item changes.
    pub fn on_select(self, on_select: impl Fn(Option<usize>) + 'static) -> Self {
        create_effect(move |_| {
            let selection = self.selection.get();
            on_select(selection);
        });
        self
    }

    /// Adds a callback for user list selection with the `Enter` key.
    pub fn on_accept(mut self, on_accept: impl Fn(Option<usize>) + 'static) -> Self {
        self.onaccept = Some(Box::new(on_accept));
        self
    }
}

/// A list of views built from an iterator which remains static and always contains the same elements in the same order.
///
/// A list is like a [stack](super::stack()) but also has built-in support for the selection of items:
/// up and down using arrow keys, top and bottom control using the home and end keys,
/// and "acceptance" of an item using the Enter key.
///
/// ## Example
/// ```rust
/// use floem::views::*;
/// list(
///     vec![1, 1, 2, 2, 3, 4, 5, 6, 7, 8, 9]
///         .iter()
///         .map(|val| text(val)),
/// );
/// ```
pub fn list<V>(iterator: impl IntoIterator<Item = V>) -> List
where
    V: IntoView + 'static,
{
    let list_id = ViewId::new();
    let selection = create_rw_signal(None);
    create_effect(move |_| {
        selection.track();
        list_id.update_state(ListUpdate::SelectionChanged);
    });
    let stack = v_stack_from_iter(iterator.into_iter().enumerate().map(move |(index, v)| {
        let id = ViewId::new();
        let v = v.into_view().class(ListItemClass);
        let child = v.id();
        id.set_children([v]);
        Item {
            id,
            selection,
            index,
            child,
        }
        .on_click_stop(move |_| {
            if selection.get_untracked() != Some(index) {
                selection.set(Some(index));
                list_id.update_state(ListUpdate::Accept);
            }
        })
    }))
    .style(|s| s.width_full().height_full());
    let length = stack.id().children().len();
    let child = stack.id();
    list_id.set_children([stack]);
    List {
        id: list_id,
        selection,
        child,
        onaccept: None,
    }
    .keyboard_navigable()
    .on_event(EventListener::KeyDown, move |e| {
        if let Event::Key(KeyboardEvent {
            state: KeyState::Down,
            key,
            ..
        }) = e
        {
            match key {
                Key::Named(NamedKey::Home) => {
                    if length > 0 {
                        selection.set(Some(0));
                    }
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::End) => {
                    if length > 0 {
                        selection.set(Some(length - 1));
                    }
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let current = selection.get_untracked();
                    match current {
                        Some(i) => {
                            if i > 0 {
                                selection.set(Some(i - 1));
                            }
                        }
                        None => {
                            if length > 0 {
                                selection.set(Some(length - 1));
                            }
                        }
                    }
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::Enter) => {
                    list_id.update_state(ListUpdate::Accept);
                    EventPropagation::Stop
                }
                Key::Character(c) if c == " " => {
                    list_id.update_state(ListUpdate::Accept);
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::ArrowDown) => {
                    let current = selection.get_untracked();
                    match current {
                        Some(i) => {
                            if i < length - 1 {
                                selection.set(Some(i + 1));
                            }
                        }
                        None => {
                            if length > 0 {
                                selection.set(Some(0));
                            }
                        }
                    }
                    EventPropagation::Stop
                }
                _ => EventPropagation::Continue,
            }
        } else {
            EventPropagation::Continue
        }
    })
    .class(ListClass)
}

impl View for List {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(change) = state.downcast::<ListUpdate>() {
            match *change {
                ListUpdate::SelectionChanged => {
                    self.id.request_style_recursive();
                    if let Some(index) = self.selection.get_untracked() {
                        self.child.children()[index].scroll_to(None);
                    }
                }
                ListUpdate::Accept => {
                    if let Some(on_accept) = &self.onaccept {
                        on_accept(self.selection.get_untracked());
                    }
                }
            }
        }
    }
}

impl View for Item {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        Some(Style::new().flex_col())
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Item".into()
    }

    fn style_pass(&mut self, cx: &mut StyleCx<'_>) {
        let selected = self.selection.get_untracked();
        if Some(self.index) == selected {
            cx.save();
            cx.selected();
            cx.style_view(self.child);
            cx.restore();
        } else {
            cx.style_view(self.child);
        }
    }
}

/// A trait that adds a `list` method to any generic type `T` that implements [`IntoIterator`] where
/// `T::Item` implements [IntoView].
pub trait ListExt {
    fn list(self) -> List;
}
impl<V: IntoView + 'static, T: IntoIterator<Item = V> + 'static> ListExt for T {
    fn list(self) -> List {
        list(self)
    }
}
