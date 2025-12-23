use taffy::style::FlexDirection;

use crate::{
    context::UpdateCx,
    view::ViewId,
    style::{Style, StyleClassRef},
    view::{IntoView, IntoViewIter, View, ViewTuple},
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
    create_stack_with_id(ViewId::new(), children, direction)
}

pub(crate) fn create_stack_with_id(
    id: ViewId,
    children: Vec<Box<dyn View>>,
    direction: Option<FlexDirection>,
) -> Stack {
    id.set_children_vec(children);
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

pub(crate) fn from_iter_with_id<V>(
    id: ViewId,
    iterator: impl IntoIterator<Item = V>,
    direction: Option<FlexDirection>,
) -> Stack
where
    V: IntoView + 'static,
{
    create_stack_with_id(
        id,
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
            self.id.set_children_vec(*state);
            self.id.request_all();
        }
    }
}

impl Stack {
    /// Creates a new stack from any type that implements [`IntoViewIter`].
    ///
    /// This accepts arrays, tuples, vectors, slices, and iterators of views.
    ///
    /// ## Example
    /// ```rust,no_run
    /// use floem::views::*;
    ///
    /// // From array
    /// Stack::new([text("child 1"), text("child 2")]);
    ///
    /// // From tuple (heterogeneous types)
    /// Stack::new((text("label"), button("click")));
    ///
    /// // From vec
    /// Stack::new(vec![text("a"), text("b"), text("c")]);
    ///
    /// // From iterator
    /// Stack::new((0..5).map(|i| text(i)).collect::<Vec<_>>());
    /// ```
    pub fn new(children: impl IntoViewIter) -> Self {
        let id = ViewId::new();
        id.set_children_iter(children.into_view_iter());
        Stack {
            id,
            direction: None,
        }
    }

    /// Creates a new stack with a specific ViewId from a tuple of views.
    ///
    /// This is useful for lazy view construction where the `ViewId` is created
    /// before the view itself.
    ///
    /// ## Example
    /// ```rust
    /// use floem::{ViewId, views::Stack};
    ///
    /// let id = ViewId::new();
    /// Stack::with_id(id, ("child 1", "child 2")).horizontal();
    /// ```
    pub fn with_id(id: ViewId, children: impl IntoViewIter) -> Self {
        id.set_children_iter(children.into_view_iter());
        Stack {
            id,
            direction: None,
        }
    }

    /// Sets the stack direction to horizontal (row).
    pub fn horizontal(mut self) -> Self {
        self.direction = Some(FlexDirection::Row);
        self
    }

    /// Sets the stack direction to vertical (column).
    pub fn vertical(mut self) -> Self {
        self.direction = Some(FlexDirection::Column);
        self
    }

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
