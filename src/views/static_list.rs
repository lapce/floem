use crate::{id::Id, style::Style, view::View};
use taffy::style::FlexDirection;

pub struct StaticList {
    id: Id,
    children: Vec<Box<dyn View>>,
}

pub fn static_list<V>(iterator: impl IntoIterator<Item = V>) -> StaticList
where
    V: View + 'static,
{
    StaticList {
        id: Id::next(),
        children: iterator
            .into_iter()
            .map(|v| -> Box<dyn View> { Box::new(v) })
            .collect(),
    }
}

impl View for StaticList {
    fn id(&self) -> Id {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "StaticList".into()
    }

    fn view_style(&self) -> Option<crate::style::Style> {
        Some(Style::new().flex_direction(FlexDirection::Column))
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
        for child in &mut self.children.iter_mut().rev() {
            if for_each(child) {
                break;
            }
        }
    }
}
