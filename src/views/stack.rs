use taffy::style::FlexDirection;

use crate::{
    context::UpdateCx,
    id::Id,
    style::Style,
    view::{ChangeFlags, View},
    view_tuple::ViewTuple,
};

pub struct Stack {
    id: Id,
    children: Vec<Box<dyn View>>,
    direction: Option<FlexDirection>,
}

pub fn stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    let id = Id::next();
    Stack {
        id,
        children: children.into_views(),
        direction: None,
    }
}

/// A stack which defaults to `FlexDirection::Row`.
pub fn h_stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    let id = Id::next();
    Stack {
        id,
        children: children.into_views(),
        direction: Some(FlexDirection::Row),
    }
}

/// A stack which defaults to `FlexDirection::Column`.
pub fn v_stack<VT: ViewTuple + 'static>(children: VT) -> Stack {
    let id = Id::next();
    Stack {
        id,
        children: children.into_views(),
        direction: Some(FlexDirection::Column),
    }
}

impl View for Stack {
    fn id(&self) -> Id {
        self.id
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

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) -> ChangeFlags {
        if let Ok(state) = state.downcast() {
            self.children = *state;
            cx.request_all(self.id);
            ChangeFlags::all()
        } else {
            ChangeFlags::empty()
        }
    }
}
