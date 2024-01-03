use super::{ListClass, ListItemClass};
use crate::{
    view::View,
    views::{
        self, container, Decorators, VirtualDirection, VirtualItemSize, VirtualList, VirtualVector,
    },
};
use std::hash::Hash;

pub fn virtual_list<T, IF, I, KF, K, VF, V>(
    direction: VirtualDirection,
    item_size: VirtualItemSize<T>,
    each_fn: IF,
    key_fn: KF,
    view_fn: VF,
) -> VirtualList<T>
where
    T: 'static,
    IF: Fn() -> I + 'static,
    I: VirtualVector<T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(T) -> V + 'static,
    V: View + 'static,
{
    views::virtual_list(direction, item_size, each_fn, key_fn, move |e| {
        container(view_fn(e))
            .class(ListItemClass)
            .style(move |s| match direction {
                VirtualDirection::Horizontal => s.flex_row(),
                VirtualDirection::Vertical => s.flex_col(),
            })
    })
    .class(ListClass)
}
