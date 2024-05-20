//! # Floem built-in Views
//!
//! This module contains the basic built-in Views of Floem.
//!
//! ## Composing Views
//! The views in this module are the main building blocks for composing UIs in Floem.
//! There is a collection of different `stacks` and `lists` that can be used to build collections of Views.
//! There are also basic widgets such as [text_inputs](text_input::text_input), [labels](label::label), [images](img::img), and [svgs](svg::svg).
//! For more widgets see the [widgets module](crate::widgets).
//!
//! ## The counter example to show composing views
//! ```rust
//! use floem::{reactive::*, views::*};
//!
//! let (counter, set_counter) = create_signal(0);
//! v_stack((
//!     label(move || format!("Value: {}", counter.get())),
//!     h_stack((
//!         button(|| "Increment").on_click_stop(move |_| {
//!             set_counter.update(|value| *value += 1);
//!         }),
//!         button(|| "Decrement").on_click_stop(move |_| {
//!             set_counter.update(|value| *value -= 1);
//!         }),
//!     )),
//! ));
//! ```
//!
//! ### Stacks and Lists
//! There are a few different stacks and lists that you can use to group your views and each is discussed here.
//! There are basic [stacks](stack()) and [lists](list()), a [dynamic stack](dyn_stack()), and [virtual stacks](virtual_stack()) and [virtual_lists](virtual_list()).
//! The basic stacks and basic lists are static and always contain the same elements in the same order, but the children can still get reactive updates.
//! The dynamic stack can dynamically change the elements in the stack by reactively updating the list of items provided to the [dyn_stack](dyn_stack()).
//! Virtual stacks and virtual lists are like the dynamic stack but they also lazily load the items as they appear in a [scroll view](scroll()) and do not support the flexbox nor grid layout algorithms.
//! Instead, they give every element a consistent size and use a basic layout.
//! This is done for perfomance and allows for lists of millions of items to be used with very high performance.
//!
//! Lists differ from stacks in that they also have built-in support for the selection of items: up and down using arrow keys, top and bottom control using the home and end keys, and for the "acceptance" of an item using the Enter key.
//! You could build this manually yourself using stacks but it is common enough that it is built-in as a list.
//!
//! For the most direct documentation for Floem Views see the [Functions](#functions) section of this module documentation.

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
pub use scroll::{scroll, Scroll};

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

mod radio_button;
pub use radio_button::*;

mod checkbox;
pub use checkbox::*;

mod toggle_button;
pub use toggle_button::*;
