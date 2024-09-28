//! # Floem built-in Views
//!
//! This module contains the basic built-in Views of Floem.
//!
//! ## Composing Views
//! The views in this module are the main building blocks for composing UIs in Floem.
//! There is a collection of different `stacks` and `lists` that can be used to build collections of Views.
//! There are also basic widgets such as [text_inputs](text_input::text_input), [labels](label::label), [images](img::img), and [svgs](svg::svg).
//!
//! ## The counter example to show composing views
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
//!
//! ### Stacks and Lists
//! There are a few different stacks and lists that you can use to group your views and each is discussed here.
//! There are basic [stacks](stack()) and [lists](list()), a [dynamic stack](dyn_stack()), and [virtual stacks](virtual_stack()) and [virtual_lists](virtual_list()).
//! The basic stacks and basic lists are static and always contain the same elements in the same order, but the children can still get reactive updates.
//! The dynamic stack can dynamically change the elements in the stack by reactively updating the list of items provided to the [dyn_stack](dyn_stack()).
//! Virtual stacks and virtual lists are like the dynamic stack but they also lazily load the items as they appear in a [scroll view](scroll()) and do not support the flexbox nor grid layout algorithms.
//! Instead, they give every element a consistent size and use a basic layout.
//! This is done for performance and allows for lists of millions of items to be used with very high performance.
//!
//! Lists differ from stacks in that they also have built-in support for the selection of items: up and down using arrow keys, top and bottom control using the home and end keys, and for the "acceptance" of an item using the Enter key.
//! You could build this manually yourself using stacks but it is common enough that it is built-in as a list.
//!
//! For the most direct documentation for Floem Views see the [Functions](#functions) section of this module documentation.

//! # View and Widget Traits
//!
//! Views are self-contained components that can be composed together to create complex UIs.
//! Views are the main building blocks of Floem.
//!
//! Views are structs that implement the View and widget traits. Many of these structs will also contain a child field that also implements View. In this way, views can be composed together easily to create complex UIs. This is the most common way to build UIs in Floem. For more information on how to compose views check out the [Views](crate::views) module.
//!
//! Creating a struct and manually implementing the View and Widget traits is typically only needed for building new widgets and for special cases. The rest of this module documentation is for help when manually implementing View and Widget on your own types.
//!
//!
//! ## The View and Widget Traits
//! The [View](crate::View) trait is the trait that Floem uses to build  and display elements. The trait contains the methods for implementing updates, styling, layout, events, and painting.
//!
//! ## State management
//!
//! For all reactive state that your type contains, either in the form of signals or derived signals, you need to process the changes within an effect.
//! The most common pattern is to [get](floem_reactive::SignalGet::get) the data in an effect and pass it in to `id.update_state()` and then handle that data in the `update` method of the View trait.
//!
//! For example a minimal slider might look like the following. First, we define the struct that contains the [ViewId](crate::ViewId).
//! Then, we use a function to construct the slider. As part of this function we create an effect that will be re-run every time the signals in the  `percent` closure change.
//! In the effect we send the change to the associated [ViewId](crate::ViewId). This change can then be handled in the [View::update](crate::View::update) method.
//! ```rust
//! use floem::ViewId;
//! use floem::reactive::*;
//!
//! struct Slider {
//!     id: ViewId,
//! }
//! pub fn slider(percent: impl Fn() -> f32 + 'static) -> Slider {
//!    let id = ViewId::new();
//!
//!    // If the following effect is not created, and `percent` is accessed directly,
//!    // `percent` will only be accessed a single time and will not be reactive.
//!    // Therefore the following `create_effect` is necessary for reactivity.
//!    create_effect(move |_| {
//!        let percent = percent();
//!        id.update_state(percent);
//!    });
//!    Slider {
//!        id,
//!    }
//! }
//! ```
//!

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
