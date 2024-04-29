use crate::{
    style_class,
    view::IntoView,
    views::{self, container, Decorators, List},
};

style_class!(pub ListClass);
style_class!(pub ListItemClass);

pub fn list<V>(iterator: impl IntoIterator<Item = V>) -> List
where
    V: IntoView + 'static,
{
    views::list(
        iterator
            .into_iter()
            .map(|view| container(view).class(ListItemClass)),
    )
    .class(ListClass)
}
