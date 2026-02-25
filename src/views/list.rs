use crate::{
    AnyView, ViewId,
    context::Phases,
    custom_event,
    event::{Event, EventPropagation, RouteKind},
    prelude::*,
    style::{Style, StyleCx, recalc::StyleReasonSet},
    style_class,
};
use floem_reactive::{Effect, RwSignal, SignalGet, SignalUpdate};
use ui_events::keyboard::{Key, KeyboardEvent, NamedKey};

style_class!(pub ListClass);
style_class!(pub ListItemClass);

/// Event fired when a list's selection changes
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListSelectionChanged {
    /// The previous selection index
    pub old_selection: Option<usize>,
    /// The new selection index
    pub new_selection: Option<usize>,
}
custom_event!(ListSelectionChanged);

/// Event fired when a list item is accepted (Enter key or Space)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListAccept {
    /// The accepted selection index
    pub selection: Option<usize>,
}
custom_event!(ListAccept);

enum ListUpdate {
    SelectionChanged(Option<usize>),
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
}

impl List {
    /// Returns the index of the current selection (if any).
    pub fn selection(&self) -> RwSignal<Option<usize>> {
        self.selection
    }

    /// Adds a callback to the [List] that is updated when the current selected item changes.
    pub fn on_select(self, on_select: impl Fn(Option<usize>) + 'static) -> Self {
        Effect::new(move |_| {
            let selection = self.selection.get();
            on_select(selection);
        });
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
    let selection = RwSignal::new(Some(0));
    Effect::new(move |old_sel: Option<Option<usize>>| {
        let new_sel = selection.get();
        let old_sel = old_sel.flatten();

        if old_sel != new_sel {
            // Dispatch custom event when selection changes
            list_id.route_event(
                Event::new_custom(ListSelectionChanged {
                    old_selection: old_sel,
                    new_selection: new_sel,
                }),
                RouteKind::Directed {
                    target: list_id.get_element_id(),
                    phases: Phases::TARGET,
                },
            );
            list_id.update_state(ListUpdate::SelectionChanged(old_sel));
        }

        new_sel
    });
    let children = iterator
        .into_iter()
        .enumerate()
        .map(move |(index, v)| {
            let item_id = ViewId::new();
            let v = v.into_view().class(ListItemClass);
            let child = v.id();
            item_id.set_children([v]);
            Item {
                id: item_id,
                selection,
                index,
                child,
            }
            .action(move || {
                if selection.get_untracked() != Some(index) {
                    selection.set(Some(index));
                    list_id.update_state(ListUpdate::Accept);
                }
            })
        })
        .map(|v| -> Box<dyn View> { v.into_any() })
        .collect::<Vec<AnyView>>();
    let length = children.len();
    list_id.set_children_vec(children);
    List {
        id: list_id,
        selection,
    }
    .on_event(
        listener::KeyDown,
        move |_cx, KeyboardEvent { key, .. }| match key {
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
        },
    )
    .on_event_stop(listener::Click, move |_cx, _| {
        list_id.update_state(ListUpdate::Accept);
    })
    .class(ListClass)
}

impl View for List {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<Style> {
        Some(Style::new().flex_col())
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(change) = state.downcast::<ListUpdate>() {
            match *change {
                ListUpdate::SelectionChanged(old_selection) => {
                    if let Some(index) = self.selection.get_untracked() {
                        if let Some(child) = self.id.children().get(index) {
                            child.request_style(StyleReasonSet::style_pass());
                            child.scroll_to(None);
                        }
                    }
                    if let Some(index) = old_selection {
                        if let Some(child) = self.id.children().get(index) {
                            child.request_style(StyleReasonSet::style_pass());
                        }
                    }
                }
                ListUpdate::Accept => {
                    let selection = self.selection.get_untracked();

                    // Dispatch custom event
                    self.id.route_event(
                        Event::new_custom(ListAccept { selection }),
                        RouteKind::Directed {
                            target: self.id.get_element_id(),
                            phases: Phases::TARGET,
                        },
                    );
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
        "List Item".into()
    }

    fn style_pass(&mut self, _cx: &mut StyleCx<'_>) {
        let selected = self.selection.get_untracked();
        if Some(self.index) == selected {
            self.child.parent_set_selected();
        } else {
            self.child.parent_clear_selected();
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
