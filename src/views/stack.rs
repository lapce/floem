use taffy::style::FlexDirection;

use crate::{
    context::UpdateCx,
    id::ViewId,
    style::{Style, StyleClassRef},
    view::{IntoView, View},
    view_tuple::ViewTuple,
};

/// A collection of static views. See [`stack`] and [`stack_from_iter`].
///
/// The children of a stack can still get reactive updates.
pub struct Stack {
    id: ViewId,
    direction: Option<FlexDirection>,
}

pub(crate) fn create_stack(
    children: Vec<Box<dyn View>>,
    direction: Option<FlexDirection>,
) -> Stack {
    let id = ViewId::new();
    id.set_children(children);

    Stack { id, direction }
}

/// A basic stack that is built from a tuple of views which remains static and always contains the same elements in the same order.
///
/// The children of a stack can still get reactive updates.
/// See also [`v_stack`] and [`h_stack`].
///
/// ## Example
/// ```rust
/// use floem::views::*;
/// stack((
///    text("first element"),
///     stack((
///        text("new stack"),
///        empty(),
///     )),
/// ));
/// ```
pub fn stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    create_stack(children.into_views(), None)
}

/// A stack which defaults to `FlexDirection::Row`. See also [`v_stack`].
pub fn h_stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    create_stack(children.into_views(), Some(FlexDirection::Row))
}

/// A stack which defaults to `FlexDirection::Column`. See also [`h_stack`].
pub fn v_stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    create_stack(children.into_views(), Some(FlexDirection::Column))
}

fn from_iter<V>(iterator: impl IntoIterator<Item = V>, direction: Option<FlexDirection>) -> Stack
where
    V: IntoView + 'static,
{
    create_stack(
        iterator
            .into_iter()
            .map(|v| -> Box<dyn View> { v.into_any() })
            .collect(),
        direction,
    )
}

/// Creates a stack from an iterator of views. See also [`v_stack_from_iter`] and [`h_stack_from_iter`].
///
/// ## Example
/// ```rust
/// use floem::views::*;
/// stack_from_iter(vec![1,1,2,2,3,4,5,6,7,8,9].iter().map(|val| text(val)));
/// ```
pub fn stack_from_iter<V>(iterator: impl IntoIterator<Item = V>) -> Stack
where
    V: IntoView + 'static,
{
    from_iter(iterator, None)
}

/// Creates a stack from an iterator of views. It defaults to `FlexDirection::Row`. See also [`v_stack_from_iter`].
pub fn h_stack_from_iter<V>(iterator: impl IntoIterator<Item = V>) -> Stack
where
    V: IntoView + 'static,
{
    from_iter(iterator, Some(FlexDirection::Row))
}

/// Creates a stack from an iterator of views. It defaults to `FlexDirection::Column`.See also [`h_stack_from_iter`].
pub fn v_stack_from_iter<V>(iterator: impl IntoIterator<Item = V>) -> Stack
where
    V: IntoView + 'static,
{
    from_iter(iterator, Some(FlexDirection::Column))
}

impl View for Stack {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        self.direction
            .map(|direction| Style::new().flex_direction(direction))
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        match self.direction {
            Some(FlexDirection::Column) => "Vertical Stack".into(),
            Some(FlexDirection::Row) => "Horizontal Stack".into(),
            _ => "Stack".into(),
        }
    }

    fn update(&mut self, _cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<Vec<Box<dyn View>>>() {
            self.id.set_children(*state);
            self.id.request_all();
        }
    }
}

impl Stack {
    pub fn add_class_by_idx(self, class: impl Fn(usize) -> StyleClassRef) -> Self {
        for (index, child) in self.id.children().into_iter().enumerate() {
            let style_class = class(index);
            child.add_class(style_class);
        }
        self
    }
}

pub trait StackExt {
    fn stack(self, direction: FlexDirection) -> Stack;
    fn v_stack(self) -> Stack
    where
        Self: Sized,
    {
        StackExt::stack(self, FlexDirection::Column)
    }
    fn h_stack(self) -> Stack
    where
        Self: Sized,
    {
        StackExt::stack(self, FlexDirection::Row)
    }
}
impl<V: IntoView + 'static, T: IntoIterator<Item = V> + 'static> StackExt for T {
    fn stack(self, direction: FlexDirection) -> Stack {
        from_iter(self, Some(direction))
    }
}
// Necessary to have a separate Ext trait because IntoIterator could be implemented on tuples of specific view types
pub trait TupleStackExt {
    fn stack(self, direction: FlexDirection) -> Stack;
    fn v_stack(self) -> Stack
    where
        Self: Sized,
    {
        TupleStackExt::stack(self, FlexDirection::Column)
    }
    fn h_stack(self) -> Stack
    where
        Self: Sized,
    {
        TupleStackExt::stack(self, FlexDirection::Row)
    }
}

impl<T: ViewTuple + 'static> TupleStackExt for T {
    fn stack(self, direction: FlexDirection) -> Stack {
        create_stack(self.into_views(), Some(direction))
    }
}
