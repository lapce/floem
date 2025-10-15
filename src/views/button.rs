#![deny(missing_docs)]
use crate::{style_class, views::Decorators, IntoView, View, ViewId};
use core::ops::FnMut;

style_class!(
    /// The style class that is applied to buttons.
    pub ButtonClass
);

/// Creates a new simple [Button] view with the default [ButtonClass] style.
///
/// ### Examples
/// ```rust
/// # use floem::views::button;
/// # use floem::prelude::{*, palette::css};
/// # use floem::views::Decorators;
/// # use floem::style::CursorStyle;
/// // Basic usage
/// let button1 = button("Click me").action(move || println!("Button1 clicked!"));
/// let button2 = button("Click me").action(move || println!("Button2 clicked!"));
/// // Apply styles for the button
/// let styled = button("Click me")
///     .action(|| println!("Styled button clicked!"))
///     .style(|s| s
///         .border(1.0)
///         .border_radius(10.0)
///         .padding(10.0)
///         .background(css::YELLOW_GREEN)
///         .color(css::DARK_GREEN)
///         .cursor(CursorStyle::Pointer)
///         .active(|s| s.color(css::WHITE).background(css::RED))
///         .hover(|s| s.background(Color::from_rgb8(244, 67, 54)))
///         .focus_visible(|s| s.border(2.).border_color(css::BLUE))
///     );
/// ```
pub fn button<V: IntoView + 'static>(child: V) -> Button {
    Button::new(child)
}

/// A simple Button view. See [`button`].
pub struct Button {
    id: ViewId,
}

impl View for Button {
    fn id(&self) -> ViewId {
        self.id
    }
}

impl Button {
    /// Create new [Button].
    /// ### Examples
    /// ```rust
    /// # use floem::views::button;
    /// # use floem::prelude::{*, palette::css};
    /// # use floem::views::Decorators;
    /// # use floem::style::CursorStyle;
    /// // Basic usage
    /// let button1 = button("Click me").action(move || println!("Button1 clicked!"));
    /// let button2 = button("Click me").action(move || println!("Button2 clicked!"));
    /// // Apply styles for the button
    /// let styled = button("Click me")
    ///     .action(|| println!("Styled button clicked!"))
    ///     .style(|s| s
    ///         .border(1.0)
    ///         .border_radius(10.0)
    ///         .padding(10.0)
    ///         .background(css::YELLOW_GREEN)
    ///         .color(css::DARK_GREEN)
    ///         .cursor(CursorStyle::Pointer)
    ///         .active(|s| s.color(css::WHITE).background(css::RED))
    ///         .hover(|s| s.background(Color::from_rgb8(244, 67, 54)))
    ///         .focus_visible(|s| s.border(2.).border_color(css::BLUE))
    ///     );
    /// ```
    /// ### Reactivity
    /// Button's label is not reactive.
    pub fn new(child: impl IntoView) -> Self {
        let id = ViewId::new();
        id.add_child(Box::new(child.into_view()));
        Button { id }.keyboard_navigable().class(ButtonClass)
    }

    /// Attach action executed on button click.
    /// ### Example
    /// ```rust
    /// # use floem::views::button;
    /// let button_with_action = button("Click me")
    ///     .action(move || println!("Button2 clicked!"));
    /// ```
    pub fn action(self, mut on_press: impl FnMut() + 'static) -> Self {
        self.on_click_stop(move |_| {
            on_press();
        })
    }
}

/// A trait that adds a `button` method to any type that implements `IntoView`.
pub trait ButtonExt {
    /// Create a [Button] from the parent.
    fn button(self) -> Button;
}
impl<T: IntoView + 'static> ButtonExt for T {
    fn button(self) -> Button {
        button(self)
    }
}
