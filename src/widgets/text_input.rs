use crate::{
    style_class,
    views::{self, Decorators, TextInput},
};

style_class!(pub TextInputClass);
style_class!(pub PlaceholderTextClass);

/// A simple single line text input.
/// If you need more advanced text handling, consider using [`views::text_editor`].
pub fn text_input(text: impl Fn() -> String + 'static) -> TextInput {
    views::text_input(text).keyboard_navigatable()
}
