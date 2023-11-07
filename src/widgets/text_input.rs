use crate::{
    style_class,
    view::View,
    views::{self, Decorators},
};
use floem_reactive::RwSignal;

style_class!(pub TextInputClass);

pub fn text_input(buffer: RwSignal<String>) -> impl View {
    views::text_input(buffer).class(TextInputClass)
}
