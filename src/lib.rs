//! # Floem
//! Floem is cross-platform GUI framework for Rust 🦀. It aims to be extremely performant while providing world-class developer ergonomics.
//!
//! ## Views
//! Floem models the UI using a tree of [View](view::View) instances that is constructed once. Views are self-contained
//! components that can be composed together to create complex UIs, capable of reacting to state changes and events.
//!
//! To ensure good UI performance, view composition functions are not rerun in response to changes in the reactive system.
//! Unnecessary rebuilds of views can be expensive. If you have a use case for which having a view composition
//! function that responds to reactivity is essential, you may wish to consider [dyn_container](views::dyn_container).
//!
//! You can read more about [authoring your own views](crate::view) or see [all built-in views](crate::views).
//!
//! ## Widgets
//! Widgets are specialized high-level views providing certain functionality. Common examples include buttons, labels or
//! text input fields. For a list of Floem's built-in widgets, refer [here](widgets#functions). You can try them out
//! via the [widget gallery example](https://github.com/lapce/floem/blob/main/examples/widget-gallery/src/main.rs).
//!
//! ## State management  
//! Floem uses reactivity built on signals and effects for its state management. This design
//! pattern has been popularized by SolidJS in the JavaScript ecosystem and directly
//! inspired Leptos in the Rust ecosystem. Floem uses its own reactive system with an API that
//! is nearly identical to the one in the leptos_reactive crate. To learn more about signals and
//! effects, you may want to explore Leptos' [documentation](https://docs.rs/leptos_reactive/latest/leptos_reactive/)
//! and their [book](https://leptos-rs.github.io/leptos/).
//!
//! #### Local state
//!
//! You can create a signal anywhere in the program using [`create_rw_signal`](floem_reactive::create_rw_signal)
//! or [`create_signal`](floem_reactive::create_signal). When you use a signal's value within a view by calling
//! [`get`](floem_reactive::ReadSignal::get) or [`with`](floem_reactive::ReadSignal::with),
//! the runtime will automatically subscribe the correct side effects
//! to changes in that signal, creating reactivity. To the programmer this is transparent. The reactivity
//! "just works" by accessing the value where you want to use it.
//!
//! ```
//! # use floem::reactive::create_rw_signal;
//! # use floem::view::View;
//! # use floem::views::{label, v_stack, Decorators};
//! # use floem::widgets::text_input;
//! #
//! fn app_view() -> impl View {
//!     let text = create_rw_signal("Hello world".to_string());
//!     v_stack((text_input(text), label(move || text.get()))).style(|s| s.padding(10.0))
//! }
//! ```
//!
//! In this example, `text` is a signal containing a `String` that can both be read from and written to.
//! The signal is used in two different places in the [vertical stack](crate::views::v_stack).
//! The [text input](crate::views::text_input) has direct access to the [`RwSignal`](floem_reactive::RwSignal)
//! and will mutate the underlying `String` when the user types in the input box. The reactivity will then
//! trigger a rerender of the [label](crate::views::label) with the updated text value.
//!
//! [`create_signal`](floem_reactive::create_signal) returns a separated
//! [`ReadSignal`](floem_reactive::ReadSignal) and [`WriteSignal`](floem_reactive::WriteSignal) for a variable.
//! An existing `RwSignal` may be converted using [`RwSignal::read_only`](floem_reactive::RwSignal::read_only)
//! and [`RwSignal::write_only`](floem_reactive::RwSignal::write_only) where necessary, but the reverse is not
//! possible.
//!
//! #### Global state
//!
//! Global state can be implemented using [provide_context](floem_reactive::provide_context) and
//! [use_context](floem_reactive::use_context).
//!
//! ## Customizing appearance
//!
//! You can style a View instance by calling its [`style`](view::View::style) method. You'll need to import the
//! `floem::views::Decorators` trait to use it. The `style` method takes a function exposing a
//! [`Style`](crate::style::Style) parameter. Through this parameter, you can access methods that modify a variety
//! of familiar properties like width, padding and background. Some `Style` properties
//! such as font size are inherited from parent views and can be overridden.
//!
//! Styles can be updated reactively using any signal. Here's how to apply a gray background color while the value
//! held by the `active_tab` signal equals 0:
//!
//! ```
//! #  use floem::peniko::Color;
//! #  use floem::reactive::create_signal;
//! #  use floem::style::Style;
//! #  use floem::unit::UnitExt;
//! #  use floem::view::View;
//! #  use floem::views::{label, Decorators};
//! #
//! # let (active_tab, _set_active_tab) = create_signal(0);
//! #
//! label(|| "Some text").style(move |s| {
//!     s.flex_row()
//!         .width(100.pct())
//!         .height(32.0)
//!         .border_bottom(1.0)
//!         .border_color(Color::LIGHT_GRAY)
//!         .apply_if(active_tab.get() == 0, |s| s.background(Color::GRAY))
//! });
//! ```
//!
//! For additional information about styling, [see here](crate::style::Style).
//!
//! ## Themes and widget customizations
//!
//! Floem widgets ship with default styling that can be customized to your liking using style
//! classes. Take the [text input widget](https://github.com/lapce/floem/blob/main/src/widgets/text_input.rs)
//! for example: it exposes a style class `TextInputClass`. Any styling rules that are attached
//! to this class using [Style's `class` method](style::Style::class) will be applied to the text input.
//! Widgets may expose multiple classes to enable customization of different aspects of their UI. The
//! labeled checkbox is an example of this: both the checkbox itself and the label next to it can
//! be customized using `CheckboxClass` and `LabeledCheckboxClass` respectively.
//!
//! To theme a window, call the [`style`](crate::view::View::style) method on your root view and inject
//! your stylesheet. In your [`WindowConfig`](crate::window::WindowConfig), you may want to disable the
//! injection of Floem's default styling. The
//! [`themes` example](https://github.com/lapce/floem/blob/main/examples/themes/src/main.rs) is available
//! as a reference.
//!
//! Don't have the time or patience to develop your own theme? Check the
//! [floem-themes](https://github.com/topics/floem-themes) GitHub topic for a list of reusable
//! themes made by the community. This list is unmoderated.
//!
//! ## Additional reading
//!
//! - [Understanding Ids](crate::id)
//! - [How the update lifecycle works](crate::renderer)
//!
pub mod action;
pub mod animate;
mod app;
mod app_handle;
mod clipboard;
pub mod context;
pub mod event;
pub mod ext_event;
pub mod file;
pub mod id;
mod inspector;
pub mod keyboard;
pub mod menu;
mod nav;
pub mod pointer;
mod profiler;
pub mod renderer;
pub mod responsive;
pub mod style;
pub mod unit;
mod update;
pub mod view;
pub(crate) mod view_data;
pub mod view_tuple;
pub mod views;
pub mod widgets;
pub mod window;
mod window_handle;

pub use app::{launch, quit_app, AppEvent, Application};
pub use clipboard::{Clipboard, ClipboardError};
pub use context::EventPropagation;
pub use floem_reactive as reactive;
pub use floem_renderer::cosmic_text;
pub use floem_renderer::Renderer;
pub use kurbo;
pub use peniko;
pub use taffy;
pub use window::{close_window, new_window};
