use taffy::style::FlexDirection;

use crate::{
    context::UpdateCx,
    id::Id,
    style::Style,
    view::{View, ViewData},
    view_tuple::ViewTuple,
};

pub struct Stack {
    data: ViewData,
    pub(crate) children: Vec<Box<dyn View>>,
    direction: Option<FlexDirection>,
}

pub fn stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    Stack {
        data: ViewData::new(Id::next()),
        children: children.into_views(),
        direction: None,
    }
}

/// A stack which defaults to `FlexDirection::Row`.
pub fn h_stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    Stack {
        data: ViewData::new(Id::next()),
        children: children.into_views(),
        direction: Some(FlexDirection::Row),
    }
}

/// A stack which defaults to `FlexDirection::Column`.
pub fn v_stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    Stack {
        data: ViewData::new(Id::next()),
        children: children.into_views(),
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
            .map(|v| -> Box<dyn View> { Box::new(v) })
            .collect(),
        direction,
    }
}

/// Creates a stack from an iterator of views.
pub fn stack_from_iter<V>(iterator: impl IntoIterator<Item = V>) -> Stack
where
    V: View + 'static,
{
    from_iter(iterator, None)
}

/// Creates a stack from an iterator of views. It defaults to `FlexDirection::Row`.
pub fn h_stack_from_iter<V>(iterator: impl IntoIterator<Item = V>) -> Stack
where
    V: View + 'static,
{
    from_iter(iterator, Some(FlexDirection::Row))
}

/// Creates a stack from an iterator of views. It defaults to `FlexDirection::Column`.
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

    fn view_style(&self) -> Option<crate::style::Style> {
        self.direction
            .map(|direction| Style::new().flex_direction(direction))
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        for child in &self.children {
            if for_each(child) {
                break;
            }
        }
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        for child in &mut self.children {
            if for_each(child) {
                break;
            }
        }
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn View) -> bool,
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
