//! # Style and animations
//!
//! Styles and style animations are defined by closures modifying the style.
//!
//! [`Style`]: A style with definite values for most fields.
//!
//! [`StyleAnimCtx`]: A wrapper for [`Style`] with an `animation_value`,
//! typically representing the progress of the animation.
//!
//! [`StyleAnimFn`]: A function or closure taking an [`StyleAnimCtx`] and returning a modified [`StyleAnimCtx`].
//!
//! When defining static styles the `animation_value` will always be 1.0, when the style is applied.
//!

mod style;
pub use style::*;

mod animation;
pub use animation::*;

mod easing;
pub use easing::*;

mod blending;
pub use blending::*;

mod timed;
pub use timed::*;

mod fixed;
pub use fixed::*;
