use taffy::FlexDirection;
use winit::keyboard::{Key, NamedKey};

use crate::event::{Event, EventListener, EventPropagation};
use crate::{prelude::*, ViewId};

use std::hash::{DefaultHasher, Hash, Hasher};

impl<T> VirtualStack<(usize, T)> {
    // For types that implement all constraints
    pub fn list_new<DF, I>(data_fn: DF) -> Self
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        T: Hash + Eq + IntoView + 'static,
    {
        Self::list_full(
            data_fn,
            |item| {
                let mut hasher = DefaultHasher::new();
                item.hash(&mut hasher);
                hasher.finish()
            },
            |item| item.into_view(),
        )
    }

    // For types that are hashable but need custom view
    pub fn list_with_view<DF, I, V>(data_fn: DF, view_fn: impl Fn(T) -> V + 'static) -> Self
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        T: Hash + Eq + 'static,
        V: IntoView,
    {
        Self::list_full(
            data_fn,
            |item| {
                let mut hasher = DefaultHasher::new();
                item.hash(&mut hasher);
                hasher.finish()
            },
            move |item| view_fn(item).into_view(),
        )
    }

    // For types that implement IntoView but need custom keys
    pub fn list_with_key<DF, I, K>(data_fn: DF, key_fn: impl Fn(&T) -> K + 'static) -> Self
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        T: IntoView + 'static,
        K: Hash + Eq + 'static,
    {
        Self::list_full(data_fn, key_fn, |item| item.into_view())
    }

    pub fn list_full<DF, I, KF, K, VF, V>(data_fn: DF, key_fn: KF, view_fn: VF) -> Self
    where
        DF: Fn() -> I + 'static,
        I: VirtualVector<T>,
        KF: Fn(&T) -> K + 'static,
        K: Eq + Hash + 'static,
        VF: Fn(T) -> V + 'static,
        V: IntoView + 'static,
        T: 'static,
    {
        virtual_list(data_fn, key_fn, view_fn)
    }
}

/// A view that supports virtual scrolling with item selection.
/// Selection is done using arrow keys, home/end for top/bottom.
pub fn virtual_list<T, DF, I, KF, K, VF, V>(
    data_fn: DF,
    key_fn: KF,
    view_fn: VF,
) -> VirtualStack<(usize, T)>
where
    DF: Fn() -> I + 'static,
    I: VirtualVector<T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(T) -> V + 'static,
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
            let child = view_fn(e).class(ListItemClass);
            let child_id = child.id();
            child.on_click_stop(move |_| {
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

    let direction = stack.direction;

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
                            stack_id.update_state(0);
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
        })
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
