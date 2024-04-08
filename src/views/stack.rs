use taffy::style::FlexDirection;

use crate::{
    context::UpdateCx,
    id::Id,
    style::{Style, StyleClassRef},
    view::{View, ViewData, Widget},
    view_tuple::ViewTuple,
};

/// A collection of static views. See [`stack`] and [`stack_from_iter`].
///
/// The children of a stack can still get reactive updates.
pub struct Stack {
    data: ViewData,
    pub(crate) children: Vec<Box<dyn Widget>>,
    direction: Option<FlexDirection>,
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
    Stack {
        data: ViewData::new(Id::next()),
        children: children.into_widgets(),
        direction: None,
    }
}

/// A stack which defaults to `FlexDirection::Row`. See also [`v_stack`].
pub fn h_stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    Stack {
        data: ViewData::new(Id::next()),
        children: children.into_widgets(),
        direction: Some(FlexDirection::Row),
    }
}

/// A stack which defaults to `FlexDirection::Column`. See also [`h_stack`].
pub fn v_stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    Stack {
        data: ViewData::new(Id::next()),
        children: children.into_widgets(),
        direction: Some(FlexDirection::Column),
    }
}

fn from_iter<V>(iterator: impl IntoIterator<Item = V>, direction: Option<FlexDirection>) -> Stack
where
    V: View + 'static,
{
    Stack {
        data: ViewData::new(Id::next()),
        children: iterator
            .into_iter()
            .map(|v| -> Box<dyn Widget> { v.build() })
            .collect(),
        direction,
    }
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
    V: View + 'static,
{
    from_iter(iterator, None)
}

/// Creates a stack from an iterator of views. It defaults to `FlexDirection::Row`. See also [`v_stack_from_iter`].
pub fn h_stack_from_iter<V>(iterator: impl IntoIterator<Item = V>) -> Stack
where
    V: View + 'static,
{
    from_iter(iterator, Some(FlexDirection::Row))
}

/// Creates a stack from an iterator of views. It defaults to `FlexDirection::Column`.See also [`h_stack_from_iter`].
pub fn v_stack_from_iter<V>(iterator: impl IntoIterator<Item = V>) -> Stack
where
    V: View + 'static,
{
    from_iter(iterator, Some(FlexDirection::Column))
}

impl View for Stack {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl Widget for Stack {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        self.direction
            .map(|direction| Style::new().flex_direction(direction))
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn Widget) -> bool) {
        for child in &self.children {
            if for_each(child) {
                break;
            }
        }
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool) {
        for child in &mut self.children {
            if for_each(child) {
                break;
            }
        }
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool,
    ) {
        for child in self.children.iter_mut().rev() {
            if for_each(child) {
                break;
            }
        }
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        match self.direction {
            Some(FlexDirection::Column) => "Vertical Stack".into(),
            Some(FlexDirection::Row) => "Horizontal Stack".into(),
            _ => "Stack".into(),
        }
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast() {
            self.children = *state;
            cx.request_all(self.id());
        }
    }
}

impl Stack {
    pub fn add_class_by_idx(self, class: impl Fn(usize) -> StyleClassRef) -> Self {
        for (index, child) in self.children.iter().enumerate() {
            let style_class = class(index);
            child.view_data().id().add_class(style_class);
        }
        self
    }
}
