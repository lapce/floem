//! Internal and public style-property value enums.
//!
//! [`StyleMapValue<T>`] is the internal representation used in the style
//! hashmap; [`StyleValue<T>`] is used in the public API. Both reference
//! [`ContextValue<T>`] for context-resolved values, which is why they live
//! here alongside it.

use crate::context_value::ContextValue;

/// Internal storage for style property values in the style map.
///
/// Unlike `StyleValue<T>` which is used in the public API, `StyleMapValue<T>`
/// is the internal representation stored in the style hashmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleMapValue<T> {
    /// Value inserted by animation interpolation
    Animated(T),
    /// Value set directly
    Val(T),
    /// Value resolved from inherited context when the property is read.
    Context(ContextValue<T>),
    /// Use the default value for the style, typically from the underlying `ComputedStyle`
    Unset,
}

/// The value for a [`Style`] property in the public API.
///
/// This represents the result of reading a style property, with additional
/// states like `Base` that indicate inheritance from parent styles.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StyleValue<T> {
    /// Value resolved from inherited context when the property is read.
    Context(ContextValue<T>),
    /// Value inserted by animation interpolation
    Animated(T),
    /// Value set directly
    Val(T),
    /// Use the default value for the style, typically from the underlying `ComputedStyle`.
    Unset,
    /// Use whatever the base style is. For an overriding style like hover, this uses the base
    /// style. For the base style, this is equivalent to `Unset`.
    #[default]
    Base,
}

impl<T: 'static> StyleValue<T> {
    pub fn map<U: 'static>(self, f: impl Fn(T) -> U + 'static) -> StyleValue<U> {
        match self {
            Self::Context(x) => StyleValue::Context(x.map(f)),
            Self::Val(x) => StyleValue::Val(f(x)),
            Self::Animated(x) => StyleValue::Animated(f(x)),
            Self::Unset => StyleValue::Unset,
            Self::Base => StyleValue::Base,
        }
    }

    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Self::Context(_) => default,
            Self::Val(x) => x,
            Self::Animated(x) => x,
            Self::Unset => default,
            Self::Base => default,
        }
    }

    pub fn unwrap_or_else(self, f: impl FnOnce() -> T) -> T {
        match self {
            Self::Context(_) => f(),
            Self::Val(x) => x,
            Self::Animated(x) => x,
            Self::Unset => f(),
            Self::Base => f(),
        }
    }

    pub fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Context(_) => None,
            Self::Val(x) => Some(x),
            Self::Animated(x) => Some(x),
            Self::Unset => None,
            Self::Base => None,
        }
    }
}

impl<T> From<T> for StyleValue<T> {
    fn from(x: T) -> Self {
        Self::Val(x)
    }
}

impl<T> From<ContextValue<T>> for StyleValue<T> {
    fn from(x: ContextValue<T>) -> Self {
        Self::Context(x)
    }
}
