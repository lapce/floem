use crate::{
    style_class,
    views::{self, Decorators, TextInput},
};
use floem_reactive::RwSignal;

style_class!(pub TextInputClass);
style_class!(pub PlaceholderTextClass);

pub fn text_input(buffer: RwSignal<String>) -> TextInput {
    views::text_input(buffer).class(TextInputClass)
}

impl TextInput {
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder_text = Some(text.into());
        self
    }
}
