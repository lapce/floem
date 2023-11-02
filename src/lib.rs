//! # Floem
//! Floem is cross-platform GUI framework for Rust 🦀. It aims to be extremely performant while providing world-class developer ergonomics.
//!
//! ## Views
//! Floem models the UI using a tree of [Views](view::View) that are built once.
//! Views ar self-contained components that can be composed together to create complex UIs that react to state changes and events.
//!
//! Views themselves do not update in response to changes in the reactive system. This is to prevent unnecessary rebuilds of View components which can be expensive. For Views that do update reactively see [dyn_container](views::dyn_container)
//!
//! ## State management  
//! Floem uses reactivity built on signals and effects for its state management. This pattern
//! of reactivity has been popularized by Solidjs in the javascript ecosystem and that directly
//! inspired Leptos in the Rust ecosystem. Floem uses it's own reactive system but it's API is
//! nearly identical to the API in the leptos_reactive crate. The leptos reactive create has
//! [documentation](https://docs.rs/leptos_reactive/latest/leptos_reactive/), as well as the
//! [Leptos book](https://leptos-rs.github.io/leptos/), that are very helpful
//! for learning about signals and effects.
//!
//! ### Local state
//!
//! You can can create a signal anywhere in the program. When you use the signal within a view by using one of the available accessor methods, such as [get](floem_reactive::ReadSignal::get) or [with](floem_reactive::ReadSignal::with), the runtime will automatically subscribe the correct side effects to changes in that signal, creating reactivity. To the programmer this is invisible and the reactivity 'just works' by accessing the value where you want to use it.
//!
//! ```ignore
//! pub fn input_and_label() -> impl View {
//!
//!     let text = create_rw_signal("Hello world".to_string());
//!
//!     stack(||
//!        (
//!            text_input(text),
//!            label(|| text.get())
//!        )
//!     ).style(|| Style::new().padding(10.0))
//! }
//! ```
//! In this example `text` is a signal, containing a `String`,
//! that can be both read from and written to. It is then used in two different places in the
//! [Stack View](views::stack). The [text_input](views::text_input) has direct access to the RwSignal and
//! will mutate the underlying `String` when the user types in the input box. This will
//! reactivly update the [label](views::label), which displays the same text, in real time.
//!
//! ### Global state
//!
//! Global state can be implemented using [provide_context](floem_reactive::provide_context) and [use_context](floem_reactive::use_context).
//!
//! ## Styling
//! You can style your views by applying [Styles](style::Style) through the
//! [style](views::Decorators::style) method that is implemented for all types that impl View.
//!
//! The sizing and positioning layout system is based on
//! the flexbox (or grid) model using Taffy as the layout engine.
//!
//! Some Style properties, such as font size, are inherited from parent views and can be overridden.
//!
//! Styles can be updated reactively using any signal
//!
//! ```ignore
//!     some_view()
//!     .style(move || {
//!         Style::new()
//!             .flex_row()
//!             .width(100.pct())
//!             .height(32.0)
//!             .border_bottom(1.0)
//!             .border_color(Color::LIGHT_GRAY)
//!             .apply_if(index == active_tab.get(), |s| {
//!                 s.background(Color::GRAY)
//!             })
//!     })
//! ```
//!
//!
//!
//! ## More
//!
//! #### Check out all of the built-in [View](view::View)s
//! See the [Views module](views) for more info.
//!
//! #### Authoring your own custom [View](view::View)s
//! See the [View module](view) for more info.
//!
//! #### Understanding Ids
//! See the [Id module](id) for more info.
//!
//! #### Understanding Styles
//! See the [Style module](style) for more info.
//!
//! #### Understanding the update lifecycle
//! See the [Renderer module](renderer) for more info.
//!
//!
pub mod action;
pub mod animate;
mod app;
mod app_handle;
pub mod context;
pub mod event;
pub mod ext_event;
pub mod file;
pub mod id;
mod inspector;
pub mod keyboard;
pub mod menu;
pub mod pointer;
pub mod renderer;
pub mod responsive;
pub mod style;
pub mod unit;
mod update;
pub mod view;
pub mod view_tuple;
pub mod views;
pub mod window;
mod window_handle;

pub use app::{launch, quit_app, AppEvent, Application};
pub use floem_reactive as reactive;
pub use floem_renderer::cosmic_text;
pub use floem_renderer::Renderer;
pub use kurbo;
pub use peniko;
pub use taffy;
pub use window::{close_window, new_window};
