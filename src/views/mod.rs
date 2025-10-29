//! # Floem built-in Views
//!
//! This module contains the basic built-in Views of Floem.
//!
//! ## Composing Views
//! The views in this module are the main building blocks for composing UIs in Floem.
//! There is a collection of different `stacks` and `lists` that can be used to build collections of views.
//! There are also basic widgets such as [text inputs](text_input::text_input), [labels](label::label), [images](img::img), and [svgs](svg::svg).
//!
//! ## Example: Counter
//! ```rust
//! use floem::{reactive::*, views::*};
//!
//! let mut counter = RwSignal::new(0);
//!
//! v_stack((
//!     label(move || format!("Value: {counter}")),
//!     h_stack((
//!         button("Increment").action(move || counter += 1),
//!         button("Decrement").action(move || counter -= 1),
//!     )),
//! ));
//! ```
//! Views in Floem can also be easily refactored.
//! ## Example: Refactored Counter
//! ```rust
//! use floem::prelude::*;
//!
//! let mut counter = RwSignal::new(0);
//!
//! let counter_label = label(move || format!("Value: {counter}"));
//!
//! let increment_button = button("Increment").action(move || counter += 1);
//! let decrement_button = button("Decrement").action(move || counter -= 1);
//!
//! let button_stack = (increment_button, decrement_button).h_stack();
//!
//! (counter_label, button_stack).v_stack();
//! ```
//!
//!
//! ### Stacks and Lists
//! There are a few different stacks and lists that you can use to group your views and each is discussed here.
//!
//!
//! They are:
//! - basic [stack](stack())
//!     - static and always contains the same elements in the same order
//! - [dynamic stack](dyn_stack())
//!     - can dynamically change the elements in the stack by reactively updating the list of items provided
//! - [virtual stack](virtual_stack::virtual_stack())
//!     - can dynamically change the elements in the stack
//!     - can lazily load the items as they appear in a [scroll view](scroll())
//!
//! There is also a basic [list](list()) and a [virtual list](virtual_list::virtual_list()).
//! Lists are like their stack counterparts but they also have built-in support for the selection of items: up and down using arrow keys, top and bottom control using the home and end keys, and for the "acceptance" of an item using the Enter key.
//! You could build this manually yourself using stacks but it is common enough that it is built-in as a list.
//!
//! ## View Trait
//! The [`View`](crate::View) trait is the trait that Floem uses to build and display elements.
//! The trait contains the methods for implementing updates, styling, layout, events, and painting.
//!
//! Views are types that implement `View`.
//! Many of these types will also be built with a child that also implements `View`.
//! In this way, views can be composed together easily to create complex UIs.
//! This composition is the most common way to build UIs in Floem.
//!
//! Creating a type and manually implementing the View trait is typically only needed for building new widgets and for special cases.

mod label;
pub use label::*;

mod rich_text;
pub use rich_text::*;

mod dyn_stack;
pub use dyn_stack::*;

mod svg;
pub use svg::*;

mod clip;
pub use clip::*;

mod container;
pub use container::*;

mod dyn_container;
pub use dyn_container::*;

mod dyn_view;
pub use dyn_view::*;

mod value_container;
pub use value_container::*;

mod decorator;
pub use decorator::*;

mod list;
pub use list::*;

mod virtual_list;
pub use virtual_list::*;

mod virtual_stack;
pub use virtual_stack::*;

pub mod scroll;
pub use scroll::{Scroll, ScrollExt, scroll};

mod tab;
pub use tab::*;

mod tooltip;
pub use tooltip::*;

mod stack;
pub use stack::*;

mod text_input;
pub use text_input::*;

mod empty;
pub use empty::*;

mod canvas;
pub use canvas::*;

mod drag_window_area;
pub use drag_window_area::*;

mod drag_resize_window_area;
pub use drag_resize_window_area::*;

mod img;
pub use img::*;

mod button;
pub use button::*;

#[cfg(feature = "editor")]
pub mod editor;

#[cfg(feature = "editor")]
pub mod text_editor;

#[cfg(feature = "editor")]
pub use text_editor::*;

pub mod dropdown;

pub mod slider;

pub mod resizable;

mod radio_button;
pub use radio_button::*;

mod checkbox;
pub use checkbox::*;

mod toggle_button;
pub use toggle_button::*;

#[cfg(feature = "localization")]
pub mod localization;
