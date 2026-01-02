//! Custom styling traits for view-specific styling capabilities.
//!
//! This module provides traits that allow views to have specialized styling methods
//! beyond the basic Style properties:
//!
//! - [`CustomStyle`] - Base trait for defining custom style types
//! - [`CustomStylable`] - Trait for views that can accept custom styling

use std::rc::Rc;

use floem_reactive::UpdaterEffect;

use crate::layout::responsive::ScreenSize;
use crate::view::{IntoView, View};

use super::{Style, StyleClass, StyleProp, StyleSelector, Transition};

/// A trait for custom styling of specific view types.
///
/// This trait allows views to have specialized styling methods beyond the basic Style properties.
/// Each implementing type provides custom styling capabilities for a particular view type.
///
/// # Example
/// ```rust
/// use floem::prelude::*;
/// use floem::style::CustomStylable;
/// use palette::css;
///
/// // Using custom styling on a text view
/// text("Hello").custom_style(|s: LabelCustomStyle| {
///     s.selection_color(css::BLUE)
/// });
/// ```
pub trait CustomStyle: Default + Clone + Into<Style> + From<Style> {
    /// The CSS class associated with this custom style type.
    type StyleClass: StyleClass;

    /// Applies standard styling methods to this custom style.
    ///
    /// This method allows you to use any of the standard Style methods while working
    /// within a custom style context.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use palette::css;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// label_custom_style.style(|s| s.padding(10.0).background(css::RED))
    /// # ;
    /// ```
    fn style(self, style: impl FnOnce(Style) -> Style) -> Self {
        let self_style = self.into();
        let new = style(self_style);
        new.into()
    }

    /// Applies custom styling when the element is hovered.
    ///
    /// This method allows you to define how the custom style should change
    /// when the mouse hovers over the element.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use palette::css;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// label_custom_style.hover(|s| s.selection_color(css::BLUE))
    /// # ;
    /// ```
    fn hover(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::Hover, |_| style(Self::default()).into());
        new.into()
    }

    /// Applies custom styling when the element has keyboard focus.
    ///
    /// This method allows you to define how the custom style should change
    /// when the element gains keyboard focus.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use palette::css;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// label_custom_style.focus(|s| s.selection_color(css::GREEN))
    /// # ;
    /// ```
    fn focus(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::Focus, |_| style(Self::default()).into());
        new.into()
    }

    /// Similar to the `:focus-visible` css selector, this style only activates when tab navigation is used.
    fn focus_visible(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::FocusVisible, |_| {
            style(Self::default()).into()
        });
        new.into()
    }

    /// Applies custom styling when the element is in a selected state.
    ///
    /// This method allows you to define how the custom style should change
    /// when the element is selected.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use palette::css;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// label_custom_style.selected(|s| s.selection_color(css::ORANGE))
    /// # ;
    /// ```
    fn selected(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::Selected, |_| style(Self::default()).into());
        new.into()
    }

    /// Applies custom styling when the element is disabled.
    ///
    /// This method allows you to define how the custom style should change
    /// when the element is in a disabled state.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use palette::css;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// label_custom_style.disabled(|s| s.selection_color(css::GRAY))
    /// # ;
    /// ```
    fn disabled(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::Disabled, |_| style(Self::default()).into());
        new.into()
    }

    /// Applies custom styling when the application is in dark mode.
    ///
    /// This method allows you to define how the custom style should change
    /// when the application switches to dark mode.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use palette::css;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// label_custom_style.dark_mode(|s| s.selection_color(css::WHITE))
    /// # ;
    /// ```
    fn dark_mode(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::DarkMode, |_| style(Self::default()).into());
        new.into()
    }

    /// Applies custom styling when the element is being actively pressed.
    ///
    /// This method allows you to define how the custom style should change
    /// when the element is being actively pressed (e.g., mouse button down).
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use palette::css;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// label_custom_style.active(|s| s.selection_color(css::RED))
    /// # ;
    /// ```
    fn active(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::Active, |_| style(Self::default()).into());
        new.into()
    }

    /// Applies custom styling that activates at specific screen sizes (responsive design).
    ///
    /// This method allows you to define how the custom style should change
    /// based on the screen size, enabling responsive custom styling.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use floem::layout::responsive::ScreenSize;
    /// # use palette::css;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// label_custom_style.responsive(ScreenSize::SM, |s| s.selection_color(css::PURPLE))
    /// # ;
    /// ```
    fn responsive(self, size: ScreenSize, style: impl FnOnce(Self) -> Self) -> Self {
        let over = style(Self::default());
        let over_style: Style = over.into();
        let mut self_style: Style = self.into();
        for breakpoint in size.breakpoints() {
            self_style.set_breakpoint(breakpoint, over_style.clone());
        }
        self_style.into()
    }

    /// Conditionally applies custom styling based on a boolean condition.
    ///
    /// This method allows you to apply custom styling only when a condition is true,
    /// providing a convenient way to chain conditional styling operations.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use palette::css;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// # let is_highlighted = true;
    /// label_custom_style.apply_if(is_highlighted, |s| s.selection_color(css::YELLOW))
    /// # ;
    /// ```
    fn apply_if(self, cond: bool, style: impl FnOnce(Self) -> Self) -> Self {
        if cond { style(self) } else { self }
    }

    /// Conditionally applies custom styling based on an optional value.
    ///
    /// This method allows you to apply custom styling only when an optional value is Some,
    /// passing the unwrapped value to the styling function.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use palette::css;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// # let maybe_color = Some(css::BLUE);
    /// label_custom_style.apply_opt(maybe_color, |s, color| s.selection_color(color))
    /// # ;
    /// ```
    fn apply_opt<T>(self, opt: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self {
        if let Some(t) = opt { f(self, t) } else { self }
    }

    /// Sets a transition animation for a specific custom style property.
    ///
    /// This method allows you to animate changes to custom style properties,
    /// creating smooth transitions when the property values change.
    ///
    /// # Example
    /// ```rust
    /// # use floem::prelude::*;
    /// # use floem::style::CustomStyle;
    /// # use std::time::Duration;
    /// # let label_custom_style = LabelCustomStyle::new();
    /// // Note: Actual property types vary by custom style implementation
    /// # let _ = label_custom_style;
    /// ```
    fn transition<P: StyleProp>(self, _prop: P, transition: Transition) -> Self {
        let mut self_style: Style = self.into();
        self_style
            .map
            .insert(P::prop_ref().info().transition_key, Rc::new(transition));
        self_style.into()
    }
}

