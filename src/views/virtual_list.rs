use floem_reactive::create_effect;
use taffy::FlexDirection;
use ui_events::keyboard::{Key, KeyState, KeyboardEvent, NamedKey};

use crate::event::{Event, EventListener, EventPropagation};
use crate::{ViewId, prelude::*};

use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::{Deref, DerefMut};

pub struct VirtualList<T: 'static> {
    stack: VirtualStack<(usize, T)>,
    selection: RwSignal<Option<usize>>,
}

impl<T> VirtualList<T> {
    // For types that implement all constraints
    pub fn new<DF, I>(data_fn: DF) -> Self
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        T: Hash + Eq + IntoView + 'static,
    {
        Self::full(
            data_fn,
            |item| {
                let mut hasher = DefaultHasher::new();
                item.hash(&mut hasher);
                hasher.finish()
            },
            |_index, item| item.into_view(),
        )
    }

    // For types that are hashable but need custom view
    pub fn with_view<DF, I, V>(data_fn: DF, view_fn: impl Fn(T) -> V + 'static) -> Self
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        T: Hash + Eq + 'static,
        V: IntoView,
    {
        Self::full(
            data_fn,
            |item| {
                let mut hasher = DefaultHasher::new();
                item.hash(&mut hasher);
                hasher.finish()
            },
            move |_index, item| view_fn(item).into_view(),
        )
    }

    // For types that implement IntoView but need custom keys
    pub fn with_key<DF, I, K>(data_fn: DF, key_fn: impl Fn(&T) -> K + 'static) -> Self
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        T: IntoView + 'static,
        K: Hash + Eq + 'static,
    {
        Self::full(data_fn, key_fn, |_index, item| item.into_view())
    }

    pub fn full<DF, I, KF, K, VF, V>(data_fn: DF, key_fn: KF, view_fn: VF) -> Self
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        KF: Fn(&T) -> K + 'static,
        K: Eq + Hash + 'static,
        VF: Fn(usize, T) -> V + 'static,
        V: IntoView + 'static,
        T: 'static,
    {
        virtual_list(data_fn, key_fn, view_fn)
    }
}

impl<T: 'static> Deref for VirtualList<T> {
    type Target = VirtualStack<(usize, T)>;
    fn deref(&self) -> &VirtualStack<(usize, T)> {
        &self.stack
    }
}

impl<T: 'static> DerefMut for VirtualList<T> {
    fn deref_mut(&mut self) -> &mut VirtualStack<(usize, T)> {
        &mut self.stack
    }
}

impl<T: 'static> VirtualList<T> {
    pub fn selection(&self) -> RwSignal<Option<usize>> {
        self.selection
    }

    /// Sets a callback function to be called whenever the selection changes in the virtual list.
    ///
    /// The callback function receives an `Option<usize>` parameter representing the currently
    /// selected item index. When `None`, no item is selected. When `Some(index)`, the item
    /// at that index is currently selected.
    ///
    /// This is a convenience helper that creates a new effect internally. Calling this method
    /// multiple times will not override previous `on_select` calls - each call creates a separate
    /// effect that will all be triggered on selection changes. For more control, you can manually
    /// create effects using the selection signal returned by [`selection()`](Self::selection).
    ///
    /// # Parameters
    ///
    /// * `on_select` - A function that takes `Option<usize>` and will be called on selection changes
    ///
    /// # Returns
    ///
    /// Returns `self` to allow method chaining.
    ///
    /// # Example
    ///
    /// ```rust
    /// use floem::prelude::*;
    ///
    /// virtual_list(
    ///     move || 1..=1000000,
    ///     |item| *item,
    ///     |index, item| format!("{index}: {item}")
    /// )
    /// .on_select(|selection| {
    ///     match selection {
    ///         Some(index) => println!("Selected item at index: {index}"),
    ///         None => println!("No item selected"),
    ///     }
    /// });
    /// ```
    pub fn on_select(self, on_select: impl Fn(Option<usize>) + 'static) -> Self {
        create_effect(move |_| {
            let selection = self.selection.get();
            on_select(selection);
        });
        self
    }
}