/// A trait that enables views to accept custom styling beyond the standard Style properties.
///
/// This trait allows specific view types to provide their own specialized styling methods
/// that are tailored to their functionality. For example, a label might have custom
/// selection styling, or a button might have custom press animations.
///
/// # Type Parameters
///
/// * `S` - The custom style type associated with this view (e.g., `LabelCustomStyle`)
///
/// # Example
///
/// ```rust
/// use floem::prelude::*;
/// use floem::style::CustomStylable;
/// use palette::css;
///
/// // Using custom styling on a view that implements CustomStylable
/// text("Hello World")
///     .custom_style(|s: LabelCustomStyle| {
///         s.selection_color(css::BLUE)
///          .selectable(false)
///     });
/// ```
pub trait CustomStylable<S: CustomStyle + 'static>: IntoView<V = Self::DV> + Sized {
    /// The view type that this custom stylable converts to.
    type DV: View;

    /// Applies custom styling to the view with access to specialized custom style methods.
    ///
    /// This method allows you to use custom styling methods that are specific to this
    /// view type, going beyond the standard styling properties available on all views.
    ///
    /// # Parameters
    ///
    /// * `style` - A closure that takes the custom style type and returns the modified style
    ///
    /// # Implementation Note
    ///
    /// For trait implementors: Don't implement this method yourself, just use the trait's
    /// default implementation. The default implementation properly handles style registration
    /// and updates.
    ///
    /// # Example
    ///
    /// ```rust
    /// use floem::prelude::*;
    /// use floem::style::CustomStylable;
    ///
    /// // Custom styling with theme integration
    /// text("Status")
    ///     .custom_style(|s: LabelCustomStyle| {
    ///         s.selection_color(Color::from_rgb8(100, 150, 255))
    ///          .selectable(true)
    ///     });
    /// ```
    fn custom_style(self, style: impl Fn(S) -> S + 'static) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        let view_state = id.state();
        let offset = view_state.borrow_mut().style.next_offset();
        let style = UpdaterEffect::new(
            move || style(S::default()),
            move |style| id.update_style(offset, style.into()),
        );
        view_state.borrow_mut().style.push(style.into());
        view
    }
}