/// A view that supports virtual scrolling with item selection.
/// Selection is done using arrow keys, home/end for top/bottom.
pub fn virtual_list<T, DF, I, KF, K, VF, V>(data_fn: DF, key_fn: KF, view_fn: VF) -> VirtualList<T>
where
    DF: Fn() -> I + 'static,
    I: VirtualVector<T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(usize, T) -> V + 'static,
    V: IntoView + 'static,
{
    let selection = RwSignal::new(None::<usize>);
    let length = RwSignal::new(0);

    let stack = virtual_stack(
        move || {
            let vector = data_fn().enumerate();
            length.set(vector.total_len());
            vector
        },
        move |(_i, d)| key_fn(d),
        move |(index, e)| {
            let child = view_fn(index, e).class(ListItemClass);
            let child_id = child.id();
            child.on_click_cont(move |_| {
                if selection.get_untracked() != Some(index) {
                    selection.set(Some(index));
                    child_id.scroll_to(None);
                    let Some(parent) = child_id.parent() else {
                        return;
                    };
                    parent.update_state(index);
                    parent.request_style_recursive();
                }
            })
        },
    )
    .style(|s| s.size_full());

    let stack_id = stack.id();

    create_effect(move |_| {
        if let Some(idx) = selection.get() {
            stack_id.update_state(idx);
        }
    });

    let direction = stack.direction;

    let stack =
        stack
            .class(ListClass)
            .keyboard_navigable()
            .on_event(EventListener::KeyDown, move |e| {
                if let Event::KeyDown(key_event) = e {
                    stack_id.request_style_recursive();
                    match key_event.key.logical_key {
                        Key::Named(NamedKey::Home) => {
                            if length.get_untracked() > 0 {
                                selection.set(Some(0));
                                stack_id.update_state(0_usize); // Must be usize to match state type
                            }
                            EventPropagation::Stop
                        }
                        Key::Named(NamedKey::End) => {
                            let len = length.get_untracked();
                            if len > 0 {
                                selection.set(Some(len - 1));
                                stack_id.update_state(len - 1);
                            }
                            EventPropagation::Stop
                        }
                        Key::Named(
                            named_key @ (NamedKey::ArrowUp
                            | NamedKey::ArrowDown
                            | NamedKey::ArrowLeft
                            | NamedKey::ArrowRight),
                        ) => handle_arrow_key(
                            selection,
                            length.get_untracked(),
                            direction.get_untracked(),
                            stack_id,
                            named_key,
                        ),
                        _ => EventPropagation::Continue,
                    }
                } else {
                    EventPropagation::Continue
                }
            });
    VirtualList { stack, selection }
}

fn handle_arrow_key(
    selection: RwSignal<Option<usize>>,
    len: usize,
    direction: FlexDirection,
    stack_id: ViewId,
    key: NamedKey,
) -> EventPropagation {
    let current = selection.get();

    // Determine if we should move forward or backward based on direction and key
    let should_move_forward = matches!(
        (direction, key),
        (FlexDirection::Row, NamedKey::ArrowRight)
            | (FlexDirection::RowReverse, NamedKey::ArrowLeft)
            | (FlexDirection::Column, NamedKey::ArrowDown)
            | (FlexDirection::ColumnReverse, NamedKey::ArrowUp)
    );

    let should_move_backward = matches!(
        (direction, key),
        (FlexDirection::Row, NamedKey::ArrowLeft)
            | (FlexDirection::RowReverse, NamedKey::ArrowRight)
            | (FlexDirection::Column, NamedKey::ArrowUp)
            | (FlexDirection::ColumnReverse, NamedKey::ArrowDown)
    );

    // Handle cross-axis navigation (e.g., up/down in Row mode)
    let is_cross_axis = matches!(
        (direction, key),
        (
            FlexDirection::Row | FlexDirection::RowReverse,
            NamedKey::ArrowUp | NamedKey::ArrowDown
        ) | (
            FlexDirection::Column | FlexDirection::ColumnReverse,
            NamedKey::ArrowLeft | NamedKey::ArrowRight
        )
    );

    if is_cross_axis {
        return EventPropagation::Continue;
    }

    match current {
        Some(i) => {
            if should_move_backward && i > 0 {
                selection.set(Some(i - 1));
                stack_id.update_state(i - 1);
            } else if should_move_forward && i < len - 1 {
                selection.set(Some(i + 1));
                stack_id.update_state(i + 1);
            }
        }
        None => {
            if len > 0 {
                let res = if should_move_backward { len - 1 } else { 0 };
                selection.set(Some(res));
                stack_id.update_state(res);
            }
        }
    }
    EventPropagation::Stop
}

impl<T: 'static> IntoView for VirtualList<T> {
    type V = VirtualStack<(usize, T)>;

    fn into_view(self) -> Self::V {
        self.stack
    }
}
